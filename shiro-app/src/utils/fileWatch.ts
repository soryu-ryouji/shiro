// 项目目录变动监听：EventSource 消费 daemon 的 SSE 推送（断线自动重连）。
// 鉴权走 ?key= 查询参数（EventSource 不支持自定义 header，见 docs/architecture.md）。
import { apiBase, apiToken } from '../api'

export interface WatchEventData {
  /** 变更的项目内相对路径（'/' 分隔）；空数组 = 服务端事件滞后溢出，应全量刷新 */
  changed: string[]
}

/** 监听项目目录变动；onEvent 在每次防抖批次推送时回调。返回停止函数（停止后不再重连）。 */
export function watchProject(path: string, onEvent: (e: WatchEventData) => void): () => void {
  const url = `${apiBase}/api/v1/projects/watch?key=${encodeURIComponent(apiToken)}&path=${encodeURIComponent(path)}`
  const es = new EventSource(url)
  es.onmessage = (e: MessageEvent<string>) => {
    try {
      onEvent(JSON.parse(e.data) as WatchEventData)
    } catch {
      // 非 JSON 帧（心跳等）：忽略
    }
  }
  return () => es.close()
}
