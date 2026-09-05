// 主进程构建：esbuild 打包 electron/main.ts 到 dist-electron/main.mjs（ESM 单文件）。
// 一次性构建（pack/CI 用）；开发态的持续重建由 dev.mjs 驱动。
import * as esbuild from 'esbuild'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

await esbuild.build({
  entryPoints: [path.join(root, 'electron/main.ts')],
  bundle: true,
  platform: 'node',
  format: 'esm',
  external: ['electron'],
  outfile: path.join(root, 'dist-electron/main.mjs'),
})
console.log('[build-electron] dist-electron/main.mjs 已生成')
