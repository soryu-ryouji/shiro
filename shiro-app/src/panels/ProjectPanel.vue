<script setup lang="ts">
// Project 面板（侧栏）：区块结构，首个区块为「项目列表」。
// - 区块右上 +：新建项目（指定目录创建工程文件夹并导入记录，见 NewProjectDialog）
// - 列表项悬停浮现 ···（单击或右键调出菜单）：打开文件位置（仅桌面端）/ 删除记录（不删文件夹）
import { onMounted, ref } from 'vue'
import { apiFetch } from '../api'
import { hasShell, shell } from '../platform'
import type { components } from '../api-types'
import Icon from '../components/Icon.vue'
import ContextMenu, { type MenuItem } from '../components/ContextMenu.vue'
import NewProjectDialog from '../components/NewProjectDialog.vue'

type ProjectItem = components['schemas']['ProjectItem']

const projects = ref<ProjectItem[]>([])
const loadError = ref('')
const showNew = ref(false)
const menu = ref<{ x: number; y: number; item: ProjectItem } | null>(null)

async function refresh() {
  try {
    const res = await apiFetch<components['schemas']['ProjectListResponse']>('/api/v1/projects')
    projects.value = res.projects ?? []
    loadError.value = ''
  } catch (e) {
    loadError.value = e instanceof Error ? e.message : String(e)
  }
}
onMounted(refresh)

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
  items.push({
    label: '删除记录',
    danger: true,
    action: () => void removeRecord(item),
  })
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
</script>

<template>
  <div class="panel">
    <!-- 区块：项目列表（后续区块在此扩展） -->
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
          :title="p.exists ? p.path : `${p.path}（目录已不存在）`"
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
      <p v-else class="hint">{{ loadError || '新建或打开过的项目会显示在这里' }}</p>
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
  font-size: 12px;
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
  font-size: 13px;
  color: var(--text);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.path {
  font-size: 11px;
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
  font-size: 12px;
}
</style>
