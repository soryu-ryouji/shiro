// 增量 SSE 解析器（WHATWG EventSource 语义子集）：event / data / id 字段，
// 多行 data 以 \n 拼接，注释行（: 开头，心跳）忽略，空行分发。
// 用于 fetch 流式消费（chat 生成流与项目目录监听共用；EventSource 无法带 header / POST）。

export interface SseEvent {
  /** 事件类型（缺省 message） */
  event: string
  /** data 字段（多行已拼接） */
  data: string
  /** 最近一次 id 字段 */
  id?: string
}

/** 创建增量解析器：把 fetch 读到的文本片段喂进去，完整帧经 onEvent 回调 */
export function createSseParser(onEvent: (e: SseEvent) => void): (chunk: string) => void {
  let buf = ''
  let eventType = ''
  let dataLines: string[] = []
  let lastId: string | undefined
  let first = true

  const dispatch = () => {
    if (dataLines.length > 0) {
      onEvent({ event: eventType || 'message', data: dataLines.join('\n'), id: lastId })
    }
    eventType = ''
    dataLines = []
  }

  const handleLine = (line: string) => {
    if (line === '') {
      dispatch()
      return
    }
    if (line.startsWith(':')) return
    const idx = line.indexOf(':')
    const field = idx === -1 ? line : line.slice(0, idx)
    let value = idx === -1 ? '' : line.slice(idx + 1)
    if (value.startsWith(' ')) value = value.slice(1)
    if (field === 'event') eventType = value
    else if (field === 'data') dataLines.push(value)
    else if (field === 'id') lastId = value
    // retry 字段由调用方重连策略决定，此处忽略
  }

  return (chunk: string) => {
    if (first) {
      first = false
      if (chunk.startsWith('\uFEFF')) chunk = chunk.slice(1)
    }
    buf += chunk
    // 行分隔支持 \n / \r\n / \r；行尾恰为 \r 时留到下一片段（可能是跨片段的 \r\n）
    let start = 0
    for (let i = 0; i < buf.length; i++) {
      const c = buf[i]
      if (c === '\n') {
        handleLine(buf.slice(start, i))
        start = i + 1
      } else if (c === '\r') {
        if (i + 1 >= buf.length) break
        handleLine(buf.slice(start, i))
        start = buf[i + 1] === '\n' ? i + 2 : i + 1
        if (buf[i + 1] === '\n') i++
      }
    }
    buf = buf.slice(start)
  }
}
