<script setup lang="ts">
// 通用确认对话框（危险操作的二次确认）。遮罩配对关闭与 Esc 见 useDialog。
import { useDialogMask } from '../composables/useDialog'

defineProps<{ title: string; message: string; confirmLabel: string }>()
const emit = defineEmits<{ close: []; confirm: [] }>()
const { onMaskDown, onMaskUp } = useDialogMask(() => emit('close'))
</script>

<template>
  <div class="mask" @pointerdown="onMaskDown" @pointerup="onMaskUp">
    <div class="dialog">
      <h3 class="dialog-title">{{ title }}</h3>
      <p class="message">{{ message }}</p>
      <div class="footer">
        <button class="btn" @click="emit('close')">取消</button>
        <button class="btn danger" @click="emit('confirm'); emit('close')">{{ confirmLabel }}</button>
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
  margin: 0 0 10px;
  font-size: 15px;
}

.message {
  margin: 0;
  font-size: 13px;
  color: var(--text-dim);
  line-height: 1.6;
  word-break: break-all;
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

.btn.danger {
  border-color: var(--danger);
  background: var(--danger);
  color: #fff;
}

@media (hover: hover) {
  .btn:not(.danger):hover {
    background: var(--bg-soft);
  }

  .btn.danger:hover {
    background: #a52f23;
  }
}
</style>
