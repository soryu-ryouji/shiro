<script setup lang="ts">
// Markdown 编辑器（CodeMirror 6）：防抖自动保存（daemon 原子写入）。
// 底部工具栏（Ulysses 式）：块级标记（标题/列表/引用）切换居中，字数（有选中时为「选中 / 总数」）与保存失败提示在右端。
// 切换文稿/卸载前先 flush 保存；磁盘外部变动（其他编辑器/同步盘/AI 写稿）经 daemon 监听推送，由 onExternalFileChange 回调重载。
// 排版对齐：正文首行与次栏列表首项同高（顶栏 40px + 标签页条 36px + 间距 24px = .cm-content 上内边距 24px）。
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { apiFetch } from '../api'
import { projectStore, onExternalFileChange } from '../stores/project'
import { countWords } from '../utils/wordcount'
import { editorParaMode } from '../utils/font'
import { livePreview, FENCE_RE, HEADING_RE } from '../utils/livePreview'
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

// ---- 选中字数：有选区时浮层显示「选中 / 总数」（与总数同一统计策略） ----
const selectedCount = ref(0)

function countSelection(state: EditorState): number {
  let n = 0
  for (const r of state.selection.ranges) {
    if (!r.empty) n += countWords(state.sliceDoc(r.from, r.to))
  }
  return n
}

// ---- 底部工具栏 ----
// 行首标记切换（选区覆盖的所有行：全部已带则统一移除，否则统一补齐）
const LIST_MARK_RE = /^- /
const QUOTE_MARK_RE = /^> /

function toggleLineMark(re: RegExp, mark: string) {
  if (!view) return
  const state = view.state
  const main = state.selection.main
  const first = state.doc.lineAt(main.from).number
  const last = state.doc.lineAt(main.to).number
  const lines = Array.from({ length: last - first + 1 }, (_, i) => state.doc.line(first + i))
  const allMarked = lines.every((l) => re.test(l.text))
  const changes: { from: number; to?: number; insert?: string }[] = []
  for (const l of lines) {
    const m = l.text.match(re)
    if (allMarked) {
      changes.push({ from: l.from, to: l.from + (m?.[0].length ?? 0) })
    } else if (!m) {
      changes.push({ from: l.from, insert: mark })
    }
  }
  const cs = state.changes(changes)
  // selection.map 用 assoc=1：行首恰在光标处的插入把光标推到标记之后（可直接输入）
  view.dispatch({ changes: cs, selection: state.selection.map(cs, 1) })
}

/** 标题层级循环（选区每行独立）：无 → 一级 → 二级 → 三级 → 四级 → 移除 */
function cycleHeading() {
  if (!view) return
  const state = view.state
  const main = state.selection.main
  const first = state.doc.lineAt(main.from).number
  const last = state.doc.lineAt(main.to).number
  const changes: { from: number; to?: number; insert?: string }[] = []
  for (let n = first; n <= last; n++) {
    const line = state.doc.line(n)
    const m = line.text.match(/^#{1,6}(?: |$)/)
    const level = m ? m[0].trimEnd().length : 0
    if (level === 0) {
      changes.push({ from: line.from, insert: '# ' })
    } else if (level >= 4) {
      changes.push({ from: line.from, to: line.from + (m?.[0].length ?? 0) })
    } else {
      changes.push({ from: line.from, to: line.from + (m?.[0].length ?? 0), insert: `${'#'.repeat(level + 1)} ` })
    }
  }
  const cs = state.changes(changes)
  view.dispatch({ changes: cs, selection: state.selection.map(cs, 1) })
}

/** 加粗切换：有选区包选区（已包裹或身处 ** 内则解除）；无选区作用于整行内容（空行插入 **** 并把光标放进中间） */
function toggleBold() {
  if (!view) return
  const state = view.state
  const sel = state.selection.main
  if (sel.empty) {
    const line = state.doc.lineAt(sel.from)
    const m = line.text.match(/^(\s*)(.*?)(\s*)$/)!
    const content = m[2] ?? ''
    const contentFrom = line.from + (m[1]?.length ?? 0)
    const contentTo = contentFrom + content.length
    if (!content) {
      view.dispatch({ changes: { from: line.from, to: line.to, insert: '****' }, selection: { anchor: line.from + 2 } })
      return
    }
    if (content.startsWith('**') && content.endsWith('**') && content.length > 4) {
      view.dispatch({
        changes: [
          { from: contentFrom, to: contentFrom + 2 },
          { from: contentTo - 2, to: contentTo },
        ],
      })
    } else {
      view.dispatch({
        changes: [
          { from: contentFrom, insert: '**' },
          { from: contentTo, insert: '**' },
        ],
      })
    }
    return
  }
  const { from, to } = sel
  const text = state.sliceDoc(from, to)
  if (text.startsWith('**') && text.endsWith('**') && text.length > 4) {
    // 选区自带标记
    view.dispatch({
      changes: [
        { from, to: from + 2 },
        { from: to - 2, to },
      ],
    })
  } else if (state.sliceDoc(Math.max(0, from - 2), from) === '**' && state.sliceDoc(to, to + 2) === '**') {
    // 选区被标记包围（如选中 **foo** 中的 foo）
    view.dispatch({
      changes: [
        { from: from - 2, to: from },
        { from: to, to: to + 2 },
      ],
    })
  } else {
    view.dispatch({
      changes: [
        { from, insert: '**' },
        { from: to, insert: '**' },
      ],
    })
  }
}

// ---- 大纲：文档标题列表（跳过代码围栏）；宽度足够时浮在右侧留白（不占布局），点击跳转、当前章节高亮 ----
interface OutlineItem {
  level: number
  text: string
  pos: number
}
const outline = ref<OutlineItem[]>([])
const outlineActive = ref(-1)

function collectOutline(state: EditorState) {
  const items: OutlineItem[] = []
  let inFence = false
  for (let n = 1; n <= state.doc.lines; n++) {
    const line = state.doc.line(n)
    if (FENCE_RE.test(line.text)) {
      inFence = !inFence
      continue
    }
    if (inFence) continue
    const m = line.text.match(HEADING_RE)
    if (m) items.push({ level: m[1].length, text: line.text.slice(m[0].length), pos: line.from })
  }
  outline.value = items
  updateOutlineActive(state)
}

/** 当前章节：光标上方最近的一个标题 */
function updateOutlineActive(state: EditorState) {
  const head = state.selection.main.head
  let pos = -1
  for (const it of outline.value) {
    if (it.pos <= head) pos = it.pos
    else break
  }
  outlineActive.value = pos
}

function jumpToHeading(pos: number) {
  if (!view) return
  view.dispatch({
    selection: { anchor: pos },
    effects: EditorView.scrollIntoView(pos, { y: 'start', yMargin: 12 }),
  })
  view.focus()
}

// 大纲面板只在右侧留白放得下时显示（浮于留白、可探入正文列内边距，不压文字 → 阈值 1040px）
const wrapEl = ref<HTMLElement | null>(null)
const outlineVisible = ref(false)
let resizeObserver: ResizeObserver | null = null

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
        if (update.selectionSet || update.docChanged) selectedCount.value = countSelection(update.state)
        if (update.docChanged) collectOutline(update.state)
        else if (update.selectionSet) updateOutlineActive(update.state)
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
    collectOutline(view.state)
    selectedCount.value = 0
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
  resizeObserver = new ResizeObserver((entries) => {
    outlineVisible.value = (entries[0]?.contentRect.width ?? 0) >= 1040
  })
  resizeObserver.observe(wrapEl.value!)
  if (projectStore.currentFile) void loadFile()
})
watch(
  () => projectStore.currentFile,
  () => void loadFile(),
)
onUnmounted(() => {
  onExternalFileChange(null)
  resizeObserver?.disconnect()
  void flushSave()
  view?.destroy()
})
</script>

<template>
  <div ref="wrapEl" class="editor-wrap">
    <!-- 编辑器容器常驻（v-show 控制显隐）：CodeMirror 视图在 onMounted 即挂载，
         放进 v-if 分支会因挂载时机晚于视图创建而导致 DOM 不渲染 -->
    <div ref="editorEl" v-show="projectStore.currentFile" class="editor" />
    <div v-if="!projectStore.currentFile" class="editor-empty">从中间列表选择文稿，或新建一篇</div>
    <!-- 大纲（Notion 式）：宽度足够时浮在右侧留白（绝对定位，不占布局；无边框无底色）；
         按级别缩进，点击跳转，当前章节高亮 -->
    <nav v-if="outlineVisible && outline.length" class="outline">
      <button
        v-for="it in outline"
        :key="it.pos"
        class="outline-item"
        :class="{ active: it.pos === outlineActive }"
        :style="{ paddingLeft: `${(it.level - 1) * 12 + 8}px` }"
        @click="jumpToHeading(it.pos)"
      >
        {{ it.text || '（无标题）' }}
      </button>
    </nav>
    <!-- 底部工具栏（Ulysses 式）：块级标记切换居中；字数与保存失败提示在右端。
         按钮用 mousedown.prevent：不夺编辑器焦点，选区与输入状态不中断 -->
    <div v-if="projectStore.currentFile" class="editor-bar">
      <button class="bar-item" title="标题（连点在一至四级间切换）" @mousedown.prevent @click="cycleHeading"><span class="mark">#</span>标题</button>
      <button class="bar-item" title="加粗（无选区时作用于整行）" @mousedown.prevent @click="toggleBold"><span class="mark">**</span>加粗</button>
      <button class="bar-item" title="列表" @mousedown.prevent @click="toggleLineMark(LIST_MARK_RE, '- ')"><span class="mark">-</span>列表</button>
      <button class="bar-item" title="引用" @mousedown.prevent @click="toggleLineMark(QUOTE_MARK_RE, '> ')"><span class="mark">&gt;</span>引用</button>
      <div class="editor-status">
        <span v-if="projectStore.saveState === 'error'" class="save-error">保存失败 · </span>
        <span><template v-if="selectedCount">{{ selectedCount }} / </template>{{ projectStore.wordCount }} 字</span>
      </div>
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
.editor :deep(.md-gmark) {
  display: inline-block;
  width: 2em;
  margin-left: -2em;
  padding-right: 0.6em;
  box-sizing: border-box;
  text-align: right;
  font-weight: 400;
  color: var(--text-dim);
}

/* 引用（极简）：> 记号隐藏，行内细竖线贴文本块左缘，正文不加颜色/斜体 */
.editor :deep(.md-quote) {
  position: relative;
  padding-left: 12px;
}
.editor :deep(.md-quote)::before {
  content: '';
  position: absolute;
  left: 0;
  top: 3px;
  bottom: 3px;
  width: 2px;
  border-radius: 1px;
  background: color-mix(in srgb, var(--text-dim) 30%, transparent);
}

/* 段间距行的竖线起点下移，只包住文本区（行盒含 padding-top，否则会富到上一行空间） */
.editor :deep(.md-quote.cm-para-start)::before {
  top: calc(var(--editor-para-gap) + 3px);
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

/* 底部工具栏（Ulysses 式）：块级标记按钮居中，字数右端 */
.editor-bar {
  position: relative;
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  height: 36px;
  user-select: none;
}

/* 顶部分隔线：与正文列同宽（760px）居中，左右不封闭（Ulysses 式） */
.editor-bar::before {
  content: '';
  position: absolute;
  top: 0;
  left: 50%;
  transform: translateX(-50%);
  width: min(760px, calc(100% - 48px));
  height: 1px;
  background: var(--border);
}

.bar-item {
  display: flex;
  align-items: center;
  gap: 5px;
  height: 26px;
  padding: 0 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text-dim);
  cursor: pointer;
}

.bar-item .mark {
  font-family: ui-monospace, Consolas, monospace;
}

@media (hover: hover) {
  .bar-item:hover {
    background: var(--bg-soft);
    color: var(--text);
  }
}

/* 大纲：浮在正文列右侧留白（绝对定位，无边框无底色、不占布局）；顶部与正文首行同高 */
.outline {
  position: absolute;
  top: 24px;
  right: 12px;
  z-index: 10;
  width: 160px;
  max-height: calc(100% - 60px);
  padding: 4px;
  display: flex;
  flex-direction: column;
  gap: 1px;
  overflow-y: auto;
  scrollbar-width: thin;
  user-select: none;
}

.outline-item {
  padding: 4px 8px;
  border: none;
  border-radius: 5px;
  background: none;
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--text-dim);
  text-align: left;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  cursor: pointer;
}

@media (hover: hover) {
  .outline-item:hover {
    background: var(--bg-soft);
    color: var(--text);
  }
}

.outline-item.active {
  color: var(--accent);
  font-weight: 600;
}

/* 字数与保存失败提示：工具栏右端（绝对定位，不挤压按钮居中） */
.editor-status {
  position: absolute;
  top: 50%;
  right: 12px;
  transform: translateY(-50%);
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
  white-space: nowrap;
}

.save-error {
  color: var(--danger);
}
</style>
