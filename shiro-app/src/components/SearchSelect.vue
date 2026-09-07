<script setup lang="ts">
// 可检索下拉框：聚焦展开选项、输入即过滤（子串匹配）、点选或回车填入全名；
// 也接受任意手填值（清单外的新模型 id 直接用）。无第三方依赖。
import { computed, ref, watch } from 'vue'

const props = withDefaults(
  defineProps<{
    modelValue: string
    options: string[]
    placeholder?: string
    /** strict：只能选列表项；失焦时非列表值自动回弹（供应商选择用） */
    strict?: boolean
  }>(),
  { placeholder: '', strict: false },
)
const emit = defineEmits<{ 'update:modelValue': [v: string] }>()

const open = ref(false)
const text = ref(props.modelValue)
/** 用户是否已在本轮输入过：未输入时展开显示全部选项（解决初始值挡住列表的问题） */
const typed = ref(false)

// 外部值变化（如切换供应商）同步进输入框，并重置输入状态
watch(
  () => props.modelValue,
  (v) => {
    if (v !== text.value) text.value = v
    typed.value = false
  },
)

const filtered = computed(() => {
  const q = text.value.trim().toLowerCase()
  // 打开后未输入 → 全部；开始输入 → 按关键词过滤
  if (!typed.value || !q) return props.options
  return props.options.filter((o) => o.toLowerCase().includes(q))
})

function onInput(e: Event) {
  text.value = (e.target as HTMLInputElement).value
  typed.value = true
  emit('update:modelValue', text.value)
  open.value = true
}

function choose(o: string) {
  text.value = o
  typed.value = false
  emit('update:modelValue', o)
  open.value = false
}

function onEnter() {
  // 只剩一个匹配时直接选中；否则保留手填文本（onInput 已即时同步）
  if (filtered.value.length === 1) choose(filtered.value[0])
  else open.value = false
}

function onBlur() {
  // 延迟关闭，让选项的 mousedown 先触发
  setTimeout(() => {
    open.value = false
    typed.value = false
    // strict 模式：手填的非法值回弹为当前选中值
    if (props.strict && !props.options.includes(text.value)) {
      text.value = props.modelValue
    }
  }, 120)
}

/** 聚焦展开：重置输入状态，显示全部选项 */
function openDropdown() {
  typed.value = false
  open.value = true
}

/** caret 点击：收起/展开切换（mousedown 防 blur 先关）；展开时显示全部 */
function onCaret(e: MouseEvent) {
  e.preventDefault()
  if (open.value) {
    open.value = false
  } else {
    openDropdown()
    ;(e.currentTarget as HTMLElement).closest('.combo')?.querySelector('input')?.focus()
  }
}
</script>

<template>
  <div class="combo">
    <input
      :value="text"
      :placeholder="placeholder"
      spellcheck="false"
      autocomplete="off"
      @input="onInput"
      @focus="openDropdown"
      @blur="onBlur"
      @keydown.enter.prevent="onEnter"
      @keydown.esc.prevent="open = false"
    />
    <button
      v-if="options.length"
      class="caret"
      :class="{ open }"
      type="button"
      tabindex="-1"
      aria-label="展开/收起选项"
      @mousedown="onCaret"
    >
      <svg width="10" height="6" viewBox="0 0 10 6" fill="none">
        <path d="M1 1l4 4 4-4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
    </button>
    <div v-if="open && filtered.length" class="drop">
      <button
        v-for="o in filtered"
        :key="o"
        class="opt"
        :class="{ hit: o === text }"
        @mousedown.prevent="choose(o)"
      >
        {{ o }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.combo {
  position: relative;
}

input {
  width: 100%;
  height: 32px;
  padding: 0 26px 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: calc(13px * var(--font-scale-ui));
  outline: none;
  background: var(--bg);
  color: var(--text);
  box-sizing: border-box;
}

input:focus {
  border-color: var(--accent);
}

.caret {
  position: absolute;
  right: 6px;
  top: 50%;
  transform: translateY(-50%);
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: 4px;
  background: none;
  color: var(--text-dim);
  cursor: pointer;
  transition: transform 0.15s ease;
}

.caret.open {
  transform: translateY(-50%) rotate(180deg);
  color: var(--accent);
}

.drop {
  position: absolute;
  z-index: 20;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  max-height: 220px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  padding: 4px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.12);
}

.opt {
  padding: 6px 10px;
  border: none;
  border-radius: 5px;
  background: none;
  color: var(--text);
  font-size: calc(13px * var(--font-scale-ui));
  text-align: left;
  cursor: pointer;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.opt.hit {
  color: var(--accent);
  font-weight: 600;
}

@media (hover: hover) {
  .opt:hover {
    background: var(--accent-soft);
  }
}
</style>
