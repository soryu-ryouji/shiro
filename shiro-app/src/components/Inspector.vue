<script setup lang="ts">
// 右侧详情栏：顶条为拖拽区（双击切换最大化），右上角由 fixed 窗口控制按钮覆盖。
import { shell } from '../platform'

function onHeadDblClick(e: MouseEvent) {
  if ((e.target as HTMLElement).closest('button')) return
  void shell.toggleMaximizeWindow()
}
</script>

<template>
  <aside class="inspector">
    <div class="inspector-head" @dblclick="onHeadDblClick" />
    <div class="inspector-body">
      <p class="inspector-hint">选中内容的详情会显示在这里</p>
    </div>
  </aside>
</template>

<style scoped>
.inspector {
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-width: 0;
}

.inspector-head {
  flex: none;
  height: 40px;
  -webkit-app-region: drag;
}

.inspector-body {
  flex: 1;
  padding: 4px 12px 12px;
  overflow-y: auto;
}

.inspector-hint {
  color: var(--text-dim);
  font-size: calc(13px * var(--font-scale-ui));
}
</style>
