import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  // file:// 加载（打包形态）需要相对路径
  base: './',
  server: {
    port: 5173,
    strictPort: true,
    // 忽略测试产物目录：自检（tools/ui-check.mjs）的 Electron 会话数据在此被高频读写，
    // Windows 上 watcher 会因 Cookies 文件被占用（EBUSY）直接崩溃
    watch: { ignored: ['**/tools/.tmp/**'] },
  },
})
