<script setup lang="ts">
// 目录树节点（递归）：目录行 = 箭头 + 文件夹图标 + 名称；悬停浮现 +（新建子目录）与 ···（菜单）。
// 点行选中并展开，点箭头只切换展开；新建子目录为该目录子级的内联命名行。
import { computed, inject, nextTick, ref, watch } from 'vue'
import { projectStore, type TreeNode } from '../stores/project'
import Icon from './Icon.vue'

const props = defineProps<{ node: TreeNode; depth?: number }>()

// 目录菜单由 ProjectPanel 持有（递归组件经 inject 拿到打开函数）
const openTreeMenu = inject<(e: MouseEvent, node: TreeNode) => void>('openTreeMenu', () => {})

const open = ref(true)
const isDir = computed(() => props.node.kind === 'dir')
const dirChildren = computed(() => (props.node.children ?? []).filter((c) => c.kind === 'dir'))

function onRowClick() {
  projectStore.selectedDir = props.node.path
  open.value = true
}

// ---- 新建子目录：内联命名行 ----
const naming = computed(() => projectStore.namingDir === props.node.path)
const namingInput = ref<HTMLInputElement | null>(null)
const newName = ref('')

watch(
  () => projectStore.namingDir,
  async (dir) => {
    if (dir === props.node.path) {
      open.value = true
      newName.value = ''
      await nextTick()
      namingInput.value?.focus()
    }
  },
)

async function confirmNaming() {
  // 目录名不允许路径分隔符（多级创建经 API 的 dir 参数另行支持，此处是单层命名）
  const name = newName.value.trim().replace(/[/\\]/g, '')
  newName.value = '' // Enter 确认后输入框卸载会再触发 blur，先清空防二次提交
  projectStore.namingDir = null
  if (!name) return
  await projectStore.createDir(props.node.path, name)
}

// ---- 重命名：本行就地变为输入框（store.renamingDir 由右键菜单触发） ----
const renaming = computed(() => projectStore.renamingDir === props.node.path)
const renameInput = ref<HTMLInputElement | null>(null)
const renameValue = ref('')

watch(
  () => projectStore.renamingDir,
  async (dir) => {
    if (dir === props.node.path) {
      renameValue.value = props.node.name
      await nextTick()
      renameInput.value?.focus()
      renameInput.value?.select()
    }
  },
)

async function confirmRename() {
  const name = renameValue.value.trim().replace(/[/\\]/g, '')
  projectStore.renamingDir = null
  if (!name || name === props.node.name) return
  await projectStore.renameEntry(props.node.path, name)
}
</script>

<template>
  <div v-if="isDir" class="tree-node">
    <div
      v-if="!renaming"
      class="tree-row"
      :class="{ active: projectStore.selectedDir === node.path }"
      :style="{ paddingLeft: `${8 + (depth ?? 0) * 14}px` }"
      @click="onRowClick"
      @contextmenu.prevent="openTreeMenu($event, node)"
    >
      <button class="arrow" :class="{ open }" @click.stop="open = !open">
        <Icon name="chevronRight" :size="12" />
      </button>
      <Icon name="folder" :size="14" class="folder" />
      <span class="node-name">{{ node.name }}</span>
      <span class="row-actions">
        <button class="row-btn" title="新建子目录" @click.stop="projectStore.namingDir = node.path">
          <Icon name="plus" :size="12" />
        </button>
        <button class="row-btn" title="更多操作" @click.stop="openTreeMenu($event, node)">
          <Icon name="more" :size="12" />
        </button>
      </span>
    </div>
    <!-- 重命名态：本行就地变为输入框 -->
    <div v-else class="rename-row" :style="{ paddingLeft: `${8 + (depth ?? 0) * 14 + 22}px` }">
      <input
        ref="renameInput"
        v-model="renameValue"
        type="text"
        @keydown.enter="confirmRename"
        @keydown.esc="projectStore.renamingDir = null"
        @blur="confirmRename"
      />
    </div>
    <div v-if="open">
      <div v-if="naming" class="naming-row" :style="{ paddingLeft: `${8 + ((depth ?? 0) + 1) * 14 + 20}px` }">
        <input
          ref="namingInput"
          v-model="newName"
          type="text"
          placeholder="目录名"
          @keydown.enter="confirmNaming"
          @keydown.esc="projectStore.namingDir = null"
          @blur="confirmNaming"
        />
      </div>
      <TreeNode v-for="child in dirChildren" :key="child.path" :node="child" :depth="(depth ?? 0) + 1" />
    </div>
  </div>
</template>

<style scoped>
.tree-row {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px 6px 5px 0;
  border-radius: 5px;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  user-select: none;
}

@media (hover: hover) {
  .tree-row:hover {
    background: var(--border);
  }
}

.tree-row.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}

.tree-row.active .folder {
  color: var(--accent);
}

.arrow {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  padding: 0;
  border: none;
  background: none;
  color: var(--text-dim);
  cursor: pointer;
  transition: transform 0.12s;
}

.arrow.open {
  transform: rotate(90deg);
}

.folder {
  flex: none;
  color: var(--text-dim);
}

.node-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 行内操作：默认隐藏，悬停浮现 */
.row-actions {
  display: flex;
  gap: 2px;
  opacity: 0;
}

.tree-row:hover .row-actions,
.row-actions:focus-within {
  opacity: 1;
}

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

@media (hover: hover) {
  .row-btn:hover {
    background: var(--bg-soft);
    color: var(--text);
  }
}

.naming-row {
  padding: 2px 6px 2px 0;
}

.naming-row input {
  width: 100%;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--accent);
  border-radius: 5px;
  font-size: calc(13px * var(--font-scale-ui));
  outline: none;
}

.rename-row {
  padding: 2px 6px 2px 0;
}

.rename-row input {
  width: 100%;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--accent);
  border-radius: 5px;
  font-size: calc(13px * var(--font-scale-ui));
  outline: none;
}
</style>
