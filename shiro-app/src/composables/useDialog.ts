// 对话框通用行为（SettingsDialog / NewProjectDialog 共用）：
// - 遮罩「按下与抬起都落在遮罩上」才关闭：面板内拖动选择文本滑出面板松开，按 click.self 判定会误关
// - Esc 关闭
// - 打开期间挂 body.dialog-open 挂起窗口拖拽区（drag 由 OS 命中测试优先消费，不挂起点遮罩会变拖动窗口）
import { onMounted, onUnmounted } from 'vue'

export function useDialogMask(close: () => void) {
  let downOnMask = false

  function onMaskDown(e: PointerEvent) {
    downOnMask = e.target === e.currentTarget
  }

  function onMaskUp(e: PointerEvent) {
    if (downOnMask && e.target === e.currentTarget) close()
    downOnMask = false
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') close()
  }

  onMounted(() => {
    document.body.classList.add('dialog-open')
    window.addEventListener('keydown', onKeydown)
  })
  onUnmounted(() => {
    document.body.classList.remove('dialog-open')
    window.removeEventListener('keydown', onKeydown)
  })

  return { onMaskDown, onMaskUp }
}
