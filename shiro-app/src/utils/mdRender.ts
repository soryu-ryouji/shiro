// 预览用 Markdown 渲染：marked 出 HTML，按「分段方式」预处理使段落切分与编辑器一致。
// 回车分段（Ulysses 式）下连续文本行是独立段落，而 CommonMark 会合并成一段——
// 预处理在相邻纯文本行之间补空行，让预览的段间距真实反映目标平台排版。
// 块级标记行（标题/引用/列表/围栏）不补，保持 Markdown 原生行为（紧凑列表等）。
import { marked } from 'marked'
import type { ParaMode } from './font'
import { FENCE_RE } from './livePreview'

// 与 livePreview 一致的块级行判定：标题、引用、无序/有序列表
const BLOCK_RE = /^(#{1,6}(?: +|$)|>|[-*+] +|\d{1,9}[.)] +)/

function normalizeParas(src: string): string {
  const lines = src.split('\n')
  const out: string[] = []
  let inFence = false
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    out.push(line)
    if (FENCE_RE.test(line)) inFence = !inFence
    if (inFence) continue
    const next = lines[i + 1]
    if (next === undefined) break
    // 当前行与下一行都是非空纯文本行：补空行，让下一行成为独立段落
    const plain = (l: string) => l.trim() !== '' && !BLOCK_RE.test(l)
    if (plain(line) && plain(next)) out.push('')
  }
  return out.join('\n')
}

/** 渲染为阅读视图 HTML（v-html 用；内容为作者本机手稿，不做 sanitize） */
export function renderMarkdown(src: string, paraMode: ParaMode): string {
  const body = paraMode === 'enter' ? normalizeParas(src) : src
  return marked.parse(body, { async: false, breaks: true })
}
