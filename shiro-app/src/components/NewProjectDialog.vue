<script setup lang="ts">
// 新建项目对话框：项目名 + 父目录（系统目录选择框，经 IPC）→ daemon 创建并导入记录。
// 遮罩/Esc 关闭行为见 useDialog。
import { computed, ref } from 'vue'
import { apiFetch } from '../api'
import { hasShell, shell } from '../platform'
import { useDialogMask } from '../composables/useDialog'
import type { components } from '../api-types'

const emit = defineEmits<{ close: []; created: [] }>()
const { onMaskDown, onMaskUp } = useDialogMask(() => emit('close'))

const name = ref('')
const parent = ref('')
const busy = ref(false)
const error = ref('')

const canCreate = computed(() => name.value.trim() !== '' && parent.value !== '' && !busy.value)

async function pickParent() {
  const picked = await shell.pickDirectory('选择项目存放目录')
  if (picked) parent.value = picked
}

async function create() {
  if (!canCreate.value) return
  busy.value = true
  error.value = ''
  try {
    await apiFetch<components['schemas']['ProjectItem']>('/api/v1/projects', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ parent: parent.value, name: name.value.trim() }),
    })
    emit('created')
    emit('close')
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="mask" @pointerdown="onMaskDown" @pointerup="onMaskUp">
    <div class="dialog">
      <h3 class="dialog-title">新建项目</h3>

      <label class="field">
        <span class="field-label">项目名</span>
        <input v-model="name" class="input" type="text" placeholder="例如：我的第一部长篇" autofocus @keydown.enter="create" />
      </label>

      <label class="field">
        <span class="field-label">存放目录</span>
        <div class="dir-row">
          <input :value="parent" class="input" type="text" placeholder="选择父目录" readonly />
          <button class="btn" :disabled="!hasShell" @click="pickParent">选择…</button>
        </div>
      </label>
      <p class="hint">将在存放目录下创建同名项目文件夹，并初始化 .shiro/ 与「正文」目录。</p>

      <p v-if="error" class="error">{{ error }}</p>

      <div class="footer">
        <button class="btn" @click="emit('close')">取消</button>
        <button class="btn primary" :disabled="!canCreate" @click="create">{{ busy ? '创建中…' : '创建' }}</button>
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
  width: min(460px, 90vw);
  padding: 20px;
  border-radius: 10px;
  background: var(--bg);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.18);
}

.dialog-title {
  margin: 0 0 16px;
  font-size: calc(15px * var(--font-scale-ui));
}

.field {
  display: block;
  margin-bottom: 12px;
}

.field-label {
  display: block;
  margin-bottom: 6px;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text-dim);
}

.input {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  background: var(--bg);
  outline: none;
}

.input:focus {
  border-color: var(--accent);
}

.dir-row {
  display: flex;
  gap: 8px;
}

.dir-row .input {
  flex: 1;
  min-width: 0;
}

.hint {
  margin: 0 0 12px;
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--text-dim);
}

.error {
  margin: 0 0 12px;
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--danger);
}

.footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.btn {
  height: 30px;
  padding: 0 14px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  cursor: pointer;
}

.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn.primary {
  border-color: var(--accent);
  background: var(--accent);
  color: #fff;
}

@media (hover: hover) {
  .btn:not(:disabled):hover {
    background: var(--bg-soft);
  }

  .btn.primary:not(:disabled):hover {
    background: #405ed6;
  }
}
</style>
