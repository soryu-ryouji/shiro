// 角色制作（拆解任务）状态：任务列表 / 详情 / 创建 / 保存到人物库 / 轮询进度。
// API 见 shiro-daemon/src/deconstruct/api.rs；规范见 docs/工具实现/角色卡提炼如何实现.md
import { reactive } from 'vue'
import { apiFetch } from '../api'
import type { components } from '../api-types'

export type TaskSummary = components['schemas']['TaskSummary']
export type TaskDetail = components['schemas']['TaskDetail']
export type Progress = components['schemas']['Progress']
export type LogSummary = components['schemas']['LogSummary']
export type CraftSettings = components['schemas']['CraftSettings']

/** 运行中的阶段（轮询继续；其余为终态或未开始） */
const RUNNING_STAGES = new Set([
  'pending',
  'probing',
  'chunking',
  'notes',
  'evidence',
  'generating',
  'verifying',
])

export function isRunning(stage: string | undefined): boolean {
  return !!stage && RUNNING_STAGES.has(stage)
}

/** 生成文件的显示名 */
export const FILE_LABELS: Record<string, string> = {
  soul: '灵魂',
  speech_patterns: '语言指纹',
  behavior_guide: '行为指南',
  relationship_dynamics: '关系动力',
  key_life_events: '编年事件',
  limit: '红线',
  index: '总索引',
}

/** 阶段显示名（流程图节点用） */
export const STAGE_LABELS: Record<string, string> = {
  probing: '素材探测',
  chunking: '切块',
  selecting: '待选择',
  notes: '逐段笔记',
  evidence: '证据汇编',
  generating: '档案生成',
  verifying: '引文回查',
  done: '完成',
  failed: '失败',
  aborted: '已中止',
  interrupted: '已中断',
}

export const deconstructStore = reactive({
  tasks: [] as TaskSummary[],
  tasksLoaded: false,
  listLoading: false,
  error: '',
  /** 选中任务（制作记录点击） */
  selectedId: null as string | null,
  detail: null as TaskDetail | null,
  detailLoading: false,
  /** 新建模式（主区显示表单而非任务详情） */
  creating: false,
  submitting: false,
  /** 轮询定时器 */
  pollTimer: null as ReturnType<typeof setInterval> | null,

  async refreshTasks() {
    this.listLoading = true
    this.error = ''
    try {
      const res = await apiFetch<components['schemas']['TaskListResponse']>(
        '/api/v1/db/deconstruct/tasks',
      )
      this.tasks = res.tasks ?? []
      this.tasksLoaded = true
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    } finally {
      this.listLoading = false
    }
  },

  /** 进入制作视图时调用：拉列表；有选中任务则恢复轮询 */
  async enter() {
    if (!this.tasksLoaded) await this.refreshTasks()
    if (this.selectedId) {
      await this.loadDetail(this.selectedId)
    }
    this.ensurePolling()
  },

  ensurePolling() {
    const active =
      isRunning(this.detail?.progress.stage) ||
      this.tasks.some((t) => isRunning(t.stage))
    if (active && !this.pollTimer) {
      this.pollTimer = setInterval(() => void this.pollTick(), 1000)
    } else if (!active && this.pollTimer) {
      clearInterval(this.pollTimer)
      this.pollTimer = null
    }
  },

  async pollTick() {
    // 运行中任务存在：刷列表 + 刷新选中详情
    const anyRunning = this.tasks.some((t) => isRunning(t.stage))
    if (!anyRunning) {
      this.ensurePolling()
      return
    }
    await this.refreshTasks()
    if (this.selectedId && isRunning(this.detail?.progress.stage)) {
      await this.loadDetail(this.selectedId)
      void this.fetchLogs(this.selectedId)
      if (this.logDetailTask && this.logDetailSeq !== null) {
        void this.openLog(this.logDetailTask, this.logDetailSeq)
      }
    } else {
      this.ensurePolling()
    }
  },

  async selectTask(id: string) {
    if (this.selectedId === id && !this.creating) return
    this.creating = false
    this.selectedId = id
    await this.loadDetail(id)
    void this.fetchLogs(id)
    this.ensurePolling()
  },

  async loadDetail(id: string) {
    this.detailLoading = true
    try {
      this.detail = await apiFetch<TaskDetail>(
        `/api/v1/db/deconstruct/tasks/${encodeURIComponent(id)}`,
      )
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    } finally {
      this.detailLoading = false
    }
  },

  /** 创建任务：导入剧本 + 角色名/别名。返回是否成功 */
  async createTask(form: {
    sourceName: string
    content: string
    character: string
    aliases: string[]
  }): Promise<boolean> {
    this.submitting = true
    this.error = ''
    try {
      const res = await apiFetch<TaskSummary>('/api/v1/db/deconstruct/tasks', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          source_name: form.sourceName,
          content: form.content,
          character: form.character,
          aliases: form.aliases,
        }),
      })
      this.creating = false
      this.selectedId = res.id
      await this.refreshTasks()
      await this.loadDetail(res.id)
      this.ensurePolling()
      return true
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
      return false
    } finally {
      this.submitting = false
    }
  },

  /** 产物保存到人物库，返回角色 id */
  async saveToLibrary(id: string): Promise<string | null> {
    this.error = ''
    try {
      const res = await apiFetch<components['schemas']['SaveTaskResponse']>(
        `/api/v1/db/deconstruct/tasks/${encodeURIComponent(id)}/save`,
        { method: 'POST' },
      )
      await this.refreshTasks()
      await this.loadDetail(id)
      return res.character_id
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
      return null
    }
  },

  /** 提交段选择（选择闸门），开始分析 */
  async selectSegments(id: string, selected: number[]) {
    this.error = ''
    try {
      await apiFetch(
        `/api/v1/db/deconstruct/tasks/${encodeURIComponent(id)}/select`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ selected }),
        },
      )
      await this.refreshTasks()
      await this.loadDetail(id)
      this.ensurePolling()
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    }
  },

  /** 失败任务从断点重试（后端跳过已有产物的阶段） */
  async retryTask(id: string) {
    this.error = ''
    try {
      await apiFetch(
        `/api/v1/db/deconstruct/tasks/${encodeURIComponent(id)}/retry`,
        { method: 'POST' },
      )
      await this.refreshTasks()
      if (this.selectedId === id) await this.loadDetail(id)
      this.ensurePolling()
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    }
  },

  /** 应用内任务剪贴板（Ctrl+C 标记 / Ctrl+V 粘贴为新制作） */
  copiedTaskId: null as string | null,
  copiedTip: '',
  copyTipTimer: null as ReturnType<typeof setTimeout> | null,

  copyTask(id: string) {
    this.copiedTaskId = id
    const t = this.tasks.find((x) => x.id === id)
    this.copiedTip = `已复制「${t?.character ?? ''} · ${t?.source_name ?? ''}」`
    if (this.copyTipTimer) clearTimeout(this.copyTipTimer)
    this.copyTipTimer = setTimeout(() => (this.copiedTip = ''), 3000)
  },

  /** 新建/复制表单的草稿（store 持有：复制任务时同步写入，无 watch 时序面） */
  formDraft: {
    sourceName: '',
    character: '',
    aliases: [] as string[],
    content: '',
    fileName: '',
  },

  resetForm() {
    this.formDraft = { sourceName: '', character: '', aliases: [], content: '', fileName: '' }
  },

  /** 打开新建表单；带 prefill 时回填（复制任务场景） */
  openCreateForm(prefill?: {
    sourceName: string
    character: string
    aliases: string[]
    content: string
  }) {
    this.creating = true
    this.selectedId = null
    this.detail = null
    if (prefill) {
      this.formDraft = {
        sourceName: prefill.sourceName,
        character: prefill.character,
        aliases: [...prefill.aliases],
        content: prefill.content,
        fileName: `${prefill.sourceName}.txt`,
      }
    } else {
      this.resetForm()
    }
  },

  /** 复制任务填写信息到新表单（原文在 daemon 侧，拉回前端回填） */
  async duplicateForCreate(id: string) {
    this.error = ''
    try {
      const res = await apiFetch<components['schemas']['TaskSourceResponse']>(
        `/api/v1/db/deconstruct/tasks/${encodeURIComponent(id)}/source`,
      )
      this.openCreateForm({
        sourceName: res.source_name,
        character: res.character,
        aliases: res.aliases ?? [],
        content: res.content,
      })
      // 粘贴即消费：复制标记与提示清除（要再粘就重新复制）
      this.copiedTaskId = null
      this.copiedTip = ''
      if (this.copyTipTimer) clearTimeout(this.copyTipTimer)
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    }
  },

  /** 调用日志（选中任务的；含 pending 条目） */
  logs: [] as LogSummary[],
  logDetail: null as Record<string, unknown> | null,
  /** 打开中的日志定位（任务轮询时持续刷新，pending 响应实时增长） */
  logDetailTask: null as string | null,
  logDetailSeq: null as number | null,

  /** 运行设置（模型选择 / 切片并发数 / 切片长度） */
  settings: null as CraftSettings | null,
  settingsLoading: false,
  settingsSaving: false,

  async loadSettings() {
    this.settingsLoading = true
    try {
      this.settings = await apiFetch<CraftSettings>('/api/v1/db/deconstruct/settings')
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    } finally {
      this.settingsLoading = false
    }
  },

  /** 保存运行设置（逐项可选，未提供的项不改动）。返回是否成功 */
  async saveSettings(patch: {
    model?: string
    max_concurrency?: number
    segment_chars?: number
  }): Promise<boolean> {
    this.settingsSaving = true
    this.error = ''
    try {
      this.settings = await apiFetch<CraftSettings>('/api/v1/db/deconstruct/settings', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(patch),
      })
      return true
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
      return false
    } finally {
      this.settingsSaving = false
    }
  },

  async fetchLogs(id: string) {
    try {
      const res = await apiFetch<components['schemas']['LogListResponse']>(
        `/api/v1/db/deconstruct/tasks/${encodeURIComponent(id)}/logs`,
      )
      this.logs = res.logs ?? []
    } catch {
      // 日志拉取失败不打扰主流程
    }
  },

  async openLog(id: string, seq: number) {
    this.logDetailTask = id
    this.logDetailSeq = seq
    try {
      this.logDetail = await apiFetch<Record<string, unknown>>(
        `/api/v1/db/deconstruct/tasks/${encodeURIComponent(id)}/logs/${seq}`,
      )
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    }
  },

  closeLog() {
    this.logDetail = null
    this.logDetailTask = null
    this.logDetailSeq = null
  },

  /** 中止运行中的任务 */
  async abortTask(id: string) {
    this.error = ''
    try {
      await apiFetch(`/api/v1/db/deconstruct/tasks/${encodeURIComponent(id)}/abort`, {
        method: 'POST',
      })
      await this.refreshTasks()
      if (this.selectedId === id) await this.loadDetail(id)
      this.ensurePolling()
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    }
  },

  /** 改名（空串清除自定义名） */
  async renameTask(id: string, title: string) {
    this.error = ''
    try {
      await apiFetch(`/api/v1/db/deconstruct/tasks/${encodeURIComponent(id)}/rename`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ title }),
      })
      await this.refreshTasks()
      if (this.selectedId === id) await this.loadDetail(id)
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    }
  },

  async deleteTask(id: string) {
    this.error = ''
    try {
      await apiFetch(`/api/v1/db/deconstruct/tasks/${encodeURIComponent(id)}`, {
        method: 'DELETE',
      })
      if (this.selectedId === id) {
        this.selectedId = null
        this.detail = null
      }
      await this.refreshTasks()
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    }
  },
})
