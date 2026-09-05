// 字数统计策略：策略表 + 当前策略常量，后续入设置面板供用户切换。
// 新增策略只追加条目，不改调用方。

export interface CountStrategy {
  label: string
  count: (text: string) => number
}

/** 仅中文字符（汉字）：去除空白、标点、英文与数字 */
function countCjk(text: string): number {
  let n = 0
  for (const ch of text) {
    if (/\p{Script=Han}/u.test(ch)) n++
  }
  return n
}

/** 全部非空白字符（含标点与英文） */
function countNonWhitespace(text: string): number {
  return text.replace(/\s/g, '').length
}

export const COUNT_STRATEGIES = {
  cjk: { label: '仅中文字符', count: countCjk },
  nonWhitespace: { label: '非空白字符', count: countNonWhitespace },
} as const satisfies Record<string, CountStrategy>

export type CountStrategyKey = keyof typeof COUNT_STRATEGIES

/** 当前生效的统计策略（后续由设置面板写入） */
export const ACTIVE_COUNT_STRATEGY: CountStrategyKey = 'cjk'

export function countWords(text: string): number {
  return COUNT_STRATEGIES[ACTIVE_COUNT_STRATEGY].count(text)
}
