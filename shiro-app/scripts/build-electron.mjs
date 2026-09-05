// 主进程构建：esbuild 打包 electron/main.ts 到 dist-electron/main.mjs（ESM 单文件）。
// 一次性构建（pack/CI 用）；开发态的持续重建由 dev.mjs 驱动。
import * as esbuild from 'esbuild'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

const common = {
  bundle: true,
  platform: 'node',
  external: ['electron'],
}

// main.mjs 为 ESM；sandbox 渲染进程要求 preload 为 CJS 单文件
await esbuild.build({
  ...common,
  entryPoints: [path.join(root, 'electron/main.ts')],
  format: 'esm',
  outfile: path.join(root, 'dist-electron/main.mjs'),
})
await esbuild.build({
  ...common,
  entryPoints: [path.join(root, 'electron/preload.ts')],
  format: 'cjs',
  outfile: path.join(root, 'dist-electron/preload.cjs'),
})
console.log('[build-electron] dist-electron/main.mjs + preload.cjs 已生成')
