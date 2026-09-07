<script setup lang="ts">
// 角色制作视图（数据库 → 制作 → 角色制作）：左列制作记录 + 新建按钮，右列新建表单或任务详情（流程图）。
// 任务状态与轮询见 stores/deconstruct.ts；管线阶段定义见 daemon/src/deconstruct/engine.rs
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import {
  FILE_LABELS,
  STAGE_LABELS,
  deconstructStore,
  isRunning,
} from '../stores/deconstruct'
import { dbStore } from '../stores/database'
import { renderMarkdown } from '../utils/mdRender'
import { editorParaMode } from '../utils/font'
import Icon from '../components/Icon.vue'
import PipelineFlow from './PipelineFlow.vue'
import SegmentPicker from './SegmentPicker.vue'
import ContextMenu, { type MenuItem } from './ContextMenu.vue'

// ---- 新建表单（草稿由 store 持有：复制任务时同步写入，无 watch 时序面） ----
const form = computed(() => deconstructStore.formDraft)
const aliasInput = ref('')
const fileError = ref('')

function addAlias() {
  const v = aliasInput.value.trim()
  if (v && !form.value.aliases.includes(v) && v !== form.value.character.trim()) {
    form.value.aliases.push(v)
  }
  aliasInput.value = ''
}

function removeAlias(a: string) {
  form.value.aliases = form.value.aliases.filter((x) => x !== a)
}

async function readFile(file: File) {
  fileError.value = ''
  if (file.size > 5 * 1024 * 1024) {
    fileError.value = '文件超过 5MB，请按集/卷拆分后导入'
    return
  }
  form.value.fileName = file.name
  form.value.content = await file.text()
  if (!form.value.sourceName) {
    form.value.sourceName = file.name.replace(/\.(txt|md|markdown|json)$/i, '')
  }
}

async function onFile(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (file) await readFile(file)
}

function onDrop(e: DragEvent) {
  const file = e.dataTransfer?.files?.[0]
  if (file) void readFile(file)
}

const canSubmit = computed(
  () =>
    !!form.value.content.trim() &&
    !!form.value.character.trim() &&
    !deconstructStore.submitting,
)

async function submit() {
  if (!canSubmit.value) return
  const ok = await deconstructStore.createTask({
    sourceName: form.value.sourceName.trim() || form.value.fileName,
    content: form.value.content,
    character: form.value.character.trim(),
    aliases: form.value.aliases,
  })
  if (ok) {
    deconstructStore.resetForm()
  }
}

// ---- 任务详情 ----
const task = computed(() => deconstructStore.detail)
const verifyRemoved = computed(
  () => deconstructStore.detail?.progress.verified?.removed ?? [],
)
const saving = ref(false)
const savedId = ref('')
/** 已入库的角色 id（本次会话保存的优先，其次任务记录里的） */
const savedCharacterId = computed(
  () => savedId.value || task.value?.progress.saved_character_id || null,
)

async function saveToLibrary() {
  if (!task.value) return
  saving.value = true
  const id = await deconstructStore.saveToLibrary(task.value.id)
  saving.value = false
  if (id) savedId.value = id
}

function gotoCharacters(id: string) {
  savedId.value = ''
  dbStore.selectCategory('characters')
  void dbStore.refreshCharacters().then(() => dbStore.selectCharacter(id))
}

/** 跳 Model 页配置模型（制作前置条件） */
function gotoModel() {
  window.dispatchEvent(new CustomEvent('shiro:navigate', { detail: 'model' }))
}

function viewSaved() {
  if (savedCharacterId.value) gotoCharacters(savedCharacterId.value)
}

// ---- 产物预览 ----
const previewFile = ref<string | null>(null)
const previewBody = computed(() => {
  const files = deconstructStore.detail?.files ?? []
  const target =
    files.find((f) => f.name === previewFile.value) ?? files[files.length - 1] ?? null
  return target
    ? { name: target.name, html: renderMarkdown(target.body, editorParaMode.value) }
    : null
})

function stageText(stage: string | undefined): string {
  if (!stage) return '—'
  return STAGE_LABELS[stage] ?? stage
}

// ---- 选择闸门弹窗：进入待选择状态时自动弹一次（按任务去重），可手动再打开 ----
const pickerOpen = ref(false)
const autoOpenedFor = ref<string | null>(null)

watch(
  () => [deconstructStore.detail?.id, deconstructStore.detail?.progress.stage] as const,
  ([id, stage]) => {
    if (stage === 'selecting' && id && autoOpenedFor.value !== id) {
      autoOpenedFor.value = id
      pickerOpen.value = true
    }
    if (stage && stage !== 'selecting') pickerOpen.value = false
  },
  { immediate: true },
)

/** 命中角色名的段数（待选择横幅文案） */
const namedCount = computed(
  () => (deconstructStore.detail?.segments ?? []).filter((s) => s.has_name).length,
)

// ---- 复制为新制作 ----
function duplicateTask() {
  if (task.value) void deconstructStore.duplicateForCreate(task.value.id)
}

// ---- 次边栏记录右键菜单（复制 / 删除） ----
const ctxMenu = ref<{ x: number; y: number; taskId: string } | null>(null)

function onRecordContext(e: MouseEvent, taskId: string) {
  // 右键先选中（菜单与详情上下文一致）
  void deconstructStore.selectTask(taskId)
  ctxMenu.value = { x: e.clientX, y: e.clientY, taskId }
}

const ctxItems = computed<MenuItem[]>(() => {
  if (!ctxMenu.value) return []
  const id = ctxMenu.value.taskId
  const t = deconstructStore.tasks.find((x) => x.id === id)
  const running = isRunning(t?.stage)
  return [
    {
      label: running ? '复制（运行中不可用）' : '复制（Ctrl+C）',
      action: () => {
        if (!running) deconstructStore.copyTask(id)
      },
    },
    {
      label: '粘贴为新制作（Ctrl+V）',
      action: () => {
        if (deconstructStore.copiedTaskId) {
          void deconstructStore.duplicateForCreate(deconstructStore.copiedTaskId)
        }
      },
    },
    { divider: true },
    {
      label: running ? '删除（运行中不可用）' : '删除',
      danger: true,
      action: () => {
        if (!running) void deconstructStore.deleteTask(id)
      },
    },
  ]
})

// ---- 快捷键：Ctrl/Cmd+C 复制选中任务，Ctrl/Cmd+V 粘贴为新制作 ----
function onKeydown(e: KeyboardEvent) {
  if (!(e.ctrlKey || e.metaKey)) return
  const target = e.target as HTMLElement | null
  if (target?.closest('input, textarea, [contenteditable="true"]')) return
  const key = e.key.toLowerCase()
  if (key === 'c') {
    // 用户选中了文字时不抢原生复制
    if (window.getSelection()?.toString()) return
    const t = task.value
    if (!t || deconstructStore.creating || isRunning(t.progress.stage)) return
    e.preventDefault()
    deconstructStore.copyTask(t.id)
  } else if (key === 'v') {
    const id = deconstructStore.copiedTaskId
    if (!id) return
    // 表单已打开且有内容时不覆盖用户输入
    if (deconstructStore.creating && form.value.content.trim()) return
    e.preventDefault()
    void deconstructStore.duplicateForCreate(id)
  }
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))

function taskBadgeClass(stage: string | undefined): string {
  if (stage === 'done') return 'ok'
  if (stage === 'failed') return 'err'
  if (stage === 'selecting') return 'warn'
  if (isRunning(stage)) return 'run'
  return ''
}

function fmtTime(sec: number | undefined | null): string {
  if (!sec) return ''
  const d = new Date(sec * 1000)
  return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}
</script>

<template>
  <section class="craft-view">
    <!-- 次边栏：制作记录 -->
    <div class="list-col">
      <div class="list-head">
        <h2>制作记录</h2>
        <button class="icon-btn" title="刷新" @click="deconstructStore.refreshTasks()">
          <Icon name="refresh" :size="14" />
        </button>
      </div>
      <button class="btn new-btn" @click="deconstructStore.openCreateForm()">
        <Icon name="plus" :size="14" />
        新建制作
      </button>

      <p v-if="deconstructStore.listLoading && !deconstructStore.tasksLoaded" class="hint">加载中…</p>
      <p v-else-if="deconstructStore.error" class="hint error">{{ deconstructStore.error }}</p>
      <p v-else-if="!deconstructStore.tasks.length" class="hint empty">
        还没有制作记录。点「新建制作」导入剧本，提炼角色档案。
      </p>

      <div v-else class="records">
        <button
          v-for="t in deconstructStore.tasks"
          :key="t.id"
          class="record"
          :class="{ active: t.id === deconstructStore.selectedId && !deconstructStore.creating }"
          @click="deconstructStore.selectTask(t.id)"
          @contextmenu.prevent="onRecordContext($event, t.id)"
        >
          <span class="row1">
            <span class="name">{{ t.character }}</span>
            <span class="badge" :class="taskBadgeClass(t.stage)">{{
              isRunning(t.stage) ? '运行中' : stageText(t.stage)
            }}</span>
            <span v-if="deconstructStore.copiedTaskId === t.id" class="copied-dot" title="已复制（Ctrl+V 粘贴）">已复制</span>
          </span>
          <span class="meta">
            {{ t.source_name }}<template v-if="t.segment_count"> · {{ t.segment_count }} 片</template>
            <template v-if="isRunning(t.stage) && t.stage === 'notes'"> · {{ t.notes_done }}/{{ t.segment_count }}</template>
            <template v-if="t.finished_at"> · {{ fmtTime(t.finished_at) }}</template>
          </span>
        </button>
      </div>
      <p v-if="deconstructStore.copiedTip" class="copy-tip">
        {{ deconstructStore.copiedTip }}，Ctrl+V 粘贴为新制作
      </p>

      <!-- 记录右键菜单 -->
      <ContextMenu
        v-if="ctxMenu"
        :x="ctxMenu.x"
        :y="ctxMenu.y"
        :items="ctxItems"
        @close="ctxMenu = null"
      />
    </div>

    <!-- 主区：新建表单 或 任务详情 -->
    <div class="main-col">
      <!-- 新建表单 -->
      <div v-if="deconstructStore.creating || !deconstructStore.selectedId" class="form" @dragover.prevent @drop.prevent="onDrop">
        <h2>新建角色制作</h2>
        <p class="desc">
          导入剧本，提炼目录形态的角色档案包（灵魂 / 语言指纹 / 行为指南 / 关系 / 编年 / 红线 / 总索引）。
          需要先在 <a class="link" @click="gotoModel">Model 页</a>配置模型供应商与密钥。
        </p>

        <label class="field">
          <span class="field-label">作品名</span>
          <input v-model="form.sourceName" type="text" placeholder="如：链锯人（用于档案标注）" />
        </label>

        <div class="field">
          <span class="field-label">剧本文件</span>
          <label class="file-drop" :class="{ filled: !!form.fileName }">
            <input type="file" accept=".txt,.md,.markdown" @change="onFile" />
            <template v-if="form.fileName">{{ form.fileName }}（{{ Math.ceil(form.content.length / 1000) }}k 字）</template>
            <template v-else>点击选择或拖入 .txt / .md 文件</template>
          </label>
          <p v-if="fileError" class="hint error">{{ fileError }}</p>
        </div>

        <label class="field">
          <span class="field-label">角色名</span>
          <input v-model="form.character" type="text" placeholder="剧本中该角色的主要称呼（如：玛奇玛）" />
        </label>

        <div class="field">
          <span class="field-label">别名（可选）</span>
          <div class="aliases">
            <span v-for="a in form.aliases" :key="a" class="alias">
              {{ a }}
              <button class="alias-x" title="移除" @click="removeAlias(a)">×</button>
            </span>
            <input
              v-model="aliasInput"
              type="text"
              placeholder="角色在剧本中的其他写法，回车添加（如：マキマ）"
              @keydown.enter.prevent="addAlias"
            />
          </div>
          <p class="hint tip">补充别名可避免台词与事件归属遗漏（例：全名、译名、昵称）。</p>
        </div>

        <p v-if="deconstructStore.error" class="hint error">{{ deconstructStore.error }}</p>
        <div class="form-actions">
          <button class="btn primary" :disabled="!canSubmit" @click="submit">
            {{ deconstructStore.submitting ? '创建中…' : '开始制作' }}
          </button>
        </div>
      </div>

      <!-- 任务详情 -->
      <template v-else>
        <p v-if="deconstructStore.detailLoading && !task" class="hint">加载中…</p>
        <div v-else-if="task" class="detail">
          <header class="detail-head">
            <div class="title-row">
              <h1>{{ task.character }}</h1>
              <span class="badge" :class="taskBadgeClass(task.progress.stage)">
                {{ isRunning(task.progress.stage) ? '运行中' : stageText(task.progress.stage) }}
              </span>
              <span class="spacer" />
              <button
                v-if="task.progress.stage === 'selecting'"
                class="btn primary"
                @click="pickerOpen = true"
              >
                选择切片
              </button>
              <button
                v-if="task.progress.stage === 'failed'"
                class="btn"
                @click="deconstructStore.retryTask(task.id)"
              >
                从断点重试
              </button>
              <button
                v-if="!isRunning(task.progress.stage)"
                class="btn"
                title="用本任务的填写信息新建一次制作"
                @click="duplicateTask"
              >
                复制为新制作
              </button>
              <button
                v-if="task.progress.stage === 'done' && !task.progress.saved_character_id && !savedId"
                class="btn primary"
                :disabled="saving"
                @click="saveToLibrary"
              >
                {{ saving ? '保存中…' : '保存到人物库' }}
              </button>
              <span v-else-if="savedCharacterId" class="saved-note">
                已入库：<a @click="viewSaved">查看角色卡</a>
              </span>
              <button
                v-if="!isRunning(task.progress.stage)"
                class="icon-btn"
                title="删除任务记录"
                @click="deconstructStore.deleteTask(task.id)"
              >
                <Icon name="trash" :size="14" />
              </button>
            </div>
            <p class="meta">
              《{{ task.source_name }}》<template v-if="task.aliases.length"> · 别名：{{ task.aliases.join(' / ') }}</template>
              · {{ fmtTime(task.created_at) }}
            </p>
            <p v-if="task.progress.error" class="hint error">{{ task.progress.error }}</p>
          </header>

          <!-- 流程图 -->
          <PipelineFlow :progress="task.progress" />

          <!-- 待选择横幅：切块完成，引导打开选择弹窗 -->
          <div
            v-if="task.progress.stage === 'selecting'"
            class="select-banner"
          >
            <span>
              拆分完成：共 {{ task.segments?.length ?? task.progress.segment_count }} 段，
              其中 {{ namedCount }} 段命中「{{ task.character }}」。
            </span>
            <button class="btn primary" @click="pickerOpen = true">选择切片</button>
          </div>

          <!-- 选择闸门弹窗 -->
          <SegmentPicker
            v-if="pickerOpen && task.progress.stage === 'selecting' && task.segments?.length"
            :segments="task.segments"
            :character="task.character"
            @submit="deconstructStore.selectSegments(task.id, $event)"
            @close="pickerOpen = false"
          />

          <!-- 回查报告 -->
          <div v-if="task.progress.verified" class="verify-report">
            <h3>引文回查</h3>
            <p>
              通过 {{ task.progress.verified.passed }} 条
              <template v-if="verifyRemoved.length">
                ；<span class="warn">未定位并移除 {{ verifyRemoved.length }} 条（论断保留，引号已去）：</span>
              </template>
            </p>
            <ul v-if="verifyRemoved.length" class="removed">
              <li v-for="(r, i) in verifyRemoved" :key="i">
                {{ FILE_LABELS[r.file] ?? r.file }}：「{{ r.quote }}」（{{ r.unit }}）
              </li>
            </ul>
          </div>

          <!-- 产物预览 -->
          <div v-if="task.files.length" class="preview">
            <div class="preview-tabs">
              <button
                v-for="f in task.files"
                :key="f.name"
                class="tab"
                :class="{ active: (previewFile ?? task.files.at(-1)!.name) === f.name }"
                @click="previewFile = f.name"
              >
                {{ FILE_LABELS[f.name] ?? f.name }}
              </button>
            </div>
            <article v-if="previewBody" class="md" v-html="previewBody.html"></article>
          </div>
        </div>
      </template>
    </div>
  </section>
</template>

<style scoped>
.craft-view {
  height: 100%;
  display: grid;
  grid-template-columns: 300px minmax(0, 1fr);
  gap: 16px;
}

.list-col,
.main-col {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

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

.new-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 32px;
  margin-bottom: 10px;
  border: 1px dashed var(--accent);
  border-radius: 8px;
  background: var(--accent-soft);
  color: var(--accent);
  font-size: calc(13px * var(--font-scale-ui));
  cursor: pointer;
}

@media (hover: hover) {
  .new-btn:hover {
    filter: brightness(0.97);
  }
}

.records {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 6px;
  overflow-y: auto;
  padding-right: 2px;
}

.copy-tip {
  flex: none;
  margin: 8px 2px 0;
  padding: 6px 10px;
  border-radius: 6px;
  background: var(--accent-soft);
  color: var(--accent);
  font-size: calc(11px * var(--font-scale-ui));
  line-height: 1.5;
}

.copied-dot {
  margin-left: auto;
  color: var(--accent);
  font-size: calc(10px * var(--font-scale-ui));
}

.record {
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
  .record:hover {
    border-color: var(--accent);
  }
}

.record.active {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.row1 {
  display: flex;
  align-items: center;
  gap: 8px;
}

.name {
  font-size: calc(14px * var(--font-scale-ui));
  font-weight: 600;
}

.meta {
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}

.badge {
  padding: 1px 8px;
  border-radius: 999px;
  font-size: calc(11px * var(--font-scale-ui));
  border: 1px solid var(--border);
  color: var(--text-dim);
}

.badge.run {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-soft);
}

.badge.ok {
  border-color: #3a9a50;
  color: #3a9a50;
  background: rgba(58, 154, 80, 0.1);
}

.badge.err {
  border-color: #c05050;
  color: #c05050;
  background: rgba(192, 80, 80, 0.08);
}

.badge.warn {
  border-color: #b08030;
  color: #b08030;
  background: rgba(176, 128, 48, 0.1);
}

/* ---- 表单 ---- */
.form {
  max-width: 560px;
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 4px 0;
}

.desc {
  margin: 0;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
  line-height: 1.7;
}

.desc code {
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--bg-soft);
}

.desc .link {
  color: var(--accent);
  cursor: pointer;
  text-decoration: underline;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.field-label {
  font-size: calc(12px * var(--font-scale-ui));
  color: var(--text-dim);
}

input[type='text'] {
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  font-size: calc(13px * var(--font-scale-ui));
  outline: none;
  background: var(--bg);
  color: var(--text);
}

input[type='text']:focus {
  border-color: var(--accent);
}

.file-drop {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  height: 56px;
  border: 1px dashed var(--border);
  border-radius: 8px;
  color: var(--text-dim);
  font-size: calc(13px * var(--font-scale-ui));
  cursor: pointer;
}

.file-drop.filled {
  border-style: solid;
  color: var(--text);
}

.file-drop input {
  position: absolute;
  inset: 0;
  opacity: 0;
  cursor: pointer;
}

.aliases {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  padding: 6px 8px;
  border: 1px solid var(--border);
  border-radius: 6px;
  min-height: 34px;
  align-items: center;
}

.aliases input {
  flex: 1;
  min-width: 160px;
  border: none;
  outline: none;
  background: transparent;
  color: var(--text);
  font-size: calc(13px * var(--font-scale-ui));
}

.alias {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border-radius: 999px;
  background: var(--bg-soft);
  font-size: calc(12px * var(--font-scale-ui));
}

.alias-x {
  border: none;
  background: none;
  color: var(--text-dim);
  cursor: pointer;
  padding: 0;
  font-size: 13px;
}

.tip {
  margin: 0;
}

.form-actions {
  display: flex;
  justify-content: flex-end;
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

/* ---- 详情 ---- */
.main-col {
  border-left: 1px solid var(--border);
  padding-left: 16px;
  overflow-y: auto;
}

.detail-head {
  margin-bottom: 14px;
}

.title-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

h1 {
  margin: 0;
  font-size: calc(20px * var(--font-scale-ui));
}

.spacer {
  flex: 1;
}

.saved-note {
  color: #3a9a50;
  font-size: calc(12px * var(--font-scale-ui));
}

.select-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-top: 12px;
  padding: 12px 14px;
  border: 1px solid #b08030;
  border-radius: 10px;
  background: rgba(176, 128, 48, 0.08);
  color: var(--text);
  font-size: calc(13px * var(--font-scale-ui));
}

.saved-note a {
  cursor: pointer;
  text-decoration: underline;
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

.verify-report {
  margin: 14px 0;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
}

.verify-report h3 {
  margin: 0 0 6px;
  font-size: calc(13px * var(--font-scale-ui));
}

.verify-report p {
  margin: 0;
  font-size: calc(12px * var(--font-scale-ui));
}

.warn {
  color: #b08030;
}

.removed {
  margin: 6px 0 0;
  padding-left: 18px;
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
}

/* ---- 产物预览 ---- */
.preview {
  margin-top: 14px;
}

.preview-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  margin-bottom: 10px;
}

.tab {
  padding: 3px 12px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: var(--bg);
  color: var(--text-dim);
  font-size: calc(12px * var(--font-scale-ui));
  cursor: pointer;
}

.tab.active {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-soft);
}

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

.hint {
  color: var(--text-dim);
  font-size: calc(13px * var(--font-scale-ui));
}

.hint.error {
  color: #d05050;
}

.hint.empty {
  line-height: 1.8;
  margin: 0;
}
</style>
