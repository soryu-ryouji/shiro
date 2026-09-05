<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { apiFetch, hasConnection } from './api'
import ProjectView from './views/ProjectView.vue'
import DatabaseView from './views/DatabaseView.vue'
import ModelView from './views/ModelView.vue'

type NavKey = 'project' | 'database' | 'model'
const navItems: { key: NavKey; label: string }[] = [
  { key: 'project', label: 'Project' },
  { key: 'database', label: 'Database' },
  { key: 'model', label: 'Model' },
]

const active = ref<NavKey>('project')
const status = ref<'connecting' | 'ready' | 'error'>('connecting')
const errorMsg = ref('')

onMounted(async () => {
  if (!hasConnection) {
    status.value = 'error'
    errorMsg.value = '缺少连接参数，请从 shiro 桌面应用启动'
    return
  }
  // 轮询启动状态：页面先于 daemon 就绪加载（先监听后启动模型），starting 继续等，
  // 进度帧展示待 daemon 引入初始化流程后接入
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
    } catch {
      // daemon 尚未监听，继续轮询
    }
    await new Promise((r) => setTimeout(r, 200))
  }
})
</script>

<template>
  <div v-if="status === 'connecting'" class="boot">正在连接 shiro-daemon…</div>
  <div v-else-if="status === 'error'" class="boot error">连接失败：{{ errorMsg }}</div>

  <div v-else class="layout">
    <nav class="sidebar">
      <button
        v-for="item in navItems"
        :key="item.key"
        :class="['nav-item', { active: active === item.key }]"
        @click="active = item.key"
      >
        {{ item.label }}
      </button>
    </nav>

    <main class="content">
      <ProjectView v-if="active === 'project'" />
      <DatabaseView v-else-if="active === 'database'" />
      <ModelView v-else />
    </main>

    <aside class="inspector">
      <p class="inspector-hint">详情</p>
    </aside>
  </div>
</template>
