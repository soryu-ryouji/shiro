<script setup lang="ts">
// 项目卡片列表后续接 history.toml 对应 API；本期为空状态占位（docs/frontend/layout.md）
</script>

<template>
  <section class="project-view">
    <div class="card-row">
      <button class="card card-open" disabled title="即将支持">
        <span class="card-plus">＋</span>
        <span>打开项目文件夹</span>
      </button>
    </div>
    <p class="empty-hint">打开过的项目会显示在这里</p>
  </section>
</template>

<style scoped>
.card-row {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}

.card {
  width: 140px;
  height: 96px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border: 1px dashed var(--border);
  border-radius: 8px;
  background: var(--bg-soft);
  color: var(--text-dim);
  font-size: 13px;
  cursor: pointer;
}

.card:disabled {
  cursor: not-allowed;
  opacity: 0.7;
}

.card-plus {
  font-size: 20px;
  line-height: 1;
}

.empty-hint {
  margin-top: 16px;
}
</style>
