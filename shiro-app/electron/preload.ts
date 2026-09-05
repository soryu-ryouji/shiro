// preload：只暴露白名单通道（形状见 ipc-contract.ts 的 ShiroShell），业务数据不经 IPC。
// sandbox 渲染进程要求 preload 为 CJS 单文件（构建产物 dist-electron/preload.cjs）。
import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import { IPC, type ShiroShell } from './ipc-contract'

const shell: ShiroShell = {
  platform: process.platform,
  minimizeWindow: () => ipcRenderer.invoke(IPC.winMinimize),
  toggleMaximizeWindow: () => ipcRenderer.invoke(IPC.winMaximizeToggle),
  closeWindow: () => ipcRenderer.invoke(IPC.winClose),
  pickDirectory: (title) => ipcRenderer.invoke(IPC.pickDirectory, title),
  showInFolder: (path) => ipcRenderer.invoke(IPC.showInFolder, path),
  onWindowMaximized: (cb) => {
    const listener = (_event: IpcRendererEvent, maximized: boolean): void => cb(maximized)
    ipcRenderer.on(IPC.winMaximized, listener)
    return () => ipcRenderer.removeListener(IPC.winMaximized, listener)
  },
}

contextBridge.exposeInMainWorld('shiroShell', shell)
