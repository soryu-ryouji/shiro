<script setup lang="ts">
// 中栏文稿列表（Ulysses 第二栏）：选中目录的直接 .md 文稿；顶部目录名 + 右侧新建（内联命名）。
import { computed, nextTick, ref } from 'vue'
import { apiFetch } from '../api'
import { hasShell, shell } from '../platform'
import { findDir, projectStore } from '../stores/project'
import ContextMenu, { type MenuItem } from './ContextMenu.vue'
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
  newName.value = '' // Enter 确认后输入框卸载会再触发 blur，先清空防二次提交
  naming.value = false
  if (!name) return
  const project = projectStore.current
  if (!project) return
  const file = `${projectStore.selectedDir ? projectStore.selectedDir + '/' : ''}${name}.md`
  try {
    await apiFetch('/api/v1/projects/file', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: project.path, file }),
    })
    await projectStore.refreshTree()
    projectStore.openFile(file)
  } catch (e) {
    createError.value = e instanceof Error ? e.message : String(e)
  }
}

// ---- 文稿右键菜单 ----
const sheetMenu = ref<{ x: number; y: number; path: string } | null>(null)

function openSheetMenu(e: MouseEvent, path: string) {
  e.preventDefault()
  sheetMenu.value = { x: e.clientX, y: e.clientY, path }
}

function sheetMenuItems(path: string): MenuItem[] {
  const items: MenuItem[] = []
  if (!projectStore.tabs.includes(path)) {
    items.push({ label: '添加到编辑器标题栏', action: () => projectStore.pinFile(path) })
  }
  items.push({ label: '重命名…', action: () => startRename(path) })
  // 文件管理器能力是桌面壳能力，局域网浏览器形态没有
  if (hasShell) {
    items.push({
      label: '在文件夹中打开',
      action: () => void shell.showInFolder(`${projectStore.current?.path}/${path}`),
    })
  }
  items.push({
    label: '删除（移至回收站）',
    danger: true,
    action: () => void projectStore.removeFile(path),
  })
  return items
}

// ---- 文稿重命名：列表项就地变为输入框 ----
const renamingFile = ref<string | null>(null)
const renameValue = ref('')
const renameInput = ref<HTMLInputElement | null>(null)

function startRename(path: string) {
  renamingFile.value = path
  renameValue.value = sheetTitle(path.split('/').at(-1) ?? path)
  void nextTick(() => {
    renameInput.value?.focus()
    renameInput.value?.select()
  })
}

async function confirmRename(path: string) {
  const base = renameValue.value.trim().replace(/[/\\]/g, '')
  const oldName = path.split('/').at(-1) ?? ''
  const suffix = oldName.match(/\.(md|markdown)$/i)?.[0] ?? '.md'
  renamingFile.value = null
  if (!base || base + suffix === oldName) return
  await projectStore.renameEntry(path, base + suffix)
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
        @click="projectStore.openFile(s.path)"
        @contextmenu="openSheetMenu($event, s.path)"
      >
        <input
          v-if="renamingFile === s.path"
          ref="renameInput"
          v-model="renameValue"
          class="rename-input"
          type="text"
          @click.stop
          @keydown.enter="confirmRename(s.path)"
          @keydown.esc="renamingFile = null"
          @blur="confirmRename(s.path)"
        />
        <template v-else>
          <span class="sheet-time">{{ fmtTime(s.modified) }}</span>
          <span class="sheet-title">{{ sheetTitle(s.name) }}</span>
        </template>
      </li>
    </ul>
    <p v-else-if="!naming" class="empty-hint">此目录还没有文稿</p>

    <ContextMenu
      v-if="sheetMenu"
      :x="sheetMenu.x"
      :y="sheetMenu.y"
      :items="sheetMenuItems(sheetMenu.path)"
      @close="sheetMenu = null"
    />
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
  font-size: calc(13px * var(--font-scale-ui));
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
  font-size: calc(13px * var(--font-scale-ui));
  outline: none;
}

.naming-error {
  margin: 6px 0 0;
  font-size: calc(12px * var(--font-scale-ui));
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
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
}

.sheet-title {
  font-size: calc(13px * var(--font-scale-ui));
  font-weight: 600;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.rename-input {
  width: 100%;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--accent);
  border-radius: 5px;
  font-size: calc(13px * var(--font-scale-ui));
  outline: none;
}

.empty-hint {
  padding: 16px 12px;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}
</style>
