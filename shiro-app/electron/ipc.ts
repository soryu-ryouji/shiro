// 窗口控制 IPC handler：自绘标题栏的最小化/最大化/关闭（壳功能，非业务数据）。
import { BrowserWindow, ipcMain } from 'electron'
import { IPC } from './ipc-contract'

export function registerIpc(getWindow: () => BrowserWindow | null): void {
  ipcMain.handle(IPC.winMinimize, () => getWindow()?.minimize())
  ipcMain.handle(IPC.winMaximizeToggle, () => {
    const win = getWindow()
    if (!win) return false
    if (win.isMaximized()) {
      win.unmaximize()
      return false
    }
    win.maximize()
    return true
  })
  ipcMain.handle(IPC.winClose, () => getWindow()?.close())
}
