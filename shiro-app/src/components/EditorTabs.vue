<script setup lang="ts">
// 编辑区标签栏（VSCode 式 tabs）：点击切换、右侧 x 关闭、HTML5 拖拽排序。
// 只有一个（或没有）文稿时整条隐藏。
import { ref } from 'vue'
import { projectStore } from '../stores/project'
import Icon from './Icon.vue'

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
  <!-- 编辑区顶行常驻：左侧标签页（溢出横向滚动），右侧状态（保存状态 + 字数）固定不被挤压 -->
  <div class="tabs-row">
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
    <div class="row-spacer" />
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
.tabs-row {
  flex: none;
  display: flex;
  align-items: center;
  height: 32px;
  padding: 0 8px;
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

.row-spacer {
  flex: none;
  min-width: 8px;
}

/* 右侧状态区：固定不被标签挤压；margin-left:auto 兜底无标签时也靠右（此时标签区 v-if 掉，无 flex:1 占位） */
.editor-status {
  flex: none;
  margin-left: auto;
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
