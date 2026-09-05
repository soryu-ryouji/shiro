<script setup lang="ts">
// 通用重命名/输入对话框：单输入框 + 确认/取消。遮罩配对关闭与 Esc 见 useDialog。
import { nextTick, onMounted, ref } from 'vue'
import { useDialogMask } from '../composables/useDialog'

const props = defineProps<{ title: string; initial: string; placeholder?: string }>()
const emit = defineEmits<{ close: []; submit: [name: string] }>()
const { onMaskDown, onMaskUp } = useDialogMask(() => emit('close'))

const value = ref(props.initial)
const input = ref<HTMLInputElement | null>(null)

onMounted(async () => {
  await nextTick()
  input.value?.focus()
  input.value?.select()
})

function submit() {
  const name = value.value.trim()
  if (!name || name === props.initial) {
    emit('close')
    return
  }
  emit('submit', name)
  emit('close')
}
</script>

<template>
  <div class="mask" @pointerdown="onMaskDown" @pointerup="onMaskUp">
    <div class="dialog">
      <h3 class="dialog-title">{{ title }}</h3>
      <input
        ref="input"
        v-model="value"
        class="input"
        type="text"
        :placeholder="placeholder"
        @keydown.enter="submit"
      />
      <div class="footer">
        <button class="btn" @click="emit('close')">取消</button>
        <button class="btn primary" @click="submit">确定</button>
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
  width: min(400px, 90vw);
  padding: 20px;
  border-radius: 10px;
  background: var(--bg);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.18);
}

.dialog-title {
  margin: 0 0 14px;
  font-size: 15px;
}

.input {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: 13px;
  outline: none;
}

.input:focus {
  border-color: var(--accent);
}

.footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}

.btn {
  height: 30px;
  padding: 0 14px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  font-size: 13px;
  color: var(--text);
  cursor: pointer;
}

.btn.primary {
  border-color: var(--accent);
  background: var(--accent);
  color: #fff;
}

@media (hover: hover) {
  .btn:not(.primary):hover {
    background: var(--bg-soft);
  }

  .btn.primary:hover {
    background: #405ed6;
  }
}
</style>
