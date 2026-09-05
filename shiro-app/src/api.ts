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
  return res.json() as Promise<T>
}
