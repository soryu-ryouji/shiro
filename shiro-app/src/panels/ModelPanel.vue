<script setup lang="ts">
// Model 面板（侧栏）：功能分层——注册分区「模型导入」、管理分区「模型管理」。
import { onMounted } from 'vue'
import { modelStore } from '../stores/model'

onMounted(() => {
  if (!modelStore.loaded) void modelStore.load()
})
</script>

<template>
  <div class="panel">
    <div class="group-title">注册分区</div>
    <button
      class="entry"
      :class="{ active: modelStore.view === 'import' }"
      @click="modelStore.newImport()"
    >
      <span class="label">模型导入</span>
    </button>

    <div class="group-title">管理分区</div>
    <button
      class="entry"
      :class="{ active: modelStore.view === 'manage' }"
      @click="modelStore.openManage()"
    >
      <span class="label">模型管理</span>
      <span class="count">{{ modelStore.profiles.length }}</span>
    </button>
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.group-title {
  margin: 10px 10px 4px;
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
  letter-spacing: 0.5px;
}

.entry {
  display: flex;
  align-items: center;
  padding: 7px 10px;
  border: none;
  border-radius: 6px;
  background: none;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  text-align: left;
  cursor: pointer;
}

@media (hover: hover) {
  .entry:hover {
    background: var(--border);
  }
}

.entry.active {
  background: var(--accent-soft);
}

.label {
  flex: 1;
}

.count {
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}
</style>
