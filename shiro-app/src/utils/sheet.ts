// 文稿格式：markdown（.md/.markdown）与纯文本（.txt）两类，判断与显示名后缀剥离集中在这里。

const SHEET_EXT_RE = /\.(md|markdown|txt)$/i

/** 文稿显示名：去 .md/.markdown/.txt 后缀 */
export function stripSheetExt(name: string): string {
  return name.replace(SHEET_EXT_RE, '')
}

/** 纯文本文稿（.txt）：编辑器走纯文本模式（无 markdown 渲染），预览按原样显示 */
export function isPlainSheet(path: string): boolean {
  return /\.txt$/i.test(path)
}
