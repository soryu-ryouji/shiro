<script setup lang="ts">
// 左侧导航栏：顶条（拖拽区；macOS 原生红绿灯压在左侧；右端侧栏开关）+ 导航列表。
// 色块通高到窗口上沿，不被中栏顶栏隔断（hawk/Eagle 式）。
import { shell } from '../platform'
import type { NavItem, NavKey } from '../types'
import Icon from './Icon.vue'

defineProps<{ items: NavItem[]; active: NavKey }>()
const emit = defineEmits<{ navigate: [key: NavKey]; toggle: [] }>()

/** 双击顶条空白切换最大化；点在按钮上不触发 */
function onHeadDblClick(e: MouseEvent) {
  if ((e.target as HTMLElement).closest('button')) return
  void shell.toggleMaximizeWindow()
}
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar-head" @dblclick="onHeadDblClick">
      <button class="panel-toggle" title="侧栏" @click="emit('toggle')">
        <Icon name="panelLeft" :size="16" />
      </button>
    </div>
    <div class="sidebar-body">
      <button
        v-for="item in items"
        :key="item.key"
        class="entry"
        :class="{ active: active === item.key }"
        @click="emit('navigate', item.key)"
      >
        <Icon :name="item.icon" :size="16" />
        <span class="label">{{ item.label }}</span>
      </button>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  background: var(--bg-soft);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  /* 侧栏隐藏（列宽 0）时内容不溢出 */
  min-width: 0;
}

.sidebar-head {
  flex: none;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  padding: 0 8px;
  -webkit-app-region: drag;
}

/* 侧栏开关：条在拖拽区内，按钮须退出拖拽 */
.panel-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  padding: 0;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--accent);
  -webkit-app-region: no-drag;
  cursor: pointer;
}

@media (hover: hover) {
  .panel-toggle:hover {
    background: var(--border);
  }
}

.sidebar-body {
  flex: 1;
  padding: 4px 8px 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  overflow-y: auto;
}

.entry {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border: none;
  border-radius: 6px;
  background: none;
  font-size: 14px;
  color: var(--text);
  text-align: left;
  cursor: pointer;
  white-space: nowrap;
}

@media (hover: hover) {
  .entry:hover {
    background: var(--border);
  }
}

.entry.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}
</style>
