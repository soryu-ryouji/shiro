<script setup lang="ts">
// Database 面板（侧栏）：元数据分组（角色，接内容库资产 API，见 docs/asset-format.md）+ 内容库分类（静态占位）。
import { dbStore } from '../stores/database'

const LIB_CATEGORIES = [
  { key: 'conventions', label: '写作规范' },
  { key: 'tropes', label: '基底套路' },
  { key: 'patterns', label: '推进模式' },
]
</script>

<template>
  <div class="panel">
    <div class="group-title">元数据</div>
    <button
      class="entry"
      :class="{ active: dbStore.category === 'characters' }"
      @click="dbStore.selectCategory('characters')"
    >
      <span class="label">角色</span>
      <span class="count">{{ dbStore.charactersLoaded ? dbStore.characters.length : '—' }}</span>
    </button>

    <div class="group-title">内容库</div>
    <button v-for="c in LIB_CATEGORIES" :key="c.key" class="entry" disabled>
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
  .entry:not(:disabled):hover {
    background: var(--border);
  }
}

.entry.active {
  background: var(--accent-soft);
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
