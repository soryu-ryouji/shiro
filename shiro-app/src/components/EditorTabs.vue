<script setup lang="ts">
// 编辑区顶栏（窗口拖拽区）：右端设置按钮；Windows/Linux 避让窗口控制按钮。
// 其下依次为标签页条（VSCode 式 tabs：点击切换、右侧 x 关闭、HTML5 拖拽排序）与字数行（编辑器窗口右上角）。
// 标签页条与字数行恒占高度：标签 ≤1 时标签页条留空，无文稿时字数行留空——显隐均不推动内容块。
import { ref } from 'vue'
import { hasShell, isMac, shell } from '../platform'
import { projectStore } from '../stores/project'
import Icon from './Icon.vue'

const emit = defineEmits<{ 'open-settings': [] }>()

/** Windows/Linux 桌面端：窗口控制按钮（fixed 右上角）压本行右端 */
const reserveControls = hasShell && !isMac

/** 双击空白切换最大化；点在标签/按钮上不触发 */
function onDblClick(e: MouseEvent) {
  if ((e.target as HTMLElement).closest('button, .tab')) return
  void shell.toggleMaximizeWindow()
}

/** 标签显示名：文件名去后缀 */
function tabTitle(path: string): string {
  return path.split('/').at(-1)?.replace(/\.(md|markdown)$/i, '') ?? path
}

// ---- 拖拽排序 ----
const dragIndex = ref<number | null>(null)
const dropIndex = ref<number | null>(null)

function onDragStart(i: number, e: DragEvent) {
  dragIndex.value = i
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = 'move'
    e.dataTransfer.setData('text/plain', String(i))
  }
}

function onDragOver(i: number, e: DragEvent) {
  e.preventDefault()
  if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
  dropIndex.value = i
}

function onDrop(i: number, e: DragEvent) {
  e.preventDefault()
  if (dragIndex.value !== null && dragIndex.value !== i) {
    projectStore.moveTab(dragIndex.value, i)
  }
  onDragEnd()
}

function onDragEnd() {
  dragIndex.value = null
  dropIndex.value = null
}
</script>

<template>
  <!-- 顶栏常驻（窗口拖拽区）：右端设置 -->
  <div class="editor-head" :class="{ 'reserve-controls': reserveControls }" @dblclick="onDblClick">
    <button class="bar-btn" title="设置" @click="emit('open-settings')">
      <Icon name="settings" :size="15" />
    </button>
  </div>

  <!-- 标签页条：恒占一行（≤1 个标签时留空）；首签与次栏列表首项平齐 -->
  <div class="tabs-row" @dblclick="onDblClick">
    <div v-if="projectStore.tabs.length > 1" class="tabs">
      <div
        v-for="(path, i) in projectStore.tabs"
        :key="path"
        class="tab"
        :class="{
          active: projectStore.currentFile === path,
          'drop-target': dropIndex === i && dragIndex !== null && dragIndex !== i,
          dragging: dragIndex === i,
        }"
        draggable="true"
        :title="path"
        @click="projectStore.currentFile = path"
        @dragstart="onDragStart(i, $event)"
        @dragover="onDragOver(i, $event)"
        @drop="onDrop(i, $event)"
        @dragend="onDragEnd"
      >
        <Icon name="fileText" :size="13" class="tab-icon" />
        <span class="tab-title">{{ tabTitle(path) }}</span>
        <button class="tab-close" title="关闭" @click.stop="projectStore.closeTab(path)">
          <Icon name="close" :size="12" />
        </button>
      </div>
    </div>
  </div>

  <!-- 字数行：编辑器窗口右上角（标签页条下一行）；恒占一行，无文稿时留空 -->
  <div class="status-row">
    <div v-if="projectStore.currentFile" class="editor-status">
      <template v-if="projectStore.saveStatusVisible">
        <span v-if="projectStore.saveState === 'saving'">保存中…</span>
        <span v-else-if="projectStore.saveState === 'error'" class="save-error">保存失败</span>
        <span v-else>已保存</span>
        <span class="sep">·</span>
      </template>
      <span>{{ projectStore.wordCount }} 字</span>
    </div>
  </div>
</template>

<style scoped>
/* 顶栏：与次栏顶栏（SheetList）同为 40px，拼成通窗工具行；本身是窗口拖拽区 */
.editor-head {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 4px;
  height: 40px;
  padding: 0 8px;
  -webkit-app-region: drag;
}

.editor-head button {
  -webkit-app-region: no-drag;
}

/* Windows/Linux：右端避让 fixed 窗口控制按钮（3 × 42px + 间隙） */
.editor-head.reserve-controls {
  padding-right: 130px;
}

/* 标签页条：恒占一行（12px 上间隙 + 24px 标签），标签显隐不推动内容块；
   顶部间隙与次栏列表（.sheets padding-top）一致，首签与列表首项平齐 */
.tabs-row {
  flex: none;
  display: flex;
  height: 36px;
  padding: 12px 8px 0;
  -webkit-app-region: drag;
}

/* 可拖拽标签与其按钮退出窗口拖拽区 */
.tabs-row button,
.tabs-row .tab {
  -webkit-app-region: no-drag;
}

.bar-btn {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  padding: 0;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}

@media (hover: hover) {
  .bar-btn:hover {
    background: var(--bg-soft);
    color: var(--text);
  }
}

/* 标签区：横向滚动（滚动条隐藏），不挤压右侧状态区 */
.tabs {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 4px;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: none;
}

.tabs::-webkit-scrollbar {
  display: none;
}

/* 字数行：编辑器窗口右上角，恒占一行 */
.status-row {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  height: 22px;
  padding: 0 12px;
  user-select: none;
}

/* 状态区（保存状态 + 字数） */
.editor-status {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px;
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
  user-select: none;
  white-space: nowrap;
}

.save-error {
  color: var(--danger);
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 24px;
  padding: 0 6px 0 8px;
  border-radius: 6px;
  background: var(--bg-soft);
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text-dim);
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
  min-width: 0;
}

/* 激活：与全局选中态一致（accent-soft 底 + accent 字） */
.tab.active {
  background: var(--accent-soft);
  color: var(--accent);
}

.tab.active .tab-icon {
  color: var(--accent);
}

.tab.dragging {
  opacity: 0.5;
}

/* 拖拽目标指示：左缘竖线 */
.tab.drop-target {
  box-shadow: inset 2px 0 0 var(--accent);
}

.tab-icon {
  flex: none;
}

.tab-title {
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 160px;
}

.tab-close {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}

@media (hover: hover) {
  .tab-close:hover {
    background: var(--border);
    color: var(--text);
  }
}
</style>
