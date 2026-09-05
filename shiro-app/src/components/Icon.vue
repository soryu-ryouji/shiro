<script setup lang="ts">
// 极简图标组件：name → 16x16 stroke SVG（path 表内联维护，随用随加；w 可覆盖单 path 线宽）
import { computed } from 'vue'

const props = withDefaults(defineProps<{ name: string; size?: number }>(), { size: 16 })

interface IconPath {
  d: string
  w?: number
}

const ICONS: Record<string, IconPath[]> = {
  // 文件夹（Project）
  folder: [{ d: 'M2 4h4l2 2h6v6H2z' }],
  // 圆柱（Database）
  database: [
    { d: 'M8 2.5C4.9 2.5 2.5 3.5 2.5 4.8v6.4c0 1.3 2.4 2.3 5.5 2.3s5.5-1 5.5-2.3V4.8c0-1.3-2.4-2.3-5.5-2.3z' },
    { d: 'M2.5 8c0 1.3 2.4 2.3 5.5 2.3s5.5-1 5.5-2.3' },
  ],
  // 立方体（Model）
  model: [{ d: 'M8 1.8L13.8 5v6L8 14.2 2.2 11V5z' }, { d: 'M2.2 5L8 8.2 13.8 5' }, { d: 'M8 8.2v6' }],
  // 侧栏开关
  panelLeft: [{ d: 'M2 3.5h12v9H2z' }, { d: 'M6 3.5v9' }],
  // 齿轮（设置）
  settings: [
    { d: 'M8 10a2 2 0 1 0 0-4 2 2 0 0 0 0 4z' },
    { d: 'M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.05 3.05l1.4 1.4M11.55 11.55l1.4 1.4M12.95 3.05l-1.4 1.4M4.45 11.55l-1.4 1.4' },
  ],
  // 加号
  plus: [{ d: 'M8 3v10M3 8h10' }],
  // ··· 更多（三个粗圆点）
  more: [
    { d: 'M3.5 8h.01', w: 2.4 },
    { d: 'M8 8h.01', w: 2.4 },
    { d: 'M12.5 8h.01', w: 2.4 },
  ],
  // 方向箭头
  chevronDown: [{ d: 'M3 6l5 5 5-5' }],
  chevronRight: [{ d: 'M6 3l5 5-5 5' }],
  chevronLeft: [{ d: 'M10 3l-5 5 5 5' }],
  // 文稿
  fileText: [{ d: 'M4 2h5.5L12 4.5V14H4z' }, { d: 'M6 7h4M6 9.5h4' }],
  // 关闭（×）
  close: [{ d: 'M3.5 3.5l9 9M12.5 3.5l-9 9' }],
}

const paths = computed(() => ICONS[props.name] ?? [])
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    stroke-linecap="round"
    stroke-linejoin="round"
  >
    <path v-for="(p, i) in paths" :key="i" :d="p.d" :stroke-width="p.w ?? 1.2" />
  </svg>
</template>
