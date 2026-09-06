<script setup lang="ts">
// 轻量右键/··· 菜单：固定定位于触发点，点击外部 / Esc 关闭；越界时向内收。
import { computed, onMounted, onUnmounted } from 'vue'

export interface MenuItem {
  label?: string
  /** 分隔线（忽略其余字段） */
  divider?: boolean
  danger?: boolean
  /** 定义即渲染勾选列（单选式菜单项，如排序方式）：true 显示 ✓，false 留空占位 */
  checked?: boolean
  action?: () => void
}

const props = defineProps<{ x: number; y: number; items: MenuItem[] }>()
const emit = defineEmits<{ close: [] }>()

const MENU_WIDTH = 168
const ITEM_HEIGHT = 32
const DIVIDER_HEIGHT = 9

const pos = computed(() => ({
  left: `${Math.min(props.x, window.innerWidth - MENU_WIDTH - 8)}px`,
  top: `${Math.min(props.y, window.innerHeight - props.items.reduce((h, it) => h + (it.divider ? DIVIDER_HEIGHT : ITEM_HEIGHT), 0) - 24)}px`,
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
    <template v-for="(item, i) in items" :key="i">
      <div v-if="item.divider" class="ctx-sep" />
      <button v-else class="ctx-item" :class="{ danger: item.danger }" @click="item.action?.(); emit('close')">
        <span v-if="item.checked !== undefined" class="ctx-tick">{{ item.checked ? '✓' : '' }}</span>{{ item.label }}
      </button>
    </template>
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

/* 分隔线 */
.ctx-sep {
  height: 1px;
  margin: 4px 8px;
  background: var(--border);
}

/* 勾选列（单选式菜单项） */
.ctx-tick {
  display: inline-block;
  width: 14px;
  color: var(--accent);
}
</style>
