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

const llm = http.createServer((req, res) => {
  let body = ''
  req.on('data', (c) => (body += c))
  req.on('end', () => {
    const content = mockContent(body)
    // Anthropic Messages 协议（Kimi Code 路径）：响应为 content 块数组
    if (req.url?.includes('/v1/messages')) {
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

const daemon = spawn(DAEMON, ['--port', '27599'], {
  env: { ...process.env, HOME: home, SHIRO_TOKEN: 'smoke-token' },
  stdio: ['ignore', 'pipe', 'pipe'],
})
daemon.stdout.on('data', () => {})
daemon.stderr.on('data', (d) => process.stderr.write(`[daemon] ${d}`))

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
for (let i = 0; i < 50; i++) {
  try {
    await api('/api/v1/app/startup')
    break
  } catch {
    await new Promise((r) => setTimeout(r, 100))
  }
}

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

  // 2. 轮询至终态
  let detail = null
  for (let i = 0; i < 120; i++) {
    detail = await api(`/api/v1/db/deconstruct/tasks/${created.id}`)
    if (['done', 'failed'].includes(detail.progress.stage)) break
    await new Promise((r) => setTimeout(r, 250))
  }
  check(detail.progress.stage === 'done', `任务完成（stage=${detail.progress.stage}${detail.progress.error ? ' err=' + detail.progress.error : ''}）`)
  check(detail.progress.segment_count >= 1, `切块 ${detail.progress.segment_count} 段`)
  check(detail.progress.notes_done === detail.progress.segment_count, `笔记 ${detail.progress.notes_done}/${detail.progress.segment_count}`)
  check(detail.files.length === 7, `产物 7 个文件（实际 ${detail.files.length}）`)

  // 3. 回查：好引文通过 + 坏引文被移除（修复打回后 mock 会产出干净版本，removed 可能为 0）
  const verified = detail.progress.verified
  check(!!verified, `回查报告存在（passed=${verified?.passed} removed=${verified?.removed?.length}）`)

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

  // 6. 清理任务
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
  let d2 = null
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
