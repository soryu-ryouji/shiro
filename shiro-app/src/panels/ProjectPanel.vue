<script setup lang="ts">
// Project 面板（侧栏）：区块结构，首个区块为「项目列表」。
// - 区块右上 +：新建项目（指定目录创建工程文件夹并导入记录，见 NewProjectDialog）
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

/** 双击项目项进入写作模式（目录已消失的项不可打开） */
function openProject(p: ProjectItem) {
  if (!p.exists) return
  void projectStore.open(p)
}

// ---- 目录树菜单（新建子目录 / 删除目录）；递归的 TreeNode 经 inject 打开 ----
const treeMenu = ref<{ x: number; y: number; node: TreeNodeData } | null>(null)
provide('openTreeMenu', (e: MouseEvent, node: TreeNodeData) => {
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

function menuItems(item: ProjectItem): MenuItem[] {
  const items: MenuItem[] = []
  // 文件管理器定位是桌面壳能力，局域网浏览器形态没有
  if (hasShell && item.exists) {
    items.push({ label: '打开文件位置', action: () => void shell.showInFolder(item.path) })
  }
  if (item.exists) {
    items.push({ label: '重命名…', action: () => (renamingProject.value = item) })
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

async function submitRename(newName: string) {
  const item = renamingProject.value
  if (!item) return
  try {
    await apiFetch('/api/v1/projects/rename', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: item.path, new_name: newName }),
    })
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
    await refresh()
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : String(e)
  }
}
</script>

<template>
  <!-- 写作模式：返回 + 目录树（只含目录；文稿列表在中栏） -->
  <div v-if="projectStore.current" class="project-nav">
    <button class="back-btn" @click="projectStore.close()">
      <Icon name="chevronLeft" :size="14" />
      <span class="back-name">{{ projectStore.current.name }}</span>
    </button>
    <div class="tree">
      <div
        class="tree-root"
        :class="{ active: projectStore.selectedDir === '' }"
        @click="projectStore.selectedDir = ''"
      >
        <Icon name="fileText" :size="14" />
        <span class="root-name">项目文件</span>
        <button
          class="row-btn root-add"
          title="新建目录"
          @click.stop="startRootNaming"
        >
          <Icon name="plus" :size="12" />
        </button>
      </div>
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
      <TreeNode v-for="node in projectStore.tree" :key="node.path" :node="node" />
    </div>
    <ContextMenu
      v-if="treeMenu"
      :x="treeMenu.x"
      :y="treeMenu.y"
      :items="treeMenuItems(treeMenu.node)"
      @close="treeMenu = null"
    />
  </div>

  <!-- 列表模式：项目列表区块 -->
  <div v-else class="panel">
    <section class="section">
      <header class="section-head">
        <span class="section-title">项目列表</span>
        <button class="add-btn" title="新建项目" @click="showNew = true">
          <Icon name="plus" :size="14" />
        </button>
      </header>

      <ul v-if="projects.length" class="project-list">
        <li
          v-for="p in projects"
          :key="p.path"
          class="project-item"
          :class="{ missing: !p.exists }"
          :title="p.exists ? `${p.path}（双击打开）` : `${p.path}（目录已不存在）`"
          @dblclick="openProject(p)"
          @contextmenu.prevent="openMenu($event, p)"
        >
          <div class="meta">
            <span class="name">{{ p.name }}</span>
            <span class="path">{{ p.path }}</span>
          </div>
          <button class="more-btn" title="更多操作（只删除记录，不删除文件夹）" @click="openMenu($event, p)">
            <Icon name="more" :size="14" />
          </button>
        </li>
      </ul>
      <p v-else class="hint">{{ loadError || '新建或打开过的项目会显示在这里，双击进入写作' }}</p>
    </section>

    <ContextMenu
      v-if="menu"
      :x="menu.x"
      :y="menu.y"
      :items="menuItems(menu.item)"
      @close="menu = null"
    />
    <NewProjectDialog v-if="showNew" @close="showNew = false" @created="refresh" />
    <PromptDialog
      v-if="renamingProject"
      title="重命名项目"
      :initial="renamingProject.name"
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
  </div>
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

/* ---- 写作模式：返回 + 目录树 ---- */
.project-nav {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.back-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px;
  border: none;
  border-radius: 6px;
  background: none;
  font-size: calc(13px * var(--font-scale-ui));
  font-weight: 600;
  color: var(--text);
  cursor: pointer;
  text-align: left;
}

@media (hover: hover) {
  .back-btn:hover {
    background: var(--border);
  }
}

.back-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tree {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.tree-root {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 8px;
  border-radius: 5px;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  cursor: pointer;
  user-select: none;
}

.root-name {
  flex: 1;
}

/* 根行的新建按钮默认隐藏，悬停浮现 */
.root-add {
  opacity: 0;
}

.tree-root:hover .root-add {
  opacity: 1;
}

.root-naming {
  padding: 2px 6px 2px 28px;
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

@media (hover: hover) {
  .tree-root:hover {
    background: var(--border);
  }
}

.tree-root.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}

/* 树行内操作按钮（与 TreeNode 行内样式同款） */
.row-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}
</style>
