<script setup lang="ts">
// 中栏顶栏（只覆盖中栏，左右栏通高）：当前视图标题 · 右端设置按钮。
// 整条为窗口拖拽区（双击空白切换最大化），按钮退出拖拽。
// 侧栏隐藏时本栏通栏到窗口左缘：macOS 左端避让原生红绿灯，并显示侧栏开关。
import { computed } from 'vue'
import { hasShell, isMac, shell } from '../platform'
import Icon from './Icon.vue'

const props = defineProps<{ title: string; sidebarVisible: boolean }>()
const emit = defineEmits<{ 'toggle-sidebar': []; 'open-settings': [] }>()

const reserveTraffic = computed(() => hasShell && isMac && !props.sidebarVisible)

function onDblClick(e: MouseEvent) {
  if ((e.target as HTMLElement).closest('button')) return
  void shell.toggleMaximizeWindow()
}
</script>

<template>
  <header class="titlebar" :class="{ 'reserve-traffic': reserveTraffic }" @dblclick="onDblClick">
    <button v-if="!sidebarVisible" class="bar-btn" title="侧栏" @click="emit('toggle-sidebar')">
      <Icon name="panelLeft" :size="16" />
    </button>
    <span class="title">{{ title }}</span>
    <div class="spacer" />
    <button class="bar-btn" title="设置" @click="emit('open-settings')">
      <Icon name="settings" :size="16" />
    </button>
  </header>
</template>

<style scoped>
.titlebar {
  flex: none;
  height: 40px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  border-bottom: 1px solid var(--border);
  background: var(--bg);
  -webkit-app-region: drag;
}

/* 交互控件退出拖拽区域 */
.titlebar button {
  -webkit-app-region: no-drag;
}

/* 侧栏隐藏时顶栏通栏：macOS 左端避开窗口左上角的原生红绿灯 */
.titlebar.reserve-traffic {
  padding-left: 78px;
}

.title {
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
}

.spacer {
  flex: 1;
}

.bar-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text);
  cursor: pointer;
}

@media (hover: hover) {
  .bar-btn:hover {
    background: var(--bg-soft);
  }
}
</style>
