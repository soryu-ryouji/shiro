<script setup lang="ts">
// 设置面板壳：左侧导航 + 右侧分区的两栏结构。交互要点（同 hawk）：
// - 遮罩「按下与抬起都落在遮罩上」才关闭：面板内拖动选择文本滑出面板松开，按 click.self 判定会误关
// - Esc 关闭
// - 打开期间挂 body.dialog-open 挂起窗口拖拽区（drag 由 OS 命中测试优先消费，不挂起点遮罩会变拖动窗口）
import { ref } from 'vue'
import { apiBase } from '../api'
import { useDialogMask } from '../composables/useDialog'
import pkg from '../../package.json'

const emit = defineEmits<{ close: [] }>()
const { onMaskDown, onMaskUp } = useDialogMask(() => emit('close'))

const SECTIONS = [
  { key: 'general', label: '通用' },
  { key: 'lan', label: '局域网' },
] as const
type SectionKey = (typeof SECTIONS)[number]['key']
const section = ref<SectionKey>('general')

</script>

<template>
  <div class="mask" @pointerdown="onMaskDown" @pointerup="onMaskUp">
    <div class="dialog">
      <nav class="dialog-nav">
        <button
          v-for="s in SECTIONS"
          :key="s.key"
          class="dialog-nav-item"
          :class="{ active: section === s.key }"
          @click="section = s.key"
        >
          {{ s.label }}
        </button>
      </nav>

      <div class="dialog-body">
        <section v-show="section === 'general'" class="pane">
          <h3>通用</h3>
          <dl class="info">
            <dt>版本</dt>
            <dd>shiro {{ pkg.version }}</dd>
            <dt>后端</dt>
            <dd class="mono">{{ apiBase }}</dd>
          </dl>
        </section>

        <section v-show="section === 'lan'" class="pane">
          <h3>局域网访问</h3>
          <p class="hint">
            开启后局域网内的设备（iPad、手机等）可通过浏览器访问 shiro。访问 key 的生成与管理将在后续版本提供（设计见
            docs/architecture.md「局域网访问」）。
          </p>
          <label class="row disabled">
            <input type="checkbox" disabled />
            <span>开启局域网访问（待实现）</span>
          </label>
        </section>
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
  width: min(640px, 90vw);
  height: min(440px, 80vh);
  display: flex;
  border-radius: 10px;
  background: var(--bg);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.18);
  overflow: hidden;
}

.dialog-nav {
  flex: none;
  width: 140px;
  padding: 12px 8px;
  background: var(--bg-soft);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.dialog-nav-item {
  padding: 7px 10px;
  border: none;
  border-radius: 6px;
  background: none;
  font-size: 13px;
  color: var(--text);
  text-align: left;
  cursor: pointer;
}

.dialog-nav-item.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}

.dialog-body {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
}

.pane {
  padding: 16px 20px;
}

.pane h3 {
  margin: 0 0 12px;
  font-size: 15px;
}

.info {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 8px 16px;
  margin: 0;
  font-size: 13px;
}

.info dt {
  color: var(--text-dim);
}

.info dd {
  margin: 0;
}

.mono {
  font-family: ui-monospace, Consolas, monospace;
}

.hint {
  font-size: 13px;
  color: var(--text-dim);
  line-height: 1.6;
}

.row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
}

.row.disabled {
  color: var(--text-dim);
}
</style>
