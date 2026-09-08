// 当前打开项目的写作状态（单例 reactive，规模还小不引 Pinia）
import { reactive } from 'vue'
import { apiPost } from '../api'
import { watchProject, type WatchEventData } from '../utils/fileWatch'
import type { components } from '../api-types'

export type ProjectItem = components['schemas']['ProjectItem']
export type TreeNode = components['schemas']['TreeNode']
type FileContent = components['schemas']['FileContent']

export const projectStore = reactive({
  /** 当前打开的项目（null = 项目列表模式） */
  current: null as ProjectItem | null,
  /** 项目目录树（目录与 .md 文件） */
  tree: [] as TreeNode[],
  /** 第二栏选中的目录（项目内相对路径；'' = 项目根） */
  selectedDir: '正文',
  /** 编辑器当前打开的文稿（项目内相对路径） */
  currentFile: null as string | null,
  /** 编辑区标题栏的标签（有序，可拖拽调整） */
  tabs: [] as string[],
  /** 正在新建子目录的父目录（'' = 项目根；null = 未在命名） */
  namingDir: null as string | null,
  /** 正在重命名的目录（null = 未在重命名） */
  renamingDir: null as string | null,
  /** 编辑器状态（Editor 写入；字数常驻右上角浮层，保存失败以错误色提示） */
  wordCount: 0,
  saveState: 'saved' as 'saved' | 'saving' | 'error',
  /** 当前文稿的最新内容（含未保存输入，Editor 同步；预览实时渲染用） */
  currentContent: '',

  async open(project: ProjectItem) {
    this.current = project
    this.currentFile = null
    this.tabs = []
    this.selectedDir = '正文'
    await this.refreshTree()
    // 默认选中「正文」；没有则退回第一个根目录，再无目录退回项目根（根级散落文稿）
    if (!findDir(this.tree, '正文')) {
      this.selectedDir = this.tree.find((n) => n.kind === 'dir')?.path ?? ''
    }
    // 监听目录变动（外部编辑/同步盘/AI 写稿）；切换项目先停旧监听
    stopWatch?.()
    stopWatch = watchProject(project.path, {
      onEvent: (e) => void handleFsChange(e),
      onAuthError: () => console.warn('[shiro] 目录监听鉴权失败，已停止重连'),
    })
  },

  close() {
    stopWatch?.()
    stopWatch = null
    this.current = null
    this.tree = []
    this.currentFile = null
    this.tabs = []
  },

  /** 打开文稿：入标签栏并设为当前（点击列表项的行为） */
  openFile(rel: string) {
    if (!this.tabs.includes(rel)) this.tabs.push(rel)
    this.currentFile = rel
  },

  /** 添加到标签栏但不切换当前编辑（右键菜单「添加到编辑器标题栏」） */
  pinFile(rel: string) {
    if (!this.tabs.includes(rel)) this.tabs.push(rel)
  },

  /** 关闭标签：当前标签被关时切到相邻标签（右侧优先） */
  closeTab(rel: string) {
    const i = this.tabs.indexOf(rel)
    if (i < 0) return
    this.tabs.splice(i, 1)
    if (this.currentFile === rel) {
      this.currentFile = this.tabs[i] ?? this.tabs[i - 1] ?? null
    }
  },

  /** 拖拽排序 */
  moveTab(from: number, to: number) {
    const [item] = this.tabs.splice(from, 1)
    this.tabs.splice(to, 0, item)
  },

  async refreshTree() {
    if (!this.current) return
    const res = await apiPost<components['schemas']['TreeResponse']>('/api/v1/projects/tree', {
      path: this.current.path,
    })
    const children = res.children ?? []
    // 内容一致不替换：避免监听推送/手动操作引发的重复刷新触发下游 watcher（文稿预览等）
    if (JSON.stringify(children) === JSON.stringify(this.tree)) return
    this.tree = children
  },

  /** 新建子目录（parentRel '' = 项目根） */
  async createDir(parentRel: string, name: string) {
    if (!this.current) return
    const dir = parentRel ? `${parentRel}/${name}` : name
    await apiPost('/api/v1/projects/dir/create', { path: this.current.path, dir })
    this.namingDir = null
    await this.refreshTree()
  },

  /** 删除目录（daemon 侧：空目录直删，非空移 .shiro/trash/） */
  async removeDir(rel: string) {
    if (!this.current) return
    await apiPost('/api/v1/projects/dir/delete', { path: this.current.path, dir: rel })
    // 选中目录/当前文稿/标签在被删子树内时回退
    if (this.selectedDir === rel || this.selectedDir.startsWith(rel + '/')) {
      this.selectedDir = rel.split('/').slice(0, -1).join('/')
    }
    this.tabs = this.tabs.filter((t) => t !== rel && !t.startsWith(rel + '/'))
    if (this.currentFile === rel || this.currentFile?.startsWith(rel + '/')) {
      this.currentFile = null
    }
    await this.refreshTree()
  },

  /** 重命名项目内条目（文件/目录）：选中目录/标签/当前文稿的路径前缀联动替换 */
  async renameEntry(rel: string, newName: string) {
    if (!this.current) return
    await apiPost('/api/v1/projects/entry/rename', {
      path: this.current.path,
      rel,
      new_name: newName,
    })
    const parent = rel.split('/').slice(0, -1).join('/')
    const newRel = parent ? `${parent}/${newName}` : newName
    const remap = (p: string) => (p === rel || p.startsWith(rel + '/') ? newRel + p.slice(rel.length) : p)
    this.selectedDir = remap(this.selectedDir)
    this.tabs = this.tabs.map(remap)
    if (this.currentFile) this.currentFile = remap(this.currentFile)
    await this.refreshTree()
  },

  /** 删除文稿（移 .shiro/trash/，可手动恢复） */
  async removeFile(rel: string) {
    if (!this.current) return
    await apiPost('/api/v1/projects/file/delete', { path: this.current.path, file: rel })
    this.tabs = this.tabs.filter((t) => t !== rel)
    if (this.currentFile === rel) this.currentFile = null
    await this.refreshTree()
  },
})

// ---- 磁盘变动监听（daemon SSE 推送，见 utils/fileWatch.ts）----
let stopWatch: (() => void) | null = null

/** 当前文稿被外部修改时的回调：content 为新内容，null 为文件已删除/不可读（Editor 挂载时注册，卸载时注销） */
let externalFileHandler: ((file: string, content: string | null) => void) | null = null

export function onExternalFileChange(cb: typeof externalFileHandler) {
  externalFileHandler = cb
}

async function handleFsChange(e: WatchEventData) {
  const project = projectStore.current
  if (!project) return
  // 目录结构/内容都可能有变：刷新目录树（内部按内容去重）
  await projectStore.refreshTree()
  // 当前打开的文稿涉及变更：拉新内容通知编辑器（changed 为空 = 服务端滞后溢出，按涉及处理）
  const file = projectStore.currentFile
  if (!file || !externalFileHandler) return
  if (e.changed.length > 0 && !e.changed.includes(file)) return
  try {
    const res = await apiPost<FileContent>('/api/v1/projects/file/read', {
      path: project.path,
      file,
    })
    externalFileHandler(file, res.content)
  } catch {
    externalFileHandler(file, null)
  }
}

// 开发调试钩子：dev 模式或深链冒烟（hash 带 open=）时可在渲染进程控制台直接操作 store
if (import.meta.env.DEV || location.hash.includes('open=')) {
  ;(window as unknown as Record<string, unknown>).projectStore = projectStore
}

/** 在目录树中查找目录节点的直接子节点；未找到返回 null */
export function findDir(nodes: TreeNode[], path: string): TreeNode[] | null {
  if (path === '') return nodes
  for (const node of nodes) {
    if (node.kind !== 'dir') continue
    if (node.path === path) return node.children ?? []
    const found = findDir(node.children ?? [], path)
    if (found !== null) return found
  }
  return null
}
