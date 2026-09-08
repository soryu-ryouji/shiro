<script setup lang="ts">
// 角色制作设置面板（弹窗）：模型选择 / 切片并发数量 / 切片长度。
// 三项都是制作任务的运行参数：调节即保存（PUT 逐项提交），无需确认按钮。
// 切片长度只影响新建任务的切块；已建任务保持创建时的切片。
import { computed, onMounted } from 'vue'
import { deconstructStore } from '../stores/deconstruct'
import { modelStore, profileLabel } from '../stores/model'
import { useDialogMask } from '../composables/useDialog'
import Icon from './Icon.vue'
import SearchSelect from './SearchSelect.vue'

const emit = defineEmits<{ close: [] }>()
const { onMaskDown, onMaskUp } = useDialogMask(() => emit('close'))

onMounted(() => {
  void deconstructStore.loadSettings()
  if (!modelStore.loaded) void modelStore.load()
})

/** 档案下拉：label 显示（供应商 · 模型），选中后按 label 反查 key 提交 */
const modelOptions = computed(() =>
  modelStore.profiles.map((p) => `${profileLabel(p)} · ${p.model}`),
)

const modelText = computed({
  get: () => {
    const key = deconstructStore.settings?.model ?? modelStore.defaultKey
    const p = modelStore.profiles.find((x) => x.key === key)
    return p ? `${profileLabel(p)} · ${p.model}` : ''
  },
  set: (v: string) => {
    const hit = modelStore.profiles.find((p) => `${profileLabel(p)} · ${p.model}` === v)
    if (hit && hit.key !== deconstructStore.settings?.model) {
      void deconstructStore.saveSettings({ model: hit.key })
    }
  },
})

/** 切片并发数量（1-8） */
const CONC_MIN = 1
const CONC_MAX = 8
const concurrency = computed(() => deconstructStore.settings?.max_concurrency ?? 4)
const busy = computed(() => deconstructStore.settingsSaving)

function stepConcurrency(delta: number) {
  const next = Math.min(CONC_MAX, Math.max(CONC_MIN, concurrency.value + delta))
  if (next !== concurrency.value) void deconstructStore.saveSettings({ max_concurrency: next })
}

/** 切片长度（字；步进 1000） */
const SEG_MIN = 3000
const SEG_MAX = 30000
const SEG_STEP = 1000
const segmentChars = computed(() => deconstructStore.settings?.segment_chars ?? 10000)
const segLabel = computed(() =>
  segmentChars.value >= 10000
    ? `${(segmentChars.value / 10000).toFixed(1)} 万字`
    : `${segmentChars.value.toLocaleString()} 字`,
)

function stepSegment(delta: number) {
  const next = Math.min(SEG_MAX, Math.max(SEG_MIN, segmentChars.value + delta * SEG_STEP))
  if (next !== segmentChars.value) void deconstructStore.saveSettings({ segment_chars: next })
}

/** 跳 Model 页注册模型 */
function gotoModel() {
  emit('close')
  window.dispatchEvent(new CustomEvent('shiro:navigate', { detail: 'model' }))
}
</script>

<template>
  <div class="mask" @pointerdown="onMaskDown" @pointerup="onMaskUp">
    <div class="dialog">
      <header class="head">
        <h3>角色制作设置</h3>
        <button class="icon-btn" title="关闭" @click="emit('close')">
          <Icon name="close" :size="14" />
        </button>
      </header>

      <div v-overlay-scrollbar class="body">
        <p v-if="deconstructStore.settingsLoading" class="hint">加载中…</p>
        <template v-else>
          <!-- 模型选择 -->
          <div class="setting">
            <div class="info">
              <span class="name">模型</span>
              <span class="desc">制作任务使用的模型档案（在 Model 页注册）。</span>
            </div>
            <div v-if="modelStore.profiles.length" class="ctrl">
              <SearchSelect v-model="modelText" :options="modelOptions" strict placeholder="选择模型档案" />
            </div>
            <p v-else class="empty">
              还没有已注册的模型。<a class="link" @click="gotoModel">去 Model 页注册</a>
            </p>
          </div>

          <!-- 切片并发数量 -->
          <div class="setting">
            <div class="info">
              <span class="name">切片并发数量</span>
              <span class="desc">
                逐段笔记与档案生成的 AI 调用并发上限。上游频繁限流时调低。
              </span>
            </div>
            <div class="stepper">
              <button
                class="step-btn"
                :disabled="busy || concurrency <= CONC_MIN"
                @click="stepConcurrency(-1)"
              >
                −
              </button>
              <span class="step-val">{{ concurrency }}</span>
              <button
                class="step-btn"
                :disabled="busy || concurrency >= CONC_MAX"
                @click="stepConcurrency(1)"
              >
                ＋
              </button>
            </div>
          </div>

          <!-- 切片长度 -->
          <div class="setting">
            <div class="info">
              <span class="name">切片长度</span>
              <span class="desc">
                无章节结构文本的切块目标长度；有章节时按标题切。只影响之后新建的任务。
              </span>
            </div>
            <div class="stepper">
              <button
                class="step-btn"
                :disabled="busy || segmentChars <= SEG_MIN"
                @click="stepSegment(-1)"
              >
                −
              </button>
              <span class="step-val wide">{{ segLabel }}</span>
              <button
                class="step-btn"
                :disabled="busy || segmentChars >= SEG_MAX"
                @click="stepSegment(1)"
              >
                ＋
              </button>
            </div>
          </div>

          <p v-if="deconstructStore.error" class="hint error">{{ deconstructStore.error }}</p>
          <p v-if="busy" class="hint saving">保存中…</p>
        </template>
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
  width: min(520px, 92vw);
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  border-radius: 10px;
  background: var(--bg);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.18);
  overflow: hidden;
}

.head {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px 10px;
}

.head h3 {
  margin: 0;
  font-size: calc(15px * var(--font-scale-ui));
}

.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}

@media (hover: hover) {
  .icon-btn:hover {
    background: var(--border);
    color: var(--text);
  }
}

.body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 0 18px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.setting {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--bg);
}

.info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.name {
  font-size: calc(13px * var(--font-scale-ui));
  font-weight: 600;
}

.desc {
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--text-dim);
  line-height: 1.5;
}

.ctrl {
  flex: none;
  width: 220px;
}

.empty {
  flex: none;
  margin: 0;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}

.link {
  color: var(--accent);
  cursor: pointer;
  text-decoration: underline;
}

.stepper {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
}

.step-btn {
  width: 28px;
  height: 28px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  color: var(--text);
  font-size: 14px;
  cursor: pointer;
}

.step-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.step-val {
  min-width: 20px;
  text-align: center;
  font-weight: 600;
}

.step-val.wide {
  min-width: 56px;
}

.hint {
  margin: 0;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}

.hint.error {
  color: #d05050;
}

.hint.saving {
  text-align: right;
}
</style>
