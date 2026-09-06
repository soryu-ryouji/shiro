<script setup lang="ts">
// 中栏：打开项目后为写作模式（文稿列表 + 编辑器，Ulysses 式分栏：各栏自带顶栏，分隔竖线贯穿窗口），未打开项目为欢迎页。
import { ref } from 'vue'
import { projectStore } from '../stores/project'
import SheetList from '../components/SheetList.vue'
import Editor from '../components/Editor.vue'
import EditorTabs from '../components/EditorTabs.vue'
import PreviewDialog from '../components/PreviewDialog.vue'

defineProps<{ sidebarVisible: boolean }>()
const emit = defineEmits<{ 'toggle-sidebar': []; 'open-settings': [] }>()

const showPreview = ref(false)
</script>

<template>
  <section v-if="projectStore.current" class="writing">
    <SheetList class="sheet-col" :sidebar-visible="sidebarVisible" @toggle-sidebar="emit('toggle-sidebar')" />
    <div class="editor-col">
      <EditorTabs @open-settings="emit('open-settings')" @open-preview="showPreview = true" />
      <Editor />
    </div>
    <PreviewDialog v-if="showPreview" @close="showPreview = false" />
  </section>

  <section v-else class="welcome">
    <h1 class="logo">shiro</h1>
    <p class="hint">从左侧打开项目文件夹开始，点击项目进入写作</p>
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
  font-size: calc(32px * var(--font-scale-ui));
  font-weight: 700;
  letter-spacing: 1px;
  color: var(--text);
}

.hint {
  margin: 0;
  color: var(--text-dim);
  font-size: calc(13px * var(--font-scale-ui));
}
</style>
