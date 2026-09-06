<script setup lang="ts">
// 中栏文稿列表（Ulysses 第二栏）：选中目录的直接 .md 文稿。
// 顶栏（窗口拖拽区）：右侧筛选（按标题子串过滤）、排序（名称/修改时间升降序）与新建（列表首行内联命名）；下方目录名横带（左下对齐 + 底部横线）。
// 每项展示：时间 + 标题 + 正文预览（daemon excerpts 接口，剥离 markdown 取开头约 160 字，两行截断）。
import { computed, nextTick, ref, watch } from 'vue'
import { apiFetch } from '../api'
import { hasShell, isMac, shell } from '../platform'
import { findDir, projectStore, type TreeNode } from '../stores/project'
import type { components } from '../api-types'
import ContextMenu, { type MenuItem } from './ContextMenu.vue'
import Icon from './Icon.vue'

const props = defineProps<{ sidebarVisible: boolean }>()
const emit = defineEmits<{ 'toggle-sidebar': [] }>()

/** 侧栏收起时 macOS 红绿灯压顶栏左端（避让 78px） */
const reserveTraffic = computed(() => hasShell && isMac && !props.sidebarVisible)

/** 顶栏双击空白切换最大化（按钮/输入框上不触发） */
function onHeadDblClick(e: MouseEvent) {
  if ((e.target as HTMLElement).closest('button, input')) return
  void shell.toggleMaximizeWindow()
}

/** 顶栏标题：选中目录名（项目根为固定文案） */
const dirTitle = computed(() => (projectStore.selectedDir ? (projectStore.selectedDir.split('/').at(-1) ?? '') : '根目录'))

/** 选中目录的直接文稿（file 节点） */
const sheets = computed(() =>
  (findDir(projectStore.tree, projectStore.selectedDir) ?? []).filter((n) => n.kind === 'file'),
)

// ---- 正文预览：目录切换 / 树变化 / 保存完成时拉取目录预览列表 ----
const excerpts = ref<Record<string, string>>({})

async function refreshExcerpts() {
  const project = projectStore.current
  if (!project) {
    excerpts.value = {}
    return
  }
  try {
    const res = await apiFetch<components['schemas']['ExcerptsResponse']>(
      `/api/v1/projects/excerpts?path=${encodeURIComponent(project.path)}&dir=${encodeURIComponent(projectStore.selectedDir)}`,
    )
    const map: Record<string, string> = {}
    for (const it of res.excerpts ?? []) map[it.file] = it.excerpt
    excerpts.value = map
  } catch {
    excerpts.value = {}
  }
}

watch(
  () => [projectStore.selectedDir, projectStore.tree] as const,
  () => void refreshExcerpts(),
)
// 保存完成后同步该篇预览
watch(
  () => projectStore.saveState,
  (s) => {
    if (s === 'saved') void refreshExcerpts()
  },
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

// ---- 列表筛选：顶栏漏斗开关，按标题子串过滤（不区分大小写）；Esc 或空值失焦关闭 ----
const filtering = ref(false)
const filter = ref('')
const filterInput = ref<HTMLInputElement | null>(null)

async function toggleFilter() {
  filtering.value = !filtering.value
  if (!filtering.value) {
    filter.value = ''
    return
  }
  await nextTick()
  filterInput.value?.focus()
}

function onFilterBlur() {
  if (!filter.value.trim()) filtering.value = false
}

const shownSheets = computed(() => {
  const q = filter.value.trim().toLowerCase()
  const list = q ? sheets.value.filter((s) => sheetTitle(s.name).toLowerCase().includes(q)) : sheets.value
  return sortSheets(list, sheetSort.value)
})

// ---- 排序：顶栏右侧排序按钮弹出方式菜单（localStorage 持久化，全局生效） ----
type SheetSort = 'name-asc' | 'name-desc' | 'mtime-desc' | 'mtime-asc'
const SORT_KEY = 'shiro.sheetSort'
const SORT_OPTIONS: { key: SheetSort; label: string }[] = [
  { key: 'name-asc', label: '按名称 A → Z' },
  { key: 'name-desc', label: '按名称 Z → A' },
  { key: 'mtime-desc', label: '按修改时间 新 → 旧' },
  { key: 'mtime-asc', label: '按修改时间 旧 → 新' },
]
const storedSort = localStorage.getItem(SORT_KEY)
const sheetSort = ref<SheetSort>(SORT_OPTIONS.some((o) => o.key === storedSort) ? (storedSort as SheetSort) : 'name-asc')

/** 名称排序用 localeCompare + numeric：「第 2 章」排在「第 10 章」前 */
function sortSheets(list: TreeNode[], sort: SheetSort): TreeNode[] {
  const sorted = [...list]
  switch (sort) {
    case 'name-asc':
      return sorted.sort((a, b) => a.name.localeCompare(b.name, 'zh-CN', { numeric: true }))
    case 'name-desc':
      return sorted.sort((a, b) => b.name.localeCompare(a.name, 'zh-CN', { numeric: true }))
    case 'mtime-desc':
      return sorted.sort((a, b) => (b.modified ?? 0) - (a.modified ?? 0))
    case 'mtime-asc':
      return sorted.sort((a, b) => (a.modified ?? 0) - (b.modified ?? 0))
  }
}

const sortMenu = ref<{ x: number; y: number } | null>(null)

// mousedown.stop 阻断菜单的「点击外部关闭」（否则先关后开，看起来永不收起）；click 显式切换
function toggleSortMenu(e: MouseEvent) {
  if (sortMenu.value) {
    sortMenu.value = null
    return
  }
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  sortMenu.value = { x: rect.right - 168, y: rect.bottom + 4 }
}

function sortMenuItems(): MenuItem[] {
  return SORT_OPTIONS.map((o) => ({
    label: o.label,
    checked: sheetSort.value === o.key,
    action: () => {
      sheetSort.value = o.key
      localStorage.setItem(SORT_KEY, o.key)
    },
  }))
}

// ---- 新建文稿：列表顶行内联输入（Enter 创建，Esc 取消） ----
const naming = ref(false)
const newName = ref('')
const nameInput = ref<HTMLInputElement | null>(null)
const createError = ref('')

async function startNaming() {
  // 命名行已打开时再点「新建」只回焦输入框，不清空已输入内容
  if (naming.value) {
    nameInput.value?.focus()
    return
  }
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
    <!-- 次栏顶栏（窗口拖拽区）：右端筛选/新建；侧栏收起时左端带展开入口 -->
    <div class="sheet-head" :class="{ 'reserve-traffic': reserveTraffic }" @dblclick="onHeadDblClick">
      <button v-if="!sidebarVisible" class="head-btn" title="展开侧栏" @click="emit('toggle-sidebar')">
        <Icon name="panelLeft" :size="15" />
      </button>
      <div class="head-actions">
        <!-- mousedown.prevent：阻止按下时筛选输入框失焦（否则 blur 先关闭、click 又打开，开关打架） -->
        <button class="head-btn" :class="{ on: filtering }" title="筛选文稿" @mousedown.prevent @click="toggleFilter">
          <Icon name="filter" :size="14" />
        </button>
        <button class="head-btn" :class="{ on: sheetSort !== 'name-asc' }" title="排序方式" @mousedown.stop @click="toggleSortMenu">
          <Icon name="sort" :size="14" />
        </button>
        <button class="head-btn" title="新建文稿" @click="startNaming">
          <Icon name="plus" :size="14" />
        </button>
      </div>
    </div>

    <!-- 目录名横带（Ulysses 式列表头）：名称左下对齐，底部横线与列表分隔 -->
    <div class="dir-band" @dblclick="onHeadDblClick">
      <span class="dir-band-text">{{ dirTitle }}</span>
    </div>

    <!-- 筛选行：顶栏漏斗开关 -->
    <div v-if="filtering" class="filter-row">
      <input
        ref="filterInput"
        v-model="filter"
        type="text"
        placeholder="按标题筛选"
        @keydown.esc="toggleFilter"
        @blur="onFilterBlur"
      />
    </div>

    <!-- 新建文稿：列表首行内联命名（Enter 创建，Esc 取消） -->
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

    <ul v-if="shownSheets.length" class="sheets">
      <li
        v-for="s in shownSheets"
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
          <span v-if="excerpts[s.path]" class="sheet-excerpt">{{ excerpts[s.path] }}</span>
        </template>
      </li>
    </ul>
    <p v-else-if="!naming" class="empty-hint">{{ filter.trim() ? '没有匹配的文稿' : '此目录还没有文稿' }}</p>

    <ContextMenu
      v-if="sheetMenu"
      :x="sheetMenu.x"
      :y="sheetMenu.y"
      :items="sheetMenuItems(sheetMenu.path)"
      @close="sheetMenu = null"
    />
    <ContextMenu v-if="sortMenu" :x="sortMenu.x" :y="sortMenu.y" :items="sortMenuItems()" @close="sortMenu = null" />
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

/* 次栏顶栏：与编辑区顶行（EditorTabs）同为 40px，拼成通窗工具行；本身是窗口拖拽区 */
.sheet-head {
  flex: none;
  height: 40px;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 0 8px;
  -webkit-app-region: drag;
}

.sheet-head button {
  -webkit-app-region: no-drag;
}

/* 侧栏收起时顶栏通到窗口左缘：macOS 避让原生红绿灯 */
.sheet-head.reserve-traffic {
  padding-left: 78px;
}

/* 目录名横带：名称左下对齐 + 底部横线（Ulysses 式列表头）；与顶栏同高 40px，兼窗口拖拽区 */
.dir-band {
  flex: none;
  display: flex;
  align-items: flex-end;
  height: 40px;
  padding: 0 12px 7px;
  border-bottom: 1px solid var(--border);
  -webkit-app-region: drag;
  user-select: none;
}

.dir-band-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: calc(14px * var(--font-scale-ui));
  font-weight: 700;
  color: var(--text);
}

.head-actions {
  flex: none;
  display: flex;
  gap: 2px;
  margin-left: auto;
}

.head-btn {
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
  .head-btn:hover {
    background: var(--bg-soft);
    color: var(--text);
  }
}

.head-btn.on {
  background: var(--accent-soft);
  color: var(--accent);
}

.filter-row {
  flex: none;
  padding: 0 8px 6px;
}

.filter-row input {
  width: 100%;
  height: 28px;
  padding: 0 8px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  outline: none;
}

.filter-row input:focus {
  border-color: var(--accent);
}

/* 命名行与列表首项同位（顶部间隙一致） */
.naming-row {
  padding: 20px 8px 4px;
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
  /* 顶部间隙：顶栏 40px + 目录名横带 40px + 20px 间距——列表首项与正文首行保持同高（100px） */
  padding: 20px 8px 8px;
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

/* 正文预览：两行截断（Ulysses 式） */
.sheet-excerpt {
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--text-dim);
  line-height: 1.5;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  overflow: hidden;
  user-select: none;
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
