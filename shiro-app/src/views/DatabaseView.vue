<script setup lang="ts">
// 中栏：内容库视图。元数据 → 角色：全局人物库（~/.config/shiro/db/人物/，格式见 docs/asset-format.md）
// 的角色卡列表（筛选：文本 + 功能角色胶囊）+ 详情（渲染态 / 原文编辑态，原子保存 + 解析反馈）。
import { computed, onMounted, ref, watch } from 'vue'
import { characterRoles, dbStore, filteredCharacters } from '../stores/database'
import { renderMarkdown } from '../utils/mdRender'
import { editorParaMode } from '../utils/font'
import Icon from '../components/Icon.vue'
import PromptDialog from '../components/PromptDialog.vue'

onMounted(() => {
  // 首次进入视图拉列表；之后切回面板沿用缓存，手动刷新走列表头按钮
  if (dbStore.category === 'characters' && !dbStore.charactersLoaded) {
    void dbStore.refreshCharacters()
  }
})

const detailHtml = computed(() =>
  dbStore.detail?.body ? renderMarkdown(dbStore.detail.body, editorParaMode.value) : '',
)

// ---- 新建 ----
const showCreate = ref(false)

// ---- 编辑态（原文 textarea：明文 markdown + frontmatter，与「文件夹是唯一权威数据源」一致）----
const editing = ref(false)
const draft = ref('')
const saving = ref(false)

// 切换选中/清空选择时退出编辑态（草稿丢弃，以文件为准）
watch(
  () => dbStore.selectedId,
  () => {
    editing.value = false
    draft.value = ''
  },
)

function startEdit() {
  if (!dbStore.detail) return
  draft.value = dbStore.detail.raw
  editing.value = true
}

async function save() {
  if (!dbStore.selectedId) return
  saving.value = true
  const ok = await dbStore.saveCharacter(dbStore.selectedId, draft.value)
  saving.value = false
  // 解析失败：内容已保存，留在编辑态修 frontmatter（顶部横幅提示）
  if (ok) editing.value = false
}
</script>

<template>
  <section v-if="dbStore.category === 'characters'" class="db-view">
    <!-- 列表列 -->
    <div class="list-col">
      <div class="list-head">
        <h2>角色</h2>
        <div class="head-actions">
          <button class="icon-btn" title="新建角色卡" @click="showCreate = true">
            <Icon name="plus" :size="14" />
          </button>
          <button class="icon-btn" title="刷新" @click="dbStore.refreshCharacters()">
            <Icon name="refresh" :size="14" />
          </button>
        </div>
      </div>

      <!-- 筛选：文本 + 功能角色胶囊 -->
      <div v-if="dbStore.characters.length" class="filter-bar">
        <input
          v-model="dbStore.filter"
          class="search"
          type="text"
          placeholder="搜索名字 / 原型 / 标签…"
        />
        <div v-if="characterRoles.length > 1" class="role-pills">
          <button
            class="pill"
            :class="{ on: dbStore.roleFilter === null }"
            @click="dbStore.roleFilter = null"
          >
            全部
          </button>
          <button
            v-for="r in characterRoles"
            :key="r"
            class="pill"
            :class="{ on: dbStore.roleFilter === r }"
            @click="dbStore.roleFilter = dbStore.roleFilter === r ? null : r"
          >
            {{ r }}
          </button>
        </div>
      </div>

      <p v-if="dbStore.loading && !dbStore.charactersLoaded" class="hint">加载中…</p>
      <p v-else-if="dbStore.error" class="hint error">{{ dbStore.error }}</p>
      <p v-else-if="!dbStore.characters.length" class="hint empty">
        全局人物库还没有角色。点右上角 + 新建，或把角色卡放入
        <code>~/.config/shiro/db/人物/</code>（格式见 docs/asset-format.md）后刷新。
      </p>
      <p v-else-if="!filteredCharacters.length" class="hint">没有匹配的角色</p>

      <div v-else class="cards">
        <button
          v-for="c in filteredCharacters"
          :key="c.id"
          class="card"
          :class="{ active: c.id === dbStore.selectedId }"
          @click="dbStore.selectCharacter(c.id)"
        >
          <span class="row1">
            <span class="name">{{ c.name }}</span>
            <span v-if="c.role" class="role">{{ c.role }}</span>
          </span>
          <span v-if="c.excerpt" class="excerpt">{{ c.excerpt }}</span>
          <span v-if="c.archetype.length" class="archetypes">
            <span v-for="a in c.archetype" :key="a" class="tag">{{ a }}</span>
          </span>
        </button>
      </div>
    </div>

    <!-- 详情列 -->
    <div class="detail-col">
      <p v-if="!dbStore.selectedId" class="hint">从左侧选择角色查看详情</p>
      <p v-else-if="dbStore.detailLoading" class="hint">加载中…</p>

      <!-- 编辑态 -->
      <template v-else-if="editing">
        <p v-if="dbStore.parseError" class="hint parse-error">{{ dbStore.parseError }}</p>
        <textarea
          v-model="draft"
          class="editor"
          spellcheck="false"
          placeholder="角色卡原文（markdown + frontmatter）"
        ></textarea>
        <div class="edit-actions">
          <button class="btn" :disabled="saving" @click="editing = false">取消</button>
          <button class="btn primary" :disabled="saving" @click="save">
            {{ saving ? '保存中…' : '保存' }}
          </button>
        </div>
      </template>

      <!-- 渲染态 -->
      <article v-else-if="dbStore.detail" class="detail">
        <header class="detail-head">
          <div class="title-row">
            <h1>{{ dbStore.detail.name }}</h1>
            <button class="icon-btn" title="编辑原文" @click="startEdit">
              <Icon name="edit" :size="14" />
            </button>
          </div>
          <div class="badges">
            <span v-if="dbStore.detail.role" class="role">{{ dbStore.detail.role }}</span>
            <span v-for="a in dbStore.detail.archetype" :key="a" class="tag">{{ a }}</span>
            <span v-for="t in dbStore.detail.tags" :key="t" class="tag dim">{{ t }}</span>
          </div>
          <p v-if="dbStore.detail.source" class="source">来源：{{ dbStore.detail.source }}</p>
        </header>
        <!-- 角色卡为本机手稿，不做 sanitize（与预览一致） -->
        <div class="md" v-html="detailHtml"></div>
      </article>
    </div>

    <!-- 新建角色卡 -->
    <PromptDialog
      v-if="showCreate"
      title="新建角色卡"
      placeholder="角色显示名（如：沈青梧）"
      initial=""
      @close="showCreate = false"
      @submit="dbStore.createCharacter($event)"
    />
  </section>

  <section v-else class="placeholder">
    <p class="hint">从左侧选择分类</p>
  </section>
</template>

<style scoped>
.db-view {
  height: 100%;
  display: grid;
  grid-template-columns: 320px minmax(0, 1fr);
  gap: 16px;
}

.list-col,
.detail-col {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

/* 列表头 */
.list-head {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

h2 {
  margin: 0;
  font-size: calc(15px * var(--font-scale-ui));
}

.head-actions {
  display: flex;
  gap: 4px;
}

.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}

@media (hover: hover) {
  .icon-btn:hover {
    background: var(--border);
    color: var(--text);
  }
}

/* 筛选 */
.filter-bar {
  flex: none;
  margin-bottom: 8px;
}

.search {
  width: 100%;
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: calc(12px * var(--font-scale-ui));
  outline: none;
  box-sizing: border-box;
}

.search:focus {
  border-color: var(--accent);
}

.role-pills {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-top: 6px;
}

.pill {
  padding: 2px 10px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: var(--bg);
  font-size: calc(11px * var(--font-scale-ui));
  color: var(--text-dim);
  cursor: pointer;
}

.pill.on {
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--accent);
}

/* 角色卡列表 */
.cards {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 6px;
  overflow-y: auto;
  padding-right: 2px;
}

.card {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
  text-align: left;
  cursor: pointer;
}

@media (hover: hover) {
  .card:hover {
    border-color: var(--accent);
  }
}

.card.active {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.row1 {
  display: flex;
  align-items: baseline;
  gap: 8px;
}

.name {
  font-size: calc(14px * var(--font-scale-ui));
  font-weight: 600;
}

.excerpt {
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

/* 详情 */
.detail-col {
  border-left: 1px solid var(--border);
  padding-left: 16px;
  overflow-y: auto;
}

.detail-head {
  margin-bottom: 12px;
}

.title-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

h1 {
  margin: 0 0 8px;
  font-size: calc(20px * var(--font-scale-ui));
}

.badges {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}

.source {
  margin: 8px 0 0;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}

.role {
  padding: 1px 8px;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--accent);
  font-size: calc(11px * var(--font-scale-ui));
}

.tag {
  padding: 1px 8px;
  border-radius: 999px;
  border: 1px solid var(--border);
  color: var(--text);
  font-size: calc(11px * var(--font-scale-ui));
}

.tag.dim {
  color: var(--text-dim);
}

/* 编辑态 */
.parse-error {
  flex: none;
  margin: 0 0 8px;
  padding: 8px 10px;
  border-radius: 6px;
  background: #fdf0f0;
  color: #c04040;
  font-size: calc(12px * var(--font-scale-ui));
}

.editor {
  flex: 1;
  min-height: 200px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
  color: var(--text);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, 'Courier New', monospace;
  font-size: calc(13px * var(--font-scale-ui));
  line-height: 1.6;
  resize: none;
  outline: none;
}

.editor:focus {
  border-color: var(--accent);
}

.edit-actions {
  flex: none;
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 10px;
}

.btn {
  height: 30px;
  padding: 0 16px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--bg);
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text);
  cursor: pointer;
}

.btn.primary {
  border-color: var(--accent);
  background: var(--accent);
  color: #fff;
}

.btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

@media (hover: hover) {
  .btn:not(.primary):not(:disabled):hover {
    background: var(--bg-soft);
  }

  .btn.primary:not(:disabled):hover {
    background: #405ed6;
  }
}

/* 详情正文（markdown 渲染） */
.md {
  font-size: calc(14px * var(--font-scale-ui));
  line-height: 1.7;
}

.md :deep(h2) {
  margin: 20px 0 8px;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--border);
  font-size: calc(15px * var(--font-scale-ui));
}

.md :deep(h3) {
  margin: 16px 0 6px;
  font-size: calc(14px * var(--font-scale-ui));
}

.md :deep(p) {
  margin: 0 0 10px;
}

.md :deep(ul),
.md :deep(ol) {
  margin: 0 0 10px;
  padding-left: 20px;
}

.md :deep(code) {
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--bg-soft);
  font-size: 0.9em;
}

.hint {
  color: var(--text-dim);
  font-size: calc(13px * var(--font-scale-ui));
}

.hint.error {
  color: #d05050;
}

.hint.empty {
  line-height: 1.8;
}

.hint.empty code {
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--bg-soft);
}

.placeholder {
  padding: 8px;
}
</style>
