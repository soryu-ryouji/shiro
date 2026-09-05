<script setup lang="ts">
// Database 面板（侧栏）：内容库分类列表（写作规范/基底套路/推进模式/人物库）。
// 条目与计数后续接内容库 API；本期为静态占位。
const CATEGORIES = [
  { key: 'conventions', label: '写作规范' },
  { key: 'tropes', label: '基底套路' },
  { key: 'patterns', label: '推进模式' },
  { key: 'characters', label: '人物库' },
]
</script>

<template>
  <div class="panel">
    <button v-for="c in CATEGORIES" :key="c.key" class="entry" disabled>
      <span class="label">{{ c.label }}</span>
      <span class="count">—</span>
    </button>
  </div>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  gap: 2px;
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

.entry:disabled {
  cursor: not-allowed;
  opacity: 0.6;
}

.label {
  flex: 1;
}

.count {
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}
</style>
