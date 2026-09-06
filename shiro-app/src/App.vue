<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { apiFetch, hasConnection } from './api'
import { hasShell, shell } from './platform'
import type { NavItem, NavKey } from './types'
import Sidebar from './components/Sidebar.vue'
import TitleBar from './components/TitleBar.vue'
import WindowControls from './components/WindowControls.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import ProjectView from './views/ProjectView.vue'
import DatabaseView from './views/DatabaseView.vue'
import ModelView from './views/ModelView.vue'
import { projectStore } from './stores/project'

const NAV_ITEMS: NavItem[] = [
  { key: 'project', label: 'Project', icon: 'folder' },
  { key: 'database', label: 'Database', icon: 'database' },
  { key: 'model', label: 'Model', icon: 'model' },
]
const active = ref<NavKey>('project')

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
        // 调试深链：hash 带 open=<项目路径>（可叠加 file=<文稿相对路径>）时启动后直接进入写作模式
        const hashParams = new URLSearchParams(location.hash.replace(/^#/, ''))
        const openPath = hashParams.get('open')
        if (openPath) {
          const name = openPath.replace(/[\\/]+$/, '').split(/[\\/]/).at(-1) ?? openPath
          void projectStore.open({ path: openPath, name, exists: true }).then(() => {
            // file 参数支持逗号分隔多个（依次入标签栏，最后一个激活）
            const files = hashParams.get('file')
            if (files) files.split(',').forEach((f) => projectStore.openFile(f))
          })
        }
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
const sidebarVisible = ref(localStorage.getItem('shiro.sidebarVisible') !== '0')
const sidebarWidth = ref(220)

function clamp(v: number, min: number, max: number) {
  return Math.min(max, Math.max(min, Math.round(v)))
}

function toggleSidebar() {
  sidebarVisible.value = !sidebarVisible.value
  localStorage.setItem('shiro.sidebarVisible', sidebarVisible.value ? '1' : '0')
}

{
  const saved = JSON.parse(localStorage.getItem('shiro.panelWidths') ?? '{}') as { sidebar?: number }
  if (typeof saved.sidebar === 'number') sidebarWidth.value = clamp(saved.sidebar, SIDEBAR_MIN, SIDEBAR_MAX)
}

const dragSide = ref<'left' | null>(null)

function startResize(side: 'left') {
  dragSide.value = side
  document.body.classList.add('col-resizing')
  window.addEventListener('mousemove', onResizeMove)
  window.addEventListener('mouseup', stopResize)
}

function onResizeMove(e: MouseEvent) {
  if (dragSide.value === 'left') {
    sidebarWidth.value = clamp(e.clientX, SIDEBAR_MIN, SIDEBAR_MAX)
  }
}

function stopResize() {
  dragSide.value = null
  document.body.classList.remove('col-resizing')
  window.removeEventListener('mousemove', onResizeMove)
  window.removeEventListener('mouseup', stopResize)
  localStorage.setItem('shiro.panelWidths', JSON.stringify({ sidebar: sidebarWidth.value }))
}

const gridStyle = computed(() => ({
  gridTemplateColumns: `${sidebarVisible.value ? sidebarWidth.value : 0}px minmax(0, 1fr)`,
}))

/** 写作模式（项目已打开）：各栏自带顶栏、分隔竖线贯穿窗口，全局顶栏让位 */
const writing = computed(() => active.value === 'project' && !!projectStore.current)
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

  <!-- 侧栏（Activity 图标置顶）+ 中栏：左栏通高；写作模式各栏自带顶栏，全局顶栏只覆盖其余形态 -->
  <div v-else class="app" :style="gridStyle">
    <Sidebar :items="NAV_ITEMS" :active="active" @activate="onActivity" @toggle="toggleSidebar" />

    <div class="center">
      <TitleBar
        v-if="!writing"
        :sidebar-visible="sidebarVisible"
        @toggle-sidebar="toggleSidebar"
        @open-settings="showSettings = true"
      />
      <div v-overlay-scrollbar class="content-body" :class="{ flush: writing }">
        <ProjectView
          v-if="active === 'project'"
          :sidebar-visible="sidebarVisible"
          @toggle-sidebar="toggleSidebar"
          @open-settings="showSettings = true"
        />
        <DatabaseView v-else-if="active === 'database'" />
        <ModelView v-else />
      </div>
    </div>

    <WindowControls />

    <!-- 栏宽拖拽手柄：4px 命中区紧贴分界线 -->
    <div
      v-show="sidebarVisible"
      class="col-resize-handle"
      :class="{ active: dragSide === 'left' }"
      :style="{ left: `${sidebarWidth}px` }"
      @mousedown.prevent="startResize('left')"
    />

    <SettingsDialog v-if="showSettings" @close="showSettings = false" />
  </div>
</template>
