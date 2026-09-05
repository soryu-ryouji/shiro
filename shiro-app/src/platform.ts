// 平台判定与 Electron shell 能力收敛。真实平台由 preload 注入；纯浏览器（局域网形态）为 no-op 壳。
// 一律经本模块访问 shell，不散点 window.shiroShell 判断：
// - hasShell：能力/UI 分支（窗口控制按钮仅 Electron 出现）
// - shell：类型化通道，浏览器端为 no-op 壳，调用方无需 ?. 兜底
import type { ShiroShell } from '../electron/ipc-contract'

declare global {
  interface Window {
    shiroShell?: ShiroShell
  }
}

/** 是否存在 Electron preload 注入的 shell（false = 纯浏览器/局域网访问） */
export const hasShell = !!window.shiroShell

/** 运行平台：darwin/win32/linux；真实平台由 preload 注入，纯浏览器按 userAgent 兜底 */
export const platform =
  window.shiroShell?.platform ??
  (navigator.userAgent.includes('Mac') ? 'darwin' : navigator.userAgent.includes('Windows') ? 'win32' : 'linux')

/** 是否 macOS（红绿灯/拖拽区避让分支）。注意仅在 hasShell 下可信：iPhone UA 含 "like Mac OS X" */
export const isMac = platform === 'darwin'

/** 浏览器端 no-op 壳：每个方法返回类型化空值，语义同「能力不存在」 */
const noopShell: ShiroShell = {
  platform,
  minimizeWindow: async () => {},
  toggleMaximizeWindow: async () => false,
  closeWindow: async () => {},
  onWindowMaximized: () => () => {},
}

/** 类型化 shell 通道：Electron 走 preload 注入，浏览器走 no-op 壳 */
export const shell = window.shiroShell ?? noopShell
