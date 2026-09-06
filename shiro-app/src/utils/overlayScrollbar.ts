// 自绘覆盖滚动条（VSCode/Ulysses 式方案）：隐藏原生条，向滚动容器注入 DOM 胶囊滑块。
// 滑块为容器的绝对定位子元素（本随内容滚动），JS 用 translate 补偿滚动偏移，使其钉在视口边缘。
// 行为：滚动时显现、停止约 0.9s 后淡出（DOM 元素支持 transition，原生伪元素做不到）、可拖拽。
// 可选 host：滑块挂载到另一个不滚动的祖先元素上（钉在 host 右缘/底缘），
// 用于「滑块越出滚动容器右缘」的场景——如编辑器大纲展开时，编辑器滚动条要在大纲面板右侧。
import type { Directive } from 'vue'

const HIDE_DELAY = 900
const MIN_THUMB = 24
/** 滑块宽 8px + 距视口边缘 2px */
const THUMB = 8
const GAP = 2

interface Axis {
  thumb: HTMLElement
  vertical: boolean
  dragging: boolean
}

/**
 * 给滚动容器挂自绘覆盖滚动条，返回卸载函数。
 * 容器需可滚动（overflow auto/scroll）；position 为 static 时由本函数补 relative（卸载时还原）。
 * host：滑块的挂载元素（默认 el 自身）。host 不滚动（如编辑器主区域），滑块钉在 host 右缘，无滚动补偿。
 */
export function attachOverlayScrollbar(el: HTMLElement, host?: HTMLElement): () => void {
  const pinned = host ?? el
  const outside = pinned !== el
  const fixPosition = getComputedStyle(pinned).position === 'static'
  if (fixPosition) pinned.style.position = 'relative'
  el.classList.add('osb-host')

  const axes: Axis[] = [true, false].map((vertical) => {
    const thumb = document.createElement('div')
    thumb.className = vertical ? 'osb-thumb osb-v' : 'osb-thumb osb-h'
    pinned.appendChild(thumb)
    return { thumb, vertical, dragging: false }
  })

  let hideTimer: ReturnType<typeof setTimeout> | undefined

  /** 刷新两轴滑块的尺寸与位置；无溢出的轴隐藏；reveal 时显现可滚动的轴 */
  function update(reveal = false) {
    const { clientWidth: vw, clientHeight: vh, scrollWidth: cw, scrollHeight: ch } = el
    const { scrollLeft: sl, scrollTop: st } = el
    // 挂载到外部 host 时：host 不滚动，不做滚动补偿；右缘取 host 宽度
    const pinW = outside ? pinned.clientWidth : vw
    const compX = outside ? 0 : sl
    const compY = outside ? 0 : st
    for (const a of axes) {
      const max = a.vertical ? ch - vh : cw - vw
      if (max <= 0) {
        a.thumb.classList.remove('osb-on')
        continue
      }
      const view = a.vertical ? vh : vw
      const content = a.vertical ? ch : cw
      const pos = a.vertical ? st : sl
      const size = Math.min(Math.max((view * view) / content, MIN_THUMB), view)
      const offset = (pos / max) * (view - size)
      // 钉在视口边缘：滚动容器内需加滚动偏移（内容坐标 = 滚动偏移 + 视口内位置）；外部 host 直接取边缘
      const x = a.vertical ? compX + pinW - THUMB - GAP : compX + offset
      const y = a.vertical ? compY + offset : compY + vh - THUMB - GAP
      const t = a.thumb.style
      const nextH = `${size}px`
      const nextT = `translate(${x}px, ${y}px)`
      // 只在变化时写入：相同值也会产生产变异，与 MutationObserver 配合时防自激
      if (a.vertical) {
        if (t.height !== nextH) t.height = nextH
      } else {
        const nextW = `${size}px`
        if (t.width !== nextW) t.width = nextW
      }
      if (t.transform !== nextT) t.transform = nextT
      if (reveal) a.thumb.classList.add('osb-on')
    }
  }

  function scheduleHide() {
    if (hideTimer !== undefined) clearTimeout(hideTimer)
    hideTimer = setTimeout(() => {
      // 拖拽中或悬停在滑块上时不隐藏（对齐 VSCode）；悬停状态由 pointerleave 触发下一次隐藏
      for (const a of axes) {
        if (!a.dragging && !a.thumb.matches(':hover')) a.thumb.classList.remove('osb-on')
      }
    }, HIDE_DELAY)
  }

  function onScroll() {
    update(true)
    scheduleHide()
  }

  function onThumbPointerDown(a: Axis, e: PointerEvent) {
    if (e.button !== 0) return
    // 阻止文本选中和容器上的指针处理（如编辑器聚焦、遮罩点击）
    e.preventDefault()
    e.stopPropagation()
    a.dragging = true
    a.thumb.classList.add('osb-drag')
    a.thumb.setPointerCapture(e.pointerId)
    const startPointer = a.vertical ? e.clientY : e.clientX
    const startScroll = a.vertical ? el.scrollTop : el.scrollLeft

    const onMove = (ev: PointerEvent) => {
      const view = a.vertical ? el.clientHeight : el.clientWidth
      const content = a.vertical ? el.scrollHeight : el.scrollWidth
      const max = content - view
      if (max <= 0) return
      const size = Math.min(Math.max((view * view) / content, MIN_THUMB), view)
      if (view - size <= 0) return
      const delta = ((a.vertical ? ev.clientY : ev.clientX) - startPointer) / (view - size)
      if (a.vertical) el.scrollTop = startScroll + delta * max
      else el.scrollLeft = startScroll + delta * max
    }
    const onUp = (ev: PointerEvent) => {
      ev.stopPropagation()
      a.dragging = false
      a.thumb.classList.remove('osb-drag')
      a.thumb.releasePointerCapture(ev.pointerId)
      a.thumb.removeEventListener('pointermove', onMove)
      a.thumb.removeEventListener('pointerup', onUp)
      a.thumb.removeEventListener('pointercancel', onUp)
      scheduleHide()
    }
    a.thumb.addEventListener('pointermove', onMove)
    a.thumb.addEventListener('pointerup', onUp)
    a.thumb.addEventListener('pointercancel', onUp)
  }

  function onThumbLeave(a: Axis) {
    if (!a.dragging) scheduleHide()
  }

  const downs = axes.map((a) => {
    const fn = (e: PointerEvent) => onThumbPointerDown(a, e)
    a.thumb.addEventListener('pointerdown', fn)
    return fn
  })
  const leaves = axes.map((a) => {
    const fn = () => onThumbLeave(a)
    a.thumb.addEventListener('pointerleave', fn)
    return fn
  })

  el.addEventListener('scroll', onScroll, { passive: true })
  // 容器自身尺寸变化（窗口/栏宽调整）时刷新；内容尺寸变化会在下次滚动时随 onScroll 刷新
  const ro = new ResizeObserver(() => update())
  ro.observe(el)
  if (outside) ro.observe(pinned)
  // 内容变化（子项增删、v-show 显隐、文本编辑）时刷新：RO 只看容器盒子，看不到内容高度变化；
  // 不刷新的话滑块会带着过期尺寸/可见状态残留（如设置面板切换分页后短页面还挂着长滑块）
  const mo = new MutationObserver((recs) => {
    // 滑块自身也是被观察元素的子节点：update 写滑块 style/class 会再触发本回调——必须过滤，否则自激死循环
    if (recs.some((r) => !(r.target as HTMLElement).classList?.contains?.('osb-thumb'))) update()
  })
  mo.observe(el, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ['style', 'class'] })
  update()

  return () => {
    el.removeEventListener('scroll', onScroll)
    ro.disconnect()
    mo.disconnect()
    if (hideTimer !== undefined) clearTimeout(hideTimer)
    axes.forEach((a, i) => {
      a.thumb.removeEventListener('pointerdown', downs[i])
      a.thumb.removeEventListener('pointerleave', leaves[i])
      a.thumb.remove()
    })
    el.classList.remove('osb-host')
    if (fixPosition) pinned.style.position = ''
  }
}

const cleanups = new WeakMap<HTMLElement, () => void>()

/** v-overlay-scrollbar：给模板里的滚动容器挂自绘覆盖滚动条 */
export const vOverlayScrollbar: Directive<HTMLElement> = {
  mounted(el) {
    cleanups.set(el, attachOverlayScrollbar(el))
  },
  unmounted(el) {
    cleanups.get(el)?.()
    cleanups.delete(el)
  },
}
