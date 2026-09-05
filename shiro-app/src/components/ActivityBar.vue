<script setup lang="ts">
// Activity Bar（顶部横排形态，对照 VSCode workbench.activityBar.location: top）：
// 模块图标横排在侧栏顶行，选中态为圆角底色块；点在拖拽区内，按钮须退出拖拽。
import type { NavItem, NavKey } from '../types'
import Icon from './Icon.vue'

defineProps<{ items: NavItem[]; active: NavKey }>()
const emit = defineEmits<{ activate: [key: NavKey] }>()
</script>

<template>
  <div class="activity-bar">
    <button
      v-for="item in items"
      :key="item.key"
      class="activity-btn"
      :class="{ active: active === item.key }"
      :title="item.label"
      @click="emit('activate', item.key)"
    >
      <Icon :name="item.icon" :size="18" />
    </button>
  </div>
</template>

<style scoped>
.activity-bar {
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 0;
}

.activity-btn {
  width: 32px;
  height: 32px;
  border: none;
  border-radius: 6px;
  background: none;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-dim);
  cursor: pointer;
  /* 顶行整体是窗口拖拽区，按钮退出拖拽 */
  -webkit-app-region: no-drag;
}

.activity-btn.active {
  background: var(--accent-soft);
  color: var(--accent);
}

@media (hover: hover) {
  .activity-btn:hover {
    color: var(--text);
  }

  .activity-btn.active:hover {
    color: var(--accent);
  }
}
</style>
