// 连接参数由 Electron 主进程经 URL hash 注入（见 docs/architecture.md sidecar 模式）
const params = new URLSearchParams(location.hash.replace(/^#/, ''))

export const apiBase = params.get('api') ?? ''
export const apiToken = params.get('token') ?? ''
export const hasConnection = Boolean(apiBase && apiToken)

export async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${apiBase}${path}`, {
    ...init,
    headers: { Authorization: `Bearer ${apiToken}`, ...init?.headers },
  })
  if (!res.ok) throw new Error(`${init?.method ?? 'GET'} ${path} → HTTP ${res.status}`)
  // 201/204 等空 body 响应：res.json() 会抛 Unexpected end of JSON input，按文本判空
  const text = await res.text()
  return (text ? JSON.parse(text) : undefined) as T
}

/** POST JSON 便捷封装：API 操作统一 POST + body（约定见 docs/structure.md） */
export function apiPost<T>(path: string, body?: unknown): Promise<T> {
  return apiFetch<T>(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
}
