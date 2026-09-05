<script setup lang="ts">
// Project 面板（侧栏）：打开项目入口 + 最近项目列表。
// 卡片列表后续接 history.toml 对应 API；本期为空状态占位（docs/frontend/layout.md）。
</script>

<template>
  <div class="panel">
    <button class="open-card" disabled title="即将支持">
      <span class="plus">＋</span>
      <span>打开项目文件夹</span>
    </button>
    <p class="hint">打开过的项目会显示在这里</p>
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.open-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 84px;
  border: 1px dashed var(--border);
  border-radius: 8px;
  background: var(--bg);
  color: var(--text-dim);
  font-size: 13px;
  cursor: pointer;
}

.open-card:disabled {
  cursor: not-allowed;
  opacity: 0.7;
}

.plus {
  font-size: 18px;
  line-height: 1;
}

.hint {
  margin: 0;
  color: var(--text-dim);
  font-size: 12px;
}
</style>
