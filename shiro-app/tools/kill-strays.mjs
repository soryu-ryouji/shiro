// 测试前清理游离进程：此前超时/中断的运行可能留下 electron（占调试端口）、vite（占 5173）、
// shiro-daemon（锁住 target 下的 exe，导致 cargo 无法重新链接）实例。
// 只杀可执行路径/命令行落在本项目的进程，不动正在运行的打包版（shiro.exe）与其他项目。
import { spawnSync } from 'node:child_process'

/**
 * 清理本项目相关的游离测试进程，返回杀掉的数量。
 * @param projectRoot 项目根目录（如 D:\Projects\Ryouji\shiro）
 */
export function killStrays(projectRoot) {
  const appDir = `${projectRoot}\\shiro-app\\node_modules\\electron`
  const viteBin = `${projectRoot}\\shiro-app\\node_modules\\vite\\bin\\vite.js`
  const daemonDir = `${projectRoot}\\shiro-daemon\\target`
  if (process.platform !== 'win32') {
    // POSIX 兜底：按命令行模式 pkill（无 ExecutablePath 可查，模式已足够窄）
    let count = 0
    for (const pat of [`${appDir}`, viteBin, `${daemonDir}`]) {
      const r = spawnSync('pkill', ['-f', pat], { stdio: 'ignore' })
      if (r.status === 0) count++
    }
    return count
  }
  const ps = [
    `$appDir = '${appDir}'`,
    `$viteBin = '${viteBin}'`,
    `$daemonDir = '${daemonDir}'`,
    `$procs = Get-CimInstance Win32_Process | Where-Object {`,
    `  ($_.Name -eq 'electron.exe' -and $_.ExecutablePath -and $_.ExecutablePath.StartsWith($appDir, 'OrdinalIgnoreCase')) -or`,
    `  ($_.Name -eq 'node.exe' -and $_.CommandLine -and $_.CommandLine.IndexOf($viteBin, 'OrdinalIgnoreCase') -ge 0) -or`,
    `  ($_.Name -eq 'shiro-daemon.exe' -and $_.ExecutablePath -and $_.ExecutablePath.StartsWith($daemonDir, 'OrdinalIgnoreCase'))`,
    `}`,
    `$count = 0`,
    `foreach ($p in $procs) { Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue; $count++ }`,
    `Write-Output $count`,
  ].join('\n')
  const r = spawnSync('powershell', ['-NoProfile', '-Command', ps], { encoding: 'utf8' })
  return Number(r.stdout?.trim()) || 0
}
