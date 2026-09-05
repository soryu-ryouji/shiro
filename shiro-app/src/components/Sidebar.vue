<script setup lang="ts">
// 侧栏（Activity Bar 置顶形态）：
// - Windows/Linux：窗口控制在右上角，左上角无占用——Activity 图标行直接放顶条（与收起按钮同行）
// - macOS：红绿灯在左上角——顶条左侧完全留白，图标行挪到第二行
// - 收起按钮无论如何都在顶条右端
// - 第三行面板标题；正文按 Activity 切换面板组件
import { computed } from 'vue'
import { hasShell, isMac, shell } from '../platform'
import type { NavItem, NavKey } from '../types'
import ActivityBar from './ActivityBar.vue'
import Icon from './Icon.vue'
import ProjectPanel from '../panels/ProjectPanel.vue'
import DatabasePanel from '../panels/DatabasePanel.vue'
import ModelPanel from '../panels/ModelPanel.vue'

const props = defineProps<{ items: NavItem[]; active: NavKey }>()
const emit = defineEmits<{ activate: [key: NavKey]; toggle: [] }>()

const PANEL_TITLES: Record<NavKey, string> = {
  project: 'Project',
  database: 'Database',
  model: 'Model',
}
const title = computed(() => PANEL_TITLES[props.active])

/** macOS 红绿灯占左上角：图标行让位到第二行（仅真 Electron macOS；浏览器形态无窗口控制，不启用） */
const trafficOnTop = hasShell && isMac

/** 双击顶行空白切换最大化；点在按钮上不触发 */
function onHeadDblClick(e: MouseEvent) {
  if ((e.target as HTMLElement).closest('button')) return
  void shell.toggleMaximizeWindow()
}
</script>

<template>
  <aside class="sidebar">
    <!-- 顶条：拖拽区；左侧留白（macOS 红绿灯位），右端收起按钮 -->
    <div class="sidebar-head" :class="{ 'with-activity': !trafficOnTop }" @dblclick="onHeadDblClick">
      <ActivityBar v-if="!trafficOnTop" :items="items" :active="active" @activate="emit('activate', $event)" />
      <button class="panel-toggle" title="收起侧栏" @click="emit('toggle')">
        <Icon name="panelLeft" :size="16" />
      </button>
    </div>
    <!-- macOS：图标行挪到顶条之下，不占红绿灯位 -->
    <div v-if="trafficOnTop" class="activity-row" @dblclick="onHeadDblClick">
      <ActivityBar :items="items" :active="active" @activate="emit('activate', $event)" />
    </div>
    <div class="panel-title">{{ title }}</div>
    <div class="sidebar-body">
      <ProjectPanel v-if="active === 'project'" />
      <DatabasePanel v-else-if="active === 'database'" />
      <ModelPanel v-else />
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
  /* 默认（macOS）：条内只有收起按钮，靠右 */
  justify-content: flex-end;
  padding: 0 8px;
  -webkit-app-region: drag;
}

/* Windows/Linux：图标行在顶条内，与收起按钮两端分布 */
.sidebar-head.with-activity {
  justify-content: space-between;
  gap: 8px;
  padding-left: 12px;
}

/* macOS：图标行在顶条之下，整行缝隙可拖拽窗口 */
.activity-row {
  flex: none;
  display: flex;
  align-items: center;
  padding: 2px 12px 6px;
  -webkit-app-region: drag;
}

/* 收起开关：条在拖拽区内，按钮须退出拖拽 */
.panel-toggle {
  flex: none;
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

.panel-title {
  flex: none;
  padding: 2px 12px 8px;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.4px;
  color: var(--text-dim);
  white-space: nowrap;
}

.sidebar-body {
  flex: 1;
  padding: 0 8px 8px;
  overflow-y: auto;
}
</style>
