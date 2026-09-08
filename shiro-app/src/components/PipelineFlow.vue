<script setup lang="ts">
// 拆解管线流程图（SVG 纵向数据流导图）：节点按阶段状态着色，
// 切块/笔记节点下挂切片格子，生成节点下挂 7 文件格，选择闸门为琥珀色等待态。
// 纯展示组件，数据来自任务进度（daemon Progress）。
import { computed } from 'vue'
import type { components } from '../api-types'
import { FILE_LABELS, STAGE_LABELS } from '../stores/deconstruct'

type Progress = components['schemas']['Progress']

const props = defineProps<{ progress: Progress }>()
/** 格子点击：打开对应调用日志（父级处理） */
const emit = defineEmits<{ 'pick-segment': [index: number]; 'pick-file': [name: string] }>()

/** 管线阶段顺序（纵向链） */
const STAGES = [
  'probing',
  'chunking',
  'selecting',
  'notes',
  'evidence',
  'generating',
  'verifying',
] as const

type NodeState = 'pending' | 'running' | 'done' | 'failed' | 'waiting'

function stateOf(stage: string): NodeState {
  const p = props.progress
  const order = STAGES.indexOf(stage as (typeof STAGES)[number])
  if (p.stage === 'done') return 'done'
  if (p.stage === 'interrupted') {
    // 重启中断：停点用等待色（琥珀），等用户点继续
    const cur = STAGES.indexOf((p.last_stage ?? '') as (typeof STAGES)[number])
    if (cur === -1) return order === 0 ? 'waiting' : 'pending'
    if (order < cur) return 'done'
    if (order === cur) return 'waiting'
    return 'pending'
  }
  if (p.stage === 'failed' || p.stage === 'aborted') {
    // 失败/中止：以 last_stage（中断时所在阶段）定位红格；缺失时全灰
    const cur = STAGES.indexOf((p.last_stage ?? '') as (typeof STAGES)[number])
    if (cur === -1) return order === 0 ? 'failed' : 'pending'
    if (order < cur) return 'done'
    if (order === cur) return 'failed'
    return 'pending'
  }
  if (p.stage === 'selecting') {
    if (order < 2) return 'done'
    if (order === 2) return 'waiting'
    return 'pending'
  }
  const cur = STAGES.indexOf(p.stage as (typeof STAGES)[number])
  if (cur === -1) return 'pending'
  if (order < cur) return 'done'
  if (order === cur) return 'running'
  return 'pending'
}

const nodes = computed(() =>
  STAGES.map((s) => ({
    key: s,
    label: STAGE_LABELS[s],
    state: stateOf(s),
  })),
)

/** 切片格子：按段的真实在飞/完成状态着色（并行时在飞多段，呼吸蓝） */
const segmentCells = computed(() => {
  const p = props.progress
  const selectedCount = p.selected?.length ?? p.segment_count ?? 0
  if (!selectedCount) return []
  const notesStage = stateOf('notes')
  const failedSet = new Set(p.notes_failed ?? [])
  const activeSet = new Set(p.notes_active ?? [])
  const finishedSet = new Set(p.notes_finished ?? [])
  return Array.from({ length: selectedCount }, (_, i) => {
    const orig = p.selected?.[i] ?? i
    if (notesStage === 'done') {
      // 失败片段保持红色（进度事实不因阶段推进而消失）
      return failedSet.has(orig) ? ('failed' as NodeState) : ('done' as NodeState)
    }
    if (notesStage === 'running') {
      if (failedSet.has(orig)) return 'failed' as NodeState
      if (finishedSet.has(orig)) return 'done' as NodeState
      if (activeSet.has(orig)) return 'running' as NodeState
      return 'pending' as NodeState
    }
    return 'pending' as NodeState
  })
})

/** 生成文件格（7 个，按完成顺序点亮） */
const GEN_FILES = [
  'soul',
  'speech_patterns',
  'behavior_guide',
  'relationship_dynamics',
  'key_life_events',
  'limit',
  'index',
]

const genCells = computed(() =>
  GEN_FILES.map((f) => ({
    key: f,
    label: FILE_LABELS[f] ?? f,
    state: (() => {
      const st = stateOf('generating')
      if (st === 'done') return 'done' as NodeState
      if (st === 'running') {
        if (props.progress.files_done?.includes(f)) return 'done' as NodeState
        if (props.progress.current_files?.includes(f)) return 'running' as NodeState
        return 'pending' as NodeState
      }
      return 'pending' as NodeState
    })(),
  })),
)

// ---- 纵向布局：节点链 + 下方切片格/文件格，行高按内容动态 ----
const nodeW = 200
const nodeH = 38
const CX = 360 // 画布中心 x

interface NodeBox {
  key: string
  label: string
  state: NodeState
  y: number
}

const layout = computed(() => {
  const out: NodeBox[] = []
  let y = 10
  const cellAreaH = segmentCells.value.length
    ? Math.ceil(segmentCells.value.length / 12) * 16 + 20
    : 0
  for (const n of nodes.value) {
    out.push({ key: n.key, label: n.label, state: n.state, y })
    y += nodeH + 24
    if (n.key === 'notes') y += cellAreaH
    if (n.key === 'generating') y += Math.ceil(GEN_FILES.length / 3) * 30 + 6
  }
  return { nodes: out, height: y + 10, cellAreaH }
})

const svgHeight = computed(() => layout.value.height)

/** 节点框左上角 x */
const nodeX = CX - nodeW / 2

/** 纵向连线（节点中心底 → 下节点中心顶） */
const links = computed(() => {
  const list = layout.value.nodes
  return list.slice(0, -1).map((n, i) => {
    const next = list[i + 1]
    return {
      d: `M ${CX} ${n.y + nodeH} L ${CX} ${next.y}`,
      state: linkState(n.state, next.state),
    }
  })
})

/** 切片格坐标（节点正下方，居中网格，12 列） */
const segCellRects = computed(() => {
  const notesNode = layout.value.nodes.find((n) => n.key === 'notes')
  if (!notesNode || !segmentCells.value.length) return []
  const cols = 12
  const w = 13
  const gap = 3
  const totalW = Math.min(segmentCells.value.length, cols) * (w + gap) - gap
  const startX = CX - totalW / 2
  const startY = notesNode.y + nodeH + 6
  return segmentCells.value.map((state, i) => ({
    x: startX + (i % cols) * (w + gap),
    y: startY + Math.floor(i / cols) * (w + gap),
    w,
    state,
  }))
})

const segLabelY = computed(() => {
  const notesNode = layout.value.nodes.find((n) => n.key === 'notes')
  if (!notesNode || !segmentCells.value.length) return 0
  const rows = Math.ceil(segmentCells.value.length / 12)
  return notesNode.y + nodeH + 6 + rows * 16 + 4
})

const segLabel = computed(() => {
  const p = props.progress
  if (p.stage === 'notes') {
    return `切片 ${p.notes_done ?? 0}/${segmentCells.value.length}`
  }
  const failed = p.notes_failed?.length ?? 0
  const base = `共 ${segmentCells.value.length} 片`
  return failed ? `${base} · ${failed} 片失败` : base
})

/** 生成文件格坐标（3 列网格） */
const genCellRects = computed(() => {
  const genNode = layout.value.nodes.find((n) => n.key === 'generating')
  if (!genNode) return []
  const w = 96
  const h = 24
  const gap = 8
  const totalW = 3 * w + 2 * gap
  const startX = CX - totalW / 2
  const startY = genNode.y + nodeH + 8
  return genCells.value.map((c, i) => ({
    ...c,
    x: startX + (i % 3) * (w + gap),
    y: startY + Math.floor(i / 3) * (h + 6),
    w,
    h,
  }))
})

function linkState(fromState: NodeState, toState: NodeState): NodeState {
  if (fromState === 'done' && toState !== 'pending') return 'done'
  return 'pending'
}

const stateColor: Record<NodeState, string> = {
  pending: 'var(--border)',
  running: 'var(--accent)',
  done: '#3a9a50',
  failed: '#c05050',
  waiting: '#b08030',
}
</script>

<template>
  <div class="flow">
    <svg :viewBox="`0 0 720 ${svgHeight}`" class="dag" role="img" aria-label="拆解管线进度图">
      <!-- 连线 -->
      <path
        v-for="(l, i) in links"
        :key="i"
        :d="l.d"
        fill="none"
        :stroke="stateColor[l.state]"
        stroke-width="1.5"
        :stroke-dasharray="l.state === 'pending' ? '4 4' : undefined"
      />

      <!-- 节点 -->
      <g v-for="n in layout.nodes" :key="n.key">
        <rect
          :x="nodeX"
          :y="n.y"
          :width="nodeW"
          :height="nodeH"
          rx="8"
          class="node"
          :class="n.state"
        />
        <text :x="CX" :y="n.y + 24" class="node-label" :class="n.state">{{ n.label }}</text>
        <circle
          v-if="n.state === 'running'"
          :cx="nodeX - 8"
          :cy="n.y + nodeH / 2"
          r="3.5"
          class="pulse"
        />
        <circle
          v-else-if="n.state === 'waiting'"
          :cx="nodeX - 8"
          :cy="n.y + nodeH / 2"
          r="3.5"
          class="waiting-dot"
        />
      </g>

      <!-- notes 节点下：切片格子（可点开该段的调用日志） -->
      <rect
        v-for="(c, i) in segCellRects"
        :key="i"
        :x="c.x"
        :y="c.y"
        :width="c.w"
        :height="c.w"
        rx="2"
        class="cell clickable"
        :class="c.state"
        @click="emit('pick-segment', progress.selected?.[i] ?? i)"
      />
      <text v-if="segmentCells.length" :x="CX" :y="segLabelY + 10" class="cell-label" text-anchor="middle">
        {{ segLabel }}
      </text>

      <!-- generating 节点下：7 文件格（可点开该文件的调用日志） -->
      <g
        v-for="c in genCellRects"
        :key="c.key"
        class="clickable"
        @click="emit('pick-file', c.key)"
      >
        <rect :x="c.x" :y="c.y" :width="c.w" :height="c.h" rx="5" class="file-cell" :class="c.state" />
        <text :x="c.x + c.w / 2" :y="c.y + 16" class="file-label" :class="c.state">{{ c.label }}</text>
      </g>
    </svg>
  </div>
</template>

<style scoped>
.flow {
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg);
  padding: 8px 4px;
  overflow-x: auto;
}

.dag {
  display: block;
  width: 100%;
  min-width: 640px;
  height: auto;
}

.node {
  fill: var(--bg);
  stroke: var(--border);
  stroke-width: 1.2;
}

.node.running {
  stroke: var(--accent);
  fill: var(--accent-soft);
}

.node.done {
  stroke: #3a9a50;
  fill: rgba(58, 154, 80, 0.08);
}

.node.failed {
  stroke: #c05050;
  fill: rgba(192, 80, 80, 0.08);
}

.node.waiting {
  stroke: #b08030;
  fill: rgba(176, 128, 48, 0.1);
  stroke-dasharray: 5 3;
}

.node-label {
  text-anchor: middle;
  font-size: 13px;
  fill: var(--text-dim);
}

.node-label.running,
.node-label.done {
  fill: var(--text);
}

.node-label.failed {
  fill: #c05050;
}

.node-label.waiting {
  fill: #b08030;
}

.pulse {
  fill: var(--accent);
  animation: pulse 1.2s ease-in-out infinite;
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.25;
  }
}

.waiting-dot {
  fill: #b08030;
}

.cell.pending {
  fill: var(--border);
}

.clickable {
  cursor: pointer;
}

.clickable:hover .cell,
.clickable:hover.cell,
.clickable:hover .file-cell {
  stroke: var(--accent);
  stroke-width: 1.5;
}

.cell.done {
  fill: #3a9a50;
}

.cell.failed {
  fill: #c05050;
}

/* 在飞的段：呼吸蓝（透明度脉动） */
.cell.running {
  fill: var(--accent);
  animation: breathe 1.4s ease-in-out infinite;
}

@keyframes breathe {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.35;
  }
}

.cell-label {
  font-size: 11px;
  fill: var(--text-dim);
}

.file-cell {
  fill: var(--bg);
  stroke: var(--border);
  stroke-width: 1;
}

.file-cell.running {
  stroke: var(--accent);
  fill: var(--accent-soft);
  animation: breathe 1.4s ease-in-out infinite;
}

.file-cell.done {
  stroke: #3a9a50;
  fill: rgba(58, 154, 80, 0.1);
}

.file-label {
  text-anchor: middle;
  font-size: 11px;
  fill: var(--text-dim);
}

.file-label.running,
.file-label.done {
  fill: var(--text);
}
</style>
