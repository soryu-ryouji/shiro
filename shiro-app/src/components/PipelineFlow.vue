<script setup lang="ts">
// 拆解管线流程图（SVG DAG）：节点按阶段状态着色，切块节点显示切片格子，生成节点展开 7 文件。
// 纯展示组件，数据来自任务进度（daemon Progress）。
import { computed } from 'vue'
import type { components } from '../api-types'
import { FILE_LABELS, STAGE_LABELS } from '../stores/deconstruct'

type Progress = components['schemas']['Progress']

const props = defineProps<{ progress: Progress }>()

/** 管线阶段顺序（节点链） */
const STAGES = ['probing', 'chunking', 'notes', 'evidence', 'generating', 'verifying'] as const

type NodeState = 'pending' | 'running' | 'done' | 'failed'

const notesDone = computed(() => props.progress.notes_done ?? 0)
const notesFailed = computed(() => props.progress.notes_failed ?? [])
const filesDone = computed(() => props.progress.files_done ?? [])

function stateOf(stage: string): NodeState {
  const p = props.progress
  if (p.stage === 'failed') {
    // 失败：当前停留阶段标红，之前完成的保持 done
    const idx = STAGES.indexOf(stage as (typeof STAGES)[number])
    const cur = STAGES.indexOf(p.stage as (typeof STAGES)[number])
    if (idx < cur) return 'done'
    if (idx === cur) return 'failed'
    return 'pending'
  }
  const order = STAGES.indexOf(stage as (typeof STAGES)[number])
  const cur = STAGES.indexOf(p.stage as (typeof STAGES)[number])
  if (p.stage === 'done') return 'done'
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

/** 切片格子：notes 阶段逐片点亮（probing/chunking 完成后按段数生成） */
const segmentCells = computed(() => {
  const p = props.progress
  if (!p.segment_count || p.segment_count === 0) return []
  return Array.from({ length: p.segment_count }, (_, i) => {
    const notesStage = stateOf('notes')
    if (notesStage === 'done') return 'done' as NodeState
    if (notesStage === 'running') {
      if (notesFailed.value.includes(i)) return 'failed' as NodeState
      return i < notesDone.value ? ('done' as NodeState) : ('pending' as NodeState)
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
        if (filesDone.value.includes(f)) return 'done' as NodeState
        if (props.progress.current_file === f) return 'running' as NodeState
        return 'pending' as NodeState
      }
      return 'pending' as NodeState
    })(),
  })),
)

// ---- 布局（固定坐标，自上而下的数据流导图） ----
const W = 720
const nodeW = 168
const nodeH = 40
// 每行两个节点的两列布局：probing→chunking / notes→evidence / generating(带文件格) / verifying
const layout = computed(() => {
  const rows: { y: number; items: { key: string; x: number; label: string; state: NodeState }[] }[] = [
    { y: 10, items: [nodes.value[0], nodes.value[1]].map((n, i) => ({ ...n, x: i === 0 ? 140 : 420 })) },
    { y: 100, items: [nodes.value[2], nodes.value[3]].map((n, i) => ({ ...n, x: i === 0 ? 140 : 420 })) },
    { y: 190, items: [{ ...nodes.value[4], x: 280 }] },
    { y: 280, items: [{ ...nodes.value[5], x: 280 }] },
  ]
  return rows
})

const svgHeight = computed(() => 280 + nodeH + 20)

/** 连线：完成的数据流高亮 */
function linkState(fromState: NodeState, toState: NodeState): NodeState {
  if (fromState === 'done' && (toState === 'done' || toState === 'running')) return 'done'
  if (fromState === 'done' && toState === 'failed') return 'done'
  return 'pending'
}

const links = computed(() => {
  const [probe, chunk, notes, evidence, gen, verify] = nodes.value.map((n) => n.state)
  return [
    // probing → chunking（同行水平）
    { d: 'M 308 30 L 420 30', state: linkState(probe, chunk) },
    // chunking → notes（下行拐弯）
    { d: 'M 504 50 L 504 75 L 224 75 L 224 100', state: linkState(chunk, notes) },
    // notes → evidence（同行水平）
    { d: 'M 308 120 L 420 120', state: linkState(notes, evidence) },
    // evidence → generating（下行拐弯）
    { d: 'M 504 140 L 504 165 L 364 165 L 364 190', state: linkState(evidence, gen) },
    // generating → verifying（垂直下行）
    { d: 'M 364 230 L 364 280', state: linkState(gen, verify) },
  ]
})

const stateColor: Record<NodeState, string> = {
  pending: 'var(--border)',
  running: 'var(--accent)',
  done: '#3a9a50',
  failed: '#c05050',
}
</script>

<template>
  <div class="flow">
    <svg :viewBox="`0 0 ${W} ${svgHeight}`" class="dag" role="img" aria-label="拆解管线进度图">
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
      <g v-for="row in layout" :key="row.y">
        <g v-for="n in row.items" :key="n.key">
          <rect
            :x="n.x"
            :y="row.y"
            :width="nodeW"
            :height="nodeH"
            rx="8"
            class="node"
            :class="n.state"
          />
          <text :x="n.x + nodeW / 2" :y="row.y + 25" class="node-label" :class="n.state">
            {{ n.label }}
          </text>
          <!-- 运行中脉冲圈 -->
          <circle v-if="n.state === 'running'" :cx="n.x - 6" :cy="row.y + nodeH / 2" r="3.5" class="pulse" />
        </g>
      </g>

      <!-- notes 节点下：切片格子 -->
      <g v-if="segmentCells.length" class="cells">
        <rect
          v-for="(s, i) in segmentCells"
          :key="i"
          :x="148 + (i % 12) * 15"
          :y="146 + Math.floor(i / 12) * 15"
          width="11"
          height="11"
          rx="2"
          class="cell"
          :class="s"
        />
        <text v-if="progress.stage === 'notes'" :x="148" :y="146 + Math.ceil(segmentCells.length / 12) * 15 + 14" class="cell-label">
          切片 {{ progress.notes_done }}/{{ progress.segment_count }}
        </text>
        <text v-else :x="148" :y="146 + Math.ceil(segmentCells.length / 12) * 15 + 14" class="cell-label">
          共 {{ progress.segment_count }} 片
        </text>
      </g>

      <!-- generating 节点下：7 文件格 -->
      <g class="cells">
        <template v-for="(f, i) in genCells" :key="f.key">
          <rect
            :x="90 + i * 82"
            :y="236"
            width="72"
            height="24"
            rx="5"
            class="file-cell"
            :class="f.state"
          />
          <text :x="90 + i * 82 + 36" :y="251" class="file-label" :class="f.state">
            {{ f.label }}
          </text>
        </template>
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

.cell {
  stroke: none;
}

.cell.pending {
  fill: var(--border);
}

.cell.done {
  fill: #3a9a50;
}

.cell.running {
  fill: var(--accent);
}

.cell.failed {
  fill: #c05050;
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
