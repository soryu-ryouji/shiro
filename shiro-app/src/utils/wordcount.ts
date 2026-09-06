// 字数统计策略：策略表 + 设置面板可切换的当前策略（localStorage 持久化，切换即时重算编辑器状态栏）。
// 新增策略只追加条目，不改调用方。
import { ref } from 'vue'

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

/** 中文字符 + 中文标点（CJK 标点区、全角形式区与常用中文标点 — … ·；英文与数字不计） */
function countCjkAndPunct(text: string): number {
  let n = 0
  for (const ch of text) {
    if (/\p{Script=Han}/u.test(ch) || /[\u3000-\u303F\uFF00-\uFF65\u2014\u2026\u00B7]/u.test(ch)) n++
  }
  return n
}

/** 英文单词：字母串（含词内 ' 与 -，如 don't、state-of-the-art 算一个词） */
const ENGLISH_WORD_RE = /[A-Za-z]+(?:['-][A-Za-z]+)*/g

function countEnglishWords(text: string): number {
  return text.match(ENGLISH_WORD_RE)?.length ?? 0
}

/** 中文字符 + 中文标点 + 英文单词（一个英文单词算一个字） */
function countCjkPunctEn(text: string): number {
  return countCjkAndPunct(text) + countEnglishWords(text)
}

/** 全部非空白字符（含标点与英文；暂未在设置中开放） */
function countNonWhitespace(text: string): number {
  return text.replace(/\s/g, '').length
}

export const COUNT_STRATEGIES = {
  cjk: { label: '仅中文字符', count: countCjk },
  cjkPunct: { label: '中文字符与标点', count: countCjkAndPunct },
  cjkPunctEn: { label: '中文字符、标点与英文单词', count: countCjkPunctEn },
  nonWhitespace: { label: '非空白字符', count: countNonWhitespace },
} as const satisfies Record<string, CountStrategy>

export type CountStrategyKey = keyof typeof COUNT_STRATEGIES

/** 设置面板开放的统计方案（nonWhitespace 不开放） */
export const COUNT_STRATEGY_OPTIONS: CountStrategyKey[] = ['cjk', 'cjkPunct', 'cjkPunctEn']

const COUNT_STRATEGY_KEY = 'shiro.count.strategy'
const COUNT_STRATEGY_DEFAULT: CountStrategyKey = 'cjk'

export function currentCountStrategy(): CountStrategyKey {
  const v = localStorage.getItem(COUNT_STRATEGY_KEY) as CountStrategyKey | null
  return v && v in COUNT_STRATEGIES ? v : COUNT_STRATEGY_DEFAULT
}

/** 响应式副本：设置面板与编辑器共享，切换后状态栏字数即时重算（Editor.vue 里 watch） */
export const countStrategy = ref<CountStrategyKey>(currentCountStrategy())

export function applyCountStrategy(key: CountStrategyKey): void {
  if (!(key in COUNT_STRATEGIES)) return
  localStorage.setItem(COUNT_STRATEGY_KEY, key)
  countStrategy.value = key
}

export function countWords(text: string): number {
  return COUNT_STRATEGIES[countStrategy.value].count(text)
}
