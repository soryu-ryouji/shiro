import { app, BrowserWindow, dialog } from 'electron'
import { spawn, type ChildProcess } from 'node:child_process'
import crypto from 'node:crypto'
import fs from 'node:fs'
import net from 'node:net'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { IPC } from './ipc-contract'
import { registerIpc } from './ipc'

/**
 * Electron 壳职责（见 docs/architecture.md 核心原则）：
 * 创建窗口、拉起/回收 shiro-daemon、注入连接参数。业务通信前端只走 HTTP。
 * 启动模型对齐 hawk：窗口与页面先行（hash 注入），页面轮询 startup 呈现启动屏；
 * 主进程不阻塞等待就绪，daemon 失败经 exit/error 事件上报。
 */

const ELECTRON_DIR = path.dirname(fileURLToPath(import.meta.url))
const isDev = !app.isPackaged

// 会话数据固定走 shiro-app 子目录：打包版 productName 在 Linux 会把默认 userData
// 解析为 ~/.config/shiro，与应用自有配置目录相撞（Chromium 数据混入）
app.setPath('userData', path.join(app.getPath('appData'), 'shiro-app'))

let mainWindow: BrowserWindow | null = null
let daemon: ChildProcess | null = null
/** 主动回收（退出应用）时抑制 exit 误报 */
let stopping = false

// 单实例：再次启动聚焦已有窗口，不拉起第二套 daemon 争用数据目录
if (!app.requestSingleInstanceLock()) {
  app.quit()
} else {
  app.on('second-instance', () => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore()
      mainWindow.focus()
    }
  })
}

/** 预选空闲环回端口：端口与 token 都由本进程生成，不需要子进程回传 */
function probeFreePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = net.createServer()
    probe.once('error', reject)
    probe.listen(0, '127.0.0.1', () => {
      const address = probe.address()
      if (typeof address !== 'object' || address === null) {
        probe.close(() => reject(new Error('预选端口失败')))
        return
      }
      probe.close(() => resolve(address.port))
    })
  })
}

function daemonBin(): string {
  if (process.env.SHIRO_DAEMON_EXE) return process.env.SHIRO_DAEMON_EXE
  const exe = process.platform === 'win32' ? 'shiro-daemon.exe' : 'shiro-daemon'
  if (isDev) {
    // 开发态：release/debug 候选按 mtime 取最新
    const targetDir = path.resolve(ELECTRON_DIR, '../../shiro-daemon/target')
    const candidates = ['release', 'debug']
      .map((d) => path.join(targetDir, d, exe))
      .filter((bin) => fs.existsSync(bin))
      .sort((a, b) => fs.statSync(b).mtimeMs - fs.statSync(a).mtimeMs)
    if (candidates.length > 0) return candidates[0]
    throw new Error('未找到 shiro-daemon 构建产物，请先 cargo build（shiro-daemon/）')
  }
  // 打包态：extraResources 携带的二进制（后续 electron-builder 配置）
  return path.join(process.resourcesPath, 'shiro-daemon', exe)
}

function fail(message: string): void {
  dialog.showErrorBox('shiro-daemon 启动失败', message)
  app.quit()
}

function startDaemon(port: number, token: string): ChildProcess {
  const child = spawn(daemonBin(), ['--port', String(port)], {
    env: { ...process.env, SHIRO_TOKEN: token },
    // stdout 不承担协议，只看 stderr 报错（留尾部用于错误框；dev 态转发终端）
    stdio: ['ignore', 'ignore', 'pipe'],
    // GUI 进程拉起控制台子进程：不隐藏在 Windows 上会弹黑窗口
    windowsHide: true,
  })
  let stderrTail = ''
  child.stderr?.on('data', (chunk: Buffer) => {
    stderrTail = (stderrTail + chunk.toString()).slice(-4000)
    if (isDev) process.stderr.write(chunk)
  })
  child.on('error', (error) => fail(`shiro-daemon 启动失败: ${error.message}`))
  child.on('exit', (code) => {
    if (!stopping) {
      fail(`shiro-daemon 异常退出（退出码 ${code}）${stderrTail.trim() ? `\n${stderrTail.trim()}` : ''}`)
    }
  })
  return child
}

app.whenReady().then(async () => {
  const port = await probeFreePort()
  const token = crypto.randomBytes(32).toString('hex')
  daemon = startDaemon(port, token)

  registerIpc(() => mainWindow)

  // 窗口立即创建并加载页面（连接参数经 URL hash 注入，不经 IPC、不落盘）。
  // 启动进度由页面轮询 /api/v1/app/startup 呈现（starting → ready/error）。
  mainWindow = new BrowserWindow({
    width: 1280,
    height: 800,
    minWidth: 960,
    minHeight: 600,
    show: false,
    // 与浅色主题一致，防启动白闪
    backgroundColor: '#ffffff',
    // macOS：隐藏系统标题栏但保留原生红绿灯（悬停 glyph/全屏行为由系统保证），
    // trafficLightPosition 按 40px 拖拽条垂直居中；Windows/Linux：无边框，窗口控制前端自绘
    ...(process.platform === 'darwin'
      ? { titleBarStyle: 'hidden' as const, trafficLightPosition: { x: 12, y: 14 } }
      : { frame: false }),
    // 开发态 / Linux 的窗口图标；打包后各平台图标由 electron-builder 嵌入
    icon: path.join(ELECTRON_DIR, '../build/icon.png'),
    webPreferences: {
      preload: path.join(ELECTRON_DIR, 'preload.cjs'),
    },
  })
  mainWindow.once('ready-to-show', () => mainWindow?.show())
  // 同步最大化状态给渲染进程（窗口控制按钮图标切换，含 Aero Snap 等系统途径）
  mainWindow.on('maximize', () => mainWindow?.webContents.send(IPC.winMaximized, true))
  mainWindow.on('unmaximize', () => mainWindow?.webContents.send(IPC.winMaximized, false))

  // 无头自检：SHIRO_SCREENSHOT=<路径> 时加载完成后截图落盘（可带 SHIRO_DIAG 输出编辑器 DOM 诊断）
  if (process.env.SHIRO_SCREENSHOT) {
    mainWindow.webContents.once('did-finish-load', () => {
      const delay = Number(process.env.SHIRO_SCREENSHOT_DELAY || 3000)
      setTimeout(async () => {
        if (process.env.SHIRO_DIAG) {
          const diag = await mainWindow?.webContents.executeJavaScript(`(() => {
            const r = (sel) => { const el = document.querySelector(sel); if (!el) return null; const b = el.getBoundingClientRect(); return { w: Math.round(b.width), h: Math.round(b.height) } }
            return JSON.stringify({ wrap: r('.editor'), cmEditor: r('.cm-editor'), scroller: r('.cm-scroller'), content: r('.cm-content'), text: document.querySelector('.cm-content')?.textContent?.slice(0, 30) ?? null })
          })()`)
          console.log('diag:', diag)
        }
        const image = await mainWindow?.webContents.capturePage()
        if (image) fs.writeFileSync(process.env.SHIRO_SCREENSHOT as string, image.toPNG())
        console.log(`screenshot saved: ${process.env.SHIRO_SCREENSHOT}`)
      }, delay)
    })
  }

  const params =
    `api=${encodeURIComponent(`http://127.0.0.1:${port}`)}&token=${token}` +
    // 调试深链透传（SHIRO_OPEN / SHIRO_FILE，无头冒烟用）
    (process.env.SHIRO_OPEN ? `&open=${encodeURIComponent(process.env.SHIRO_OPEN)}` : '') +
    (process.env.SHIRO_FILE ? `&file=${encodeURIComponent(process.env.SHIRO_FILE)}` : '')
  const devServer = process.env.VITE_DEV_SERVER_URL
  if (devServer) {
    mainWindow.loadURL(`${devServer}/#${params}`)
  } else {
    mainWindow.loadFile(path.join(ELECTRON_DIR, '../dist/index.html'), { hash: params })
  }
})

app.on('window-all-closed', () => app.quit())
app.on('will-quit', () => {
  stopping = true
  daemon?.kill()
})
