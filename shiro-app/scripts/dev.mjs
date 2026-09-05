// 开发启动：构建 daemon 与主进程 → 起 vite dev server → 起 electron（注入 VITE_DEV_SERVER_URL）
// 全程 spawn node + cli 入口，不经 npm/npx（Windows 下 .cmd shim 需要 shell，行为不可靠）
import { spawn, spawnSync } from 'node:child_process'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { build } from 'esbuild'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const stdio = { stdio: 'inherit', cwd: root }

// 1. 构建 daemon
const daemon = spawnSync(
  'cargo',
  ['build', '--manifest-path', path.join(root, '../shiro-daemon/Cargo.toml')],
  stdio,
)
if (daemon.status !== 0) process.exit(daemon.status ?? 1)

// 2. 构建主进程与 preload（esbuild JS API；main 为 ESM，sandbox 要求 preload 为 CJS）
const common = {
  bundle: true,
  platform: 'node',
  external: ['electron'],
}
await build({
  ...common,
  entryPoints: [path.join(root, 'electron/main.ts')],
  format: 'esm',
  outfile: path.join(root, 'dist-electron/main.mjs'),
})
await build({
  ...common,
  entryPoints: [path.join(root, 'electron/preload.ts')],
  format: 'cjs',
  outfile: path.join(root, 'dist-electron/preload.cjs'),
})

// 3. vite dev server
const vite = spawn(process.execPath, [path.join(root, 'node_modules/vite/bin/vite.js')], stdio)

async function waitVite() {
  for (let i = 0; i < 100; i++) {
    try {
      await fetch('http://localhost:5173')
      return
    } catch {
      await new Promise((r) => setTimeout(r, 200))
    }
  }
  throw new Error('vite dev server 启动超时')
}
await waitVite()

// 4. electron（VITE_DEV_SERVER_URL 通知主进程加载 dev server）
const electron = spawn(
  process.execPath,
  [path.join(root, 'node_modules/electron/cli.js'), '.'],
  { ...stdio, env: { ...process.env, VITE_DEV_SERVER_URL: 'http://localhost:5173' } },
)

const shutdown = () => {
  electron.kill()
  vite.kill()
  process.exit(0)
}
electron.on('exit', shutdown)
process.on('SIGINT', shutdown)
