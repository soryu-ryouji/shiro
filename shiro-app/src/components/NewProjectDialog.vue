<script setup lang="ts">
// 新建项目对话框：项目名（选填，写入 .shiro/project.toml 的 name 用于显示）+
// 项目文件夹（系统目录选择框，经 IPC；任意已有文件夹，VSCode「打开文件夹」式）。
// 显示名与文件夹名解耦；遮罩/Esc 关闭行为见 useDialog。
import { computed, ref } from 'vue'
import { apiPost } from '../api'
import { hasShell, shell } from '../platform'
import { useDialogMask } from '../composables/useDialog'
import type { components } from '../api-types'

const emit = defineEmits<{ close: []; created: [] }>()
const { onMaskDown, onMaskUp } = useDialogMask(() => emit('close'))

const name = ref('')
const dir = ref('')
const busy = ref(false)
const error = ref('')

const canCreate = computed(() => dir.value !== '' && !busy.value)

async function pickDir() {
  const picked = await shell.pickDirectory('选择项目文件夹')
  if (picked) dir.value = picked
}

async function create() {
  if (!canCreate.value) return
  busy.value = true
  error.value = ''
  try {
    await apiPost<components['schemas']['ProjectItem']>('/api/v1/projects/create', {
      path: dir.value,
      name: name.value.trim(),
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
        <span class="field-label">项目文件夹</span>
        <div class="dir-row">
          <input :value="dir" class="input" type="text" placeholder="选择已有的文件夹" readonly />
          <button class="btn" :disabled="!hasShell" @click="pickDir">选择…</button>
        </div>
      </label>

      <label class="field">
        <span class="field-label">项目名</span>
        <input v-model="name" class="input" type="text" placeholder="选填，默认使用文件夹名" @keydown.enter="create" />
      </label>
      <p class="hint">项目名写入项目配置（.shiro/project.toml）用于显示，文件夹保持原名不动；文件夹内的已有内容不受影响。</p>

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
