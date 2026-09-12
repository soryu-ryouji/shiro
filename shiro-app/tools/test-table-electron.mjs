// 表格挂件真实浏览器（Electron/Chromium）集成测试：jsdom 的焦点/选区行为不可信，用真内核跑
// 用法：cd shiro-app && node tools/test-table-electron.mjs
import { build } from 'esbuild'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { spawn } from 'node:child_process'
import { killStrays } from './kill-strays.mjs'

// 预检：清理此前中断运行残留的 electron 实例（隐藏窗口在父进程被杀后会残留）
const strays = killStrays(resolve(process.cwd(), '..'))
if (strays > 0) {
  console.log(`预检清理：杀掉 ${strays} 个游离进程`)
  await new Promise((r) => setTimeout(r, 800))
}

const outDir = mkdtempSync(join(process.cwd(), 'tools', '.etest-'))
let exitCode = 1
try {
  await build({
    entryPoints: ['tools/test-table-electron-harness.ts'],
    bundle: true,
    format: 'iife',
    outfile: join(outDir, 'harness.js'),
    logLevel: 'silent',
  })
  writeFileSync(join(outDir, 'index.html'), '<!DOCTYPE html><html><body><script src="harness.js"></script></body></html>')
  writeFileSync(
    join(outDir, 'main.cjs'),
    `const { app, BrowserWindow } = require('electron')
app.disableHardwareAcceleration()
app.whenReady().then(async () => {
  const win = new BrowserWindow({ show: false, width: 900, height: 700 })
  await win.loadFile(${JSON.stringify(join(outDir, 'index.html'))})
  let result = null
  for (let i = 0; i < 40; i++) {
    await new Promise((r) => setTimeout(r, 250))
    result = await win.webContents.executeJavaScript('window.__result ? JSON.stringify(window.__result) : ""')
    if (result) break
  }
  console.log('RESULT:' + (result || 'TIMEOUT'))
  app.exit(0)
})
`,
  )
  // CI Linux（ubuntu 24.04+）无 setuid 的 chrome-sandbox 助手且 userns 受限，不关沙箱 electron 启动即退
  const args = [join(process.cwd(), 'node_modules/electron/cli.js'), join(outDir, 'main.cjs')]
  if (process.platform === 'linux') args.push('--no-sandbox')
  const child = spawn(process.execPath, args, {
    stdio: ['ignore', 'pipe', 'inherit'],
  })
  let out = ''
  child.stdout.on('data', (d) => (out += d))
  const code = await new Promise((r) => child.on('exit', r))
  const m = out.match(/RESULT:(.*)/s)
  if (!m) {
    console.error('未拿到测试结果，退出码', code, '\n', out)
  } else {
    const { failures, log } = JSON.parse(m[1].trim())
    for (const line of log) console.log(line)
    console.log(failures.length ? `\ntest-table-electron: ${failures.length} failed` : '\ntest-table-electron: all passed')
    exitCode = failures.length ? 1 : 0
  }
} finally {
  // 清理必须抢在 process.exit 之前（try 内 exit 会跳过 finally）
  rmSync(outDir, { recursive: true, force: true })
}
process.exit(exitCode)
