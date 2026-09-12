// 导航项定义（App/Sidebar/SettingsDialog 共用）
export type NavKey = 'project' | 'chat' | 'database' | 'model'

export interface NavItem {
  key: NavKey
  label: string
  icon: string
}

/** 全部导航项（Activity Bar 顺序）；显隐过滤见 utils/navModules.ts */
export const NAV_ITEMS: NavItem[] = [
  { key: 'project', label: 'Project', icon: 'folder' },
  { key: 'chat', label: 'Chat', icon: 'chat' },
  { key: 'database', label: 'Database', icon: 'database' },
  { key: 'model', label: 'Model', icon: 'model' },
]
