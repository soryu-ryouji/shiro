<script setup lang="ts">
// 极简图标组件：name → stroke SVG（path 表内联维护，随用随加）。
// vb 可切换坐标系（用于直接引入 24px 网格的图标，如 lucide settings）。
import { computed } from 'vue'

const props = withDefaults(defineProps<{ name: string; size?: number }>(), { size: 16 })

interface IconPath {
  d: string
  w?: number
}

interface IconDef {
  vb?: string
  paths: IconPath[]
}

const ICONS: Record<string, IconDef> = {
  // 文件夹（Project）
  folder: { paths: [{ d: 'M2 4h4l2 2h6v6H2z' }] },
  // 圆柱（Database）
  database: {
    paths: [
      { d: 'M8 2.5C4.9 2.5 2.5 3.5 2.5 4.8v6.4c0 1.3 2.4 2.3 5.5 2.3s5.5-1 5.5-2.3V4.8c0-1.3-2.4-2.3-5.5-2.3z' },
      { d: 'M2.5 8c0 1.3 2.4 2.3 5.5 2.3s5.5-1 5.5-2.3' },
    ],
  },
  // 立方体（Model）
  model: { paths: [{ d: 'M8 1.8L13.8 5v6L8 14.2 2.2 11V5z' }, { d: 'M2.2 5L8 8.2 13.8 5' }, { d: 'M8 8.2v6' }] },
  // 侧栏开关
  panelLeft: { paths: [{ d: 'M2 3.5h12v9H2z' }, { d: 'M6 3.5v9' }] },
  // 齿轮（设置，lucide settings 24px 网格）
  settings: {
    vb: '0 0 24 24',
    paths: [
      {
        d: 'M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z',
      },
      { d: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z' },
    ],
  },
  // 加号
  plus: { paths: [{ d: 'M8 3v10M3 8h10' }] },
  // ··· 更多（三个粗圆点）
  more: {
    paths: [
      { d: 'M3.5 8h.01', w: 2.4 },
      { d: 'M8 8h.01', w: 2.4 },
      { d: 'M12.5 8h.01', w: 2.4 },
    ],
  },
  // 方向箭头
  chevronDown: { paths: [{ d: 'M3 6l5 5 5-5' }] },
  chevronRight: { paths: [{ d: 'M6 3l5 5-5 5' }] },
  chevronLeft: { paths: [{ d: 'M10 3l-5 5 5 5' }] },
  // 排序（上下箭头）
  sort: {
    paths: [
      { d: 'M5 2.5v11M2.5 10.5L5 13.5l2.5-3' },
      { d: 'M11 13.5v-11M8.5 5.5L11 2.5l2.5 3' },
    ],
  },
  // 文稿
  fileText: { paths: [{ d: 'M4 2h5.5L12 4.5V14H4z' }, { d: 'M6 7h4M6 9.5h4' }] },
  // 漏斗（筛选）
  filter: { paths: [{ d: 'M2.5 3h11L9.3 7.8v4.2l-2.6 1.3V7.8z' }] },
  // 关闭（×）
  close: { paths: [{ d: 'M3.5 3.5l9 9M12.5 3.5l-9 9' }] },
}

const def = computed(() => ICONS[props.name] ?? { paths: [] })
</script>

<template>
  <svg
    :width="size"
    :height="size"
    :viewBox="def.vb ?? '0 0 16 16'"
    fill="none"
    stroke="currentColor"
    stroke-linecap="round"
    stroke-linejoin="round"
  >
    <path v-for="(p, i) in def.paths" :key="i" :d="p.d" :stroke-width="p.w ?? (def.vb ? 2 : 1.2)" />
  </svg>
</template>
