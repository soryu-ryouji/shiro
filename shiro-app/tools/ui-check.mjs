// shiro-app UI 端到端自检：真实启动 vite + electron + shiro-daemon（临时项目库），
// 经 Chrome DevTools Protocol 断言 DOM、模拟交互，验证自动保存落盘与文件监听回载，并截图。
// 用法：node tools/ui-check.mjs（需 shiro-daemon 已构建：cargo build --manifest-path ../shiro-daemon/Cargo.toml）
// 产物在 tools/.tmp/ui-check/，成功即清理，失败保留供排查（含截图）。
// 注意：单实例锁按 userData 互斥——自检用独立会话目录（SHIRO_USER_DATA），与正在运行的实例互不干扰
import { spawn } from 'node:child_process'
import { createRequire } from 'node:module'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'
import { killStrays } from './kill-strays.mjs'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const require = createRequire(import.meta.url)
const tmp = path.join(root, 'tools', '.tmp', 'ui-check')
const projDir = path.join(tmp, 'project')
const sheetRel = '正文/测试文稿.md'
const sheetAbs = path.join(projDir, '正文', '测试文稿.md')

// ---------- 预检：清理此前运行残留的进程（electron/vite/daemon），防止占端口、锁 exe、抢 CDP ----------

const strays = killStrays(path.resolve(root, '..'))
if (strays > 0) {
  console.log(`预检清理：杀掉 ${strays} 个游离进程`)
  // 等端口与文件锁释放
  await new Promise((r) => setTimeout(r, 800))
}

// ---------- 工具 ----------

async function waitFor(fn, timeoutMs, label = '', interval = 300) {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    try {
      const result = await fn()
      if (result) return result
    } catch {
      // 页面重载期间执行上下文暂不存在，忽略后继续轮询
    }
    if (Date.now() > deadline) throw new Error(`等待超时${label ? `: ${label}` : ''}`)
    await new Promise((r) => setTimeout(r, interval))
  }
}

let pass = 0
let fail = 0
function check(name, actual, expected) {
  if (actual === expected) {
    pass++
    console.log(`ok   - ${name}`)
  } else {
    fail++
    console.log(`FAIL - ${name}: 期望 [${expected}] 实际 [${actual}]`)
  }
}

// ---------- 准备项目库 ----------

fs.rmSync(tmp, { recursive: true, force: true })
fs.mkdirSync(path.dirname(sheetAbs), { recursive: true })
fs.writeFileSync(sheetAbs, '# 自检文稿\n\n种子段落文本。\n')
fs.writeFileSync(path.join(projDir, '正文', '笔记.txt'), '纯文本笔记。\n# 不是标题\nhello bug\n')

// ---------- 构建主进程与 preload（同 scripts/dev.mjs） ----------

const common = { bundle: true, platform: 'node', external: ['electron'], logLevel: 'silent' }
await build({ ...common, entryPoints: [path.join(root, 'electron/main.ts')], format: 'esm', outfile: path.join(root, 'dist-electron/main.mjs') })
await build({ ...common, entryPoints: [path.join(root, 'electron/preload.ts')], format: 'cjs', outfile: path.join(root, 'dist-electron/preload.cjs') })

// daemon 二进制（开发态 main.ts 按 mtime 取 release/debug 最新；这里只检查存在性）
const daemonExe = process.platform === 'win32' ? 'shiro-daemon.exe' : 'shiro-daemon'
const daemonOk = ['release', 'debug'].some((d) => fs.existsSync(path.join(root, '..', 'shiro-daemon', 'target', d, daemonExe)))
if (!daemonOk) {
  console.error('未找到 shiro-daemon 构建产物，请先 cargo build（shiro-daemon/）')
  process.exit(1)
}

// 打开记录存 ~/.config/shiro/history.toml：备份后注入临时项目，任何退出路径都恢复（ hawk 同策略）
const historyFile = path.join(os.homedir(), '.config', 'shiro', 'history.toml')
const historyBackup = fs.existsSync(historyFile) ? fs.readFileSync(historyFile, 'utf8') : null
process.on('exit', () => {
  if (historyBackup !== null) fs.writeFileSync(historyFile, historyBackup)
  else fs.rmSync(historyFile, { force: true })
})

// ---------- 启动 vite + electron ----------

const viteLogPath = path.join(tmp, 'vite.log')
const viteLog = fs.openSync(viteLogPath, 'w')
const vite = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js')], { cwd: root, stdio: ['ignore', viteLog, viteLog] })
vite.on('exit', (code) => {
  if (!stopping) console.log(`[ui-check] vite 意外退出（码 ${code}），详见 ${path.relative(root, viteLogPath)}`)
})
let electron
let stopping = false

// Windows 上 node 子进程不随父进程退出：所有退出路径（含异常）都杀整棵进程树，防孤儿 vite
const killTree = (pid) => {
  try {
    if (process.platform === 'win32') spawn('taskkill', ['/pid', String(pid), '/T', '/F'], { stdio: 'ignore' })
    else process.kill(pid)
  } catch {}
}
process.on('exit', () => {
  if (vite.pid) killTree(vite.pid)
  if (electron?.pid) killTree(electron.pid)
})

let exitCode = 1
// 失败诊断要用 evaljs，声明在 try 外（catch 可访问）
let evaljs = async () => null
// 全局看门狗：任何环节挂死都不让脚本无限等待
const watchdog = setTimeout(() => {
  console.error('ui-check 全局超时，强制退出')
  process.exit(2)
}, 240_000)
try {
  await waitFor(async () => (await fetch('http://localhost:5173/').catch(() => null))?.ok, 60_000, 'vite 就绪')

  electron = spawn(require('electron'), ['.', '--remote-debugging-port=9225'], {
    cwd: root,
    stdio: 'ignore',
    env: { ...process.env, VITE_DEV_SERVER_URL: 'http://localhost:5173', SHIRO_USER_DATA: path.join(tmp, 'userdata') },
  })

  // ---------- 连接 CDP ----------

  const target = await waitFor(async () => {
    const list = await fetch('http://127.0.0.1:9225/json').catch(() => null)
    if (!list?.ok) return null
    const pages = await list.json()
    // 找不到页面多半是单实例锁或窗口未加载，给出可操作的提示
    return pages.find((t) => t.type === 'page' && t.url.includes('localhost:5173'))
  }, 60_000, 'vite/electron 启动').catch(() => {
    throw new Error('未找到自检页面（CDP）：electron 未正常启动或页面未加载')
  })

  const ws = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('CDP WebSocket 连接超时')), 10_000)
    ws.onopen = () => {
      clearTimeout(timer)
      resolve()
    }
    ws.onerror = () => {
      clearTimeout(timer)
      reject(new Error('CDP WebSocket 连接失败'))
    }
  })
  let msgId = 0
  const pending = new Map()
  // 断连（页面崩溃/electron 被杀）时拒绝所有在途请求，避免无限挂起
  ws.onclose = () => {
    for (const p of pending.values()) p({ error: { message: 'CDP 连接已断开' } })
    pending.clear()
  }
  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data)
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg)
      pending.delete(msg.id)
    }
    // 页面异常与 console.error 实时转发：自检失败时能在终端直接看到根因
    if (msg.method === 'Runtime.exceptionThrown') {
      console.log('[页面异常]', JSON.stringify(msg.params.exceptionDetails).slice(0, 400))
    }
    if (msg.method === 'Runtime.consoleAPICalled' && msg.params.type === 'error') {
      console.log('[页面 console.error]', msg.params.args.map((a) => a.value ?? a.description ?? '').join(' ').slice(0, 300))
    }
  }
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++msgId
      pending.set(id, (msg) => (msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result)))
      ws.send(JSON.stringify({ id, method, params }))
    })
  evaljs = async (expression) => {
    const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    // 页面内异常不显式转抛的话，失败会被推迟成后续 waitFor 的「等待超时」，排障方向全错
    if (result.exceptionDetails) {
      const d = result.exceptionDetails
      throw new Error(`页面脚本异常: ${d.exception?.description || d.exception?.value || d.text || 'unknown'}`)
    }
    return result.result?.value
  }

  await send('Page.enable')
  await send('Runtime.enable')

  // 首载可能撞上 vite 冷启动优化（chrome-error 页）：等连接参数，失败则重载重试
  let conn = null
  for (let attempt = 0; attempt < 3 && !conn; attempt++) {
    conn = await waitFor(
      () => evaljs(`(() => { const p = new URLSearchParams(location.hash.slice(1)); return p.get('api') ? { api: p.get('api'), token: p.get('token') } : null })()`),
      10_000,
      'hash 连接参数',
    ).catch(() => null)
    if (!conn) await send('Page.reload').catch(() => {})
  }
  if (!conn) {
    const href = await evaljs('location.href').catch(() => '(不可评估)')
    throw new Error(`页面 hash 未携带 daemon 连接参数（重载重试后仍失败，当前 href: ${href}）`)
  }

  // ---------- 注册临时项目（经 daemon API；连接参数从页面 hash 读） ----------

  const created = await fetch(`${conn.api}/api/v1/projects/create`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${conn.token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({ path: projDir, name: '自检项目' }),
  })
  if (!created.ok) throw new Error(`注册临时项目失败: HTTP ${created.status}`)

  // 项目列表在挂载时拉取，重载页面让新项目出现
  await send('Page.reload')
  await waitFor(async () => evaljs(`document.readyState === 'complete' && !!document.querySelector('.project-list')`), 30_000, '项目列表')

  // ---------- 断言 ----------

  // 项目列表 → 进入项目 → 目录树与文稿列表
  check('项目列表显示自检项目', await evaljs(`[...document.querySelectorAll('.project-item')].some((el) => el.textContent.includes('自检项目'))`), true)
  await evaljs(`[...document.querySelectorAll('.project-item')].find((el) => el.textContent.includes('自检项目')).click()`)
  check('目录树渲染「正文」目录', await waitFor(async () => evaljs(`document.querySelector('.tree')?.textContent?.includes('正文') ?? false`), 15_000, '目录树'), true)
  check('文稿列表显示测试文稿', await waitFor(async () => evaljs(`[...document.querySelectorAll('.sheets .sheet')].some((el) => el.textContent.includes('测试文稿'))`), 15_000, '文稿列表'), true)

  // 打开文稿 → 编辑器加载内容、字数状态
  await evaljs(`[...document.querySelectorAll('.sheets .sheet')].find((el) => el.textContent.includes('测试文稿')).click()`)
  check('编辑器加载文稿内容', await waitFor(async () => evaljs(`document.querySelector('.cm-content')?.textContent?.includes('种子段落文本') ?? false`), 15_000, '编辑器加载'), true)
  check('字数状态显示', await evaljs(`document.querySelector('.editor-status')?.textContent?.includes('字') ?? false`), true)

  // 工具栏插入表格 → 挂件渲染、首格激活且焦点入格
  await evaljs(`[...document.querySelectorAll('.bar-item')].find((b) => b.title.startsWith('表格')).click()`)
  check('表格挂件渲染', await waitFor(async () => evaljs(`!!document.querySelector('.md-table')`), 10_000, '表格挂件'), true)
  check('首格激活且焦点入格', await evaljs(`document.activeElement?.classList?.contains('md-cell-active') ?? false`), true)

  // 单元格输入（真实输入链路）→ 防抖自动保存 → 磁盘文件回读校验
  await send('Input.insertText', { text: '标题列' })
  await waitFor(async () => {
    await new Promise((r) => setTimeout(r, 400))
    return fs.readFileSync(sheetAbs, 'utf8').includes('标题列') ? true : null
  }, 15_000)
  check('自动保存落盘（单元格输入）', fs.readFileSync(sheetAbs, 'utf8').includes('标题列'), true)
  check('落盘内容为表格语法', fs.readFileSync(sheetAbs, 'utf8').includes('| ----'), true)

  // 外部修改（其他编辑器/AI 写稿）→ daemon 监听推送 → 编辑器静默重载（内容加长到可滚动，供滚动条断言）
  fs.writeFileSync(sheetAbs, '# 外部标题\n\n' + Array.from({ length: 40 }, (_, i) => `外部写入段落 ${i + 1}。`).join('\n\n') + '\n')
  check('文件监听回载', await waitFor(async () => evaljs(`document.querySelector('.cm-content')?.textContent?.includes('外部写入段落') ?? false`), 15_000, '监听回载'), true)

  // 大纲按钮：三态循环（自动 → 开启 → 关闭）
  const outlineTitle0 = await evaljs(`[...document.querySelectorAll('.editor-head .bar-btn')].find((b) => b.title.startsWith('大纲'))?.title ?? ''`)
  check('大纲按钮存在', outlineTitle0.includes('大纲'), true)
  await evaljs(`[...document.querySelectorAll('.editor-head .bar-btn')].find((b) => b.title.startsWith('大纲')).click()`)
  const outlineTitle1 = await evaljs(`[...document.querySelectorAll('.editor-head .bar-btn')].find((b) => b.title.startsWith('大纲'))?.title ?? ''`)
  check('大纲按钮循环切换', outlineTitle0 !== outlineTitle1, true)
  // 大纲参与布局：面板出现且与正文不重叠（正文被往左挤）
  check(
    '大纲挤压正文不遮挡',
    await waitFor(
      () =>
        evaljs(`(() => {
          const nav = document.querySelector('.outline')
          const content = document.querySelector('.cm-content')
          if (!nav || !content) return null
          const nr = nav.getBoundingClientRect()
          const cr = content.getBoundingClientRect()
          return nr.width > 0 && cr.right <= nr.left + 1
        })()`),
      10_000,
      '大纲布局',
    ),
    true,
  )
  // 编辑器滚动条钉在主区域最右缘（大纲面板右侧）
  await evaljs(`(() => { const s = document.querySelector('.cm-scroller'); s.scrollTop = 200; s.dispatchEvent(new Event('scroll')) })()`)
  check(
    '编辑器滚动条在大纲右侧',
    await evaljs(`(() => {
      const main = document.querySelector('.editor-main')?.getBoundingClientRect()
      const thumb = document.querySelector('.editor-main > .osb-v')?.getBoundingClientRect()
      if (!main || !thumb) return null
      return Math.abs(main.right - thumb.right) <= 4
    })()`),
    true,
  )

  // 纯文本（.txt）：列表显示、纯文本编辑模式（无 markdown 渲染、无标记按钮）
  check('文稿列表显示 txt 文稿', await evaljs(`[...document.querySelectorAll('.sheets .sheet')].some((el) => el.textContent.includes('笔记'))`), true)
  await evaljs(`[...document.querySelectorAll('.sheets .sheet')].find((el) => el.textContent.includes('笔记')).click()`)
  check('txt 文稿加载', await waitFor(async () => evaljs(`document.querySelector('.cm-content')?.textContent?.includes('纯文本笔记') ?? false`), 15_000, 'txt 加载'), true)
  check('txt 不做 markdown 渲染（# 不成标题）', await evaljs(`document.querySelector('.cm-content .md-h') === null`), true)
  check('txt 隐藏标记工具栏按钮', await evaljs(`document.querySelectorAll('.editor-bar .bar-item').length === 0`), true)

  // 设置面板：分页共用滚动容器——外观页滚动后切到通用页，通用页应从顶部开始
  await evaljs(`[...document.querySelectorAll('.editor-head .bar-btn')].find((b) => b.title === '设置').click()`)
  await waitFor(async () => evaljs(`!!document.querySelector('.dialog-body')`), 10_000, '设置面板打开')
  await evaljs(`(() => { const b = document.querySelector('.dialog-body'); b.scrollTop = 200; b.dispatchEvent(new Event('scroll')) })()`)
  await evaljs(`[...document.querySelectorAll('.dialog-nav-item')].find((b) => b.textContent.includes('通用')).click()`)
  check('设置切换分页后从顶部开始', await evaljs(`document.querySelector('.dialog-body').scrollTop`), 0)
  // 通用页无溢出：滚动条应立即消失（不残留上一页的滑块）
  check(
    '设置切换分页后滚动条不残留',
    await waitFor(async () => evaljs(`!document.querySelector('.dialog-body .osb-v')?.classList.contains('osb-on') ?? false`), 5_000, '滚动条残留'),
    true,
  )
  // 字数统计策略：通用页切换到「中文字符与标点」→ 状态栏字数即时重算（当前 txt 文稿恰有 1 个句号）
  const countBefore = await evaljs(`Number(document.querySelector('.editor-status').textContent.match(/(\\d+)\\s*字/)?.[1] ?? 0)`)
  await evaljs(`(() => { const s = [...document.querySelectorAll('.grow')].find((r) => r.textContent.includes('字数统计'))?.querySelector('select'); if (s) { s.value = 'cjkPunct'; s.dispatchEvent(new Event('change')) } })()`)
  const countAfter = await waitFor(async () => {
    const n = await evaljs(`Number(document.querySelector('.editor-status').textContent.match(/(\\d+)\\s*字/)?.[1] ?? 0)`)
    return n === countBefore + 1 ? n : null
  }, 5_000, '字数重算')
  check('字数统计切换为含标点（+1）', countAfter - countBefore, 1)
  // 再切换到「中文字符、标点与英文单词」（当前文稿有 2 个英文单词：hello、bug）
  await evaljs(`(() => { const s = [...document.querySelectorAll('.grow')].find((r) => r.textContent.includes('字数统计'))?.querySelector('select'); if (s) { s.value = 'cjkPunctEn'; s.dispatchEvent(new Event('change')) } })()`)
  const countEn = await waitFor(async () => {
    const n = await evaljs(`Number(document.querySelector('.editor-status').textContent.match(/(\\d+)\\s*字/)?.[1] ?? 0)`)
    return n === countAfter + 2 ? n : null
  }, 5_000, '字数含英文')
  check('字数统计含英文单词（+2）', countEn - countAfter, 2)
  // 预览首行缩进：外观页打开开关 → 预览阅读视图里段落首行缩进生效
  await evaljs(`[...document.querySelectorAll('.dialog-nav-item')].find((b) => b.textContent.includes('外观')).click()`)
  await evaljs(`(() => { const c = [...document.querySelectorAll('.grow')].find((r) => r.textContent.includes('首行缩进'))?.querySelector('input[type="checkbox"]'); if (c && !c.checked) c.click() })()`)
  check('首行缩进开关写入', await evaljs(`localStorage.getItem('shiro.preview.firstLineIndent')`), '1')
  // 关闭设置（Esc）
  await evaljs(`window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))`)
  await waitFor(async () => evaljs(`!document.querySelector('.dialog-body')`), 10_000, '设置面板关闭')

  // 预览（markdown 文稿）：打开手机预览，段落首行缩进应为非 0
  await evaljs(`[...document.querySelectorAll('.sheets .sheet')].find((el) => el.textContent.includes('测试文稿')).click()`)
  await waitFor(async () => evaljs(`document.querySelector('.cm-content')?.textContent?.includes('外部写入段落') ?? false`), 15_000, '切回 md 文稿')
  await evaljs(`[...document.querySelectorAll('.editor-head .bar-btn')].find((b) => b.title === '手机预览').click()`)
  await waitFor(async () => evaljs(`!!document.querySelector('.reading p')`), 10_000, '预览打开')
  check('预览首行缩进生效', await evaljs(`getComputedStyle(document.querySelector('.reading p')).textIndent !== '0px'`), true)
  await evaljs(`document.querySelector('.phone-close').click()`)

  const { data } = await send('Page.captureScreenshot', { format: 'png' })
  fs.writeFileSync(path.join(tmp, 'ui-check.png'), Buffer.from(data, 'base64'))

  exitCode = fail ? 1 : 0
  console.log(`\nui-check: ${pass} passed, ${fail} failed`)
  clearTimeout(watchdog)
} catch (e) {
  console.error('ui-check 异常:', e.message ?? e)
  // 失败诊断：页面地址/挂载状态/正文长度，帮助区分「app 没挂载」与「业务断言失败」
  try {
    const diag = await evaljs(`JSON.stringify({ href: location.href, ready: document.readyState, appChildren: document.querySelector('#app')?.childElementCount, bodyLen: document.body?.innerHTML?.length })`)
    console.error('页面诊断:', diag)
  } catch {
    console.error('页面诊断: 不可评估')
  }
} finally {
  stopping = true
  electron?.kill()
  vite.kill()
  fs.closeSync(viteLog)
  // Windows 文件锁释放滞后：等进程树退出再清理；rmSync 重试覆盖残留占用
  if (electron && !electron.killed) {
    await Promise.race([new Promise((r) => electron.once('exit', r)), new Promise((r) => setTimeout(r, 3000))])
  }
  await new Promise((r) => setTimeout(r, 500))
  // 成功即清理，失败保留 tools/.tmp/ui-check/（含截图与临时项目库）供排查
  if (exitCode === 0) {
    fs.rmSync(tmp, { recursive: true, force: true, maxRetries: 8, retryDelay: 400 })
    // 父目录 .tmp 空了一并收掉（非空则保留，不报错）
    try {
      fs.rmdirSync(path.dirname(tmp))
    } catch {}
  }
}
process.exit(exitCode)
