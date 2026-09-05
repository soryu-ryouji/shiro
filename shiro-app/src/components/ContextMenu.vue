<script setup lang="ts">
// 轻量右键/··· 菜单：固定定位于触发点，点击外部 / Esc 关闭；越界时向内收。
import { computed, onMounted, onUnmounted } from 'vue'

export interface MenuItem {
  label: string
  danger?: boolean
  action: () => void
}

const props = defineProps<{ x: number; y: number; items: MenuItem[] }>()
const emit = defineEmits<{ close: [] }>()

const MENU_WIDTH = 168
const ITEM_HEIGHT = 32

const pos = computed(() => ({
  left: `${Math.min(props.x, window.innerWidth - MENU_WIDTH - 8)}px`,
  top: `${Math.min(props.y, window.innerHeight - props.items.length * ITEM_HEIGHT - 16)}px`,
}))

function onDocDown(e: MouseEvent) {
  if (!(e.target as HTMLElement).closest('.ctx-menu')) emit('close')
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}

onMounted(() => {
  document.addEventListener('mousedown', onDocDown)
  window.addEventListener('keydown', onKeydown)
})
onUnmounted(() => {
  document.removeEventListener('mousedown', onDocDown)
  window.removeEventListener('keydown', onKeydown)
})
</script>

<template>
  <div class="ctx-menu" :style="pos" @contextmenu.prevent>
    <button
      v-for="(item, i) in items"
      :key="i"
      class="ctx-item"
      :class="{ danger: item.danger }"
      @click="item.action(); emit('close')"
    >
      {{ item.label }}
    </button>
  </div>
</template>

<style scoped>
.ctx-menu {
  position: fixed;
  z-index: 300;
  min-width: 168px;
  padding: 4px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
  box-shadow: 0 6px 24px rgba(0, 0, 0, 0.12);
  /* Windows 上右键按下即弹菜单，按住拖过菜单项会选中文字；菜单文本不可选 */
  user-select: none;
  display: flex;
  flex-direction: column;
}

.ctx-item {
  padding: 6px 10px;
  border: none;
  border-radius: 5px;
  background: none;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  text-align: left;
  cursor: pointer;
}

@media (hover: hover) {
  .ctx-item:hover {
    background: var(--bg-soft);
  }

  .ctx-item.danger:hover {
    background: #fdecec;
    color: var(--danger);
  }
}

.ctx-item.danger {
  color: var(--danger);
}
</style>
