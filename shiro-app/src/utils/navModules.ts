// 导航模块显隐：可选模块（Chat / Database / Model）在设置面板逐个开关，localStorage 持久化。
// Project 是核心写作模块，恒显示，不在可选之列。
import { ref } from 'vue'
import type { NavKey } from '../types'

/** 可隐藏的导航模块（label 为设置面板显示名） */
export const OPTIONAL_NAV_MODULES: { key: NavKey; label: string }[] = [
  { key: 'chat', label: 'Chat（AI 对话）' },
  { key: 'database', label: 'Database（内容库）' },
  { key: 'model', label: 'Model（模型配置）' },
]

/** 可隐藏的导航模块 key 列表 */
export const OPTIONAL_NAV_KEYS: readonly NavKey[] = OPTIONAL_NAV_MODULES.map((m) => m.key)

const NAV_HIDDEN_KEY = 'shiro.nav.hidden'

/** 已隐藏的模块集合（替换式更新，watch 可直接消费） */
export const hiddenNavKeys = ref<NavKey[]>(readHidden())

function readHidden(): NavKey[] {
  try {
    const v = JSON.parse(localStorage.getItem(NAV_HIDDEN_KEY) ?? '[]') as unknown
    return Array.isArray(v) ? v.filter((k): k is NavKey => OPTIONAL_NAV_KEYS.includes(k as NavKey)) : []
  } catch {
    return []
  }
}

export function isNavHidden(key: NavKey): boolean {
  return hiddenNavKeys.value.includes(key)
}

/** 设置模块显隐；Project 等不可隐藏模块调用无效 */
export function setNavVisible(key: NavKey, visible: boolean): void {
  if (!OPTIONAL_NAV_KEYS.includes(key)) return
  const next = OPTIONAL_NAV_KEYS.filter((k) => (k === key ? !visible : hiddenNavKeys.value.includes(k)))
  hiddenNavKeys.value = next
  localStorage.setItem(NAV_HIDDEN_KEY, JSON.stringify(next))
}
