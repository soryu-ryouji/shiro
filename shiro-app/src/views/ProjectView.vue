<script setup lang="ts">
// 中栏：打开项目后为写作模式（文稿列表 + 编辑器，Ulysses 式），未打开项目为欢迎页。
import { projectStore } from '../stores/project'
import SheetList from '../components/SheetList.vue'
import Editor from '../components/Editor.vue'
</script>

<template>
  <section v-if="projectStore.current" class="writing">
    <SheetList class="sheet-col" />
    <Editor class="editor-col" />
  </section>

  <section v-else class="welcome">
    <h1 class="logo">shiro</h1>
    <p class="hint">从左侧打开项目文件夹开始，双击项目进入写作</p>
  </section>
</template>

<style scoped>
.writing {
  height: 100%;
  display: grid;
  grid-template-columns: 240px minmax(0, 1fr);
}

/* 子列用 flex 链传高度（grid stretch 子项里 height:100% 不可靠） */
.sheet-col {
  border-right: 1px solid var(--border);
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.editor-col {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.welcome {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
}

.logo {
  margin: 0;
  font-size: 32px;
  font-weight: 700;
  letter-spacing: 1px;
  color: var(--text);
}

.hint {
  margin: 0;
  color: var(--text-dim);
  font-size: 13px;
}
</style>
