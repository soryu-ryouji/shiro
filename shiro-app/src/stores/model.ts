// 模型档案状态（Model 视图）：多档案 CRUD，可设默认（默认档案是 Chat 等任务实际使用的档案）。
// 界面分区：注册分区「模型导入」（注册表单）、管理分区「模型管理」（档案列表）。
// 供应商预设学 pi 内置目录：模型清单完整内置，端点与协议不暴露给用户。
import { reactive } from 'vue'
import { apiFetch } from '../api'
import type { components } from '../api-types'

export type ProfileSummary = components['schemas']['ProfileSummary']
export type ProfileListResponse = components['schemas']['ProfileListResponse']

/** 供应商预设：端点 + 完整内置模型清单（首个为默认；清单外模型可手填）。
 *  模型清单来源 pi 目录（providers/data/*.json），随版本更新。
 *  Kimi 三家：国内/国际走 OpenAI 兼容，Kimi Code 订阅走 Anthropic 协议 */
export const PROVIDERS: {
  key: string
  label: string
  baseUrl: string
  models: string[]
  protocol: 'openai' | 'anthropic'
}[] = [
  {
    key: 'deepseek',
    label: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com/v1',
    models: ['deepseek-v4-flash', 'deepseek-v4-pro', 'deepseek-v4-flash-vision-exp'],
    protocol: 'openai',
  },
  {
    key: 'moonshot-cn',
    label: 'Kimi 国内',
    baseUrl: 'https://api.moonshot.cn/v1',
    models: [
      'kimi-k3',
      'kimi-k2.6',
      'kimi-k2.5',
      'kimi-k2-0905-preview',
      'kimi-k2-0711-preview',
      'kimi-k2-turbo-preview',
      'kimi-k2-thinking',
      'kimi-k2-thinking-turbo',
      'kimi-k2.7-code',
      'kimi-k2.7-code-highspeed',
    ],
    protocol: 'openai',
  },
  {
    key: 'moonshot',
    label: 'Kimi 国际',
    baseUrl: 'https://api.moonshot.ai/v1',
    models: [
      'kimi-k3',
      'kimi-k2.6',
      'kimi-k2.5',
      'kimi-k2-0905-preview',
      'kimi-k2-0711-preview',
      'kimi-k2-turbo-preview',
      'kimi-k2-thinking',
      'kimi-k2-thinking-turbo',
      'kimi-k2.7-code',
      'kimi-k2.7-code-highspeed',
    ],
    protocol: 'openai',
  },
  {
    key: 'kimi-code',
    label: 'Kimi Code',
    baseUrl: 'https://api.kimi.com/coding',
    models: ['k3', 'k3-256k', 'kimi-for-coding', 'kimi-for-coding-highspeed'],
    protocol: 'anthropic',
  },
  {
    key: 'qwen-plan-cn',
    label: 'Qwen 订阅（国内）',
    baseUrl: 'https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1',
    models: [
      'qwen3.8-max',
      'qwen3.8-flash',
      'qwen3.7-max',
      'qwen3.7-plus',
      'qwen3.6-plus',
      'kimi-k2.6',
      'glm-5.2',
      'deepseek-v4-pro',
      'MiniMax-M2.5',
    ],
    protocol: 'openai',
  },
  {
    key: 'qwen-plan-intl',
    label: 'Qwen 订阅（国际）',
    baseUrl: 'https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1',
    models: [
      'qwen3.8-max',
      'qwen3.8-flash',
      'qwen3.7-max',
      'qwen3.7-plus',
      'qwen3.6-plus',
      'kimi-k2.6',
      'glm-5.2',
      'deepseek-v4-pro',
      'MiniMax-M2.5',
    ],
    protocol: 'openai',
  },
  {
    key: 'zhipu',
    label: '智谱 GLM',
    baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    models: [
      'glm-5.3',
      'glm-5.3-flash',
      'glm-5.3-highspeed',
      'glm-5.2',
      'glm-5.2-highspeed',
      'glm-5.1',
      'glm-5-turbo',
      'glm-5v-turbo',
      'glm-4.7',
      'glm-4.6v',
    ],
    protocol: 'openai',
  },
  {
    key: 'qwen',
    label: '通义千问',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    models: ['qwen-plus', 'qwen3-max', 'qwen-turbo', 'qwen-max'],
    protocol: 'openai',
  },
  {
    key: 'siliconflow',
    label: '硅基流动',
    baseUrl: 'https://api.siliconflow.cn/v1',
    models: [
      'deepseek-ai/DeepSeek-V3.2',
      'deepseek-ai/DeepSeek-V3.1',
      'Qwen/Qwen3-235B-A22B',
    ],
    protocol: 'openai',
  },
  {
    key: 'openai',
    label: 'OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    models: [
      'gpt-5.2',
      'gpt-5.2-pro',
      'gpt-5.2-chat-latest',
      'gpt-5.1',
      'gpt-5-mini',
      'gpt-5-nano',
      'gpt-4.1',
      'gpt-4.1-mini',
      'gpt-4.1-nano',
      'gpt-4o-mini',
      'o3-mini',
      'o4-mini',
    ],
    protocol: 'openai',
  },
  {
    key: 'openrouter',
    label: 'OpenRouter',
    baseUrl: 'https://openrouter.ai/api/v1',
    models: ['openrouter/auto'],
    protocol: 'openai',
  },
]

/** 由端点反查供应商 key；不匹配任何预设 → null（自定义端点） */
export function providerOf(baseUrl: string): string | null {
  const b = baseUrl.trim().replace(/\/$/, '')
  const hit = PROVIDERS.find((p) => b === p.baseUrl.replace(/\/$/, ''))
  return hit?.key ?? null
}

/** 档案显示名：预设供应商用 label，自定义端点显示 key */
export function profileLabel(p: { key: string; base_url: string }): string {
  const preset = PROVIDERS.find((x) => x.key === p.key)
  if (preset) return preset.label
  const byUrl = providerOf(p.base_url)
  return byUrl ? (PROVIDERS.find((x) => x.key === byUrl)?.label ?? p.key) : p.key
}

export type ModelViewMode = 'import' | 'manage'

export const modelStore = reactive({
  loaded: false,
  loading: false,
  error: '',
  /** 档案列表与默认项 */
  profiles: [] as ProfileSummary[],
  defaultKey: null as string | null,

  /** 当前分区视图 */
  view: 'import' as ModelViewMode,
  /** 注册表单态 */
  provider: '',
  baseUrl: '',
  model: '',
  protocol: 'openai' as 'openai' | 'anthropic',
  /** 思考强度：'' 不启用（不发参数）；low/medium/high */
  thinking: '',
  modelOptions: [] as string[],
  apiKey: '',
  showKey: false,
  saving: false,
  savedOk: false,

  async load() {
    this.loading = true
    this.error = ''
    try {
      const res = await apiFetch<ProfileListResponse>('/api/v1/model/profiles')
      this.profiles = res.profiles ?? []
      this.defaultKey = res.default_key ?? null
      this.loaded = true
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    } finally {
      this.loading = false
    }
  },

  /** 进「模型导入」：清空表单从头注册 */
  newImport() {
    this.view = 'import'
    this.provider = ''
    this.baseUrl = ''
    this.model = ''
    this.protocol = 'openai'
    this.modelOptions = []
    this.apiKey = ''
    this.showKey = false
  },

  /** 表单供应商切换：端点/协议回预设，模型取清单首项 */
  selectProvider(key: string) {
    this.provider = key
    this.thinking = ''
    const preset = PROVIDERS.find((p) => p.key === key)
    this.modelOptions = preset?.models ?? []
    if (preset) {
      this.baseUrl = preset.baseUrl
      this.protocol = preset.protocol
      this.model = preset.models[0] ?? ''
    }
  },

  /** 进「模型管理」 */
  openManage() {
    this.view = 'manage'
    void this.load()
  },

  /** 从管理列表点「编辑」：档案回填进注册表单（Key 不回填，改则输入新值） */
  editProfile(p: ProfileSummary) {
    const preset = PROVIDERS.find((x) => x.key === p.key)
    this.view = 'import'
    this.provider = preset ? p.key : ''
    this.baseUrl = p.base_url
    this.model = p.model
    this.protocol = p.protocol === 'anthropic' ? 'anthropic' : 'openai'
    this.thinking = p.thinking ?? ''
    this.modelOptions = preset?.models ?? []
    this.apiKey = ''
  },

  /** 保存表单：同 key 覆盖（编辑语义），新 key 注册 */
  async save() {
    const key = this.provider
    if (!key) {
      this.error = '请先选择供应商'
      return
    }
    this.saving = true
    this.error = ''
    this.savedOk = false
    try {
      const res = await apiFetch<ProfileListResponse>('/api/v1/model/profiles', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          key,
          base_url: this.baseUrl.trim(),
          model: this.model.trim(),
          protocol: this.protocol,
          thinking: this.thinking || undefined,
          api_key: this.apiKey.trim() || undefined,
        }),
      })
      this.profiles = res.profiles ?? []
      this.defaultKey = res.default_key ?? null
      this.apiKey = ''
      this.showKey = false
      this.savedOk = true
      setTimeout(() => (this.savedOk = false), 2500)
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    } finally {
      this.saving = false
    }
  },

  /** 连接测试结果（按档案 key） */
  testing: {} as Record<string, 'run' | null>,
  testResults: {} as Record<string, { ok: boolean; message: string } | undefined>,

  async testProfile(key: string) {
    this.testing[key] = 'run'
    this.testResults[key] = undefined
    try {
      const res = await apiFetch<components['schemas']['ProfileTestResponse']>(
        `/api/v1/model/profiles/${encodeURIComponent(key)}/test`,
        { method: 'POST' },
      )
      this.testResults[key] = { ok: res.ok, message: res.message }
    } catch (e) {
      this.testResults[key] = {
        ok: false,
        message: e instanceof Error ? e.message : String(e),
      }
    } finally {
      this.testing[key] = null
    }
  },

  async remove(key: string) {
    this.error = ''
    try {
      const res = await apiFetch<ProfileListResponse>(
        `/api/v1/model/profiles/${encodeURIComponent(key)}`,
        { method: 'DELETE' },
      )
      this.profiles = res.profiles ?? []
      this.defaultKey = res.default_key ?? null
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e)
    }
  },
})
