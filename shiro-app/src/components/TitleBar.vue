<script setup lang="ts">
// 中栏顶栏：纯拖拽区 + 右端设置按钮（无标题——当前模块由 Activity 图标表达）。
// 整条为窗口拖拽区（双击空白切换最大化），按钮退出拖拽。
// 侧栏收起时：展开入口在本栏左端，且 macOS 红绿灯改压本栏左端（避让 78px）；
// 详情栏已移除，顶栏恒通窗口右缘：Windows/Linux 右端恒避让 fixed 窗口控制按钮。
import { computed } from 'vue'
import { hasShell, isMac, shell } from '../platform'
import Icon from './Icon.vue'

const props = defineProps<{ sidebarVisible: boolean }>()
const emit = defineEmits<{ 'toggle-sidebar': []; 'open-settings': [] }>()

const reserveTraffic = computed(() => hasShell && isMac && !props.sidebarVisible)
const reserveControls = computed(() => hasShell && !isMac)

function onDblClick(e: MouseEvent) {
  if ((e.target as HTMLElement).closest('button')) return
  void shell.toggleMaximizeWindow()
}
</script>

<template>
  <header class="titlebar" :class="{ 'reserve-traffic': reserveTraffic, 'reserve-controls': reserveControls }" @dblclick="onDblClick">
    <button v-if="!sidebarVisible" class="bar-btn" title="侧栏" @click="emit('toggle-sidebar')">
      <Icon name="panelLeft" :size="16" />
    </button>
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
  -webkit-app-region: drag;
}

/* 交互控件退出拖拽区域 */
.titlebar button {
  -webkit-app-region: no-drag;
}

/* 侧栏收起时顶栏通栏到窗口左缘：macOS 避让原生红绿灯 */
.titlebar.reserve-traffic {
  padding-left: 78px;
}

/* 窗口控制按钮（fixed 右上角 3 × 42px + 间隙）恒压本栏右端 */
.titlebar.reserve-controls {
  padding-right: 130px;
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
