// 角色制作（拆解任务）状态：任务列表 / 详情 / 创建 / 保存到人物库 / 轮询进度。
// API 见 shiro-daemon/src/deconstruct/api.rs；规范见 docs/工具实现/角色卡提炼如何实现.md
import { reactive } from 'vue'
import { apiFetch } from '../api'
import type { components } from '../api-types'

export type TaskSummary = components['schemas']['TaskSummary']
export type TaskDetail = components['schemas']['TaskDetail']
export type Progress = components['schemas']['Progress']

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
  notes: '逐段笔记',
  evidence: '证据汇编',
  generating: '档案生成',
  verifying: '引文回查',
  done: '完成',
  failed: '失败',
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
    } else {
      this.ensurePolling()
    }
  },

  openCreateForm() {
    this.creating = true
    this.selectedId = null
    this.detail = null
  },

  async selectTask(id: string) {
    if (this.selectedId === id && !this.creating) return
    this.creating = false
    this.selectedId = id
    await this.loadDetail(id)
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
      await this.loadDetail(id)
      return res.character_id
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
      return null
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
