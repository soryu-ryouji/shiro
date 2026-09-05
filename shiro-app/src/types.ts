// 导航项定义（App/Sidebar/TitleBar 共用）
export type NavKey = 'project' | 'database' | 'model'

export interface NavItem {
  key: NavKey
  label: string
  icon: string
}
