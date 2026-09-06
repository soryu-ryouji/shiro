<script setup lang="ts">
// Project 面板（侧栏）：区块结构，首个区块为「项目列表」。
// - 区块右上 +：新建项目（选已有文件夹作项目目录，项目名写入 .shiro/project.toml 显示，见 NewProjectDialog）
// - 列表项悬停浮现 ···（单击或右键调出菜单）：打开文件位置（仅桌面端）/ 删除记录（不删文件夹）
import { nextTick, onMounted, provide, ref } from 'vue'
import { apiFetch } from '../api'
import { hasShell, shell } from '../platform'
import { projectStore, type ProjectItem, type TreeNode as TreeNodeData } from '../stores/project'
import Icon from '../components/Icon.vue'
import TreeNode from '../components/TreeNode.vue'
import ContextMenu, { type MenuItem } from '../components/ContextMenu.vue'
import NewProjectDialog from '../components/NewProjectDialog.vue'
import PromptDialog from '../components/PromptDialog.vue'
import ConfirmDialog from '../components/ConfirmDialog.vue'

const projects = ref<ProjectItem[]>([])
const loadError = ref('')
const showNew = ref(false)
const menu = ref<{ x: number; y: number; item: ProjectItem } | null>(null)

async function refresh() {
  try {
    const res = await apiFetch<{ projects?: ProjectItem[] }>('/api/v1/projects')
    projects.value = res.projects ?? []
    loadError.value = ''
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : String(e)
  }
}
onMounted(refresh)

/** 单击项目项进入写作模式（目录已消失的项不可打开） */
function openProject(p: ProjectItem) {
  if (!p.exists) return
  void projectStore.open(p)
}

// ---- 目录树菜单（新建子目录 / 删除目录）；递归的 TreeNode 经 inject 打开 ----
const treeMenu = ref<{ x: number; y: number; node: TreeNodeData } | null>(null)
provide('openTreeMenu', (e: MouseEvent, node: TreeNodeData) => {
  // click（··· 按钮）切换开合；contextmenu（右键）总是打开
  if (e.type !== 'contextmenu' && treeMenu.value?.node === node) {
    treeMenu.value = null
    return
  }
  treeMenu.value = { x: e.clientX, y: e.clientY, node }
})

function treeMenuItems(node: TreeNodeData): MenuItem[] {
  const items: MenuItem[] = [
    { label: '新建子目录', action: () => (projectStore.namingDir = node.path) },
    { label: '重命名…', action: () => (projectStore.renamingDir = node.path) },
  ]
  // 文件管理器能力是桌面壳能力，局域网浏览器形态没有
  if (hasShell) {
    items.push({
      label: '在文件夹中打开',
      action: () => void shell.openPath(`${projectStore.current?.path}/${node.path}`),
    })
  }
  items.push({
    label: '删除目录（移至回收站）',
    danger: true,
    action: () => void projectStore.removeDir(node.path),
  })
  return items
}

/** 根级新建目录的命名行输入框引用与确认（'' = 项目根） */
const rootNamingInput = ref<HTMLInputElement | null>(null)
const rootNewName = ref('')

function startRootNaming() {
  projectStore.namingDir = ''
  rootNewName.value = ''
  void nextTick(() => rootNamingInput.value?.focus())
}

async function confirmRootNaming() {
  const name = rootNewName.value.trim().replace(/[/\\]/g, '')
  projectStore.namingDir = null
  if (!name) return
  await projectStore.createDir('', name)
}

function openMenu(e: MouseEvent, item: ProjectItem) {
  e.preventDefault()
  menu.value = { x: e.clientX, y: e.clientY, item }
}

/** ··· 按钮切换开合（配 mousedown.stop 阻断菜单的「点击外部关闭」，否则先关后开永不收起） */
function toggleMenu(e: MouseEvent, item: ProjectItem) {
  menu.value = menu.value?.item === item ? null : { x: e.clientX, y: e.clientY, item }
}

function menuItems(item: ProjectItem): MenuItem[] {
  const items: MenuItem[] = []
  // 文件管理器定位是桌面壳能力，局域网浏览器形态没有
  if (hasShell && item.exists) {
    items.push({ label: '打开文件位置', action: () => void shell.showInFolder(item.path) })
  }
  if (item.exists) {
    items.push({ label: '重命名文件夹…', action: () => (renamingProject.value = item) })
  }
  items.push({
    label: '从列表移除',
    action: () => void removeRecord(item),
  })
  if (hasShell && item.exists) {
    items.push({
      label: '删除文件夹…',
      danger: true,
      action: () => (deletingProject.value = item),
    })
  }
  return items
}

/** 只删 history.toml 里的记录，不动文件夹本身 */
async function removeRecord(item: ProjectItem) {
  try {
    await apiFetch(`/api/v1/projects?path=${encodeURIComponent(item.path)}`, { method: 'DELETE' })
    await refresh()
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : String(e)
  }
}

// ---- 项目重命名（文件夹本体改名，history 同步） ----
const renamingProject = ref<ProjectItem | null>(null)

/** 文件夹名（path 末段）；重命名文件夹对话框预填用——与项目显示名（project.toml 的 name）解耦，不能混用 */
function dirName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path
}

async function submitRename(newName: string) {
  const item = renamingProject.value
  if (!item) return
  try {
    const renamed = await apiFetch<ProjectItem>('/api/v1/projects/rename', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: item.path, new_name: newName }),
    })
    // 写作模式下重命名当前打开的项目时，同步项目头标题与路径（后端返回新 path 与显示名）
    if (projectStore.current?.path === item.path) {
      projectStore.current = renamed
    }
    await refresh()
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : String(e)
  }
}

// ---- 删除项目文件夹（移系统回收站，确认后执行） ----
const deletingProject = ref<ProjectItem | null>(null)

async function confirmDeleteFolder() {
  const item = deletingProject.value
  if (!item) return
  try {
    await shell.trashItem(item.path)
    await apiFetch(`/api/v1/projects?path=${encodeURIComponent(item.path)}`, { method: 'DELETE' })
    // 写作模式下删除当前打开的项目时，退回列表模式
    if (projectStore.current?.path === item.path) projectStore.close()
    await refresh()
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : String(e)
  }
}

// ---- 项目头 ··· 菜单（项目级操作，复用列表模式的改名/删除流程） ----
const projMenu = ref<{ x: number; y: number; items: MenuItem[] } | null>(null)

function openProjectMenu(e: MouseEvent) {
  const cur = projectStore.current
  if (!cur) return
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect()
  const items: MenuItem[] = []
  if (hasShell) {
    items.push({ label: '在文件夹中打开', action: () => void shell.openPath(cur.path) })
  }
  items.push({ label: '重命名文件夹…', action: () => (renamingProject.value = cur) })
  items.push({ label: '删除文件夹…', danger: true, action: () => (deletingProject.value = cur) })
  projMenu.value = { x: rect.left, y: rect.bottom + 4, items }
}
</script>

<template>
  <!-- 写作模式：项目头（返回 + 大标题 + 操作行）+ 虚线 + 根级列表（子目录相对缩进；文稿列表在中栏） -->
  <div v-if="projectStore.current" class="project-nav">
    <div class="proj-head">
      <button class="back-btn" title="返回项目列表" @click="projectStore.close()">
        <Icon name="chevronLeft" :size="14" />
      </button>
      <div class="proj-title" :title="projectStore.current.path">{{ projectStore.current.name }}</div>
    </div>
    <div class="proj-actions">
      <button class="act-btn" title="新建根文件夹" @click="startRootNaming">
        <Icon name="plus" :size="14" />
      </button>
      <button class="act-btn" title="项目操作" @click="openProjectMenu">
        <Icon name="more" :size="14" />
      </button>
    </div>
    <div class="proj-divider" />
    <div class="tree">
      <!-- 根级新建文件夹的命名行 -->
      <div v-if="projectStore.namingDir === ''" class="naming-row root-naming">
        <input
          ref="rootNamingInput"
          v-model="rootNewName"
          type="text"
          placeholder="目录名"
          @keydown.enter="confirmRootNaming"
          @keydown.esc="projectStore.namingDir = null"
          @blur="confirmRootNaming"
        />
      </div>
      <!-- 根目录行：选中的中栏列出项目根级散落的文稿 -->
      <div
        class="root-dir-row"
        :class="{ active: projectStore.selectedDir === '' }"
        title="项目根目录"
        @click="projectStore.selectedDir = ''"
      >
        <Icon name="folder" :size="14" class="folder" />
        <span class="row-name">根目录</span>
      </div>
      <!-- 根级目录直接成列表；子目录由 TreeNode 逐级缩进 -->
      <TreeNode v-for="node in projectStore.tree" :key="node.path" :node="node" />
    </div>
    <ContextMenu
      v-if="treeMenu"
      :x="treeMenu.x"
      :y="treeMenu.y"
      :items="treeMenuItems(treeMenu.node)"
      @close="treeMenu = null"
    />
    <ContextMenu
      v-if="projMenu"
      :x="projMenu.x"
      :y="projMenu.y"
      :items="projMenu.items"
      @close="projMenu = null"
    />
  </div>

  <!-- 列表模式：项目列表区块 -->
  <div v-else class="panel">
    <section class="section">
      <header class="section-head">
        <span class="section-title">项目列表</span>
        <button class="add-btn" title="新建项目（选择已有文件夹）" @click="showNew = true">
          <Icon name="plus" :size="14" />
        </button>
      </header>

      <ul v-if="projects.length" class="project-list">
        <li
          v-for="p in projects"
          :key="p.path"
          class="project-item"
          :class="{ missing: !p.exists }"
          :title="p.exists ? p.path : `${p.path}（目录已不存在）`"
          @click="openProject(p)"
          @contextmenu.prevent="openMenu($event, p)"
        >
          <div class="meta">
            <span class="name">{{ p.name }}</span>
            <span class="path">{{ p.path }}</span>
          </div>
          <button class="more-btn" title="更多操作（只删除记录，不删除文件夹）" @mousedown.stop @click.stop="toggleMenu($event, p)">
            <Icon name="more" :size="14" />
          </button>
        </li>
      </ul>
      <p v-else class="hint">{{ loadError || '新建或打开过的项目会显示在这里，点击进入写作' }}</p>
    </section>

    <ContextMenu
      v-if="menu"
      :x="menu.x"
      :y="menu.y"
      :items="menuItems(menu.item)"
      @close="menu = null"
    />
    <NewProjectDialog v-if="showNew" @close="showNew = false" @created="refresh" />
  </div>

  <!-- 项目重命名 / 删除文件夹确认：列表右键菜单与写作模式 ··· 菜单共用 -->
  <PromptDialog
    v-if="renamingProject"
    title="重命名文件夹"
    :initial="dirName(renamingProject.path)"
    placeholder="新名称"
    @close="renamingProject = null"
    @submit="submitRename"
  />
  <ConfirmDialog
    v-if="deletingProject"
    title="删除文件夹"
    :message="`将把项目文件夹移到系统回收站（可恢复）：\n${deletingProject.path}`"
    confirm-label="删除"
    @close="deletingProject = null"
    @confirm="confirmDeleteFolder"
  />
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 4px 6px;
}

.section-title {
  font-size: calc(12px * var(--font-scale-ui));
  font-weight: 600;
  color: var(--text-dim);
  letter-spacing: 0.4px;
}

.add-btn {
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

.add-btn:disabled {
  opacity: 0.4;
  cursor: default;
}

@media (hover: hover) {
  .add-btn:hover {
    background: var(--border);
    color: var(--text);
  }
}

.project-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.project-item {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px;
  border-radius: 6px;
  cursor: default;
}

@media (hover: hover) {
  .project-item:hover {
    background: var(--border);
  }
}

.project-item.missing .meta {
  opacity: 0.45;
}

.meta {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.name {
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.path {
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* ··· 按钮默认隐藏，悬停项时浮现 */
.more-btn {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
  opacity: 0;
}

.project-item:hover .more-btn,
.more-btn:focus-visible {
  opacity: 1;
}

@media (hover: hover) {
  .more-btn:hover {
    background: var(--bg-soft);
    color: var(--text);
  }
}

.hint {
  margin: 0;
  padding: 0 4px;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}

/* ---- 写作模式：项目头（返回 + 大标题 + 操作行）+ 虚线 + 根级列表 ---- */
.project-nav {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.proj-head {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px 6px 0;
}

.back-btn {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  padding: 0;
  border: none;
  border-radius: 5px;
  background: none;
  color: var(--text-dim);
  cursor: pointer;
}

@media (hover: hover) {
  .back-btn:hover {
    background: var(--border);
    color: var(--text);
  }
}

/* 项目大标题（Ulysses 式） */
.proj-title {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: calc(19px * var(--font-scale-ui));
  font-weight: 700;
  color: var(--text);
}

.proj-actions {
  display: flex;
  gap: 2px;
  padding: 2px 6px;
}

.act-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}

@media (hover: hover) {
  .act-btn:hover {
    background: var(--border);
    color: var(--text);
  }
}

/* 项目信息区块与目录列表的虚线分界 */
.proj-divider {
  margin: 2px 8px 4px;
  border-bottom: 1px dashed var(--border);
}

.tree {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.root-naming {
  padding: 2px 6px 2px 26px;
}

.root-naming input {
  width: 100%;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--accent);
  border-radius: 5px;
  font-size: calc(13px * var(--font-scale-ui));
  outline: none;
}

/* 根目录行：查看项目根级散落文稿的入口，与目录行的 folder 图标对齐（箭头槽 14px + gap 4px + 行缩进 8px）；
   与下方目录列表留出间隔，区分「根目录」与同级目录列表 */
.root-dir-row {
  margin-bottom: 6px;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px 6px 5px 26px;
  border-radius: 5px;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  user-select: none;
}

@media (hover: hover) {
  .root-dir-row:hover {
    background: var(--border);
  }
}

.root-dir-row .folder {
  flex: none;
  color: var(--text-dim);
}

.root-dir-row .row-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.root-dir-row.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}

.root-dir-row.active .folder {
  color: var(--accent);
}

</style>
