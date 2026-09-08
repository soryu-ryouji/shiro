<script setup lang="ts">
// 角色制作视图（数据库 → 制作 → 角色制作）：左列制作记录 + 新建按钮，右列新建表单或任务详情（流程图）。
// 任务状态与轮询见 stores/deconstruct.ts；管线阶段定义见 daemon/src/deconstruct/engine.rs
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import {
  FILE_LABELS,
  STAGE_LABELS,
  deconstructStore,
  isRunning,
} from '../stores/deconstruct'
import type { LogSummary } from '../stores/deconstruct'
import { dbStore } from '../stores/database'
import { renderMarkdown } from '../utils/mdRender'
import { editorParaMode } from '../utils/font'
import Icon from '../components/Icon.vue'
import PipelineFlow from './PipelineFlow.vue'
import SegmentPicker from './SegmentPicker.vue'
import CraftSettingsDialog from './CraftSettingsDialog.vue'
import ContextMenu, { type MenuItem } from './ContextMenu.vue'
import LogDialog from './LogDialog.vue'
import PromptDialog from './PromptDialog.vue'

// ---- 新建表单（草稿由 store 持有：复制任务时同步写入，无 watch 时序面） ----
const form = computed(() => deconstructStore.formDraft)
const aliasInput = ref('')
const fileError = ref('')

function commitAliases() {
  const raw = aliasInput.value
  if (!raw.trim()) return
  // 逗号/顿号/分号/换行分隔批量识别（粘贴一串也成多条）
  for (const part of raw.split(/[\n,，、;；]/)) {
    const v = part.trim()
    if (v && !form.value.aliases.includes(v) && v !== form.value.character.trim()) {
      form.value.aliases.push(v)
    }
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
  commitAliases() // 输入框里还挂着文本时先落盘
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

// ---- 设置面板（模型选择 / 切片并发数 / 切片长度） ----
const showSettings = ref(false)

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

// ---- 日志秒表：进行中条目的耗时自增（pending 日志的 elapsed_ms 落盘为 0，前端按 at 自增） ----
const nowTick = ref(Date.now())
let tickTimer: ReturnType<typeof setInterval> | null = null
onMounted(() => {
  tickTimer = setInterval(() => (nowTick.value = Date.now()), 500)
})
onUnmounted(() => {
  if (tickTimer) clearInterval(tickTimer)
})

/** 日志耗时显示：已完成/失败用落盘值；进行中用「现在 - 发起时间」自增；放弃显示 — */
function logElapsed(l: LogSummary): string {
  if (l.abandoned) return '—'
  if (l.ok !== null && l.ok !== undefined) return fmtMs(l.elapsed_ms)
  const ms = Math.max(0, nowTick.value - (l.at ?? 0) * 1000)
  return `${(ms / 1000).toFixed(1)}s`
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

// ---- 调用日志 ----
const NODE_LABELS: Record<string, string> = {
  notes: '逐段笔记',
  generating: '档案生成',
  verifying: '引文回查',
}

/** 点流程图的格子：打开对应节点的最新一次调用日志 */
function openLogFor(node: string, match: (l: LogSummary) => boolean) {
  const hit = [...deconstructStore.logs]
    .reverse()
    .find((l) => l.node === node && match(l))
  if (hit && task.value) void deconstructStore.openLog(task.value.id, hit.seq)
}

function pickSegment(i: number) {
  openLogFor('notes', (l) => l.segment === i)
}

function pickFile(name: string) {
  // 文件格子对应生成或回查修复的调用（取最新一条）
  openLogFor('', (l) => l.detail === name)
}

/** 错误摘要：取首行并截断，行内展示完整见详情弹窗 */
function shortErr(e: string): string {
  const first = e.split('\n')[0]
  return first.length > 60 ? first.slice(0, 60) + '…' : first
}

/** token 数字缩写（≥1000 显 k） */
function fmtTokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
}

function fmtMs(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(1)}s` : `${ms}ms`
}

// ---- 复制为新制作 ----
function duplicateTask() {
  if (task.value) void deconstructStore.duplicateForCreate(task.value.id)
}

// ---- 重命名 ----
const showRename = ref(false)
const renameTarget = ref('')
const renameText = ref('')

function submitRename(v: string) {
  if (renameTarget.value) void deconstructStore.renameTask(renameTarget.value, v)
  showRename.value = false
}

// ---- 中止 ----
function abortTask() {
  if (task.value) void deconstructStore.abortTask(task.value.id)
}

function openRename(id: string, title: string | null | undefined) {
  renameTarget.value = id
  renameText.value = title ?? ''
  showRename.value = true
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
    {
      label: '重命名…',
      action: () => {
        if (t) openRename(id, t.title)
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
    // 粘贴 = 打开预填的信息填写界面，用户点「开始制作」才跑
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
  if (['selecting', 'aborted', 'interrupted'].includes(stage ?? '')) return 'warn'
  if (isRunning(stage)) return 'run'
  return ''
}

/** 任务显示名：自定义 title 优先，回退角色名 */
function taskTitle(t: { title?: string | null; character: string }): string {
  return t.title?.trim() || t.character
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
        <div class="head-ops">
          <button class="icon-btn" title="设置（模型 / 切片）" @click="showSettings = true">
            <Icon name="settings" :size="14" />
          </button>
          <button class="icon-btn" title="刷新" @click="deconstructStore.refreshTasks()">
            <Icon name="refresh" :size="14" />
          </button>
        </div>
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
            <span class="name">{{ taskTitle(t) }}</span>
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

      <!-- 设置面板 -->
      <CraftSettingsDialog v-if="showSettings" @close="showSettings = false" />
    </div>

    <!-- 主区：新建表单 或 任务详情 -->
    <div class="main-col">
      <!-- 新建表单 -->
      <div v-if="deconstructStore.creating || !deconstructStore.selectedId" class="form" @dragover.prevent @drop.prevent="onDrop">
        <h2>新建角色制作</h2>
        <p class="desc">
          导入剧本，提炼目录形态的角色档案包（灵魂 / 语言指纹 / 行为指南 / 关系 / 编年 / 红线 / 总索引）。
          使用的模型与切片参数在 <a class="link" @click="showSettings = true">设置面板</a>中选择；
          还没有模型档案时先去 <a class="link" @click="gotoModel">Model 页</a>注册。
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
              placeholder="多个别名用逗号分隔，如：マキマ, 支配恶魔"
              @keydown.enter.prevent="commitAliases"
              @blur="commitAliases"
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
              <h1>{{ taskTitle(task) }}</h1>
              <button
                class="icon-btn"
                title="重命名任务"
                @click="openRename(task.id, task.title)"
              >
                <Icon name="edit" :size="14" />
              </button>
              <span class="badge" :class="taskBadgeClass(task.progress.stage)">
                {{ isRunning(task.progress.stage) ? '运行中' : stageText(task.progress.stage) }}
              </span>
              <span class="spacer" />
              <button
                v-if="isRunning(task.progress.stage)"
                class="btn danger"
                @click="abortTask"
              >
                中止
              </button>
              <button
                v-if="task.progress.stage === 'selecting'"
                class="btn primary"
                @click="pickerOpen = true"
              >
                选择切片
              </button>
              <button
                v-if="['failed', 'aborted', 'interrupted'].includes(task.progress.stage)"
                class="btn primary"
                @click="deconstructStore.retryTask(task.id)"
              >
                {{ task.progress.stage === 'interrupted' ? '继续' : '从断点重试' }}
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

          <!-- 流程图（切片/文件格子可点开对应调用日志） -->
          <PipelineFlow
            :progress="task.progress"
            @pick-segment="pickSegment"
            @pick-file="pickFile"
          />


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

          <!-- 调用日志：每次 AI 调用的发出/收到全文 -->
          <div v-if="deconstructStore.logs.length" class="logs">
            <h3>调用日志</h3>
            <button
              v-for="l in deconstructStore.logs"
              :key="l.seq"
              class="log-row"
              @click="deconstructStore.openLog(task.id, l.seq)"
            >
              <span class="seq">#{{ l.seq }}</span>
              <span class="lnode">{{ NODE_LABELS[l.node] ?? l.node }}</span>
              <span class="ldetail">
                {{ l.detail }}
                <span v-if="(l.attempt_count ?? 1) > 1" class="lretry">{{ l.attempt_count }} 次尝试</span>
                <span v-if="l.error" class="lerr" :title="l.error">{{ shortErr(l.error) }}</span>
              </span>
              <span class="lms">
                <template v-if="l.input_tokens">
                  ↑{{ fmtTokens(l.input_tokens) }} ↓{{ fmtTokens(l.output_tokens ?? 0) }}<template v-if="l.cache_hit_rate != null"> · 命中{{ l.cache_hit_rate }}%</template>
                </template>
                {{ logElapsed(l) }}
              </span>
              <span
                class="lok"
                :class="{
                  ok: l.ok === true,
                  err: l.ok === false,
                  off: !!l.abandoned,
                  run: l.ok === null && !l.abandoned,
                }"
              >
                {{
                  l.abandoned
                    ? '放弃'
                    : l.ok === true
                      ? '已完成'
                      : l.ok === false
                        ? '失败'
                        : '进行中'
                }}
              </span>
            </button>
          </div>

          <!-- 选择闸门弹窗 -->
          <SegmentPicker
            v-if="pickerOpen && task.progress.stage === 'selecting' && task.segments?.length"
            :segments="task.segments"
            :character="task.character"
            @submit="deconstructStore.selectSegments(task.id, $event)"
            @close="pickerOpen = false"
          />

          <!-- 日志详情弹窗 -->
          <LogDialog
            v-if="deconstructStore.logDetail"
            :log="deconstructStore.logDetail"
            @close="deconstructStore.closeLog()"
          />

          <!-- 重命名弹窗 -->
          <PromptDialog
            v-if="showRename"
            title="重命名任务"
            :initial="renameText"
            placeholder="留空则回退为角色名显示"
            @close="showRename = false"
            @submit="submitRename"
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

.head-ops {
  display: flex;
  align-items: center;
  gap: 2px;
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

/* ---- 调用日志列表 ---- */
.logs {
  margin-top: 14px;
}

.logs h3 {
  margin: 0 0 8px;
  font-size: calc(13px * var(--font-scale-ui));
  color: var(--text-dim);
}

.log-row {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 7px 10px;
  border: 1px solid var(--border);
  border-radius: 7px;
  background: var(--bg);
  font-size: calc(12px * var(--font-scale-ui));
  text-align: left;
  cursor: pointer;
  margin-bottom: 4px;
}

@media (hover: hover) {
  .log-row:hover {
    border-color: var(--accent);
  }
}

.seq {
  color: var(--text-dim);
  width: 34px;
  flex: none;
}

.lnode {
  color: var(--text);
  flex: none;
}

.ldetail {
  flex: 1;
  min-width: 0;
  color: var(--text-dim);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.lok {
  flex: none;
  min-width: 44px;
  text-align: right;
  font-size: calc(11px * var(--font-scale-ui));
}

.lerr {
  margin-left: 8px;
  color: #c05050;
  font-size: calc(11px * var(--font-scale-ui));
}

.lretry {
  margin-left: 8px;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--bg-soft);
  color: var(--text-dim);
  font-size: calc(10px * var(--font-scale-ui));
}

.lok.off {
  color: var(--text-dim);
}

.lok.ok {
  color: #3a9a50;
}

.lok.err {
  color: #c05050;
}

.lok.run {
  color: var(--accent);
  animation: breathe 1.4s ease-in-out infinite;
}

.lms {
  flex: none;
  color: var(--text-dim);
  font-size: calc(11px * var(--font-scale-ui));
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
