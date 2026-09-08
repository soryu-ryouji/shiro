<script setup lang="ts">
// 调用日志详情弹窗：元信息 + 发出（system/user）+ 各次尝试（状态/用量/响应/思考/错误）。
import { computed } from 'vue'
import { useDialogMask } from '../composables/useDialog'

const props = defineProps<{ log: Record<string, unknown> }>()
const emit = defineEmits<{ close: [] }>()
const { onMaskDown, onMaskUp } = useDialogMask(() => emit('close'))

const NODE_LABELS: Record<string, string> = {
  notes: '逐段笔记',
  generating: '档案生成',
  verifying: '引文回查',
}

interface Attempt {
  at?: number
  ok: boolean | null
  response?: string
  thinking?: string
  error?: string | null
  usage?: { input?: number; output?: number; cache_read?: number; cache_write?: number } | null
  fallback?: boolean
  elapsed_ms?: number
}

const request = computed(() => (props.log.request ?? {}) as { system?: string; user?: string })
const attempts = computed<Attempt[]>(() => (props.log.attempts as Attempt[]) ?? [])
const ok = computed(() => props.log.ok as boolean | null)
const statusText = computed(() =>
  ok.value === true ? '成功' : ok.value === false ? '失败' : '进行中',
)
const elapsed = computed(() => {
  const ms = props.log.elapsed_ms as number | undefined
  return ms ? `${(ms / 1000).toFixed(1)}s` : '—'
})

function fmtTokens(u: Attempt['usage']): string {
  if (!u) return ''
  const parts: string[] = []
  if (u.input) {
    let s = `输入 ${fmtNum(u.input)}`
    if (u.cache_read && u.input) {
      s += `（缓存命中 ${Math.round((u.cache_read / u.input) * 100)}%）`
    }
    parts.push(s)
  }
  if (u.output) parts.push(`输出 ${fmtNum(u.output)}`)
  return parts.join(' · ')
}

function fmtNum(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
}

function fmtMs(ms?: number): string {
  if (!ms) return '—'
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`
}
</script>

<template>
  <div class="mask" @pointerdown="onMaskDown" @pointerup="onMaskUp">
    <div class="dialog">
      <h3 class="title">
        #{{ log.seq }} · {{ NODE_LABELS[String(log.node)] ?? log.node }} · {{ log.detail }}
      </h3>
      <p class="meta">
        状态：{{ statusText }} · 总耗时 {{ elapsed }}
        <template v-if="attempts.length > 1"> · 共 {{ attempts.length }} 次尝试</template>
      </p>

      <details class="block">
        <summary>发出 · system</summary>
        <pre>{{ request.system }}</pre>
      </details>
      <details class="block">
        <summary>发出 · user</summary>
        <pre>{{ request.user }}</pre>
      </details>

      <!-- 各次尝试（重试追加展示，错误历史全部保留） -->
      <div v-for="(a, i) in attempts" :key="i" class="attempt" :class="{ failed: a.ok === false }">
        <div class="attempt-head">
          <span class="attempt-no">第 {{ i + 1 }} 次尝试</span>
          <span class="attempt-status" :class="{ ok: a.ok === true, err: a.ok === false, run: a.ok === null }">
            {{ a.ok === true ? '已完成' : a.ok === false ? '失败' : '进行中' }}
          </span>
          <span class="dim">{{ fmtMs(a.elapsed_ms) }}</span>
          <span v-if="a.fallback" class="dim">· 非流式兜底</span>
          <span v-if="a.usage" class="usage">{{ fmtTokens(a.usage) }}</span>
        </div>

        <details v-if="a.thinking" class="block">
          <summary>思考过程</summary>
          <pre class="thinking">{{ a.thinking }}</pre>
        </details>

        <div class="block">
          <div class="block-label">收到</div>
          <pre v-if="a.response" class="resp">{{ a.response }}</pre>
          <pre v-else-if="a.error" class="resp err">{{ a.error }}</pre>
          <p v-else class="hint">（进行中，尚无响应）</p>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.mask {
  position: fixed;
  inset: 0;
  z-index: 200;
  background: rgba(0, 0, 0, 0.3);
  display: flex;
  align-items: center;
  justify-content: center;
}

.dialog {
  width: min(760px, 94vw);
  max-height: 84vh;
  overflow-y: auto;
  padding: 18px 20px;
  border-radius: 10px;
  background: var(--bg);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.18);
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.title {
  margin: 0;
  font-size: calc(15px * var(--font-scale-ui));
}

.meta {
  margin: 0;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}

.block summary {
  cursor: pointer;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
  margin-bottom: 4px;
}

.block-label {
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
  margin-bottom: 4px;
}

.block pre {
  max-height: 220px;
  overflow-y: auto;
  margin: 0;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg-soft);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: calc(12px * var(--font-scale-ui));
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-all;
}

.attempt {
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.attempt.failed {
  border-color: rgba(192, 80, 80, 0.4);
  background: rgba(192, 80, 80, 0.03);
}

.attempt-head {
  display: flex;
  align-items: baseline;
  gap: 10px;
  flex-wrap: wrap;
}

.attempt-no {
  font-weight: 600;
  font-size: calc(12px * var(--font-scale-ui));
}

.attempt-status.ok {
  color: #3a9a50;
}

.attempt-status.err {
  color: #c05050;
}

.attempt-status.run {
  color: var(--accent);
}

.dim {
  color: var(--text-dim);
  font-size: calc(11px * var(--font-scale-ui));
}

.usage {
  margin-left: auto;
  color: var(--text-dim);
  font-size: calc(11px * var(--font-scale-ui));
}

.thinking {
  color: var(--text-dim);
}

.resp.err {
  color: #c05050;
}

.hint {
  margin: 0;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}
</style>
