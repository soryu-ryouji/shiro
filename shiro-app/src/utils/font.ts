// 外观配置：字体（界面/正文分设，内置 + 系统字体）、字号、编辑区排版（行距/段距/分段方式）、
// 正文宽度、预览排版（行距/段距，独立于编辑器——阅读偏好与写作偏好分开）、大纲显示模式。
// localStorage 持久化；内置字体经 @fontsource 自托管（main.ts 引入），切换经 CSS 变量全局生效。
import { ref } from 'vue'

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

export const UI_FONT_DEFAULT = 'lxgw'
export const EDITOR_FONT_DEFAULT = 'lxgw'

function stackFor(key: string): string {
  const builtin = FONT_OPTIONS.find((o) => o.key === key)
  return builtin ? builtin.stack : `'${key}', ${SANS_STACK}`
}

/** 读取存储的字体 key：新 key 缺失时沿用旧的全局设置（shiro.font）值，最后回退默认 */
function readFontKey(storageKey: string, fallback: string): string {
  return localStorage.getItem(storageKey) ?? localStorage.getItem(LEGACY_FONT_KEY) ?? fallback
}

/** 界面字体（默认霞鹜文楷，与正文一致） */
export function currentUiFontKey(): string {
  return readFontKey(UI_FONT_KEY, UI_FONT_DEFAULT)
}

/** 正文字体（默认霞鹜文楷） */
export function currentEditorFontKey(): string {
  return readFontKey(EDITOR_FONT_KEY, EDITOR_FONT_DEFAULT)
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
export const EDITOR_FONT_SIZE_DEFAULT = 17

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

/** 正文字号（旧全局值是缩放语义不沿用，回退默认 17） */
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

// ---- 编辑区排版：行间距（行高倍数）与段间距（段落首行的额外距离 px），实时生效 ----

export const LINE_HEIGHT_MIN = 1.2
export const LINE_HEIGHT_MAX = 3
export const LINE_HEIGHT_DEFAULT = 1.8
export const PARA_GAP_MIN = 0
export const PARA_GAP_MAX = 64
export const PARA_GAP_DEFAULT = 15

const LINE_HEIGHT_KEY = 'shiro.editor.lineHeight'
const PARA_GAP_KEY = 'shiro.editor.paraSpacing'

function readInRange(key: string, fallback: number, min: number, max: number): number {
  const n = Number(localStorage.getItem(key))
  return n >= min && n <= max ? n : fallback
}

function clampNum(v: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, v))
}

/** 编辑区行间距（line-height 倍数，默认 1.8） */
export function currentEditorLineHeight(): number {
  return readInRange(LINE_HEIGHT_KEY, LINE_HEIGHT_DEFAULT, LINE_HEIGHT_MIN, LINE_HEIGHT_MAX)
}

export function applyEditorLineHeight(v: number): void {
  const n = clampNum(v, LINE_HEIGHT_MIN, LINE_HEIGHT_MAX)
  document.documentElement.style.setProperty('--editor-line-height', String(n))
  localStorage.setItem(LINE_HEIGHT_KEY, String(n))
}

/** 编辑区段间距（px，段落首行的额外上间距，默认 15） */
export function currentEditorParaGap(): number {
  return readInRange(PARA_GAP_KEY, PARA_GAP_DEFAULT, PARA_GAP_MIN, PARA_GAP_MAX)
}

export function applyEditorParaGap(v: number): void {
  const n = clampNum(v, PARA_GAP_MIN, PARA_GAP_MAX)
  document.documentElement.style.setProperty('--editor-para-gap', `${n}px`)
  localStorage.setItem(PARA_GAP_KEY, String(n))
}

// ---- 分段方式：决定哪些行算「新段落」（段落间距的生效范围） ----

export type ParaMode = 'enter' | 'blank'
export const PARA_MODE_DEFAULT: ParaMode = 'enter'
const PARA_MODE_KEY = 'shiro.editor.paraMode'
const PARA_MODES: ParaMode[] = ['enter', 'blank']

/** 回车分段：每次回车都算新段落（Ulysses 式）；空行分段：空行隔开才算段落（Markdown 式） */
export function currentEditorParaMode(): ParaMode {
  const v = localStorage.getItem(PARA_MODE_KEY) as ParaMode | null
  return v && PARA_MODES.includes(v) ? v : PARA_MODE_DEFAULT
}

/** 响应式副本：编辑器装饰插件与设置面板共享，切换后编辑器即时重算 */
export const editorParaMode = ref<ParaMode>(currentEditorParaMode())

export function applyEditorParaMode(mode: ParaMode): void {
  localStorage.setItem(PARA_MODE_KEY, mode)
  editorParaMode.value = mode
}

// ---- 正文宽度（内容列 max-width px） ----

export const CONTENT_WIDTH_MIN = 560
export const CONTENT_WIDTH_MAX = 1400
export const CONTENT_WIDTH_DEFAULT = 760

const CONTENT_WIDTH_KEY = 'shiro.editor.contentWidth'

/** 正文内容列宽度（px，默认 760） */
export function currentContentWidth(): number {
  return readInRange(CONTENT_WIDTH_KEY, CONTENT_WIDTH_DEFAULT, CONTENT_WIDTH_MIN, CONTENT_WIDTH_MAX)
}

export function applyContentWidth(v: number): void {
  const n = clampNum(v, CONTENT_WIDTH_MIN, CONTENT_WIDTH_MAX)
  document.documentElement.style.setProperty('--editor-content-width', `${n}px`)
  localStorage.setItem(CONTENT_WIDTH_KEY, String(n))
}

// ---- 预览排版：行间距与段间距（独立于编辑器设置，作用于手机预览阅读视图） ----

export const PREVIEW_LINE_HEIGHT_KEY = 'shiro.preview.lineHeight'
export const PREVIEW_PARA_GAP_KEY = 'shiro.preview.paraSpacing'

/** 预览行间距（line-height 倍数，默认 1.8） */
export function currentPreviewLineHeight(): number {
  return readInRange(PREVIEW_LINE_HEIGHT_KEY, LINE_HEIGHT_DEFAULT, LINE_HEIGHT_MIN, LINE_HEIGHT_MAX)
}

export function applyPreviewLineHeight(v: number): void {
  const n = clampNum(v, LINE_HEIGHT_MIN, LINE_HEIGHT_MAX)
  document.documentElement.style.setProperty('--preview-line-height', String(n))
  localStorage.setItem(PREVIEW_LINE_HEIGHT_KEY, String(n))
}

/** 预览段间距（px，默认 15） */
export function currentPreviewParaGap(): number {
  return readInRange(PREVIEW_PARA_GAP_KEY, PARA_GAP_DEFAULT, PARA_GAP_MIN, PARA_GAP_MAX)
}

export function applyPreviewParaGap(v: number): void {
  const n = clampNum(v, PARA_GAP_MIN, PARA_GAP_MAX)
  document.documentElement.style.setProperty('--preview-para-gap', `${n}px`)
  localStorage.setItem(PREVIEW_PARA_GAP_KEY, String(n))
}

// ---- 大纲显示模式：自动（宽度足够才显示）/ 开启 / 关闭 ----

export type OutlineMode = 'auto' | 'on' | 'off'
export const OUTLINE_MODE_DEFAULT: OutlineMode = 'auto'
const OUTLINE_MODE_KEY = 'shiro.editor.outline'
const OUTLINE_MODES: OutlineMode[] = ['auto', 'on', 'off']

/** 响应式副本：编辑器与顶栏按钮共享，切换即时生效 */
export const outlineMode = ref<OutlineMode>(currentOutlineMode())

export function currentOutlineMode(): OutlineMode {
  const v = localStorage.getItem(OUTLINE_MODE_KEY) as OutlineMode | null
  return v && OUTLINE_MODES.includes(v) ? v : OUTLINE_MODE_DEFAULT
}

export function applyOutlineMode(mode: OutlineMode): void {
  localStorage.setItem(OUTLINE_MODE_KEY, mode)
  outlineMode.value = mode
}

// ---- 恢复默认 ----

/** 外观全部恢复默认：界面/正文字体与字号、分段方式、段内行距、段落间距、正文宽度、预览排版、大纲模式 */
export function resetAppearance(): void {
  applyUiFont(UI_FONT_DEFAULT)
  applyEditorFont(EDITOR_FONT_DEFAULT)
  applyUiFontSize(UI_FONT_SIZE_DEFAULT)
  applyEditorFontSize(EDITOR_FONT_SIZE_DEFAULT)
  applyEditorLineHeight(LINE_HEIGHT_DEFAULT)
  applyEditorParaGap(PARA_GAP_DEFAULT)
  applyEditorParaMode(PARA_MODE_DEFAULT)
  applyContentWidth(CONTENT_WIDTH_DEFAULT)
  applyPreviewLineHeight(LINE_HEIGHT_DEFAULT)
  applyPreviewParaGap(PARA_GAP_DEFAULT)
  applyOutlineMode(OUTLINE_MODE_DEFAULT)
}
