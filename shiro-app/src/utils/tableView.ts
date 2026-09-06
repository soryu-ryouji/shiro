// 表格编辑（Obsidian 式就地编辑）：
// - 表格在编辑区始终渲染为 <table> 挂件（块级装饰见本文件 tableField；livePreview 负责跳过表内行），没有「源码态」
// - 点击单元格就地编辑（contenteditable），输入即时同步回 markdown 文本；IME 组合期间不同步，上屏后一次性写入
// - 键盘：Tab / Shift+Tab / Enter / 方向键跨格（末格 Tab/Enter/下方向键追加行），Escape 退出，Cmd/Ctrl+Z 走文档撤销
// - 结构操作：右键菜单与行/列手柄（点击弹菜单）增删移行列、设置对齐；表格下缘/右缘「+」加行加列
// - 单元格内 | 以 \| 转义写回、解析时解码；粘贴的多行内容折叠为空格（单元格不含换行）
// - 光标离开表格时按列显示宽度（CJK/全角按 2）自动对齐管道（只在离开时触发，IME 安全）
// 内容始终是纯 markdown 文本，挂件只是视图。
//
// 分层：解析（纯函数）→ 网格模型（结构变换，纯函数）→ 控制器（ViewPlugin，持有全部编辑状态）→ 挂件（纯渲染）。
// 关键约束：
// - 块级装饰（table 挂件）不允许由 ViewPlugin 以函数形式提供（CM 会抛 "Block decorations may not be specified
//   via plugins"），必须由 StateField 经 EditorView.decorations 提供静态 DecorationSet（见 tableField）。
// - StateField 拿不到 view，挂件经控制器注册表（currentController）找控制器；应用只有一个编辑器视图。
// - 挂件 DOM 复用时其事件监听器仍属于旧挂件实例（block 位置可能已过期），所有操作必须先经控制器的
//   resolve() 按原文/选区重新定位块再使用。扫描结果按文档 Text 实例缓存（WeakMap），同一份文档多处消费只扫一遍。
// 有意从简：不支持引用/列表内嵌表格；行列移动走菜单不支持拖拽；手柄只跟随活动单元格（不悬停跟踪）。
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from '@codemirror/view'
import { RangeSetBuilder, StateField, type EditorState, type Extension, type Line, type SelectionRange, type Text, type Transaction } from '@codemirror/state'
import { redo, undo } from '@codemirror/commands'

// ---- 共享正则（livePreview.ts 从这里转发导出，避免循环依赖）----

export const FENCE_RE = /^ {0,3}`{3,}/
export const HEADING_RE = /^(#{1,6})(?: +|$)/
const LIST_MARK_RE = /^ {0,3}[-*+] /
const QUOTE_MARK_RE = /^ {0,3}>/

// ---- 单元格转义：| ↔ \| ----

/** 显示文本 → 文档原文（管道转义） */
export function escapeCellText(text: string): string {
  return text.replace(/\|/g, '\\|')
}

/** 文档原文 → 显示文本（去转义） */
function unescapeCellText(raw: string): string {
  return raw.replace(/\\\|/g, '|')
}

/** 显示文本偏移 → 文档原文偏移（\| 占原文 2 字符、显示 1 字符；内部使用，导出供测试） */
export function rawOffsetOfDecoded(raw: string, decoded: number): number {
  let d = 0
  let i = 0
  while (i < raw.length && d < decoded) {
    if (raw.charAt(i) === '\\' && raw.charAt(i + 1) === '|') {
      i += 2
      d += 1
    } else {
      i++
      d++
    }
  }
  return i
}

// ---- 数据结构 ----

export interface TableCell {
  /** 显示文本（已解码转义）；文档原文区间 [from, to) 内含转义符 */
  text: string
  from: number
  to: number
}

export interface TableRow {
  line: number
  from: number
  to: number
  cells: TableCell[]
}

export type TableAlign = 'none' | 'left' | 'center' | 'right'

export interface TableBlock {
  from: number
  to: number
  startLine: number
  endLine: number
  header: TableRow
  aligns: TableAlign[]
  rows: TableRow[]
  /** 各列显示宽度（已含对齐冒号与最短 3 连字符的要求） */
  widths: number[]
  /** 块原文（挂件 eq 与块重新定位用） */
  source: string
}

/** 活动单元格（正在编辑的格子）：r 含表头（0 = 表头行）；offset/all 是光标意图（点击落点 / 全选），按显示文本计 */
export interface ActiveCellPos {
  r: number
  c: number
  offset: number
  all: boolean
}

// ---- 解析 ----

const isDelimiterCell = (text: string): boolean => /^:?-+:?$/.test(text)

/** 行 → 单元格（支持行首最多 3 空格缩进；必须有 |；\| 不作分隔、解码进单元格文本；单元格保留裁剪后的文档位置） */
function parseRowLine(line: Line): TableRow | null {
  const text = line.text
  if (!text.includes('|')) return null
  let s = 0
  while (s < text.length && text.charAt(s) === ' ') s++
  if (s > 3) return null
  let e = text.length
  // 尾管道：转义的 \| 不算收尾分隔
  if (text.charAt(e - 1) === '|' && text.charAt(e - 2) !== '\\') e--
  if (text.charAt(s) === '|') s++
  if (e <= s) return null
  const bounds = [s]
  for (let i = s; i < e; i++) {
    if (text.charAt(i) === '\\' && text.charAt(i + 1) === '|') {
      i++
      continue
    }
    if (text.charAt(i) === '|') bounds.push(i)
  }
  bounds.push(e)
  const cells: TableCell[] = []
  for (let i = 0; i + 1 < bounds.length; i++) {
    let cs = bounds[i] + 1
    let ce = bounds[i + 1]
    while (cs < ce && text.charAt(cs) === ' ') cs++
    while (ce > cs && text.charAt(ce - 1) === ' ') ce--
    cells.push({ text: unescapeCellText(text.slice(cs, ce)), from: line.from + cs, to: line.from + ce })
  }
  return { line: line.number, from: line.from + s, to: line.from + e, cells }
}

function isDelimiterLine(line: Line): boolean {
  const row = parseRowLine(line)
  if (!row || row.cells.length === 0) return false
  return row.cells.every((c) => isDelimiterCell(c.text))
}

function alignOf(delimCell: string): TableAlign {
  const lead = delimCell.startsWith(':')
  const trail = delimCell.endsWith(':')
  if (lead && trail) return 'center'
  if (lead) return 'left'
  if (trail) return 'right'
  return 'none'
}

function makeBlock(headLine: Line, delimLine: Line, lastLine: Line, header: TableRow, rows: TableRow[], doc: Text): TableBlock {
  const delim = parseRowLine(delimLine)!
  const aligns: TableAlign[] = delim.cells.map((c) => alignOf(c.text))
  const colCount = Math.max(header.cells.length, delim.cells.length, ...rows.map((r) => r.cells.length))
  const widths: number[] = []
  for (let c = 0; c < colCount; c++) {
    let w = 0
    for (const row of [header, ...rows]) {
      const cell = row.cells[c]
      if (cell) w = Math.max(w, displayWidth(cell.text))
    }
    const colons = (aligns[c] ?? 'none') === 'center' ? 2 : (aligns[c] ?? 'none') === 'none' ? 0 : 1
    widths.push(Math.max(w, 3 + colons))
  }
  return {
    from: headLine.from,
    to: lastLine.to,
    startLine: headLine.number,
    endLine: lastLine.number,
    header,
    aligns,
    rows,
    widths,
    source: doc.sliceString(headLine.from, lastLine.to),
  }
}

/** 扫描缓存：Text 是不可变持久结构，同一文档实例多次消费（livePreview / 段间距 / 控制器）只扫一遍 */
const scanCache = new WeakMap<Text, TableBlock[]>()

/** 全文扫描表格块（从文档头跟踪围栏状态；超大文档不处理） */
export function scanTables(doc: Text): TableBlock[] {
  const hit = scanCache.get(doc)
  if (hit) return hit
  const blocks = scanTablesUncached(doc)
  scanCache.set(doc, blocks)
  return blocks
}

function scanTablesUncached(doc: Text): TableBlock[] {
  if (doc.lines > 50000) return []
  const blocks: TableBlock[] = []
  let inFence = false
  for (let n = 1; n <= doc.lines; n++) {
    const line = doc.line(n)
    if (FENCE_RE.test(line.text)) {
      inFence = !inFence
      continue
    }
    if (inFence) continue
    if (n >= doc.lines) continue
    const next = doc.line(n + 1)
    if (!isDelimiterLine(next)) continue
    if (HEADING_RE.test(line.text) || LIST_MARK_RE.test(line.text) || QUOTE_MARK_RE.test(line.text)) continue
    const header = parseRowLine(line)
    if (!header) continue
    const rows: TableRow[] = []
    let m = n + 2
    while (m <= doc.lines) {
      const rowLine = doc.line(m)
      if (FENCE_RE.test(rowLine.text)) break
      const row = parseRowLine(rowLine)
      if (!row) break
      rows.push(row)
      m++
    }
    blocks.push(makeBlock(line, next, doc.line(m - 1), header, rows, doc))
    n = m - 1
  }
  return blocks
}

/** 光标所在表格（整行落在块内即算） */
export function findTableAt(state: EditorState): TableBlock | null {
  const headLine = state.doc.lineAt(state.selection.main.head).number
  for (const b of scanTables(state.doc)) {
    if (headLine >= b.startLine && headLine <= b.endLine) return b
  }
  return null
}

/** 与可视区相交的表格块（livePreview 与段间距装饰共用） */
export function visibleTableBlocks(view: EditorView): TableBlock[] {
  const all = scanTables(view.state.doc)
  if (!all.length || !view.visibleRanges.length) return []
  const doc = view.state.doc
  const firstLine = doc.lineAt(view.visibleRanges[0].from).number
  const lastLine = doc.lineAt(view.visibleRanges[view.visibleRanges.length - 1].to).number
  return all.filter((b) => b.endLine >= firstLine && b.startLine <= lastLine)
}

/** 选区 → 活动单元格；落在管道/空隙上时视为未激活（livePreview 构建挂件时用）。
 *  选区恰好覆盖整个单元格（goto 全选）→ all；否则 offset 为显示文本偏移。 */
export function activeFromRange(block: TableBlock, range: SelectionRange, doc: Text): ActiveCellPos | null {
  const head = range.head
  if (head < block.from || head > block.to) return null
  const rows = [block.header, ...block.rows]
  for (let r = 0; r < rows.length; r++) {
    const cells = rows[r].cells
    for (let c = 0; c < cells.length; c++) {
      const cell = cells[c]
      if (head < cell.from || head > cell.to) continue
      if (!range.empty && range.from <= cell.from && range.to >= cell.to) return { r, c, offset: 0, all: true }
      return {
        r,
        c,
        offset: unescapeCellText(doc.sliceString(cell.from, head)).length,
        all: cell.from === cell.to,
      }
    }
  }
  return null
}

// ---- 显示宽度与网格渲染 ----

// 全角/CJK 字符按 2 列宽计（含韩文、注音、兼容表意文字等宽形区段）
const WIDE_RE =
  /[\u1100-\u115F\u2E80-\u303E\u3041-\u33FF\u3400-\u4DBF\u4E00-\u9FFF\uA000-\uA4CF\uAC00-\uD7A3\uF900-\uFAFF\uFE30-\uFE4F\uFF00-\uFF60\uFFE0-\uFFE6\u{20000}-\u{3FFFD}]/u

export function displayWidth(s: string): number {
  let w = 0
  for (const ch of s) w += WIDE_RE.test(ch) ? 2 : 1
  return w
}

/** 表格网格模型：结构变换的操作对象（文本均为显示文本，渲染时才转义） */
export interface TableGrid {
  aligns: TableAlign[]
  header: string[]
  rows: string[][]
}

export interface RenderedGrid {
  text: string
  /** [r][c]（r=0 为表头）：单元格内容在 text 中的起点与原文长度（含转义），供变换后直接落选区 */
  cells: { start: number; len: number }[][]
}

/** 块 → 网格（行列数归一到列数，缺格补空串、缺对齐补 none） */
export function gridOf(block: TableBlock): TableGrid {
  const cols = block.widths.length
  const norm = (cells: TableCell[]) => Array.from({ length: cols }, (_, i) => cells[i]?.text ?? '')
  return {
    aligns: Array.from({ length: cols }, (_, i) => block.aligns[i] ?? 'none'),
    header: norm(block.header.cells),
    rows: block.rows.map((r) => norm(r.cells)),
  }
}

/** 网格 → 对齐后的表格源码（按列显示宽度重排管道，CJK/全角按 2） */
export function renderGrid(grid: TableGrid): RenderedGrid {
  const cols = grid.aligns.length
  const widths: number[] = []
  for (let c = 0; c < cols; c++) {
    let w = 0
    for (const text of [grid.header[c] ?? '', ...grid.rows.map((r) => r[c] ?? '')]) w = Math.max(w, displayWidth(text))
    const colons = grid.aligns[c] === 'center' ? 2 : grid.aligns[c] === 'none' ? 0 : 1
    widths.push(Math.max(w, 3 + colons))
  }
  const delimCell = (align: TableAlign, w: number) => {
    const lead = align === 'left' || align === 'center' ? 1 : 0
    const trail = align === 'right' || align === 'center' ? 1 : 0
    return (lead ? ':' : '') + '-'.repeat(w - lead - trail) + (trail ? ':' : '')
  }
  const lines: string[] = []
  const cells: RenderedGrid['cells'] = []
  let offset = 0
  const emitRow = (texts: string[]): void => {
    const segs: string[] = []
    const rowCells: { start: number; len: number }[] = []
    let pos = 2 // "| " 之后
    for (let c = 0; c < cols; c++) {
      const text = texts[c] ?? ''
      const raw = escapeCellText(text)
      const gap = Math.max(0, widths[c] - displayWidth(text))
      const align = grid.aligns[c] ?? 'none'
      const lead = align === 'right' ? gap : align === 'center' ? gap >> 1 : 0
      rowCells.push({ start: offset + pos + lead, len: raw.length })
      segs.push(' '.repeat(lead) + raw + ' '.repeat(gap - lead))
      pos += raw.length + gap + 3 // 原文长度 + 补白 + " | "（转义使原文长于显示宽度，不能用 widths）
    }
    const line = '| ' + segs.join(' | ') + ' |'
    lines.push(line)
    cells.push(rowCells)
    offset += line.length + 1 // \n
  }
  emitRow(grid.header)
  const delim = '| ' + widths.map((w, c) => delimCell(grid.aligns[c] ?? 'none', w)).join(' | ') + ' |'
  lines.push(delim)
  offset += delim.length + 1
  for (const row of grid.rows) emitRow(row)
  return { text: lines.join('\n'), cells }
}

/** 块已失效（结构被外部破坏）返回 null；否则返回对齐后的表格源码 */
export function alignTableText(doc: Text, block: TableBlock): string | null {
  const headLine = doc.line(block.startLine)
  const delimLine = doc.line(block.startLine + 1)
  const header = parseRowLine(headLine)
  if (!header || !isDelimiterLine(delimLine)) return null
  const rows: TableRow[] = []
  for (let n = block.startLine + 2; n <= block.endLine; n++) {
    const row = parseRowLine(doc.line(n))
    if (!row) return null
    rows.push(row)
  }
  return renderGrid(gridOf(makeBlock(headLine, delimLine, doc.line(block.endLine), header, rows, doc))).text
}

// ---- 结构变换（纯函数，返回新网格） ----

const cloneGrid = (g: TableGrid): TableGrid => ({
  aligns: [...g.aligns],
  header: [...g.header],
  rows: g.rows.map((r) => [...r]),
})

/** 在数据行索引 i 前插入空行（i = rows.length 即追加） */
export function gridInsertRow(grid: TableGrid, i: number): TableGrid {
  const g = cloneGrid(grid)
  g.rows.splice(Math.max(0, Math.min(i, g.rows.length)), 0, g.aligns.map(() => ''))
  return g
}

export function gridDeleteRow(grid: TableGrid, i: number): TableGrid {
  const g = cloneGrid(grid)
  if (i >= 0 && i < g.rows.length) g.rows.splice(i, 1)
  return g
}

export function gridMoveRow(grid: TableGrid, i: number, d: 1 | -1): TableGrid {
  const g = cloneGrid(grid)
  const j = i + d
  if (i < 0 || j < 0 || i >= g.rows.length || j >= g.rows.length) return g
  ;[g.rows[i], g.rows[j]] = [g.rows[j], g.rows[i]]
  return g
}

/** 在列索引 c 前插入空列（c = 列数即追加到右侧） */
export function gridInsertCol(grid: TableGrid, c: number): TableGrid {
  const g = cloneGrid(grid)
  const at = Math.max(0, Math.min(c, g.aligns.length))
  g.aligns.splice(at, 0, 'none')
  g.header.splice(at, 0, '')
  for (const row of g.rows) row.splice(at, 0, '')
  return g
}

export function gridDeleteCol(grid: TableGrid, c: number): TableGrid {
  const g = cloneGrid(grid)
  if (g.aligns.length <= 1 || c < 0 || c >= g.aligns.length) return g
  g.aligns.splice(c, 1)
  g.header.splice(c, 1)
  for (const row of g.rows) row.splice(c, 1)
  return g
}

export function gridMoveCol(grid: TableGrid, c: number, d: 1 | -1): TableGrid {
  const g = cloneGrid(grid)
  const j = c + d
  if (c < 0 || j < 0 || c >= g.aligns.length || j >= g.aligns.length) return g
  ;[g.aligns[c], g.aligns[j]] = [g.aligns[j], g.aligns[c]]
  ;[g.header[c], g.header[j]] = [g.header[j], g.header[c]]
  for (const row of g.rows) [row[c], row[j]] = [row[j], row[c]]
  return g
}

export function gridSetAlign(grid: TableGrid, c: number, align: TableAlign): TableGrid {
  const g = cloneGrid(grid)
  if (c >= 0 && c < g.aligns.length) g.aligns[c] = align
  return g
}

// ---- 右键菜单（经 CustomEvent 桥接到 Editor.vue 的 ContextMenu 组件） ----

export const TABLE_MENU_EVENT = 'shiro-table-menu'

/** 与 ContextMenu.vue 的 MenuItem 结构一致（这里不能 import .vue，否则 node 冒烟测试打包会失败） */
export interface TableMenuItem {
  label?: string
  divider?: boolean
  danger?: boolean
  checked?: boolean
  action?: () => void
}

// ---- 控制器（ViewPlugin，持有全部编辑状态；挂件 DOM 事件一律委托到这里） ----

/** 当前编辑器视图的控制器（应用只有一个编辑器；StateField 构建的挂件拿不到 view，经这里中转） */
let currentController: TableController | null = null

class TableController {
  /** 输入法组合中：组合期间的 input 事件忽略，上屏后 compositionend 一次性同步 */
  composing = false
  /** 光标最近所在的表格块（光标离开后找回做自动对齐；位置可能过期，用前必须 resolve） */
  private lastBlock: TableBlock | null = null

  constructor(readonly view: EditorView) {
    currentController = this
  }

  destroy(): void {
    if (currentController === this) currentController = null
  }

  /** 光标离开表格时自动对齐管道（只在离开时触发，IME 安全） */
  update(update: ViewUpdate): void {
    if (!update.docChanged && !update.selectionSet) return
    const cur = findTableAt(update.state)
    const prev = this.lastBlock
    this.lastBlock = cur
    // 未离开（同一块，输入中 source 会变但 startLine 不变）不处理；无焦点（初始加载等）不处理
    if (!prev || (cur && cur.startLine === prev.startLine)) return
    if (!update.view.hasFocus) return
    const block = this.resolve(prev)
    if (!block) return
    const doc = update.state.doc
    const text = alignTableText(doc, block)
    if (text === null) return
    const from = doc.line(block.startLine).from
    const to = doc.line(block.endLine).to
    if (text === doc.sliceString(from, to)) return
    const changes = update.state.changes({ from, to, insert: text })
    update.view.dispatch({ changes, selection: update.state.selection.map(changes, 1) })
  }

  /** 按原文重新定位块：位置随上方编辑漂移，source 匹配（取行号最接近的）；
   *  活动格输入改变了原文时按选区兜底（此时选区必在本表内） */
  private resolve(stale: TableBlock): TableBlock | null {
    const all = scanTables(this.view.state.doc)
    let best: TableBlock | null = null
    for (const b of all) {
      if (b.source !== stale.source) continue
      if (b.startLine === stale.startLine) return b
      if (!best || Math.abs(b.startLine - stale.startLine) < Math.abs(best.startLine - stale.startLine)) best = b
    }
    return best ?? findTableAt(this.view.state)
  }

  // ---- 挂件 DOM 事件（事件委托；stale 是构建 DOM 的挂件携带的块，一律 resolve 后再用） ----

  onMouseDown(e: MouseEvent, stale: TableBlock): void {
    if (e.button !== 0) return // 右键交给 contextmenu
    const target = e.target as HTMLElement
    if (target.closest('button')) return // 手柄/加号按钮各自处理
    const el = target.closest('td,th') as HTMLElement | null
    if (!el) {
      // 点在表格边框/留白：仅编辑中时退出（光标落到表格后）
      if ((e.currentTarget as HTMLElement).classList.contains('md-table-active')) {
        e.preventDefault()
        this.exit(stale)
      }
      return
    }
    if (el.classList.contains('md-cell-active')) return // 活动格内：原生光标行为
    e.preventDefault()
    this.goto(stale, Number(el.dataset.r), Number(el.dataset.c), false, caretOffsetFromPoint(el, e))
  }

  onKeyDown(e: KeyboardEvent, stale: TableBlock): void {
    // 输入法组合期间（含确认候选的 Enter）不接管按键
    if (e.isComposing || this.composing) return
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault()
      if (e.shiftKey ? redo(this.view) : undo(this.view)) this.syncDomFromDoc(stale)
      return
    }
    const el = (e.target as HTMLElement).closest('td,th') as HTMLElement | null
    if (!el) return
    const block = this.resolve(stale)
    if (!block) return
    const r = Number(el.dataset.r)
    const c = Number(el.dataset.c)
    switch (e.key) {
      case 'Tab':
        e.preventDefault()
        this.step(block, r, c, e.shiftKey ? -1 : 1)
        break
      case 'Enter':
        e.preventDefault()
        if (r < block.rows.length) this.goto(block, r + 1, c, true)
        else this.appendRow(block, c)
        break
      case 'Escape':
        e.preventDefault()
        this.exit(block)
        break
      case 'ArrowUp':
        e.preventDefault()
        if (r > 0) this.goto(block, r - 1, c, true)
        break
      case 'ArrowDown':
        e.preventDefault()
        if (r < block.rows.length) this.goto(block, r + 1, c, true)
        else this.appendRow(block, c)
        break
      case 'ArrowLeft':
        if (caretAtEdge(true)) {
          e.preventDefault()
          this.step(block, r, c, -1)
        }
        break
      case 'ArrowRight':
        if (caretAtEdge(false)) {
          e.preventDefault()
          this.step(block, r, c, 1)
        }
        break
    }
  }

  onInput(e: Event, stale: TableBlock): void {
    if (this.composing || (e as InputEvent).isComposing) return
    const el = (e.target as HTMLElement).closest('td,th') as HTMLElement | null
    if (el) this.syncCell(el, stale)
  }

  onCompositionEnd(e: Event, stale: TableBlock): void {
    this.composing = false
    const el = (e.target as HTMLElement).closest('td,th') as HTMLElement | null
    if (el) this.syncCell(el, stale)
  }

  /** 单元格不含换行：粘贴时折叠为空格（insertText 会触发 input，随即同步回文档） */
  onPaste(e: ClipboardEvent): void {
    if (!(e.target as HTMLElement).closest('td,th')) return
    e.preventDefault()
    const text = (e.clipboardData?.getData('text/plain') ?? '').replace(/\n+/g, ' ')
    document.execCommand('insertText', false, text)
  }

  onContextMenu(e: MouseEvent, stale: TableBlock): void {
    e.preventDefault()
    if ((e.target as HTMLElement).closest('button')) return
    const el = (e.target as HTMLElement).closest('td,th') as HTMLElement | null
    if (!el) return
    const r = Number(el.dataset.r)
    const c = Number(el.dataset.c)
    if (!el.classList.contains('md-cell-active')) this.goto(stale, r, c, false, caretOffsetFromPoint(el, e))
    this.openMenu(e.clientX, e.clientY, this.buildMenu(stale, r, c, 'cell'))
  }

  /** 手柄点击：弹出行/列操作菜单 */
  openGripMenu(e: MouseEvent, stale: TableBlock, kind: 'row' | 'col', r: number, c: number): void {
    this.openMenu(e.clientX, e.clientY, this.buildMenu(stale, r, c, kind))
  }

  /** 表格下缘「+」：末尾追加行，焦点落新行（沿用活动格列） */
  addRowEnd(stale: TableBlock, active: ActiveCellPos | null): void {
    const block = this.resolve(stale)
    if (!block) return
    this.applyGrid(block, (g) => gridInsertRow(g, g.rows.length), { r: block.rows.length + 1, c: active?.c ?? 0 })
  }

  /** 表格右缘「+」：末尾追加列，焦点落新列（沿用活动格行） */
  addColEnd(stale: TableBlock, active: ActiveCellPos | null): void {
    const block = this.resolve(stale)
    if (!block) return
    const cols = block.widths.length
    this.applyGrid(block, (g) => gridInsertCol(g, cols), { r: active?.r ?? 0, c: cols })
  }

  // ---- 内部 ----

  /** 把活动格的 DOM 文本写回文档（每次输入调用；转义管道；输入法组合中不写） */
  private syncCell(el: HTMLElement, stale: TableBlock): void {
    const block = this.resolve(stale)
    if (!block) return
    const cell = cellAt(block, Number(el.dataset.r), Number(el.dataset.c))
    if (!cell) return
    // contenteditable 会产出 nbsp（行尾空格等场景），统一回普通空格
    const text = (el.textContent ?? '').replace(/\u00a0/g, ' ').replace(/\n/g, ' ')
    if (text === cell.text) return
    this.view.dispatch({ changes: { from: cell.from, to: cell.to, insert: escapeCellText(text) }, scrollIntoView: false })
  }

  /** 文档回滚（撤销/重做）后，把活动格 DOM 刷成文档当前值，光标落到文本末尾 */
  private syncDomFromDoc(stale: TableBlock): void {
    const el = this.view.dom.querySelector<HTMLElement>('.md-cell-active')
    if (!el) return
    const block = this.resolve(stale)
    const cell = block && cellAt(block, Number(el.dataset.r), Number(el.dataset.c))
    if (!cell || (el.textContent ?? '') === cell.text) return
    el.textContent = cell.text
    const sel = window.getSelection()
    if (!sel) return
    const range = document.createRange()
    range.selectNodeContents(el)
    range.collapse(false)
    sel.removeAllRanges()
    sel.addRange(range)
  }

  /** 结构变换：解析当前块 → 网格变换 → 整体重渲染替换，选区直接落到目标格（重建后活动格可复算） */
  private applyGrid(stale: TableBlock, op: (grid: TableGrid) => TableGrid, focus: { r: number; c: number }): void {
    const block = this.resolve(stale)
    if (!block) return
    const rendered = renderGrid(op(gridOf(block)))
    const fc = rendered.cells[focus.r]?.[focus.c]
    const anchor = fc ? block.from + fc.start : block.to
    this.view.dispatch({
      changes: { from: block.from, to: block.to, insert: rendered.text },
      selection: { anchor, head: fc ? anchor + fc.len : anchor },
      scrollIntoView: false,
    })
    this.focusCell(focus.r, focus.c, true)
  }

  /** 激活目标单元格（文档为事实来源，选区藏进块级 replace 装饰内） */
  private goto(stale: TableBlock, r: number, c: number, all: boolean, offset = -1): void {
    const block = this.resolve(stale)
    if (!block) return
    const cell = cellAt(block, r, c)
    if (!cell) return
    let anchor = cell.from
    if (!all && offset >= 0) {
      const raw = this.view.state.doc.sliceString(cell.from, cell.to)
      anchor = cell.from + rawOffsetOfDecoded(raw, Math.min(offset, unescapeCellText(raw).length))
    }
    this.view.dispatch({ selection: { anchor, head: all ? cell.to : anchor }, scrollIntoView: false })
    this.focusCell(r, c, all, offset)
  }

  /** 聚焦指定活动格并落光标（dispatch 后同步调用，此时 DOM 已重建；不等挂件微任务，避免与 CM 选区同步竞争） */
  focusCell(r: number, c: number, all: boolean, offset = -1): void {
    const el = this.view.dom.querySelector<HTMLElement>('.md-cell-active')
    if (!el || Number(el.dataset.r) !== r || Number(el.dataset.c) !== c) return
    placeCaretInCell(el, { all, offset })
  }

  /** 行主序相邻移动（Tab/Shift+Tab）；末格前进时追加新行 */
  private step(block: TableBlock, r: number, c: number, d: 1 | -1): void {
    const cols = block.widths.length
    const i = r * cols + c + d
    if (i < 0) return
    if (i >= (block.rows.length + 1) * cols) {
      this.appendRow(block, 0)
      return
    }
    this.goto(block, Math.floor(i / cols), i % cols, true)
  }

  private appendRow(block: TableBlock, c: number): void {
    this.applyGrid(block, (g) => gridInsertRow(g, g.rows.length), { r: block.rows.length + 1, c })
  }

  private exit(stale: TableBlock): void {
    const block = this.resolve(stale)
    if (!block) return
    this.view.dispatch({ selection: { anchor: block.to }, scrollIntoView: true })
    this.view.focus()
  }

  /** 行/列操作菜单（r=0 为表头：不可删、不可移，只能在下方插入数据行） */
  private buildMenu(stale: TableBlock, r: number, c: number, scope: 'cell' | 'row' | 'col'): TableMenuItem[] {
    const block = this.resolve(stale)
    if (!block) return []
    const cols = block.widths.length
    const rows = block.rows.length
    const items: TableMenuItem[] = []
    const run = (op: (g: TableGrid) => TableGrid, focus: { r: number; c: number }) => () =>
      this.applyGrid(stale, op, focus)

    if (scope !== 'col') {
      if (r === 0) {
        items.push({ label: '在下方插入行', action: run((g) => gridInsertRow(g, 0), { r: 1, c }) })
      } else {
        items.push(
          { label: '在上方插入行', action: run((g) => gridInsertRow(g, r - 1), { r, c }) },
          { label: '在下方插入行', action: run((g) => gridInsertRow(g, r), { r: r + 1, c }) },
        )
        if (r > 1) items.push({ label: '上移行', action: run((g) => gridMoveRow(g, r - 1, -1), { r: r - 1, c }) })
        if (r < rows) items.push({ label: '下移行', action: run((g) => gridMoveRow(g, r - 1, 1), { r: r + 1, c }) })
        items.push({ label: '删除行', danger: true, action: run((g) => gridDeleteRow(g, r - 1), { r: Math.min(r, rows - 1), c }) })
      }
    }
    if (scope !== 'row') {
      if (items.length) items.push({ divider: true })
      items.push(
        { label: '在左侧插入列', action: run((g) => gridInsertCol(g, c), { r, c }) },
        { label: '在右侧插入列', action: run((g) => gridInsertCol(g, c + 1), { r, c: c + 1 }) },
      )
      if (c > 0) items.push({ label: '左移列', action: run((g) => gridMoveCol(g, c, -1), { r, c: c - 1 }) })
      if (c < cols - 1) items.push({ label: '右移列', action: run((g) => gridMoveCol(g, c, 1), { r, c: c + 1 }) })
      if (cols > 1) items.push({ label: '删除列', danger: true, action: run((g) => gridDeleteCol(g, c), { r, c: Math.min(c, cols - 2) }) })
      items.push({ divider: true })
      const align = block.aligns[c] ?? 'none'
      const aligns: [TableAlign, string][] = [
        ['left', '左对齐'],
        ['center', '居中'],
        ['right', '右对齐'],
      ]
      for (const [a, label] of aligns) {
        items.push({
          label,
          checked: a === 'left' ? align === 'left' || align === 'none' : align === a,
          action: run((g) => gridSetAlign(g, c, a), { r, c }),
        })
      }
    }
    return items
  }

  private openMenu(x: number, y: number, items: TableMenuItem[]): void {
    if (!items.length) return
    this.view.dom.dispatchEvent(new CustomEvent(TABLE_MENU_EVENT, { bubbles: true, detail: { x, y, items } }))
  }
}

export const tableController = ViewPlugin.fromClass(TableController)

// ---- 表格块级装饰（StateField：块级装饰不允许由 ViewPlugin 以函数形式提供，必须经 EditorView.decorations 提供静态集合） ----
// 对全文所有表格生效（不按视口裁剪，扫描有缓存）；doc/选区变化时重建，挂件 eq 负责复用 DOM。

function buildTableDecorations(state: EditorState): DecorationSet {
  const blocks = scanTables(state.doc)
  if (!blocks.length) return Decoration.none
  const builder = new RangeSetBuilder<Decoration>()
  for (const b of blocks) {
    const active = activeFromRange(b, state.selection.main, state.doc)
    builder.add(b.from, b.to, Decoration.replace({ widget: new TableViewWidget(b, active), block: true }))
  }
  return builder.finish()
}

const tableField = StateField.define<DecorationSet>({
  create: (state) => buildTableDecorations(state),
  // Transaction 没有 selectionSet（只有 ViewUpdate 有），用 tr.selection 判断
  update: (deco, tr: Transaction) => (tr.docChanged || tr.selection || tr.reconfigured ? buildTableDecorations(tr.state) : deco),
  provide: (f) => EditorView.decorations.from(f),
})

// ---- 共享 DOM 工具 ----

function cellAt(block: TableBlock, r: number, c: number): TableCell | null {
  return [block.header, ...block.rows][r]?.cells[c] ?? null
}

function caretAtEdge(atStart: boolean): boolean {
  const sel = window.getSelection()
  if (!sel || !sel.isCollapsed) return false
  return atStart ? sel.anchorOffset === 0 : sel.anchorOffset === (sel.anchorNode?.textContent?.length ?? 0)
}

/** 点击位置 → 单元格内显示文本偏移（用于把光标放到位；拿不到回退 -1 = 格首） */
function caretOffsetFromPoint(el: HTMLElement, e: MouseEvent): number {
  const pos = document.caretRangeFromPoint?.(e.clientX, e.clientY)
  if (!pos || pos.startContainer.nodeType !== Node.TEXT_NODE) return -1
  const walk = document.createTreeWalker(el, NodeFilter.SHOW_TEXT)
  let off = 0
  let node = walk.nextNode()
  while (node) {
    if (node === pos.startContainer) return off + pos.startOffset
    off += node.textContent?.length ?? 0
    node = walk.nextNode()
  }
  return -1
}

/** 把焦点与光标放进单元格（all 全选内容，否则按显示文本偏移落点） */
function placeCaretInCell(cell: HTMLElement, active: { all: boolean; offset: number }): void {
  cell.focus()
  const sel = window.getSelection()
  if (sel) {
    const range = document.createRange()
    let placed = false
    if (!active.all) {
      const walk = document.createTreeWalker(cell, NodeFilter.SHOW_TEXT)
      let off = 0
      let node = walk.nextNode()
      while (node) {
        const len = node.textContent?.length ?? 0
        if (active.offset <= off + len) {
          range.setStart(node, Math.max(0, active.offset - off))
          range.collapse(true)
          placed = true
          break
        }
        off += len
        node = walk.nextNode()
      }
    }
    if (!placed) range.selectNodeContents(cell)
    sel.removeAllRanges()
    sel.addRange(range)
  }
  cell.scrollIntoView({ block: 'nearest' })
}

// ---- 渲染挂件（纯渲染，不持有编辑状态；事件全部委托给控制器） ----

const SVG_NS = 'http://www.w3.org/2000/svg'

/** 手柄图标：2×3 圆点阵列（行手柄竖排，列手柄横排） */
function gripIcon(horizontal: boolean): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, 'svg')
  svg.setAttribute('viewBox', '0 0 14 14')
  svg.setAttribute('width', '14')
  svg.setAttribute('height', '14')
  const dots = horizontal
    ? [[2, 3], [7, 3], [12, 3], [2, 11], [7, 11], [12, 11]]
    : [[3, 2], [3, 7], [3, 12], [11, 2], [11, 7], [11, 12]]
  for (const [cx, cy] of dots) {
    const dot = document.createElementNS(SVG_NS, 'circle')
    dot.setAttribute('cx', String(cx))
    dot.setAttribute('cy', String(cy))
    dot.setAttribute('r', '1.5')
    dot.setAttribute('fill', 'currentColor')
    svg.appendChild(dot)
  }
  return svg
}

export class TableViewWidget extends WidgetType {
  constructor(
    readonly block: TableBlock,
    readonly active: ActiveCellPos | null,
  ) {
    super()
  }

  /** 形状、对齐、活动单元格、非活动单元格文本都没变才复用 DOM——
   *  活动格的文本以 DOM 为准（输入先落在 DOM 再同步进文档），参与比较会杀掉光标；
   *  offset/all 不参与比较（光标意图只影响重建后的落点，输入中 CM 选区会漂移） */
  eq(other: TableViewWidget): boolean {
    const a = this.block
    const b = other.block
    if (a.startLine !== b.startLine || a.endLine !== b.endLine) return false
    if (a.widths.length !== b.widths.length) return false
    if (a.aligns.some((x, i) => b.aligns[i] !== x)) return false
    if ((other.active?.r ?? -1) !== (this.active?.r ?? -1) || (other.active?.c ?? -1) !== (this.active?.c ?? -1)) return false
    const ra = [a.header, ...a.rows]
    const rb = [b.header, ...b.rows]
    for (let r = 0; r < ra.length; r++) {
      for (let c = 0; c < a.widths.length; c++) {
        if (this.active && r === this.active.r && c === this.active.c) continue
        if ((ra[r].cells[c]?.text ?? '') !== (rb[r].cells[c]?.text ?? '')) return false
      }
    }
    return true
  }

  ignoreEvent(): boolean {
    return true
  }

  private ctrl(): TableController | null {
    return currentController
  }

  toDOM(): HTMLElement {
    const wrap = document.createElement('div')
    wrap.className = 'md-table-wrap'
    if (this.active) wrap.classList.add('md-table-active')
    const scroll = document.createElement('div')
    scroll.className = 'md-table-scroll'
    const table = document.createElement('table')
    table.className = 'md-table'
    const cols = this.block.widths.length
    const alignAt = (c: number) => {
      const a = this.block.aligns[c]
      return a === 'center' || a === 'right' ? a : 'left'
    }
    let cellEl: HTMLElement | null = null
    const makeCell = (tag: 'th' | 'td', text: string, r: number, c: number): HTMLElement => {
      const el = document.createElement(tag)
      // 空单元格放 <br> 撑出行高（不进 textContent，不影响输入同步）
      if (text) el.textContent = text
      else el.appendChild(document.createElement('br'))
      el.dataset.r = String(r)
      el.dataset.c = String(c)
      el.style.textAlign = alignAt(c)
      if (this.active?.r === r && this.active.c === c) {
        cellEl = el
        el.classList.add('md-cell-active')
        try {
          el.contentEditable = 'plaintext-only'
        } catch {
          el.contentEditable = 'true'
        }
        if (!el.isContentEditable) el.contentEditable = 'true'
      }
      return el
    }
    const head = document.createElement('thead')
    const htr = document.createElement('tr')
    for (let c = 0; c < cols; c++) htr.appendChild(makeCell('th', this.block.header.cells[c]?.text ?? '', 0, c))
    head.appendChild(htr)
    const body = document.createElement('tbody')
    this.block.rows.forEach((row, i) => {
      const tr = document.createElement('tr')
      for (let c = 0; c < cols; c++) tr.appendChild(makeCell('td', row.cells[c]?.text ?? '', i + 1, c))
      body.appendChild(tr)
    })
    table.append(head, body)
    scroll.appendChild(table)
    wrap.appendChild(scroll)

    // 加列按钮：绝对定位于右上角留白（表格吃满整宽，不能再作 flex 邻居，否则会溢出出横向滚动条）
    const addCol = this.makeButton('md-table-addcol', '添加列', () => this.ctrl()?.addColEnd(this.block, this.active))
    wrap.appendChild(addCol)
    const addRow = this.makeButton('md-table-addrow', '添加行', () => this.ctrl()?.addRowEnd(this.block, this.active))
    wrap.appendChild(addRow)

    // 行/列手柄：只在编辑中渲染，跟随活动单元格（点击弹出操作菜单）
    let rowGrip: HTMLElement | null = null
    let colGrip: HTMLElement | null = null
    if (this.active) {
      rowGrip = this.makeGrip('row', this.active.r, this.active.c)
      colGrip = this.makeGrip('col', this.active.r, this.active.c)
      wrap.append(rowGrip, colGrip)
    }

    wrap.addEventListener('mousedown', (e) => this.ctrl()?.onMouseDown(e, this.block))
    wrap.addEventListener('keydown', (e) => this.ctrl()?.onKeyDown(e, this.block))
    wrap.addEventListener('input', (e) => this.ctrl()?.onInput(e, this.block))
    wrap.addEventListener('paste', (e) => this.ctrl()?.onPaste(e))
    wrap.addEventListener('compositionstart', () => {
      const ctrl = this.ctrl()
      if (ctrl) ctrl.composing = true
    })
    wrap.addEventListener('compositionend', (e) => this.ctrl()?.onCompositionEnd(e, this.block))
    wrap.addEventListener('contextmenu', (e) => this.ctrl()?.onContextMenu(e, this.block))

    const positionGrips = () => {
      if (!rowGrip || !colGrip || !cellEl) return
      const wr = wrap.getBoundingClientRect()
      const cr = cellEl.getBoundingClientRect()
      rowGrip.style.top = `${cr.top - wr.top + (cr.height - 14) / 2}px`
      colGrip.style.left = `${cr.left - wr.left + (cr.width - 14) / 2}px`
    }
    // 横向滚动时手柄跟随活动格
    scroll.addEventListener('scroll', positionGrips)
    queueMicrotask(() => {
      positionGrips()
      // 仅编辑器区域内已有焦点时才恢复光标（打开文件/外部重载/窗口未聚焦时不抢焦点）；
      // goto/applyGrid/insertTable 的光标由控制器在 dispatch 后同步落位，不经过这里
      const view = this.ctrl()?.view
      const doc = view?.dom.ownerDocument
      if (cellEl && this.active && view && doc?.hasFocus() && view.dom.contains(doc.activeElement)) {
        placeCaretInCell(cellEl, this.active)
      }
    })
    return wrap
  }

  private makeButton(cls: string, title: string, onClick: () => void): HTMLElement {
    const btn = document.createElement('button')
    btn.className = cls
    btn.type = 'button'
    btn.title = title
    btn.textContent = '+'
    // mousedown.prevent：不夺活动格焦点（输入法组合不中断）
    btn.addEventListener('mousedown', (e) => e.preventDefault())
    btn.addEventListener('click', (e) => {
      e.preventDefault()
      onClick()
    })
    return btn
  }

  private makeGrip(kind: 'row' | 'col', r: number, c: number): HTMLElement {
    const btn = document.createElement('button')
    btn.className = `md-tgrip md-tgrip-${kind}`
    btn.type = 'button'
    btn.title = kind === 'row' ? '行操作' : '列操作'
    btn.appendChild(gripIcon(kind === 'col'))
    btn.addEventListener('mousedown', (e) => e.preventDefault())
    btn.addEventListener('click', (e) => this.ctrl()?.openGripMenu(e as MouseEvent, this.block, kind, r, c))
    return btn
  }
}

// ---- 工具栏插入模板 ----

export function insertTable(view: EditorView): void {
  const state = view.state
  const inTable = findTableAt(state)
  const line = state.doc.lineAt(state.selection.main.head)
  const empty = !inTable && line.text.trim() === ''
  const tpl = ['| 列一 | 列二 | 列三 |', '| ---- | ---- | ---- |', '|      |      |      |']
  const at = inTable ? inTable.to : empty ? line.from : line.to
  const pad = empty ? '' : '\n\n'
  const base = at + pad.length
  view.dispatch({
    changes: { from: at, insert: pad + tpl.join('\n') },
    selection: { anchor: base + 2, head: base + 4 },
    scrollIntoView: true,
  })
  view.focus()
  // 焦点落进首个表头格（控制器在 dispatch 后同步定位；编辑器未聚焦时也能工作）
  view.plugin(tableController)?.focusCell(0, 0, true)
}

// ---- 扩展入口 ----

export function tableEditing(): Extension {
  return [tableField, tableController]
}
