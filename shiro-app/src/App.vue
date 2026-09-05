<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { apiFetch, hasConnection } from './api'
import { hasShell, shell } from './platform'
import type { NavItem, NavKey } from './types'
import Sidebar from './components/Sidebar.vue'
import TitleBar from './components/TitleBar.vue'
import Inspector from './components/Inspector.vue'
import WindowControls from './components/WindowControls.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import ProjectView from './views/ProjectView.vue'
import DatabaseView from './views/DatabaseView.vue'
import ModelView from './views/ModelView.vue'

const NAV_ITEMS: NavItem[] = [
  { key: 'project', label: 'Project', icon: 'folder' },
  { key: 'database', label: 'Database', icon: 'database' },
  { key: 'model', label: 'Model', icon: 'model' },
]
const active = ref<NavKey>('project')
const activeTitle = computed(() => NAV_ITEMS.find((i) => i.key === active.value)?.label ?? '')

/** 侧栏图标行点击：点当前激活图标收起侧栏（VSCode 行为）；切换模块直接生效 */
function onActivity(key: NavKey) {
  if (key === active.value) {
    toggleSidebar()
    return
  }
  active.value = key
}

// ---- 启动阶段：页面先于 daemon 就绪加载（先监听后启动模型），轮询 startup 直到 ready/error ----
const status = ref<'connecting' | 'ready' | 'error'>('connecting')
const errorMsg = ref('')

onMounted(async () => {
  if (!hasConnection) {
    status.value = 'error'
    errorMsg.value = '缺少连接参数，请从 shiro 桌面应用启动'
    return
  }
  for (;;) {
    try {
      const res = await apiFetch<{ status: string }>('/api/v1/app/startup')
      if (res.status === 'ready') {
        status.value = 'ready'
        return
      }
      if (res.status === 'error') {
        status.value = 'error'
        errorMsg.value = 'shiro-daemon 启动失败'
        return
      }
      // starting：继续轮询（进度帧展示待 daemon 引入初始化流程后接入）
    } catch {
      // daemon 尚未监听，继续轮询
    }
    await new Promise((r) => setTimeout(r, 200))
  }
})

// ---- 设置 ----
const showSettings = ref(false)

// ---- 侧栏显隐与栏宽（localStorage 持久化） ----
const SIDEBAR_MIN = 180
const SIDEBAR_MAX = 480
const INSPECTOR_MIN = 240
const INSPECTOR_MAX = 560
const sidebarVisible = ref(localStorage.getItem('shiro.sidebarVisible') !== '0')
const sidebarWidth = ref(220)
const inspectorWidth = ref(280)

function clamp(v: number, min: number, max: number) {
  return Math.min(max, Math.max(min, Math.round(v)))
}

function toggleSidebar() {
  sidebarVisible.value = !sidebarVisible.value
  localStorage.setItem('shiro.sidebarVisible', sidebarVisible.value ? '1' : '0')
}

{
  const saved = JSON.parse(localStorage.getItem('shiro.panelWidths') ?? '{}') as {
    sidebar?: number
    inspector?: number
  }
  if (typeof saved.sidebar === 'number') sidebarWidth.value = clamp(saved.sidebar, SIDEBAR_MIN, SIDEBAR_MAX)
  if (typeof saved.inspector === 'number') inspectorWidth.value = clamp(saved.inspector, INSPECTOR_MIN, INSPECTOR_MAX)
}

const dragSide = ref<'left' | 'right' | null>(null)

function startResize(side: 'left' | 'right') {
  dragSide.value = side
  document.body.classList.add('col-resizing')
  window.addEventListener('mousemove', onResizeMove)
  window.addEventListener('mouseup', stopResize)
}

function onResizeMove(e: MouseEvent) {
  if (dragSide.value === 'left') {
    sidebarWidth.value = clamp(e.clientX, SIDEBAR_MIN, SIDEBAR_MAX)
  } else if (dragSide.value === 'right') {
    inspectorWidth.value = clamp(window.innerWidth - e.clientX, INSPECTOR_MIN, INSPECTOR_MAX)
  }
}

function stopResize() {
  dragSide.value = null
  document.body.classList.remove('col-resizing')
  window.removeEventListener('mousemove', onResizeMove)
  window.removeEventListener('mouseup', stopResize)
  localStorage.setItem(
    'shiro.panelWidths',
    JSON.stringify({ sidebar: sidebarWidth.value, inspector: inspectorWidth.value }),
  )
}

const gridStyle = computed(() => ({
  gridTemplateColumns: `${sidebarVisible.value ? sidebarWidth.value : 0}px minmax(0, 1fr) ${inspectorWidth.value}px`,
}))
</script>

<template>
  <!-- 启动/错误：前置阶段；无边框窗口下仍需拖拽条与窗口控制按钮 -->
  <div v-if="status !== 'ready'" class="standalone">
    <div v-if="hasShell" class="drag-strip" @dblclick="shell.toggleMaximizeWindow()" />
    <div class="boot" :class="{ error: status === 'error' }">
      {{ status === 'error' ? `连接失败：${errorMsg}` : '正在连接 shiro-daemon…' }}
    </div>
    <WindowControls />
  </div>

  <!-- 侧栏（Activity 图标置顶）+ 中栏 + 详情栏：左右栏通高，顶栏只覆盖中栏 -->
  <div v-else class="app" :style="gridStyle">
    <Sidebar :items="NAV_ITEMS" :active="active" @activate="onActivity" @toggle="toggleSidebar" />

    <div class="center">
      <TitleBar
        :title="activeTitle"
        :sidebar-visible="sidebarVisible"
        @toggle-sidebar="toggleSidebar"
        @open-settings="showSettings = true"
      />
      <div class="content-body">
        <ProjectView v-if="active === 'project'" />
        <DatabaseView v-else-if="active === 'database'" />
        <ModelView v-else />
      </div>
    </div>

    <Inspector />
    <WindowControls />

    <!-- 栏宽拖拽手柄：4px 命中区紧贴分界线 -->
    <div
      v-show="sidebarVisible"
      class="col-resize-handle"
      :class="{ active: dragSide === 'left' }"
      :style="{ left: `${sidebarWidth}px` }"
      @mousedown.prevent="startResize('left')"
    />
    <div
      class="col-resize-handle"
      :class="{ active: dragSide === 'right' }"
      :style="{ left: `calc(100% - ${inspectorWidth}px)` }"
      @mousedown.prevent="startResize('right')"
    />

    <SettingsDialog v-if="showSettings" @close="showSettings = false" />
  </div>
</template>
