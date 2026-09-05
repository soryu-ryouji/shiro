// 外观配置：字体（界面/正文分设，内置 + 系统字体）与字号（自由数值）。localStorage 持久化。
// 内置字体经 @fontsource 自托管（main.ts 引入）；切换经 --font-ui / --font-editor / --font-scale 变量全局生效。

export interface FontOption {
  key: string
  label: string
  /** CSS font-family 栈 */
  stack: string
}

const SANS_STACK = "system-ui, -apple-system, 'Segoe UI', 'Microsoft YaHei', sans-serif"
const SERIF_STACK = "Georgia, 'Songti SC', 'SimSun', 'Microsoft YaHei', serif"

export const FONT_OPTIONS: FontOption[] = [
  { key: 'lxgw', label: '霞鹜文楷（内置）', stack: `'LXGW WenKai', ${SANS_STACK}` },
  { key: 'sans', label: '系统无衬线', stack: SANS_STACK },
  { key: 'serif', label: '系统衬线', stack: SERIF_STACK },
]

const UI_FONT_KEY = 'shiro.font.ui'
const EDITOR_FONT_KEY = 'shiro.font.editor'
const LEGACY_FONT_KEY = 'shiro.font'

function stackFor(key: string): string {
  const builtin = FONT_OPTIONS.find((o) => o.key === key)
  return builtin ? builtin.stack : `'${key}', ${SANS_STACK}`
}

/** 读取存储的字体 key：新 key 缺失时沿用旧的全局设置（shiro.font）值，最后回退默认 */
function readFontKey(storageKey: string, fallback: string): string {
  return localStorage.getItem(storageKey) ?? localStorage.getItem(LEGACY_FONT_KEY) ?? fallback
}

/** 界面字体（默认系统无衬线） */
export function currentUiFontKey(): string {
  return readFontKey(UI_FONT_KEY, 'sans')
}

/** 正文字体（默认霞鹜文楷） */
export function currentEditorFontKey(): string {
  return readFontKey(EDITOR_FONT_KEY, 'lxgw')
}

export function applyUiFont(key: string): void {
  document.documentElement.style.setProperty('--font-ui', stackFor(key))
  localStorage.setItem(UI_FONT_KEY, key)
}

export function applyEditorFont(key: string): void {
  document.documentElement.style.setProperty('--font-editor', stackFor(key))
  localStorage.setItem(EDITOR_FONT_KEY, key)
}

/** 枚举系统字体 family 名（Local Font Access API；浏览器/旧内核不可用返回 null） */
export async function listSystemFonts(): Promise<string[] | null> {
  const query = (window as unknown as { queryLocalFonts?: () => Promise<{ family: string }[]> })
    .queryLocalFonts
  if (!query) return null
  const fonts = await query.call(window)
  return [...new Set(fonts.map((f) => f.family))].sort((a, b) => a.localeCompare(b, 'zh-CN'))
}

// ---- 字号：界面/正文分设的自由数值（px；内部转为缩放系数驱动 calc） ----

export const FONT_SIZE_MIN = 12
export const FONT_SIZE_MAX = 24
export const UI_FONT_SIZE_DEFAULT = 14
export const EDITOR_FONT_SIZE_DEFAULT = 15

const UI_SIZE_KEY = 'shiro.fontSize.ui'
const EDITOR_SIZE_KEY = 'shiro.fontSize.editor'
const LEGACY_SIZE_KEY = 'shiro.fontSize'

function clampSize(v: number): number {
  return Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, v))
}

function readSize(key: string, fallback: number): number {
  const v = Number(localStorage.getItem(key))
  return v >= FONT_SIZE_MIN && v <= FONT_SIZE_MAX ? v : fallback
}

/** 界面字号（沿用旧全局设置值作为初值） */
export function currentUiFontSize(): number {
  const v = localStorage.getItem(UI_SIZE_KEY) ?? localStorage.getItem(LEGACY_SIZE_KEY)
  const n = Number(v)
  return v !== null && n >= FONT_SIZE_MIN && n <= FONT_SIZE_MAX ? n : UI_FONT_SIZE_DEFAULT
}

/** 正文字号（旧全局值是缩放语义不沿用，回退默认 15） */
export function currentEditorFontSize(): number {
  return readSize(EDITOR_SIZE_KEY, EDITOR_FONT_SIZE_DEFAULT)
}

export function applyUiFontSize(px: number): void {
  const clamped = clampSize(px)
  document.documentElement.style.setProperty('--font-scale-ui', String(clamped / UI_FONT_SIZE_DEFAULT))
  localStorage.setItem(UI_SIZE_KEY, String(clamped))
}

export function applyEditorFontSize(px: number): void {
  const clamped = clampSize(px)
  document.documentElement.style.setProperty('--font-scale-editor', String(clamped / EDITOR_FONT_SIZE_DEFAULT))
  localStorage.setItem(EDITOR_SIZE_KEY, String(clamped))
}
