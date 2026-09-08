// 制作视图 UI 实测：真实 vite + electron + CDP 驱动。
// 场景：带别名的任务 → 详情页「复制为新制作」→ 新建表单别名 chips 必须带回来。
// 全程临时 HOME（daemon 数据隔离），不动真实配置与人物库。
// 用法：node tools/ui-check-craft.mjs（需 shiro-daemon 已构建）
import { spawn } from 'node:child_process'
import { createRequire } from 'node:module'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'
import { killStrays } from './kill-strays.mjs'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const require = createRequire(import.meta.url)
const tmpHome = fs.mkdtempSync(path.join(require('os').tmpdir(), 'shiro-craft-ui-'))
const tmp = path.join(root, 'tools', '.tmp', 'ui-check-craft')

// ---------- 预检：清理游离进程 ----------
killStrays(path.resolve(root, '..'))

const failures = []
const check = (cond, msg) => {
  console.log(`${cond ? '✓' : '✗'} ${msg}`)
  if (!cond) failures.push(msg)
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

// ---------- 构建主进程与 preload ----------
const common = { bundle: true, platform: 'node', external: ['electron'], logLevel: 'silent' }
await build({ ...common, entryPoints: [path.join(root, 'electron/main.ts')], format: 'esm', outfile: path.join(root, 'dist-electron/main.mjs') })
await build({ ...common, entryPoints: [path.join(root, 'electron/preload.ts')], format: 'cjs', outfile: path.join(root, 'dist-electron/preload.cjs') })

const daemonExe = process.platform === 'win32' ? 'shiro-daemon.exe' : 'shiro-daemon'
if (!fs.existsSync(path.join(root, '..', 'shiro-daemon', 'target', 'debug', daemonExe))) {
  console.error('缺少 daemon 二进制，先 cargo build')
  process.exit(1)
}

// ---------- 启动 vite + electron（HOME 指向临时目录，daemon 数据隔离） ----------
const vite = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js')], { cwd: root, stdio: 'ignore' })
let electron = null
const killTree = (pid) => { try { process.kill(pid) } catch {} }
process.on('exit', () => {
  if (vite.pid) killTree(vite.pid)
  if (electron?.pid) killTree(electron.pid)
  fs.rmSync(tmpHome, { recursive: true, force: true })
})

const watchdog = setTimeout(() => {
  console.error('全局超时，强制退出')
  process.exit(2)
}, 120_000)

let evaljs = async () => null
try {
  // 等 vite 就绪
  for (let i = 0; i < 200; i++) {
    const ok = await fetch('http://localhost:5173/').then((r) => r.ok).catch(() => false)
    if (ok) break
    await sleep(200)
  }

  electron = spawn(require('electron'), ['.', '--remote-debugging-port=9226'], {
    cwd: root,
    stdio: 'ignore',
    env: {
      ...process.env,
      VITE_DEV_SERVER_URL: 'http://localhost:5173',
      SHIRO_USER_DATA: path.join(tmp, 'userdata'),
      HOME: tmpHome, // daemon 经 std::env::home_dir() 读 $HOME → 数据全在临时目录
    },
  })

  // ---------- CDP ----------
  let target = null
  for (let i = 0; i < 200 && !target; i++) {
    const pages = await fetch('http://127.0.0.1:9226/json').then((r) => (r.ok ? r.json() : null)).catch(() => null)
    target = pages?.find((t) => t.type === 'page' && t.url.includes('localhost:5173')) ?? null
    if (!target) await sleep(200)
  }
  if (!target) throw new Error('未找到自检页面（CDP）')

  const ws = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((resolve, reject) => {
    ws.onopen = resolve
    ws.onerror = () => reject(new Error('CDP 连接失败'))
  })
  let msgId = 0
  const pending = new Map()
  ws.onmessage = (event) => {
    const msg = JSON.parse(event.data)
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg)
      pending.delete(msg.id)
    }
  }
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = ++msgId
      pending.set(id, (m) => (m.error ? reject(new Error(m.error.message)) : resolve(m.result)))
      ws.send(JSON.stringify({ id, method, params }))
    })
  evaljs = async (expression) => {
    const r = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    if (r.exceptionDetails) throw new Error(`页面脚本异常: ${JSON.stringify(r.exceptionDetails).slice(0, 300)}`)
    return r.result?.value
  }
  await send('Page.enable')
  await send('Runtime.enable')

  // 等应用就绪（hash 连接参数 + 启动屏结束）
  let conn = null
  for (let i = 0; i < 50 && !conn; i++) {
    conn = await evaljs(`(() => { const p = new URLSearchParams(location.hash.slice(1)); return p.get('api') ? { api: p.get('api'), token: p.get('token') } : null })()`).catch(() => null)
    if (!conn) await sleep(300)
  }
  check(!!conn, '应用就绪（hash 连接参数）')

  // ---------- 经页面内 fetch 创建带别名的任务（不走 LLM：无配置会失败，但记录与原文已落盘） ----------
  const created = await evaljs(`fetch(location.hash.match(/api=([^&]*)/) ? decodeURIComponent(location.hash.match(/api=([^&]*)/)[1]) + '/api/v1/db/deconstruct/tasks' : '', {
    method: 'POST',
    headers: { Authorization: 'Bearer ' + new URLSearchParams(location.hash.slice(1)).get('token'), 'Content-Type': 'application/json' },
    body: JSON.stringify({
      source_name: '复制别名验证',
      content: '第1场\\n玛奇玛：你好。\\n第2场\\n电次：好。\\n第3场\\n岸边：嗯。',
      character: '玛奇玛',
      aliases: ['マキマ', '支配恶魔'],
    }),
  }).then((r) => r.json())`)
  check(!!created?.id, `任务创建（${created?.id}）`)

  // ---------- 界面驱动：Database → 角色制作 → 选中记录 → 复制为新制作 ----------
  // 全程条件等待（应用启动屏未就绪时点击会被吃掉，固定 sleep 不可靠）
  const waitFor = async (fn, label, tries = 100) => {
    for (let i = 0; i < tries; i++) {
      const ok = await fn().catch(() => false)
      if (ok) return true
      await sleep(200)
    }
    console.log(`等待超时：${label}`)
    return false
  }

  // 等启动屏结束（Database 导航可交互）
  await waitFor(
    () => evaljs(`[...document.querySelectorAll('button')].some((b) => b.title === 'Database')`),
    '启动屏结束',
  )
  await evaljs(`[...document.querySelectorAll('button')].find((b) => b.title === 'Database')?.click()`)
  await waitFor(
    () => evaljs(`[...document.querySelectorAll('.panel .entry')].some((b) => b.textContent.includes('角色制作'))`),
    'Database 面板',
  )
  await evaljs(`[...document.querySelectorAll('.panel .entry')].find((b) => b.textContent.includes('角色制作'))?.click()`)

  check(
    await waitFor(
      () => evaljs(`[...document.querySelectorAll('.records .record')].some((el) => el.textContent.includes('玛奇玛'))`),
      '制作记录',
    ),
    '制作记录出现在次边栏',
  )
  await evaljs(`[...document.querySelectorAll('.records .record')].find((el) => el.textContent.includes('玛奇玛'))?.click()`)

  // 点「复制为新制作」
  await waitFor(
    () => evaljs(`[...document.querySelectorAll('button')].some((b) => b.textContent.includes('复制为新制作'))`),
    '复制按钮',
  )
  await evaljs(`[...document.querySelectorAll('button')].find((b) => b.textContent.includes('复制为新制作'))?.click()`)
  await waitFor(() => evaljs(`!!document.querySelector('.form .aliases')`), '表单打开')

  // 断言：表单打开，且别名 chips 全部带回
  const formOpen = await evaljs(`!!document.querySelector('.form .aliases')`)
  check(formOpen, '信息填写表单打开')
  const chips = await evaljs(`[...document.querySelectorAll('.aliases .alias')].map((el) => el.textContent.replace('×', '').trim())`)
  check(
    JSON.stringify(chips) === JSON.stringify(['マキマ', '支配恶魔']),
    `别名 chips 回填（实际：${JSON.stringify(chips)}）`,
  )

  // 逗号分隔批量输入：输入框里输入一串 → 失焦自动落为多条 chip
  await evaljs(`(() => {
    const input = document.querySelector('.aliases input')
    input.value = '青子, 有珠小姐、久远寺有珠'
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new FocusEvent('blur'))
  })()`)
  await sleep(300)
  const chips2 = await evaljs(`[...document.querySelectorAll('.aliases .alias')].map((el) => el.textContent.replace('×', '').trim())`)
  check(
    chips2.length === 5 && chips2.includes('青子') && chips2.includes('有珠小姐') && chips2.includes('久远寺有珠'),
    `逗号分隔批量落盘（实际：${JSON.stringify(chips2)}）`,
  )
} catch (e) {
  failures.push(`异常：${e.message}`)
  console.error(e)
} finally {
  clearTimeout(watchdog)
  if (vite.pid) killTree(vite.pid)
  if (electron?.pid) killTree(electron.pid)
  fs.rmSync(tmpHome, { recursive: true, force: true })
  fs.rmSync(tmp, { recursive: true, force: true })
}

console.log(failures.length ? `\n✗ ${failures.length} 项失败` : '\n✓ 制作视图 UI 实测全部通过')
process.exit(failures.length ? 1 : 0)
