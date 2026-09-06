<!-- 手机预览浮层：把当前文稿渲染成手机阅读视图，作者据此检查目标平台排版。
     内容取 projectStore.currentContent（编辑器实时同步，含未保存输入），边写边看。
     markdown 文稿渲染排版；纯文本（.txt）按原样显示（不分段不解析）。
     排版用预览专用设置（--preview-line-height/--preview-para-gap，设置面板可调），字体/字号沿用正文。 -->
<script setup lang="ts">
import { computed } from 'vue'
import { projectStore } from '../stores/project'
import { renderMarkdown } from '../utils/mdRender'
import { editorParaMode, previewFirstLineIndent } from '../utils/font'
import { isPlainSheet, stripSheetExt } from '../utils/sheet'
import { useDialogMask } from '../composables/useDialog'
import Icon from './Icon.vue'

const emit = defineEmits<{ close: [] }>()
const { onMaskDown, onMaskUp } = useDialogMask(() => emit('close'))

/** 窗口高度不足时收缩手机框（844 = iPhone 逻辑高度，宽 390） */
const phoneHeight = 'min(844px, calc(100vh - 72px))'

const docTitle = computed(() => stripSheetExt(projectStore.currentFile?.split('/').at(-1) ?? ''))

/** 纯文本（.txt）：预览不解析 markdown，按原样纯文本显示 */
const plain = computed(() => isPlainSheet(projectStore.currentFile ?? ''))

const html = computed(() => renderMarkdown(projectStore.currentContent, editorParaMode.value))
</script>

<template>
  <div class="mask" @pointerdown="onMaskDown" @pointerup="onMaskUp">
    <div class="phone" :style="{ height: phoneHeight }">
      <!-- 顶部小条：兼作文稿名与关闭 -->
      <div class="phone-head">
        <span class="phone-title">{{ docTitle }}</span>
        <button class="phone-close" title="关闭预览" @click="emit('close')">
          <Icon name="close" :size="12" />
        </button>
      </div>
      <!-- 阅读视图（markdown 渲染 / 纯文本原样显示）；indent = 首行缩进 -->
      <!-- eslint-disable-next-line vue/no-v-html -->
      <div v-if="!plain" v-overlay-scrollbar class="reading" :class="{ indent: previewFirstLineIndent }" v-html="html" />
      <div v-else v-overlay-scrollbar class="reading plain">{{ projectStore.currentContent }}</div>
    </div>
  </div>
</template>

<style scoped>
.mask {
  position: fixed;
  inset: 0;
  z-index: 200;
  background: rgba(0, 0, 0, 0.25);
  display: flex;
  align-items: center;
  justify-content: center;
}

/* 手机框：390 逻辑宽，高度超出窗口时收缩 */
.phone {
  width: 390px;
  max-width: calc(100vw - 48px);
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border);
  border-radius: 24px;
  background: var(--bg);
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.2);
  overflow: hidden;
}

.phone-head {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  height: 40px;
  padding: 0 8px 0 16px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-soft);
}

.phone-title {
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.phone-close {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}

@media (hover: hover) {
  .phone-close:hover {
    background: var(--border);
    color: var(--text);
  }
}

/* 阅读视图：预览专用行距/段距（--preview-*，设置面板可调），字体/字号沿用正文设置 */
.reading {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 24px 24px 32px;
  font-family: var(--font-editor);
  font-size: calc(17px * var(--font-scale-editor));
  line-height: var(--preview-line-height);
}

/* 纯文本（.txt）：按原样显示 */
.reading.plain {
  white-space: pre-wrap;
  word-break: break-word;
}

/* 首行缩进（设置面板开关）：段落首行缩进 2 字符；列表内段落除外 */
.reading.indent :deep(p) {
  text-indent: 2em;
}

.reading.indent :deep(li p) {
  text-indent: 0;
}

.reading :deep(p) {
  margin: 0 0 var(--preview-para-gap);
}

.reading :deep(p:last-child) {
  margin-bottom: 0;
}

/* 标题不放大（网文排版）：与编辑器实时预览一致，只加粗不改字号 */
.reading :deep(h1),
.reading :deep(h2),
.reading :deep(h3),
.reading :deep(h4),
.reading :deep(h5),
.reading :deep(h6) {
  margin: calc(var(--preview-para-gap) * 2) 0 var(--preview-para-gap);
  font-weight: 700;
  font-size: 1em;
  line-height: var(--preview-line-height);
}

.reading :deep(h1:first-child),
.reading :deep(h2:first-child),
.reading :deep(h3:first-child),
.reading :deep(h4:first-child),
.reading :deep(h5:first-child),
.reading :deep(h6:first-child) {
  margin-top: 0;
}

.reading :deep(blockquote) {
  margin: 0 0 var(--preview-para-gap);
  padding-left: 12px;
  border-left: 2px solid color-mix(in srgb, var(--text-dim) 30%, transparent);
}

.reading :deep(blockquote p:last-child) {
  margin-bottom: 0;
}

.reading :deep(ul),
.reading :deep(ol) {
  margin: 0 0 var(--preview-para-gap);
  padding-left: 1.5em;
}

.reading :deep(li p) {
  margin-bottom: 0;
}

.reading :deep(code) {
  font-family: ui-monospace, Consolas, monospace;
  font-size: 0.9em;
  background: var(--bg-soft);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 0 4px;
}

.reading :deep(pre) {
  margin: 0 0 var(--preview-para-gap);
  padding: 10px 12px;
  background: var(--bg-soft);
  border-radius: 8px;
  overflow-x: auto;
}

.reading :deep(pre code) {
  background: none;
  border: none;
  padding: 0;
}

.reading :deep(hr) {
  border: none;
  border-top: 1px solid var(--border);
  margin: calc(var(--preview-para-gap) * 1.5) 0;
}

.reading :deep(a) {
  color: var(--accent);
}
</style>
