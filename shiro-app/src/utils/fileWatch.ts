// 项目目录变动监听：POST + fetch 流式消费 daemon SSE（非 EventSource：需自定义 Authorization header）。
// 断线自动重连（指数退避 + 抖动 + 封顶），401 停止重连并回调；连上/后台恢复时补全量刷新（覆盖错过的变更）。
import { apiBase, apiToken } from '../api'
import { createSseParser } from './sse'

export interface WatchEventData {
  /** 变更的项目内相对路径（'/' 分隔）；空数组 = 服务端事件滞后或重连补刷，应全量刷新 */
  changed: string[]
}

export interface WatchHandlers {
  /** 每次变更推送 */
  onEvent: (e: WatchEventData) => void
  /** 鉴权失败（key 轮换等）：已停止重连，由调用方提示 */
  onAuthError?: () => void
}

const RECONNECT_BASE_MS = 1000
const RECONNECT_MAX_MS = 30_000
/** 后台挂起超过该时长，恢复时补一次全量刷新 */
const RESUME_REFRESH_AFTER_MS = 5_000

/** 监听项目目录变动；返回停止函数（停止后不再重连）。 */
export function watchProject(path: string, handlers: WatchHandlers): () => void {
  let stopped = false
  let controller: AbortController | null = null
  let retryMs = RECONNECT_BASE_MS
  let timer: ReturnType<typeof setTimeout> | null = null
  let hiddenAt = 0

  async function connect() {
    if (stopped) return
    controller = new AbortController()
    try {
      const res = await fetch(`${apiBase}/api/v1/projects/watch`, {
        method: 'POST',
        signal: controller.signal,
        headers: { Authorization: `Bearer ${apiToken}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ path }),
      })
      if (res.status === 401) {
        stopped = true
        handlers.onAuthError?.()
        return
      }
      if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`)
      // 连上即复位退避，并补一次全量刷新（覆盖断线期间错过的变更）
      retryMs = RECONNECT_BASE_MS
      handlers.onEvent({ changed: [] })

      const reader = res.body.getReader()
      const decoder = new TextDecoder()
      const parse = createSseParser((e) => {
        try {
          handlers.onEvent(JSON.parse(e.data) as WatchEventData)
        } catch {
          // 非 JSON 帧：忽略
        }
      })
      for (;;) {
        const { done, value } = await reader.read()
        if (done) break
        parse(decoder.decode(value, { stream: true }))
      }
    } catch (e) {
      // 主动停止或已退订：不重连
      if (stopped || (e instanceof DOMException && e.name === 'AbortError')) return
    }
    scheduleReconnect()
  }

  function scheduleReconnect() {
    if (stopped) return
    const delay = retryMs + Math.random() * 300
    retryMs = Math.min(retryMs * 2, RECONNECT_MAX_MS)
    timer = setTimeout(() => void connect(), delay)
  }

  const onVisibility = () => {
    if (document.hidden) {
      hiddenAt = Date.now()
      return
    }
    if (hiddenAt && Date.now() - hiddenAt > RESUME_REFRESH_AFTER_MS) {
      hiddenAt = 0
      handlers.onEvent({ changed: [] })
    }
  }
  document.addEventListener('visibilitychange', onVisibility)

  void connect()

  return () => {
    stopped = true
    if (timer) clearTimeout(timer)
    document.removeEventListener('visibilitychange', onVisibility)
    controller?.abort()
  }
}
