<script setup lang="ts">
// 中栏文稿列表（Ulysses 第二栏）：选中目录的直接 .md 文稿；顶部目录名 + 右侧新建（内联命名）。
import { computed, nextTick, ref } from 'vue'
import { apiFetch } from '../api'
import { findDir, projectStore } from '../stores/project'
import Icon from './Icon.vue'

/** 选中目录的直接文稿（file 节点） */
const sheets = computed(() =>
  (findDir(projectStore.tree, projectStore.selectedDir) ?? []).filter((n) => n.kind === 'file'),
)

function fmtTime(epochSecs?: number | null): string {
  if (!epochSecs) return ''
  const d = new Date(epochSecs * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** 文稿显示名：去 .md/.markdown 后缀 */
function sheetTitle(name: string): string {
  return name.replace(/\.(md|markdown)$/i, '')
}

// ---- 新建文稿：列表顶行内联输入（Enter 创建，Esc 取消） ----
const naming = ref(false)
const newName = ref('')
const nameInput = ref<HTMLInputElement | null>(null)
const createError = ref('')

async function startNaming() {
  naming.value = true
  createError.value = ''
  newName.value = ''
  await nextTick()
  nameInput.value?.focus()
}

async function confirmNaming() {
  const name = newName.value.trim()
  if (!name) {
    naming.value = false
    return
  }
  const project = projectStore.current
  if (!project) return
  const file = `${projectStore.selectedDir ? projectStore.selectedDir + '/' : ''}${name}.md`
  try {
    await apiFetch('/api/v1/projects/file', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: project.path, file }),
    })
    naming.value = false
    await projectStore.refreshTree()
    projectStore.currentFile = file
  } catch (e) {
    createError.value = e instanceof Error ? e.message : String(e)
  }
}
</script>

<template>
  <div class="sheet-list">
    <!-- 无独立头部横条：新建入口为列表首行（内联命名行同位置展开） -->
    <div class="top-row">
      <div v-if="naming" class="naming-row">
        <input
          ref="nameInput"
          v-model="newName"
          class="naming-input"
          type="text"
          placeholder="文稿名"
          @keydown.enter="confirmNaming"
          @keydown.esc="naming = false"
          @blur="confirmNaming"
        />
        <p v-if="createError" class="naming-error">{{ createError }}</p>
      </div>
      <button v-else class="new-row" @click="startNaming">
        <Icon name="plus" :size="13" />
        <span>新建文稿</span>
      </button>
    </div>

    <ul v-if="sheets.length" class="sheets">
      <li
        v-for="s in sheets"
        :key="s.path"
        class="sheet"
        :class="{ active: projectStore.currentFile === s.path }"
        @click="projectStore.currentFile = s.path"
      >
        <span class="sheet-time">{{ fmtTime(s.modified) }}</span>
        <span class="sheet-title">{{ sheetTitle(s.name) }}</span>
      </li>
    </ul>
    <p v-else-if="!naming" class="empty-hint">此目录还没有文稿</p>
  </div>
</template>

<style scoped>
.sheet-list {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.top-row {
  flex: none;
  padding: 8px 8px 4px;
}

.new-row {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 6px 10px;
  border: none;
  border-radius: 6px;
  background: none;
  font-size: 13px;
  color: var(--text-dim);
  cursor: pointer;
}

@media (hover: hover) {
  .new-row:hover {
    background: var(--bg-soft);
    color: var(--text);
  }
}

.naming-row {
  padding: 0;
}

.naming-input {
  width: 100%;
  height: 28px;
  padding: 0 8px;
  border: 1px solid var(--accent);
  border-radius: 5px;
  font-size: 13px;
  outline: none;
}

.naming-error {
  margin: 6px 0 0;
  font-size: 12px;
  color: var(--danger);
}

.sheets {
  flex: 1;
  list-style: none;
  margin: 0;
  padding: 8px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.sheet {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 10px;
  border-radius: 6px;
  cursor: pointer;
  user-select: none;
}

@media (hover: hover) {
  .sheet:hover {
    background: var(--bg-soft);
  }
}

.sheet.active {
  background: var(--accent-soft);
}

.sheet-time {
  font-size: 11px;
  color: var(--text-dim);
}

.sheet-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.empty-hint {
  padding: 16px 12px;
  color: var(--text-dim);
  font-size: 12px;
}
</style>
