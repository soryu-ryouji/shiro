// Chat（对话式修改项目文档）状态：项目选择 / 会话列表 / SSE 消息流 / 提案应用。
// API 见 shiro-daemon/src/chat/api.rs；提案为整文件覆盖，用户确认后写盘。
import { reactive } from 'vue'
import { apiPost, apiBase, apiToken } from '../api'
import { createSseParser } from '../utils/sse'
import type { components } from '../api-types'
import { projectStore, type TreeNode } from './project'

export type SessionSummary = components['schemas']['SessionSummary']
export type SessionDetail = components['schemas']['SessionDetail']
export type ChatMessage = components['schemas']['ChatMessage']
export type Proposal = components['schemas']['Proposal']
type ProjectItem = components['schemas']['ProjectItem']

/** SSE data 帧（daemon send_chat_messages 推送）；error 帧的 message 是错误文案 */
interface StreamFrame {
  type: 'delta' | 'thinking' | 'done' | 'error'
  text?: string
  message?: ChatMessage | string
}

export const chatStore = reactive({
  /** 项目列表与当前选中（绝对路径；'' = 未选） */
  projects: [] as ProjectItem[],
  projectPath: '',
  /** 当前项目的会话列表（最近更新在前） */
  sessions: [] as SessionSummary[],
  sessionsLoaded: false,
  /** 当前会话详情（null = 未选会话） */
  session: null as SessionDetail | null,
  /** 生成中（SSE 流接收中；同一会话 daemon 侧也有并发闸门） */
  streaming: false,
  /** 流式中的助手消息增量（完成后并入 session.messages 并清空） */
  streamText: '',
  streamThinking: '',
  /** 当前项目的文稿相对路径（附件选择用） */
  files: [] as string[],
  error: '',

  /** 中止控制器（stop() 断开 SSE → daemon 中止生成） */
  abort: null as AbortController | null,

  get projectName(): string {
    return this.projects.find((p) => p.path === this.projectPath)?.name ?? ''
  },

  /** 拉项目列表；默认选中写作页正在打开的项目 */
  async loadProjects() {
    const res = await apiPost<{ projects: ProjectItem[] }>('/api/v1/projects/list')
    this.projects = res.projects.filter((p) => p.exists)
    const opened = projectStore.current
    if (opened && this.projects.some((p) => p.path === opened.path)) {
      await this.selectProject(opened.path, true)
    } else if (this.projectPath && this.projects.some((p) => p.path === this.projectPath)) {
      await this.refreshSessions()
    } else if (this.projects.length) {
      await this.selectProject(this.projects[0].path, true)
    }
  },

  async selectProject(path: string, force = false) {
    if (this.projectPath === path && !force) return
    this.projectPath = path
    this.session = null
    this.sessionsLoaded = false
    this.files = []
    await Promise.all([this.refreshSessions(), this.loadFiles()])
  },

  async refreshSessions() {
    if (!this.projectPath) return
    try {
      const res = await apiPost<{ sessions: SessionSummary[] }>('/api/v1/chat/sessions/list', {
        path: this.projectPath,
      })
      this.sessions = res.sessions
      this.sessionsLoaded = true
    } catch (e) {
      this.error = String(e)
    }
  },

  /** 目录树拉平为文稿相对路径列表（附件选择用） */
  async loadFiles() {
    if (!this.projectPath) return
    const res = await apiPost<{ children: TreeNode[] }>('/api/v1/projects/tree', {
      path: this.projectPath,
    })
    const out: string[] = []
    const walk = (nodes: TreeNode[]) => {
      for (const n of nodes) {
        if (n.kind === 'folder') walk(n.children ?? [])
        else out.push(n.path)
      }
    }
    walk(res.children ?? [])
    this.files = out
  },

  async openSession(id: string) {
    if (this.streaming) return
    this.error = ''
    try {
      this.session = await apiPost<SessionDetail>('/api/v1/chat/sessions/get', {
        path: this.projectPath,
        id,
      })
    } catch (e) {
      this.error = String(e)
    }
  },

  async newSession() {
    if (!this.projectPath) return
    this.error = ''
    try {
      this.session = await apiPost<SessionDetail>('/api/v1/chat/sessions/create', {
        path: this.projectPath,
      })
      await this.refreshSessions()
    } catch (e) {
      this.error = String(e)
    }
  },

  async removeSession(id: string) {
    try {
      await apiPost('/api/v1/chat/sessions/delete', { path: this.projectPath, id })
      if (this.session?.id === id) this.session = null
      await this.refreshSessions()
    } catch (e) {
      this.error = String(e)
    }
  },

  /** 发送消息：SSE 流式接收，增量进 streamText，完成后并入消息列表 */
  async send(content: string, attachments: string[]) {
    if (!this.session || this.streaming || !content.trim()) return
    const sid = this.session.id
    this.streaming = true
    this.streamText = ''
    this.streamThinking = ''
    this.error = ''
    // 乐观追加上用户消息（daemon 侧同步持久化）
    this.session.messages.push({
      role: 'user',
      content,
      at: Math.floor(Date.now() / 1000),
      attachments: [...attachments],
      proposals: [],
    } as ChatMessage)

    const ctrl = new AbortController()
    this.abort = ctrl
    try {
      const res = await fetch(`${apiBase}/api/v1/chat/sessions/messages`, {
        method: 'POST',
        signal: ctrl.signal,
        headers: { Authorization: `Bearer ${apiToken}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: this.projectPath, id: sid, content, attachments }),
      })
      if (!res.ok || !res.body) {
        // 4xx 带错误文案（如模型未配置），优先展示
        let detail = `HTTP ${res.status}`
        try {
          const body = (await res.json()) as { message?: string }
          if (body.message) detail = body.message
        } catch {
          // 非 JSON 响应体，保留状态码
        }
        throw new Error(detail)
      }
      const reader = res.body.getReader()
      const decoder = new TextDecoder()
      const parse = createSseParser((e) => {
        try {
          this.handleFrame(JSON.parse(e.data) as StreamFrame)
        } catch {
          // 非 JSON 帧：忽略
        }
      })
      for (;;) {
        const { done, value } = await reader.read()
        if (done) break
        parse(decoder.decode(value, { stream: true }))
      }
    } catch (e) {
      // 主动中止不报错（daemon 侧也不保存部分回复）
      if (!(e instanceof DOMException && e.name === 'AbortError')) {
        this.error = String(e)
      }
    } finally {
      this.abort = null
      this.streaming = false
      this.streamText = ''
      this.streamThinking = ''
      // 会话标题可能被首条消息回填
      void this.refreshSessions()
    }
  },

  handleFrame(f: StreamFrame) {
    if (f.type === 'delta' && f.text) {
      this.streamText += f.text
    } else if (f.type === 'thinking' && f.text) {
      this.streamThinking += f.text
    } else if (f.type === 'done' && f.message && typeof f.message !== 'string') {
      this.session?.messages.push(f.message)
    } else if (f.type === 'error') {
      this.error = typeof f.message === 'string' ? f.message : '生成失败'
      // 错误占位：乐观追加的用户消息保留（服务端已持久化），历史里就是最新状态
    }
  },

  /** 中止生成：断开 SSE，daemon 检测断开后停止（不保存部分回复） */
  stop() {
    this.abort?.abort()
  },

  /** 应用提案（整文件覆盖写盘）；成功后就地更新提案状态 */
  async applyProposal(p: Proposal) {
    if (!this.session || p.applied) return
    this.error = ''
    try {
      const res = await apiPost<{ proposal: Proposal }>('/api/v1/chat/sessions/apply', {
        path: this.projectPath,
        id: this.session.id,
        proposal_id: p.id,
      })
      // 就地更新（reactive 深层替换字段，卡片状态即时变化）
      for (const msg of this.session.messages) {
        const hit = msg.proposals?.find((x) => x.id === res.proposal.id)
        if (hit) {
          hit.applied = res.proposal.applied
          hit.applied_at = res.proposal.applied_at
        }
      }
    } catch (e) {
      this.error = String(e)
    }
  },
})

/** 剥离回复里的 shiro-edit 提案块（提案以卡片形式单独渲染） */
export function stripProposalBlocks(text: string): string {
  return text.replace(/```shiro-edit\n[\s\S]*?```/g, '').trim()
}
