// IPC 契约：主进程 handler、preload 暴露、web 前端消费的三方对齐点（单一定义）。
// 只承载壳功能（窗口控制）；业务数据一律走 REST，不经 IPC（docs/architecture.md 核心原则 1）。
// 通道名一律经 IPC 常量引用，不手写字符串。

/** IPC 通道名（invoke/handle 与 send/on 的全集） */
export const IPC = {
  // ---- 渲染进程 → 主进程（ipcRenderer.invoke / ipcMain.handle） ----
  winMinimize: 'shiro:win-minimize',
  winMaximizeToggle: 'shiro:win-maximize-toggle',
  winClose: 'shiro:win-close',
  // ---- 主进程 → 渲染进程（webContents.send / ipcRenderer.on） ----
  winMaximized: 'shiro:win-maximized',
} as const

/** Electron preload 注入的白名单通道（window.shiroShell；浏览器/局域网形态不存在） */
export interface ShiroShell {
  /** 运行平台（darwin/win32/linux） */
  platform: string
  minimizeWindow(): Promise<void>
  /** 最大化/还原切换，返回切换后的最大化状态 */
  toggleMaximizeWindow(): Promise<boolean>
  closeWindow(): Promise<void>
  /** 订阅最大化状态变化（含 Aero Snap 等系统途径），返回退订函数 */
  onWindowMaximized(cb: (maximized: boolean) => void): () => void
}
