#!/usr/bin/env node
// 拆解管线冒烟测试：mock LLM 服务器 + 临时 daemon 实例，端到端跑通
// 创建任务 → 探测/切块 → 笔记 → 证据 → 生成 → 回查 → 保存入库。
// 用法：node tools/smoke-deconstruct.mjs（需先 cargo build 出 shiro-daemon）
import { spawn } from 'node:child_process'
import { mkdtempSync, mkdirSync, rmSync, writeFileSync, existsSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import http from 'node:http'

const DAEMON = join(import.meta.dirname, '..', 'shiro-daemon', 'target', 'debug', 'shiro-daemon')
if (!existsSync(DAEMON)) {
  console.error('缺少 daemon 二进制，先 cd shiro-daemon && cargo build')
  process.exit(1)
}

// ---- 样例剧本（4 场，章节路径 + 台词归属） ----
const scene = (n, place, lines) =>
  [`第${n}场 ${place} 日`, ...lines, ''].join('\n')
const SCRIPT = [
  scene(1, '会议室', [
    '玛奇玛：电次君，你的选项有两个。',
    '电次：什么选项？',
    '玛奇玛：作为恶魔被我杀掉，还是作为人被我饲养。饲养的话，我会好好喂你吃的。',
    '（电次抬起头，看着她）',
    '电次：……我选饲养。',
  ]),
  scene(2, '公园', [
    '电次：真的可以吗，这样下去。',
    '玛奇玛：回复只需要「是」或「汪」。会说「不」的狗我不需要。',
    '（电次沉默地低下头）',
  ]),
  scene(3, '办公室', [
    '岸边：那家伙是恶魔，你不能信她。',
    '电次：可是她给了我吃的。',
    '岸边：你这条狗。',
    '（岸边把手枪放在桌上，转身离开）',
  ]),
  scene(4, '电影院', [
    '玛奇玛：人类真是可爱。会为了虚构的东西流泪。',
    '电次：你在哭吗？',
    '玛奇玛：电影结束前不要说话。',
    '（银幕的光映在她的脸上，她确实在流泪）',
  ]),
].join('\n')

const NOTES_JSON = JSON.stringify({
  scene_summaries: [{ scene: 'SEG', summary: '玛奇玛向电次提出饲养选项，电次接受。' }],
  characters_on_stage: ['玛奇玛', '电次', '岸边'],
  events: [
    { scene: 'SEG', what: '玛奇玛提出两个选项', who: ['玛奇玛', '电次'], emotion: '平静' },
    { scene: 'SEG', what: '电次选择被饲养', who: ['电次'] },
  ],
  relationship_signals: [
    { a: '玛奇玛', b: '电次', signal: '支配与投喂', scene: 'SEG' },
  ],
  mentions: [
    { about: '玛奇玛', by: '岸边', content: '那家伙是恶魔', scene: 'SEG' },
  ],
  dialogues: [],
})

// 生成文件 mock：一条可回查命中的引文 + 一条必失败的引文（验证降级路径）
const GEN_MD =
  '## 核心驱动力\n\n她以驯养代替杀戮。「电次君，你的选项有两个。」（第1场 会议室），语调平稳。\n\n## 佐证\n\n她也说过「这条引文在原文中不存在」（第2场 公园），用于测试回查降级。\n'

const reply = (content) =>
  JSON.stringify({
    choices: [{ message: { role: 'assistant', content } }],
  })

// ---- mock LLM（OpenAI 兼容 + Anthropic 两种协议） ----
/** 按请求体里的 system 提示词返回对应 mock 内容 */
function mockContent(body) {
  let system = ''
  try {
    const p = JSON.parse(body)
    // OpenAI：messages[0] 为 system；Anthropic：system 是顶层字段
    system = p.system ?? p.messages?.[0]?.content ?? ''
  } catch {}
  if (system.includes('结构研究员')) return NOTES_JSON
  if (system.includes('修复')) {
    // 引文修复打回：去掉坏引文
    return '## 核心驱动力\n\n她以驯养代替杀戮。「电次君，你的选项有两个。」（第1场 会议室）。\n'
  }
  return GEN_MD
}

let llmCallCount = 0
const llm = http.createServer((req, res) => {
  let body = ''
  req.on('data', (c) => (body += c))
  req.on('end', () => {
    llmCallCount++
    // 前两次调用返回垃圾 body（流式 + 兜底双失败，触发段内真重试）：验证重试日志追加
    if (llmCallCount <= 2 && req.method === 'POST') {
      res.writeHead(200, { 'content-type': 'application/json' })
      res.end('<html>网关错误页（非 JSON，模拟截断响应）</html>')
      return
    }
    const content = mockContent(body)
    let payload = {}
    try {
      payload = JSON.parse(body)
    } catch {}
    const isAnthropic = req.url?.includes('/v1/messages')
    // 流式（引擎全走 stream:true）：按协议发 SSE 分块，60ms/块模拟流式
    if (payload.stream) {
      res.writeHead(200, { 'content-type': 'text/event-stream' })
      const third = Math.ceil(content.length / 3)
      const chunks = [content.slice(0, third), content.slice(third, third * 2), content.slice(third * 2)]
      let i = 0
      const tick = () => {
        if (i < chunks.length) {
          if (isAnthropic) {
            if (i === 0) {
              res.write(`data: ${JSON.stringify({ type: 'message_start', message: { usage: { input_tokens: 1200, cache_read_input_tokens: 400, cache_creation_input_tokens: 800 } } })}\n\n`)
            }
            res.write(`data: ${JSON.stringify({ type: 'content_block_delta', delta: { type: 'text_delta', text: chunks[i] } })}\n\n`)
            if (i === chunks.length - 1) {
              res.write(`data: ${JSON.stringify({ type: 'message_delta', usage: { output_tokens: 320 } })}\n\n`)
            }
          } else {
            if (i === 0) {
              res.write(`data: ${JSON.stringify({ choices: [{ delta: { reasoning_content: '先分析场景与角色…' } }] })}\n\n`)
            }
            res.write(`data: ${JSON.stringify({ choices: [{ delta: { content: chunks[i] } }] })}\n\n`)
            if (i === chunks.length - 1) {
              res.write(`data: ${JSON.stringify({ choices: [], usage: { prompt_tokens: 1200, completion_tokens: 320, prompt_cache_hit_tokens: 400, prompt_cache_miss_tokens: 800 } })}\n\n`)
            }
          }
          i++
          setTimeout(tick, 200)
        } else {
          res.write(isAnthropic ? `data: ${JSON.stringify({ type: 'message_stop' })}\n\n` : 'data: [DONE]\n\n')
          res.end()
        }
      }
      tick()
      return
    }
    // 非流式兜底（理论上不走）
    if (isAnthropic) {
      res.end(JSON.stringify({ content: [{ type: 'text', text: content }] }))
      return
    }
    res.end(reply(content))
  })
})
await new Promise((r) => llm.listen(9911, r))

// ---- 临时 daemon（独立 HOME，避免污染真实配置与人物库） ----
const home = mkdtempSync(join(tmpdir(), 'shiro-smoke-'))
mkdirSync(join(home, '.config', 'shiro'), { recursive: true })
writeFileSync(
  join(home, '.config', 'shiro', 'config.toml'),
  `[llm]\nbase_url = "http://127.0.0.1:9911/v1"\napi_key = "test"\nmodel = "mock"\ntimeout_secs = 10\n`,
)

let daemon = null
function startDaemon() {
  daemon = spawn(DAEMON, ['--port', '27599'], {
    env: { ...process.env, HOME: home, SHIRO_TOKEN: 'smoke-token' },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  daemon.stdout.on('data', () => {})
  daemon.stderr.on('data', (d) => process.stderr.write(`[daemon] ${d}`))
}
async function waitReady() {
  for (let i = 0; i < 50; i++) {
    try {
      await api('/api/v1/app/startup')
      return
    } catch {
      await new Promise((r) => setTimeout(r, 100))
    }
  }
}
startDaemon()

const api = async (path, init) => {
  const res = await fetch(`http://127.0.0.1:27599${path}`, {
    ...init,
    headers: { Authorization: 'Bearer smoke-token', ...init?.headers },
  })
  const text = await res.text()
  if (!res.ok) throw new Error(`${init?.method ?? 'GET'} ${path} → ${res.status}: ${text}`)
  return text ? JSON.parse(text) : undefined
}

// 等待就绪
await waitReady()

const failures = []
const check = (cond, msg) => {
  console.log(`${cond ? '✓' : '✗'} ${msg}`)
  if (!cond) failures.push(msg)
}

try {
  // 1. 创建任务
  const created = await api('/api/v1/db/deconstruct/tasks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      source_name: '冒烟剧本',
      content: SCRIPT,
      character: '玛奇玛',
      aliases: ['マキマ'],
    }),
  })
  check(!!created.id, `任务创建 id=${created.id}`)

  // 2. 轮询至选择闸门（切块完成，等用户挑选）
  let detail = null
  for (let i = 0; i < 120; i++) {
    detail = await api(`/api/v1/db/deconstruct/tasks/${created.id}`)
    if (['selecting', 'done', 'failed'].includes(detail.progress.stage)) break
    await new Promise((r) => setTimeout(r, 250))
  }
  check(detail.progress.stage === 'selecting', `进入选择闸门（stage=${detail.progress.stage}${detail.progress.error ? ' err=' + detail.progress.error : ''}）`)
  check(detail.progress.segment_count >= 1, `切块 ${detail.progress.segment_count} 段`)
  check(detail.segments.length === detail.progress.segment_count, `段清单 ${detail.segments.length} 段（含预览）`)
  check(
    detail.segments.every((s) => s.label && s.chars > 0),
    '段清单含 label 与字数',
  )
  check(
    detail.segments[0]?.has_name === true,
    '名称命中标记（has_name）',
  )
  const src = await api(`/api/v1/db/deconstruct/tasks/${created.id}/source`)
  check(
    src.content === SCRIPT && src.character === '玛奇玛' && src.aliases.includes('マキマ'),
    '源文件读取（复制为新制作用）',
  )

  // 2.5 提交选择（全选 = 全收）
  const all = detail.segments.map((s) => s.index)
  await api(`/api/v1/db/deconstruct/tasks/${created.id}/select`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ selected: all }),
  })
  for (let i = 0; i < 120; i++) {
    detail = await api(`/api/v1/db/deconstruct/tasks/${created.id}`)
    if (['done', 'failed'].includes(detail.progress.stage)) break
    await new Promise((r) => setTimeout(r, 250))
  }
  check(detail.progress.stage === 'done', `任务完成（stage=${detail.progress.stage}${detail.progress.error ? ' err=' + detail.progress.error : ''}）`)
  check(
    detail.progress.notes_done === all.length,
    `笔记 ${detail.progress.notes_done}/${all.length}`,
  )
  check(detail.files.length === 7, `产物 7 个文件（实际 ${detail.files.length}）`)

  // 2.8 实时回复写入 pending 日志：运行中某时刻应有「进行中 + 已有部分响应」的条目
  // （前面提交选择后任务已完成过快，故单独建一个任务边跑边抓）
  const tLive = await api('/api/v1/db/deconstruct/tasks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ source_name: '实时回复验证', content: SCRIPT, character: '玛奇玛', aliases: [] }),
  })
  let liveCaught = false
  for (let i = 0; i < 100 && !liveCaught; i++) {
    const dLive = await api(`/api/v1/db/deconstruct/tasks/${tLive.id}`)
    if (dLive.progress.stage === 'selecting') {
      await api(`/api/v1/db/deconstruct/tasks/${tLive.id}/select`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ selected: dLive.segments.map((x) => x.index) }),
      })
      continue
    }
    const logsLive = await api(`/api/v1/db/deconstruct/tasks/${tLive.id}/logs`)
    liveCaught = logsLive.logs.some((l) => l.ok === null && l.response_chars > 0)
    if (['done', 'failed'].includes(dLive.progress.stage)) break
    await new Promise((r) => setTimeout(r, 100))
  }
  check(liveCaught, '运行中 pending 日志携带增长中的响应（实时回复）')
  // 等该任务完成后删除
  for (let i = 0; i < 120; i++) {
    const dLive = await api(`/api/v1/db/deconstruct/tasks/${tLive.id}`)
    if (['done', 'failed'].includes(dLive.progress.stage)) break
    await new Promise((r) => setTimeout(r, 250))
  }
  await api(`/api/v1/db/deconstruct/tasks/${tLive.id}`, { method: 'DELETE' })

  // 3. 回查：好引文通过 + 坏引文被移除（修复打回后 mock 会产出干净版本，removed 可能为 0）
  const verified = detail.progress.verified
  check(!!verified, `回查报告存在（passed=${verified?.passed} removed=${verified?.removed?.length}）`)

  // 3.5 调用日志：每次 AI 调用有持久化记录（pending → 结果回写）
  const logs = await api(`/api/v1/db/deconstruct/tasks/${created.id}/logs`)
  check(logs.logs.length >= 8, `调用日志条数（${logs.logs.length}：笔记+生成+回查修复）`)
  // 重试不吞错误：垃圾响应（空内容→非流式兜底）的错误链路在日志中可辨识
  const noteFails = logs.logs.filter((l) => l.node === 'notes' && l.ok === false)
  const noteOkCount = logs.logs.filter((l) => l.node === 'notes' && l.ok === true).length
  const noteEntries = logs.logs.filter((l) => l.node === 'notes').length
  check(
    noteEntries === noteOkCount + noteFails.length && noteOkCount >= 1,
    `垃圾响应路径完整记录（成功 ${noteOkCount} / 失败 ${noteFails.length}）`,
  )

  const noteLog = logs.logs.find((l) => l.node === 'notes' && l.ok === true)
  check(!!noteLog && noteLog.request_chars > 0, '笔记日志摘要完整')
  // 重试追加：前两次垃圾（流式+兜底双失败）后段内重试成功——同一条日志内含失败历史
  check(
    (noteLog.attempt_count ?? 1) > 1,
    `重试追加到同一条日志（${noteLog.attempt_count} 次尝试）`,
  )
  const noteDetail = await api(
    `/api/v1/db/deconstruct/tasks/${created.id}/logs/${noteLog.seq}`,
  )
  const attempts = noteDetail.attempts ?? []
  check(
    attempts.some((a) => a.ok === false) && attempts.some((a) => a.ok === true),
    `多尝试含失败历史与成功（${attempts.map((a) => a.ok).join(',')}）`,
  )
  // token 用量与思考：流式正常路径（生成类日志）
  const genLog = logs.logs.find((l) => l.node === 'generating' && l.ok === true)
  check(
    genLog && genLog.input_tokens > 0 && genLog.output_tokens > 0 && genLog.cache_read > 0,
    `token 用量与缓存（↑${genLog?.input_tokens} ↓${genLog?.output_tokens} 读缓存 ${genLog?.cache_read}）`,
  )
  check(genLog?.cache_hit_rate > 0, `缓存命中率（${genLog?.cache_hit_rate}%）`)
  check(genLog?.has_thinking === true, '思考过程已捕获')
  const logDetail = await api(
    `/api/v1/db/deconstruct/tasks/${created.id}/logs/${noteLog.seq}`,
  )
  check(
    !!logDetail.request?.user && (logDetail.attempts ?? []).some((a) => !!a.response) && logDetail.ok === true,
    '日志详情含请求与响应全文',
  )

  // 4. 保存入人物库
  const saved = await api(`/api/v1/db/deconstruct/tasks/${created.id}/save`, { method: 'POST' })
  check(!!saved.character_id, `保存到人物库 id=${saved.character_id}`)

  // 5. 人物库列表与详情（深卡）
  const list = await api('/api/v1/db/characters')
  const entry = list.characters.find((c) => c.id === saved.character_id)
  check(!!entry, '人物库列表包含新角色')
  check(entry?.depth === 'full', `深卡标记 depth=${entry?.depth}`)
  const char = await api(`/api/v1/db/characters/${saved.character_id}`)
  check(char.files.length === 6, `深卡详情含 6 个子文件（实际 ${char.files.length}）`)
  check(char.body.length > 0, 'index 正文非空')

  // 6. 断点续跑：模拟崩溃（删 soul 与回查报告产物，任务标失败）→ 重试应跳过已有阶段补齐缺失
  const taskDir = join(home, '.config', 'shiro', 'db', 'deconstruct', created.id)
  rmSync(join(taskDir, 'card', 'soul.md'))
  rmSync(join(taskDir, 'report.json'))
  const rec = JSON.parse(readFileSync(join(taskDir, 'task.json'), 'utf8'))
  rec.progress.stage = 'failed'
  writeFileSync(join(taskDir, 'task.json'), JSON.stringify(rec))
  await api(`/api/v1/db/deconstruct/tasks/${created.id}/retry`, { method: 'POST' })
  let dRes = null
  for (let i = 0; i < 120; i++) {
    dRes = await api(`/api/v1/db/deconstruct/tasks/${created.id}`)
    if (['done', 'failed'].includes(dRes.progress.stage)) break
    await new Promise((r) => setTimeout(r, 250))
  }
  check(dRes.progress.stage === 'done', `断点续跑完成（${dRes.progress.stage}）`)
  check(dRes.files.length === 7, `续跑后产物补齐（${dRes.files.length} 个）`)
  check(!!dRes.progress.verified, '回查重新执行')

  // 7. 已完成任务拒绝重试
  let rejected = false
  try {
    await api(`/api/v1/db/deconstruct/tasks/${created.id}/retry`, { method: 'POST' })
  } catch (e) {
    rejected = String(e.message).includes('400')
  }
  check(rejected, '已完成任务拒绝重试')

  // 9. 中止：运行中中止 → aborted 终态 → 断点重试 → done
  const t3 = await api('/api/v1/db/deconstruct/tasks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ source_name: '中止验证', content: SCRIPT, character: '电次', aliases: [] }),
  })
  let d3 = null
  for (let i = 0; i < 120; i++) {
    d3 = await api(`/api/v1/db/deconstruct/tasks/${t3.id}`)
    if (d3.progress.stage === 'selecting') break
    await new Promise((r) => setTimeout(r, 100))
  }
  await api(`/api/v1/db/deconstruct/tasks/${t3.id}/select`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ selected: d3.segments.map((x) => x.index) }),
  })
  // 等进入运行态再中止
  let aborted = null
  for (let i = 0; i < 120; i++) {
    d3 = await api(`/api/v1/db/deconstruct/tasks/${t3.id}`)
    if (['notes', 'generating', 'verifying'].includes(d3.progress.stage)) {
      aborted = await api(`/api/v1/db/deconstruct/tasks/${t3.id}/abort`, { method: 'POST' })
      break
    }
    if (['done', 'failed'].includes(d3.progress.stage)) break
    await new Promise((r) => setTimeout(r, 50))
  }
  check(aborted?.stage === 'aborted', `中止生效（${aborted?.stage}）`)
  const d3ab = await api(`/api/v1/db/deconstruct/tasks/${t3.id}`)
  check(
    !!d3ab.progress.last_stage,
    `记录中止阶段（${d3ab.progress.last_stage}）`,
  )
  // 中止时在飞的 pending 日志被标记为已放弃（不再是「…」悬挂）
  const logsAb = await api(`/api/v1/db/deconstruct/tasks/${t3.id}/logs`)
  const pendingAb = logsAb.logs.filter((l) => l.ok === null && !l.abandoned)
  const abandonedAb = logsAb.logs.filter((l) => l.abandoned)
  check(pendingAb.length === 0, `无悬挂 pending 日志（剩 ${pendingAb.length}）`)
  check(abandonedAb.length >= 1, `在飞日志标记放弃（${abandonedAb.length} 条）`)
  await api(`/api/v1/db/deconstruct/tasks/${t3.id}/retry`, { method: 'POST' })
  for (let i = 0; i < 120; i++) {
    d3 = await api(`/api/v1/db/deconstruct/tasks/${t3.id}`)
    if (['done', 'failed'].includes(d3.progress.stage)) break
    await new Promise((r) => setTimeout(r, 250))
  }
  check(d3.progress.stage === 'done', `中止后断点重试完成（${d3.progress.stage}）`)

  // 9.3 并行设置：读写 + 多段任务并发笔记
  const put = await api('/api/v1/model/settings', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ max_concurrency: 3 }),
  })
  check(put.max_concurrency === 3, `并行数写入（${put.max_concurrency}）`)
  const got = await api('/api/v1/model/settings')
  check(got.max_concurrency === 3, `并行数读回（${got.max_concurrency}）`)

  // 多段素材（无场次/章节标题 → plain 滑窗路径，约 3 段）
  const para = '电次：这段台词用于凑长度。\n玛奇玛：嗯。\n（两人沉默地走着）\n' + '这是一个没有标题的段落，用于测试滑窗切块。'.repeat(40) + '\n\n'
  const PLAIN = para.repeat(55)
  const t5 = await api('/api/v1/db/deconstruct/tasks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ source_name: '并行验证', content: PLAIN, character: '玛奇玛', aliases: [] }),
  })
  let d5 = null
  for (let i = 0; i < 120; i++) {
    d5 = await api(`/api/v1/db/deconstruct/tasks/${t5.id}`)
    if (d5.progress.stage === 'selecting') break
    await new Promise((r) => setTimeout(r, 100))
  }
  check(d5.progress.segment_count >= 2, `多段切块（${d5.progress.segment_count} 段）`)
  await api(`/api/v1/db/deconstruct/tasks/${t5.id}/select`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ selected: d5.segments.map((x) => x.index) }),
  })
  for (let i = 0; i < 120; i++) {
    d5 = await api(`/api/v1/db/deconstruct/tasks/${t5.id}`)
    if (['done', 'failed'].includes(d5.progress.stage)) break
    await new Promise((r) => setTimeout(r, 250))
  }
  check(d5.progress.stage === 'done', `多段并行任务完成（${d5.progress.stage}${d5.progress.error ? ' ' + d5.progress.error : ''}）`)
  const logs5 = await api(`/api/v1/db/deconstruct/tasks/${t5.id}/logs`)
  const noteLogs5 = logs5.logs.filter((l) => l.node === 'notes')
  check(
    noteLogs5.length === d5.progress.segment_count && noteLogs5.every((l) => l.ok === true),
    `每段一条笔记日志且全部成功（${noteLogs5.length}/${d5.progress.segment_count}）`,
  )
  await api(`/api/v1/db/deconstruct/tasks/${t5.id}`, { method: 'DELETE' })

  // 9.5 重启不自动续跑：杀掉 daemon（模拟退出应用）→ 重启 → 任务标「已中断」且未自己跑 → 点继续才完成
  const t4 = await api('/api/v1/db/deconstruct/tasks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ source_name: '重启验证', content: SCRIPT, character: '电次', aliases: [] }),
  })
  let d4 = null
  for (let i = 0; i < 120; i++) {
    d4 = await api(`/api/v1/db/deconstruct/tasks/${t4.id}`)
    if (d4.progress.stage === 'selecting') break
    await new Promise((r) => setTimeout(r, 100))
  }
  await api(`/api/v1/db/deconstruct/tasks/${t4.id}/select`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ selected: d4.segments.map((x) => x.index) }),
  })
  // 等进入运行态
  for (let i = 0; i < 120; i++) {
    d4 = await api(`/api/v1/db/deconstruct/tasks/${t4.id}`)
    if (['notes', 'generating', 'verifying'].includes(d4.progress.stage)) break
    await new Promise((r) => setTimeout(r, 50))
  }
  daemon.kill()
  await new Promise((r) => setTimeout(r, 300))
  startDaemon()
  await waitReady()
  d4 = await api(`/api/v1/db/deconstruct/tasks/${t4.id}`)
  check(d4.progress.stage === 'interrupted', `重启后任务已中断而非自动续跑（${d4.progress.stage}）`)
  await api(`/api/v1/db/deconstruct/tasks/${t4.id}/retry`, { method: 'POST' })
  for (let i = 0; i < 120; i++) {
    d4 = await api(`/api/v1/db/deconstruct/tasks/${t4.id}`)
    if (['done', 'failed'].includes(d4.progress.stage)) break
    await new Promise((r) => setTimeout(r, 250))
  }
  check(d4.progress.stage === 'done', `点继续后从断点完成（${d4.progress.stage}）`)
  await api(`/api/v1/db/deconstruct/tasks/${t4.id}`, { method: 'DELETE' })

  // 10. 改名
  await api(`/api/v1/db/deconstruct/tasks/${t3.id}/rename`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ title: '电次卡-重跑' }),
  })
  const list2 = await api('/api/v1/db/deconstruct/tasks')
  const renamed = list2.tasks.find((x) => x.id === t3.id)
  check(renamed?.title === '电次卡-重跑', `任务改名（${renamed?.title}）`)
  await api(`/api/v1/db/deconstruct/tasks/${t3.id}`, { method: 'DELETE' })

  // 11. 清理任务
  await api(`/api/v1/db/deconstruct/tasks/${created.id}`, { method: 'DELETE' })
  const tasks = await api('/api/v1/db/deconstruct/tasks')
  check(!tasks.tasks.some((t) => t.id === created.id), '任务删除')

  // 7. 多档案 + Anthropic 协议路径（Kimi Code）：注册档案 → 设为默认 → 跑任务验证双协议分流
  const reg = await api('/api/v1/model/profiles', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      key: 'kimi-code',
      base_url: 'http://127.0.0.1:9911/coding',
      model: 'k3',
      protocol: 'anthropic',
      api_key: 'sk-anthropic-test',
    }),
  })
  check(reg.profiles.length >= 2, `档案注册（迁移 1 + 新增 = ${reg.profiles.length}）`)
  const def = await api('/api/v1/model/profiles/default', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ key: 'kimi-code' }),
  })
  check(def.default_key === 'kimi-code', `默认档案设为 kimi-code（${def.default_key}）`)
  const kimi = def.profiles.find((p) => p.key === 'kimi-code')
  check(kimi?.protocol === 'anthropic' && kimi?.api_key_set && !!kimi?.api_key_preview, 'Key 掩码回显')

  // 档案连接测试：对 mock 上游发 ping
  const test = await api('/api/v1/model/profiles/kimi-code/test', { method: 'POST' })
  check(test.ok === true && test.latency_ms != null, `连接测试（${test.ok} ${test.message}）`)
  const test404 = await api('/api/v1/model/profiles/not-exist/test', { method: 'POST' }).catch(
    (e) => String(e.message),
  )
  check(String(test404).includes('400'), '不存在档案测试被拒')
  const t2 = await api('/api/v1/db/deconstruct/tasks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      source_name: 'anthropic 路径验证',
      content: SCRIPT,
      character: '玛奇玛',
      aliases: [],
    }),
  })
  // 同走选择闸门：全选提交
  let d2 = null
  for (let i = 0; i < 120; i++) {
    d2 = await api(`/api/v1/db/deconstruct/tasks/${t2.id}`)
    if (d2.progress.stage === 'selecting') break
    await new Promise((r) => setTimeout(r, 250))
  }
  await api(`/api/v1/db/deconstruct/tasks/${t2.id}/select`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ selected: d2.segments.map((s) => s.index) }),
  })
  for (let i = 0; i < 120; i++) {
    d2 = await api(`/api/v1/db/deconstruct/tasks/${t2.id}`)
    if (['done', 'failed'].includes(d2.progress.stage)) break
    await new Promise((r) => setTimeout(r, 250))
  }
  check(d2.progress.stage === 'done', `anthropic 协议任务完成（${d2.progress.stage}${d2.progress.error ? ' ' + d2.progress.error : ''}）`)
  await api(`/api/v1/db/deconstruct/tasks/${t2.id}`, { method: 'DELETE' })

} catch (e) {
  failures.push(`异常：${e.message}`)
  console.error(e)
} finally {
  daemon.kill()
  llm.close()
  rmSync(home, { recursive: true, force: true })
}

console.log(failures.length ? `\n✗ ${failures.length} 项失败` : '\n✓ 冒烟测试全部通过')
process.exit(failures.length ? 1 : 0)
