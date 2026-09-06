<script setup lang="ts">
// 编辑区顶栏（窗口拖拽区）：预览与设置按钮；Windows/Linux 避让窗口控制按钮。
// 其下为标签页条（VSCode 式 tabs：点击切换、右侧 x 关闭、HTML5 拖拽排序），恒占高度：标签 ≤1 时留空，显隐不推动内容块。
// 字数与保存状态在 Editor.vue 底部工具栏右端。
import { ref } from 'vue'
import { hasShell, isMac, shell } from '../platform'
import { projectStore } from '../stores/project'
import Icon from './Icon.vue'

const emit = defineEmits<{ 'open-settings': []; 'open-preview': [] }>()

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
  <!-- 顶栏常驻（窗口拖拽区）：预览（有文稿时可用）与设置 -->
  <div class="editor-head" :class="{ 'reserve-controls': reserveControls }" @dblclick="onDblClick">
    <button class="bar-btn" title="手机预览" :disabled="!projectStore.currentFile" @click="emit('open-preview')">
      <Icon name="eye" :size="15" />
    </button>
    <button class="bar-btn" title="设置" @click="emit('open-settings')">
      <Icon name="settings" :size="15" />
    </button>
  </div>

  <!-- 标签页条：恒占一行（≤1 个标签时留空）；本行底部与次栏列表首项顶部平齐 -->
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
   次栏列表首项与正文首行同起点，与本行底部保持 24px 间距（.sheets padding-top 60px / .cm-content padding-top 24px） */
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

.bar-btn:disabled {
  opacity: 0.4;
  cursor: default;
}

.bar-btn:disabled:hover {
  background: transparent;
  color: var(--text-dim);
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
