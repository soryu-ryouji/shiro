<script setup lang="ts">
// Markdown 编辑器（CodeMirror 6）：防抖自动保存（daemon 原子写入）；字数与保存失败提示在右上角浮层（不占布局高度，无常驻保存提示——类 VSCode）。
// 切换文稿/卸载前先 flush 保存；磁盘外部变动（其他编辑器/同步盘/AI 写稿）经 daemon 监听推送，由 onExternalFileChange 回调重载。
// 排版对齐：正文首行与次栏列表首项同高（顶栏 40px + 标签页条 36px + 间距 24px = .cm-content 上内边距 24px）。
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { apiFetch } from '../api'
import { projectStore, onExternalFileChange } from '../stores/project'
import { countWords } from '../utils/wordcount'
import { editorParaMode } from '../utils/font'
import { livePreview } from '../utils/livePreview'
import type { components } from '../api-types'
import { EditorView, keymap, drawSelection, ViewPlugin, Decoration, type DecorationSet, type ViewUpdate } from '@codemirror/view'
import { EditorState, RangeSetBuilder } from '@codemirror/state'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { markdown, markdownLanguage } from '@codemirror/lang-markdown'
import { languages } from '@codemirror/language-data'

type FileContent = components['schemas']['FileContent']

const editorEl = ref<HTMLElement | null>(null)
let view: EditorView | null = null

const loading = ref(false)

/** 待保存内容与其所属文稿（切文稿时 pending 仍指向旧文件，保证不串写） */
let pending: { file: string; content: string } | null = null
let saveTimer: ReturnType<typeof setTimeout> | null = null

async function flushSave() {
  if (!pending || !projectStore.current) return
  const { file, content } = pending
  pending = null
  projectStore.saveState = 'saving'
  try {
    await apiFetch<FileContent>('/api/v1/projects/file', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: projectStore.current.path, file, content }),
    })
    // 保存期间又有输入（pending 重新有值）时保持 saving，等下一轮 flush
    projectStore.saveState = pending === null ? 'saved' : 'saving'
  } catch {
    projectStore.saveState = 'error'
  }
}

function scheduleSave(content: string) {
  const file = projectStore.currentFile
  if (!file) return
  pending = { file, content }
  projectStore.wordCount = countWords(content)
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => void flushSave(), 800)
}

// ---- 磁盘外部变动重载（daemon 文件监听，接线见 stores/project.ts）----
// 冲突策略：该文稿有未保存内容时本地优先（下次 flush 覆盖磁盘），否则静默替换
let suppressSave = false

function applyExternalChange(file: string, content: string | null) {
  if (!view) return
  if (pending && pending.file === file) return
  if (content === null) {
    // 外部删除/移走：磁盘为准，关闭标签
    projectStore.closeTab(file)
    return
  }
  if (view.state.doc.toString() === content) return
  suppressSave = true
  // 全量替换为一次变更：光标随变更映射，滚动与撤销历史保留；屏蔽自动保存（内容与磁盘一致，无需回写）
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: content } })
  suppressSave = false
  projectStore.wordCount = countWords(content)
}

const theme = EditorView.theme({
  // 字号基数须与 font.ts EDITOR_FONT_SIZE_DEFAULT 一致（渲染值 = 基数 × 设定值 ÷ 默认值）
  '&': { fontSize: 'calc(17px * var(--font-scale-editor))', backgroundColor: 'transparent' },
  '.cm-content': {
    fontFamily: 'var(--font-editor)',
    padding: '24px 32px 28px',
    maxWidth: '760px',
    margin: '0 auto',
  },
  '.cm-scroller': { lineHeight: 'var(--editor-line-height)', overflow: 'auto' },
  // 段间距：段落首行的额外上间距（--editor-para-gap 默认 15px）
  '.cm-line.cm-para-start': { paddingTop: 'var(--editor-para-gap)' },
  '&.cm-focused': { outline: 'none' },
  '.cm-cursor': { borderLeftColor: 'var(--text)' },
})

// ---- 段间距装饰：段落间距只加在「新段落首行」上，哪些行算新段落由分段方式决定（editorParaMode）；
// 段内行距管折行密度，段落间距管段落之间的额外距离，两者独立 ----
const paraStartDeco = Decoration.line({ class: 'cm-para-start' })

function buildParaDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>()
  const doc = view.state.doc
  const blankSeparated = editorParaMode.value === 'blank'
  for (const { from, to } of view.visibleRanges) {
    const first = Math.max(doc.lineAt(from).number, 2)
    const last = doc.lineAt(to).number
    for (let n = first; n <= last; n++) {
      const line = doc.line(n)
      if (blankSeparated) {
        // 空行分段：空行本身不加（分隔由空行行高承担），仅空行后的新段落首行加
        if (line.text.trim() === '') continue
        if (doc.line(n - 1).text.trim() !== '') continue
      }
      // 回车分段：每行都加，含刚回车出的空行——否则光标紧贴上一行，打出字后才跳下去
      builder.add(line.from, line.from, paraStartDeco)
    }
  }
  return builder.finish()
}

const paraSpacingPlugin = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet
    mode: string
    constructor(view: EditorView) {
      this.mode = editorParaMode.value
      this.decorations = buildParaDecorations(view)
    }
    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged || this.mode !== editorParaMode.value) {
        this.mode = editorParaMode.value
        this.decorations = buildParaDecorations(update.view)
      }
    }
  },
  { decorations: (v) => v.decorations },
)

function makeState(doc: string): EditorState {
  return EditorState.create({
    doc,
    extensions: [
      history(),
      drawSelection(),
      markdown({ base: markdownLanguage, codeLanguages: languages }),
      EditorView.lineWrapping,
      keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
      EditorView.updateListener.of((update) => {
        if (update.docChanged && !suppressSave) scheduleSave(update.state.doc.toString())
      }),
      paraSpacingPlugin,
      livePreview(),
      theme,
    ],
  })
}

async function loadFile() {
  await flushSave()
  const file = projectStore.currentFile
  if (!view || !file || !projectStore.current) return
  loading.value = true
  try {
    const res = await apiFetch<FileContent>(
      `/api/v1/projects/file?path=${encodeURIComponent(projectStore.current.path)}&file=${encodeURIComponent(file)}`,
    )
    view.setState(makeState(res.content ?? ''))
    projectStore.wordCount = countWords(res.content ?? '')
    pending = null
    projectStore.saveState = 'saved'
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  view = new EditorView({ state: makeState(''), parent: editorEl.value! })
  onExternalFileChange(applyExternalChange)
  if (projectStore.currentFile) void loadFile()
})
watch(
  () => projectStore.currentFile,
  () => void loadFile(),
)
onUnmounted(() => {
  onExternalFileChange(null)
  void flushSave()
  view?.destroy()
})
</script>

<template>
  <div class="editor-wrap">
    <!-- 编辑器容器常驻（v-show 控制显隐）：CodeMirror 视图在 onMounted 即挂载，
         放进 v-if 分支会因挂载时机晚于视图创建而导致 DOM 不渲染 -->
    <div ref="editorEl" v-show="projectStore.currentFile" class="editor" />
    <div v-if="!projectStore.currentFile" class="editor-empty">从中间列表选择文稿，或新建一篇</div>
    <!-- 字数（常驻）与保存失败（仅出错时）：右上角浮层，不占布局高度 -->
    <div v-if="projectStore.currentFile" class="editor-status">
      <span v-if="projectStore.saveState === 'error'" class="save-error">保存失败 · </span>
      <span>{{ projectStore.wordCount }} 字</span>
    </div>
  </div>
</template>

<style scoped>
.editor-wrap {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.editor {
  flex: 1;
  min-height: 0;
}

.editor :deep(.cm-editor) {
  height: 100%;
}

/* ---- Markdown 实时预览（utils/livePreview.ts 生成 md-* 类） ---- */

/* 标题：只加粗不改字号；# 记号是行内挂件，负外边距挂进内容列左侧留白，与文本基线天然对齐 */
.editor :deep(.md-h) {
  font-weight: 700;
}
.editor :deep(.md-hmark) {
  display: inline-block;
  width: 2em;
  margin-left: -2em;
  padding-right: 0.6em;
  box-sizing: border-box;
  text-align: right;
  font-weight: 400;
  color: var(--text-dim);
}

/* 引用：竖线挂行左缘外，正文淡色斜体 */
.editor :deep(.md-quote) {
  position: relative;
  color: var(--text-dim);
  font-style: italic;
}
.editor :deep(.md-quote)::before {
  content: '';
  position: absolute;
  left: -20px;
  top: 0;
  bottom: 0;
  border-left: 3px solid var(--border);
}

/* 列表圆点 */
.editor :deep(.md-bullet) {
  color: var(--text-dim);
}

/* 行内格式 */
.editor :deep(.md-strong) {
  font-weight: 700;
}
.editor :deep(.md-em) {
  font-style: italic;
}
.editor :deep(.md-strike) {
  text-decoration: line-through;
}
.editor :deep(.md-code) {
  font-family: ui-monospace, Consolas, monospace;
  font-size: 0.9em;
  background: var(--bg-soft);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 0 4px;
}

/* 代码围栏 */
.editor :deep(.md-fence) {
  background: var(--bg-soft);
  font-family: ui-monospace, Consolas, monospace;
}

.editor-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-dim);
  font-size: calc(13px * var(--font-scale-ui));
}

/* 字数与保存失败提示：编辑器右上角浮层（pointer-events 放行下方文本点击） */
.editor-status {
  position: absolute;
  top: 0;
  right: 12px;
  z-index: 10;
  display: flex;
  align-items: center;
  gap: 6px;
  height: 22px;
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
  user-select: none;
  white-space: nowrap;
  pointer-events: none;
}

.save-error {
  color: var(--danger);
}
</style>
