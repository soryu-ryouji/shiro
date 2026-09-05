<script setup lang="ts">
// 无边框窗口的最小化/最大化/关闭按钮（Windows/Linux 风格，fixed 在窗口右上角，与 40px 拖拽条对齐）。
// macOS 用系统原生红绿灯（titleBarStyle: 'hidden'，压在左导航顶部拖拽条上），本组件不渲染。
// 纯浏览器（局域网访问）无 shell，整体不显示。
import { ref } from 'vue'
import { hasShell, isMac, shell } from '../platform'

const isMaximized = ref(false)

// 顶层常驻组件，订阅随应用生命周期，不手动退订
shell.onWindowMaximized((maximized) => (isMaximized.value = maximized))

async function toggleMaximize() {
  isMaximized.value = await shell.toggleMaximizeWindow()
}
</script>

<template>
  <div v-if="hasShell && !isMac" class="win-controls">
    <button class="win-btn" title="最小化" @click="shell.minimizeWindow()">
      <svg width="14" height="14" viewBox="0 0 16 16"><path d="M3 12.5h10" stroke="currentColor" stroke-width="1.2" /></svg>
    </button>
    <button class="win-btn" :title="isMaximized ? '还原' : '最大化'" @click="toggleMaximize">
      <svg v-if="isMaximized" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
        <path d="M5 2.5h8.5V11" />
        <rect x="2.5" y="5" width="8.5" height="8.5" fill="#ffffff" />
      </svg>
      <svg v-else width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2">
        <rect x="2.75" y="2.75" width="10.5" height="10.5" />
      </svg>
    </button>
    <button class="win-btn close" title="关闭" @click="shell.closeWindow()">
      <svg width="14" height="14" viewBox="0 0 16 16"><path d="M3.5 3.5l9 9M12.5 3.5l-9 9" stroke="currentColor" stroke-width="1.2" /></svg>
    </button>
  </div>
</template>

<style scoped>
.win-controls {
  /* fixed 而非放进某一栏：三栏通高后右上角属详情栏顶部拖拽条，栏宽变化时位置不变 */
  position: fixed;
  top: 0;
  right: 0;
  z-index: 100;
  display: flex;
  height: 40px;
  /* 退出窗口拖拽区：下方是拖拽区域，缺了会被拖拽区拦截真实点击 */
  -webkit-app-region: no-drag;
}

.win-btn {
  width: 42px;
  padding: 0;
  border: none;
  border-radius: 0;
  background: transparent;
  color: var(--text);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

@media (hover: hover) {
  .win-btn:hover {
    background: var(--bg-soft);
  }

  .win-btn.close:hover {
    background: #e81123;
    color: #fff;
  }
}
</style>
