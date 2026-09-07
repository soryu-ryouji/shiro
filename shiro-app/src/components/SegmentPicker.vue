<script setup lang="ts">
// 段选择器（选择闸门弹窗）：列出全部段（标识 + 字数 + 开头预览），
// 命中角色名的段标绿；快选：全选 / 全不选 / 只选名称段。不设上限，全选即全收。
import { computed, ref, watch } from 'vue'
import { useDialogMask } from '../composables/useDialog'
import type { components } from '../api-types'

type SegmentInfoDto = components['schemas']['SegmentInfoDto']

const props = defineProps<{
  segments: SegmentInfoDto[]
  character: string
}>()
const emit = defineEmits<{ submit: [selected: number[]]; close: [] }>()
const { onMaskDown, onMaskUp } = useDialogMask(() => emit('close'))

const checked = ref<Set<number>>(new Set(props.segments.map((s) => s.index)))

watch(
  () => props.segments,
  (list) => {
    checked.value = new Set(list.map((s) => s.index))
  },
)

const nameSegs = computed(() => props.segments.filter((s) => s.has_name))
const totalChars = computed(() => props.segments.reduce((sum, s) => sum + s.chars, 0))
const selectedChars = computed(() =>
  props.segments.filter((s) => checked.value.has(s.index)).reduce((sum, s) => sum + s.chars, 0),
)

function fmtChars(n: number): string {
  return n >= 10_000 ? `${(n / 10_000).toFixed(1)} 万字` : `${n.toLocaleString()} 字`
}

function toggle(i: number) {
  const next = new Set(checked.value)
  if (next.has(i)) next.delete(i)
  else next.add(i)
  checked.value = next
}

function selectAll() {
  checked.value = new Set(props.segments.map((s) => s.index))
}
function selectNone() {
  checked.value = new Set()
}
function selectNameOnly() {
  checked.value = new Set(nameSegs.value.map((s) => s.index))
}

function submit() {
  emit('submit', [...checked.value].sort((a, b) => a - b))
  emit('close')
}
</script>

<template>
  <div class="mask" @pointerdown="onMaskDown" @pointerup="onMaskUp">
    <div class="dialog">
      <h3 class="title">
        拆分结果 · 共 {{ segments.length }} 段（全文 {{ fmtChars(totalChars) }}）
      </h3>
      <p class="hint">
        勾选值得分析的段落；命中「{{ character }}」的段已标绿（{{ nameSegs.length }} 段）。
      </p>
      <div class="quick">
        <button class="link" @click="selectAll">全选</button>
        <button class="link" @click="selectNone">全不选</button>
        <button class="link accent" @click="selectNameOnly">只选名称段（{{ nameSegs.length }}）</button>
      </div>

      <div class="seg-list">
        <button
          v-for="s in segments"
          :key="s.index"
          class="seg"
          :class="{ on: checked.has(s.index), named: s.has_name }"
          @click="toggle(s.index)"
        >
          <span class="check" :class="{ on: checked.has(s.index) }">{{
            checked.has(s.index) ? '✓' : ''
          }}</span>
          <span class="seg-main">
            <span class="seg-label">
              {{ s.label }}
              <span v-if="s.has_name" class="name-mark">含 {{ character }}</span>
            </span>
            <span class="seg-excerpt">{{ s.excerpt }}</span>
          </span>
          <span class="seg-chars">{{ s.chars.toLocaleString() }}</span>
        </button>
      </div>

      <div class="footer">
        <span class="hint">已选 {{ checked.size }} 段 · {{ fmtChars(selectedChars) }}</span>
        <button class="btn" @click="emit('close')">取消</button>
        <button class="btn primary" :disabled="!checked.size" @click="submit">
          开始分析（{{ checked.size }} 段）
        </button>
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
  width: min(720px, 92vw);
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 18px 20px;
  border-radius: 10px;
  background: var(--bg);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.18);
}

.title {
  margin: 0;
  font-size: calc(15px * var(--font-scale-ui));
}

.hint {
  margin: 0;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}

.quick {
  display: flex;
  gap: 12px;
}

.link {
  border: none;
  background: none;
  padding: 0;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
  cursor: pointer;
}

.link.accent {
  color: var(--accent);
}

.seg-list {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-height: 120px;
}

.seg {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-left-width: 3px;
  border-radius: 8px;
  background: var(--bg);
  text-align: left;
  cursor: pointer;
}

@media (hover: hover) {
  .seg:hover {
    border-color: var(--accent);
  }
}

/* 命中角色名的段：绿色左边条 + 淡绿底（未选中时也可见） */
.seg.named {
  border-left-color: #3a9a50;
  background: rgba(58, 154, 80, 0.05);
}

.seg.on {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.seg.on.named {
  border-left-color: #3a9a50;
}

.name-mark {
  margin-left: 6px;
  padding: 0 6px;
  border-radius: 999px;
  background: rgba(58, 154, 80, 0.14);
  color: #3a9a50;
  font-size: calc(10px * var(--font-scale-ui));
  font-weight: 400;
}

.check {
  flex: none;
  width: 16px;
  height: 16px;
  margin-top: 2px;
  border: 1px solid var(--border);
  border-radius: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  color: #fff;
  background: var(--bg);
}

.check.on {
  background: var(--accent);
  border-color: var(--accent);
}

.seg-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.seg-label {
  font-size: calc(13px * var(--font-scale-ui));
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.seg-excerpt {
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.seg-chars {
  flex: none;
  margin-top: 2px;
  color: var(--text-dim);
  font-size: calc(11px * var(--font-scale-ui));
}

.footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  border-top: 1px solid var(--border);
  padding-top: 10px;
}

.footer .hint {
  flex: 1;
}

.btn {
  height: 30px;
  padding: 0 16px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  cursor: pointer;
}

.btn.primary {
  border-color: var(--accent);
  background: var(--accent);
  color: #fff;
}

.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
</style>
