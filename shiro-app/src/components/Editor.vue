<script setup lang="ts">
// Markdown 编辑器（CodeMirror 6）：防抖自动保存（daemon 原子写入）、字数与保存状态（右上角，Ulysses 风格）。
// 注意：切换文稿/卸载前先 flush 保存；外部修改（其他编辑器/同步盘）的监听同步待 daemon watcher 引入后做。
import { onMounted, onUnmounted, ref, watch } from 'vue'
import { apiFetch } from '../api'
import { projectStore } from '../stores/project'
import { countWords } from '../utils/wordcount'
import type { components } from '../api-types'
import { EditorView, keymap, drawSelection } from '@codemirror/view'
import { EditorState } from '@codemirror/state'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { markdown, markdownLanguage } from '@codemirror/lang-markdown'
import { languages } from '@codemirror/language-data'

type FileContent = components['schemas']['FileContent']

const editorEl = ref<HTMLElement | null>(null)
let view: EditorView | null = null

const loading = ref(false)
const wordCount = ref(0)
const saveState = ref<'saved' | 'saving' | 'error'>('saved')
/** 保存状态文字是否可见（保存活动瞬时反馈，自动淡出；失败常驻） */
const statusVisible = ref(false)
let hideTimer: ReturnType<typeof setTimeout> | null = null

watch(saveState, (s) => {
  if (hideTimer) clearTimeout(hideTimer)
  if (s === 'saved') {
    hideTimer = setTimeout(() => (statusVisible.value = false), 1600)
  } else {
    statusVisible.value = true
  }
})

/** 待保存内容与其所属文稿（切文稿时 pending 仍指向旧文件，保证不串写） */
let pending: { file: string; content: string } | null = null
let saveTimer: ReturnType<typeof setTimeout> | null = null

async function flushSave() {
  if (!pending || !projectStore.current) return
  const { file, content } = pending
  pending = null
  saveState.value = 'saving'
  try {
    await apiFetch<FileContent>('/api/v1/projects/file', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: projectStore.current.path, file, content }),
    })
    // 保存期间又有输入（pending 重新有值）时保持 saving，等下一轮 flush
    saveState.value = pending === null ? 'saved' : 'saving'
  } catch {
    saveState.value = 'error'
  }
}

function scheduleSave(content: string) {
  const file = projectStore.currentFile
  if (!file) return
  pending = { file, content }
  wordCount.value = countWords(content)
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => void flushSave(), 800)
}

const theme = EditorView.theme({
  '&': { fontSize: '15px', backgroundColor: 'transparent' },
  '.cm-content': {
    fontFamily: "'Georgia', 'Songti SC', 'Microsoft YaHei', serif",
    padding: '28px 32px',
    maxWidth: '760px',
    margin: '0 auto',
  },
  '.cm-scroller': { lineHeight: '1.8', overflow: 'auto' },
  '&.cm-focused': { outline: 'none' },
  '.cm-cursor': { borderLeftColor: 'var(--text)' },
})

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
        if (update.docChanged) scheduleSave(update.state.doc.toString())
      }),
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
    wordCount.value = countWords(res.content ?? '')
    pending = null
    saveState.value = 'saved'
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  view = new EditorView({ state: makeState(''), parent: editorEl.value! })
  if (projectStore.currentFile) void loadFile()
})
watch(
  () => projectStore.currentFile,
  () => void loadFile(),
)
onUnmounted(() => {
  void flushSave()
  view?.destroy()
})
</script>

<template>
  <div class="editor-wrap">
    <!-- 编辑器容器常驻（v-show 控制显隐）：CodeMirror 视图在 onMounted 即挂载，
         放进 v-if 分支会因挂载时机晚于视图创建而导致 DOM 不渲染 -->
    <div ref="editorEl" v-show="projectStore.currentFile" class="editor" />
    <div v-if="projectStore.currentFile" class="editor-status">
      <template v-if="statusVisible">
        <span v-if="saveState === 'saving'">保存中…</span>
        <span v-else-if="saveState === 'error'" class="save-error">保存失败</span>
        <span v-else>已保存</span>
        <span class="sep">·</span>
      </template>
      <span>{{ wordCount }} 字</span>
    </div>
    <div v-if="!projectStore.currentFile" class="editor-empty">从中间列表选择文稿，或新建一篇</div>
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

.editor-status {
  position: absolute;
  top: 10px;
  right: 16px;
  z-index: 10;
  display: flex;
  gap: 6px;
  font-size: 11px;
  color: var(--text-dim);
  user-select: none;
  pointer-events: none;
}

.save-error {
  color: var(--danger);
}

.editor-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-dim);
  font-size: 13px;
}
</style>
