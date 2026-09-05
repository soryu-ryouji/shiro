import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  // file:// 加载（打包形态）需要相对路径
  base: './',
  server: { port: 5173, strictPort: true },
})
