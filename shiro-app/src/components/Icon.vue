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
  // 气泡（Chat）
  chat: { paths: [{ d: 'M2.5 3.5h11a.5.5 0 0 1 .5.5v7a.5.5 0 0 1-.5.5H7.8L4.8 14v-2.5H2.5a.5.5 0 0 1-.5-.5V4a.5.5 0 0 1 .5-.5z' }] },
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
  // 大纲（行内圆点 + 线）
  list: {
    paths: [
      { d: 'M2.8 4h.01', w: 2.2 },
      { d: 'M2.8 8h.01', w: 2.2 },
      { d: 'M2.8 12h.01', w: 2.2 },
      { d: 'M6 4h7.5M6 8h7.5M6 12h7.5' },
    ],
  },
  // 刷新（循环箭头，lucide rotate-cw 24px 网格）
  refresh: {
    vb: '0 0 24 24',
    paths: [
      { d: 'M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8' },
      { d: 'M21 3v5h-5' },
    ],
  },
  // 铅笔（编辑，lucide pencil 24px 网格）
  edit: {
    vb: '0 0 24 24',
    paths: [{ d: 'M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z' }],
  },
  // 关闭（×）
  close: { paths: [{ d: 'M3.5 3.5l9 9M12.5 3.5l-9 9' }] },
  trash: {
    vb: '0 0 24 24',
    paths: [
      { d: 'M3 6h18' },
      { d: 'M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6' },
      { d: 'M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2' },
    ],
  },
  // 眼睛（预览，lucide eye 24px 网格）
  eye: {
    vb: '0 0 24 24',
    paths: [
      {
        d: 'M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0',
      },
      { d: 'M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z' },
    ],
  },
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
