<script setup lang="ts">
// Model 面板（侧栏）：模型 provider 档案列表 + 添加入口。
// 档案读写 config.toml 的 API 后续提供；本期为空状态占位（设计见 docs/backend/model-access.md）。
</script>

<template>
  <div class="panel">
    <button class="add-btn" disabled title="即将支持">＋ 添加 provider</button>
    <p class="hint">配置好的模型档案会显示在这里</p>
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.add-btn {
  padding: 8px;
  border: 1px dashed var(--border);
  border-radius: 8px;
  background: var(--bg);
  color: var(--text-dim);
  font-size: 13px;
  cursor: pointer;
}

.add-btn:disabled {
  cursor: not-allowed;
  opacity: 0.7;
}

.hint {
  margin: 0;
  color: var(--text-dim);
  font-size: 12px;
}
</style>
