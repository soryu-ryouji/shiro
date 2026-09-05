// 窗口控制与系统能力 IPC handler：窗口控制/目录选择/文件管理器定位（壳功能，非业务数据）。
import { BrowserWindow, dialog, ipcMain, shell as electronShell } from 'electron'
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

  ipcMain.handle(IPC.pickDirectory, async (_event, title: string) => {
    const win = getWindow()
    if (!win) return null
    const result = await dialog.showOpenDialog(win, {
      title,
      properties: ['openDirectory', 'createDirectory'],
    })
    return result.canceled ? null : result.filePaths[0]
  })
  ipcMain.handle(IPC.showInFolder, (_event, path: string) => electronShell.showItemInFolder(path))
}
