// 当前打开项目的写作状态（单例 reactive，规模还小不引 Pinia）
import { reactive } from 'vue'
import { apiFetch } from '../api'
import type { components } from '../api-types'

export type ProjectItem = components['schemas']['ProjectItem']
export type TreeNode = components['schemas']['TreeNode']

export const projectStore = reactive({
  /** 当前打开的项目（null = 项目列表模式） */
  current: null as ProjectItem | null,
  /** 项目目录树（目录与 .md 文件） */
  tree: [] as TreeNode[],
  /** 第二栏选中的目录（项目内相对路径；'' = 项目根） */
  selectedDir: '正文',
  /** 编辑器当前打开的文稿（项目内相对路径） */
  currentFile: null as string | null,
  /** 正在新建子目录的父目录（'' = 项目根；null = 未在命名） */
  namingDir: null as string | null,

  async open(project: ProjectItem) {
    this.current = project
    this.currentFile = null
    this.selectedDir = '正文'
    await this.refreshTree()
    // 默认选中「正文」；项目里没有则退回项目根
    if (!findDir(this.tree, '正文')) {
      this.selectedDir = ''
    }
  },

  close() {
    this.current = null
    this.tree = []
    this.currentFile = null
  },

  async refreshTree() {
    if (!this.current) return
    const res = await apiFetch<components['schemas']['TreeResponse']>(
      `/api/v1/projects/tree?path=${encodeURIComponent(this.current.path)}`,
    )
    this.tree = res.children ?? []
  },

  /** 新建子目录（parentRel '' = 项目根） */
  async createDir(parentRel: string, name: string) {
    if (!this.current) return
    const dir = parentRel ? `${parentRel}/${name}` : name
    await apiFetch('/api/v1/projects/dir', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: this.current.path, dir }),
    })
    this.namingDir = null
    await this.refreshTree()
  },

  /** 删除目录（daemon 侧：空目录直删，非空移 .shiro/trash/） */
  async removeDir(rel: string) {
    if (!this.current) return
    await apiFetch(
      `/api/v1/projects/dir?path=${encodeURIComponent(this.current.path)}&dir=${encodeURIComponent(rel)}`,
      { method: 'DELETE' },
    )
    // 选中目录/当前文稿在被删子树内时回退
    if (this.selectedDir === rel || this.selectedDir.startsWith(rel + '/')) {
      this.selectedDir = rel.split('/').slice(0, -1).join('/')
    }
    if (this.currentFile === rel || this.currentFile?.startsWith(rel + '/')) {
      this.currentFile = null
    }
    await this.refreshTree()
  },
})

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
