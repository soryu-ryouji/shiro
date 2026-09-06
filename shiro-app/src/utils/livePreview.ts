// Markdown 实时预览（Obsidian 式）：光标不在处渲染格式并隐藏语法记号，光标所在处显示原文可编辑。
// 范围：标题（# 记号以挂件挂在内容列左缘留白，Ulysses 式）、引用（记号隐藏，行内细竖线贴文本左缘）、无序列表符号、粗体、斜体、删除线、行内码、代码围栏。
// 表格例外：始终整块渲染为挂件就地编辑（块级装饰由 utils/tableView.ts 的 tableField 提供——
// 块级装饰不允许来自 ViewPlugin，这里只负责跳过表内行不做行内装饰），不参与「光标处显示原文」。
// 有意从简（v1）：不解析嵌套/跨行标记、不支持转义、有序列表保持原样、围栏内无语法高亮。
import { Decoration, EditorView, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from '@codemirror/view'
import { RangeSetBuilder, type Extension } from '@codemirror/state'
import { visibleTableBlocks, FENCE_RE, HEADING_RE, type TableBlock } from './tableView'

export { FENCE_RE, HEADING_RE } from './tableView'

// ---- 行内标记扫描（单行、扁平不嵌套）：行内码 > 粗体/删除线 > 斜体 ----
// 扫描严格从左到右，命中后直接跳到标记末尾继续，因此结果按位置升序且天然互不重叠。

type MarkKind = 'code' | 'strong' | 'em' | 'strike'

interface InlineMark {
  kind: MarkKind
  from: number
  to: number
  openLen: number
  closeLen: number
}

const isSpace = (ch: string): boolean => ch === '' || /\s/.test(ch)

function scanInline(text: string): InlineMark[] {
  const marks: InlineMark[] = []
  let i = 0
  while (i < text.length) {
    const c = text.charAt(i)
    if (c === '`') {
      // 行内码：等长反引号串包裹
      let run = i
      while (text.charAt(run) === '`') run++
      const tickLen = run - i
      const close = text.indexOf(text.slice(i, run), run)
      if (close === -1) {
        i = run
      } else {
        marks.push({ kind: 'code', from: i, to: close + tickLen, openLen: tickLen, closeLen: tickLen })
        i = close + tickLen
      }
      continue
    }
    const pair = text.slice(i, i + 2)
    if (pair === '**' || pair === '__' || pair === '~~') {
      // 粗体/删除线：内容非空且首尾非空白
      const close = text.indexOf(pair, i + 2)
      if (close > i + 2 && !isSpace(text.charAt(i + 2)) && !isSpace(text.charAt(close - 1))) {
        marks.push({ kind: pair === '~~' ? 'strike' : 'strong', from: i, to: close + 2, openLen: 2, closeLen: 2 })
        i = close + 2
      } else {
        i += 2
      }
      continue
    }
    if (c === '*' || c === '_') {
      // 斜体：开标记前、闭标记后不能是字母/数字/记号字符（排除 snake_case 与 ** 边界），内容非空且首尾非空白
      const prevOk = i === 0 || !/[\w*_]/.test(text.charAt(i - 1))
      const close = text.indexOf(c, i + 1)
      if (
        prevOk &&
        close > i + 1 &&
        !isSpace(text.charAt(i + 1)) &&
        !isSpace(text.charAt(close - 1)) &&
        !/[\w*_]/.test(text.charAt(close + 1))
      ) {
        marks.push({ kind: 'em', from: i, to: close + 1, openLen: 1, closeLen: 1 })
        i = close + 1
        continue
      }
    }
    i++
  }
  return marks
}

// ---- 装饰 ----

const hideDeco = Decoration.replace({})
const markDeco: Record<MarkKind, Decoration> = {
  code: Decoration.mark({ class: 'md-code' }),
  strong: Decoration.mark({ class: 'md-strong' }),
  em: Decoration.mark({ class: 'md-em' }),
  strike: Decoration.mark({ class: 'md-strike' }),
}

/** 行首记号挂件：替换原文行首记号（标题 #、引用 >），渲染到内容列左缘留白（CSS 负外边距使净宽度为零，正文位置不动）。
 *  挂件是行内元素，与文本天然基线对齐，不受字号、行高与段间距影响。 */
class GutterMarkWidget extends WidgetType {
  constructor(readonly marks: string) {
    super()
  }
  eq(other: GutterMarkWidget): boolean {
    return other.marks === this.marks
  }
  toDOM(): HTMLElement {
    const span = document.createElement('span')
    span.className = 'md-gmark'
    span.textContent = this.marks
    return span
  }
}
// 1-6 级标题记号各一份共享装饰实例
const headingDeco = Array.from({ length: 6 }, (_, i) =>
  Decoration.replace({ widget: new GutterMarkWidget('#'.repeat(i + 1)) }),
)

/** 无序列表符号挂件：- / * / + 渲染为圆点 */
class BulletWidget extends WidgetType {
  eq(): boolean {
    return true
  }
  toDOM(): HTMLElement {
    const span = document.createElement('span')
    span.className = 'md-bullet'
    span.textContent = '• '
    return span
  }
}
const bulletDeco = Decoration.replace({ widget: new BulletWidget() })

const QUOTE_RE = /^>(?: +|$)/
const LIST_RE = /^[-*+] +/
const INDENT_RE = /^ */

function buildDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>()
  const doc = view.state.doc
  const ranges = view.state.selection.ranges
  // 未聚焦时（初次打开选区默认在文首、切去其他面板等）不算「光标所在」，始终渲染预览
  const focused = view.hasFocus
  const touches = (from: number, to: number) => focused && ranges.some((r) => r.from <= to && r.to >= from)

  // 围栏状态：从文档头扫到首个可视行（手稿规模下开销可忽略；超大文档不判定，视为不在围栏内）
  const firstVisible = view.visibleRanges.length ? doc.lineAt(view.visibleRanges[0].from).number : 1
  let inFence = false
  if (firstVisible > 1 && doc.lines <= 50000) {
    for (let n = 1; n < firstVisible; n++) {
      if (FENCE_RE.test(doc.line(n).text)) inFence = !inFence
    }
  }

  // 表格块：挂件渲染在 tableView 的 tableField；这里只需要跳过块内行
  const tableByStart = new Map<number, TableBlock>()
  for (const b of visibleTableBlocks(view)) tableByStart.set(b.startLine, b)

  for (const visible of view.visibleRanges) {
    let first = doc.lineAt(visible.from).number
    const last = doc.lineAt(visible.to).number
    // 表格块首行可能在视口上方（上下滚动到表格中部）：从块首行开始遍历，保证跳过完整块
    for (const b of tableByStart.values()) {
      if (b.startLine < first && b.endLine >= first) first = b.startLine
    }
    for (let n = first; n <= last; n++) {
      const line = doc.line(n)
      const text = line.text
      let lineClass = ''
      const items: { from: number; to: number; deco: Decoration }[] = []

      // 表格区域：跳过块内其余行（块内无围栏，不影响围栏状态跟踪）
      const tableBlock = tableByStart.get(n)
      if (tableBlock) {
        n = tableBlock.endLine
        continue
      }

      // 代码围栏：开/闭行与内容行整行染色，内部不做行内解析
      if (FENCE_RE.test(text)) {
        lineClass = 'md-fence'
        inFence = !inFence
      } else if (inFence) {
        lineClass = 'md-fence'
      } else {
        const indent = (INDENT_RE.exec(text) ?? [''])[0].length
        const raw = touches(line.from, line.to) // 光标在行内：保留原文记号，方便编辑
        if (indent <= 3) {
          const rest = text.slice(indent)
          const heading = HEADING_RE.exec(rest)
          const quote = heading ? null : QUOTE_RE.exec(rest)
          const list = heading || quote ? null : LIST_RE.exec(rest)
          const block = heading ?? quote ?? list
          if (block) {
            const from = line.from + indent
            const to = from + block[0].length
            if (heading) {
              lineClass = 'md-h'
              if (!raw) items.push({ from, to, deco: headingDeco[heading[1].length - 1] })
            } else if (quote) {
              // 极简引用：隐藏 > 记号，行内细竖线贴文本块左缘（样式见 Editor.vue .md-quote）
              lineClass = 'md-quote'
              if (!raw) items.push({ from, to, deco: hideDeco })
            } else if (!raw) {
              items.push({ from, to, deco: bulletDeco })
            }
          }
        }
        // 行内格式（整行扫描）
        for (const m of scanInline(text)) {
          const from = line.from + m.from
          const to = line.from + m.to
          items.push({ from, to, deco: markDeco[m.kind] })
          if (!touches(from, to)) {
            items.push({ from, to: from + m.openLen, deco: hideDeco })
            items.push({ from: to - m.closeLen, to, deco: hideDeco })
          }
        }
      }

      // 行装饰先于区间装饰写入；RangeSetBuilder 要求按位置升序，乱序会抛错
      if (lineClass) builder.add(line.from, line.from, Decoration.line({ class: lineClass }))
      items.sort((a, b) => a.from - b.from || a.to - b.to)
      for (const it of items) builder.add(it.from, it.to, it.deco)
    }
  }
  return builder.finish()
}

/** 实时预览扩展：文档、可视区或选区变化时重建装饰 */
export function livePreview(): Extension {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet
      constructor(view: EditorView) {
        this.decorations = buildDecorations(view)
      }
      update(update: ViewUpdate) {
        if (update.docChanged || update.viewportChanged || update.selectionSet || update.focusChanged) {
          this.decorations = buildDecorations(update.view)
        }
      }
    },
    { decorations: (v) => v.decorations },
  )
}
