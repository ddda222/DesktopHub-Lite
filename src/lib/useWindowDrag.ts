import { onBeforeUnmount, onMounted, type Ref } from 'vue'
import { startBarDrag } from './desktop/desktop'

type DragOptions = {
  native: () => boolean
  onPress?: () => void
  onEnd?: () => void
  onError: (error: unknown) => void
}

/** 标题栏拖动与点击共存；预览只移动 DOM，桌面交给宿主移动当前窗口。 */
export function useWindowDrag(element: Ref<HTMLElement | null>, options: DragOptions) {
  let offsetX = 0
  let offsetY = 0
  let blockClick = false
  let observer: ResizeObserver | undefined
  let session: {
    pointer: number; handle: HTMLElement; startX: number; startY: number
    baseX: number; baseY: number; rect: DOMRect; moved: boolean; native: boolean
  } | undefined

  function apply(x: number, y: number): void {
    offsetX = x
    offsetY = y
    element.value?.style.setProperty('--window-drag-x', `${x}px`)
    element.value?.style.setProperty('--window-drag-y', `${y}px`)
  }

  function clamp(): void {
    if (options.native() || !element.value || session) return
    const box = element.value.getBoundingClientRect()
    const dx = Math.min(Math.max(box.left, 8), Math.max(8, window.innerWidth - box.width - 8)) - box.left
    const dy = Math.min(Math.max(box.top, 8), Math.max(8, window.innerHeight - box.height - 8)) - box.top
    if (dx || dy) apply(offsetX + dx, offsetY + dy)
  }

  function finish(): void {
    const previous = session
    session = undefined
    window.removeEventListener('pointermove', move)
    window.removeEventListener('pointerup', up)
    window.removeEventListener('pointercancel', up)
    window.removeEventListener('blur', finish)
    if (previous?.handle.hasPointerCapture(previous.pointer)) previous.handle.releasePointerCapture(previous.pointer)
    element.value?.classList.remove('window-dragging')
    if (previous) options.onEnd?.()
    clamp()
  }

  function up(event: PointerEvent): void {
    if (event.pointerId === session?.pointer) finish()
  }

  function move(event: PointerEvent): void {
    const current = session
    if (!current || event.pointerId !== current.pointer) return
    if (!(event.buttons & 1)) { finish(); return }
    const dx = event.clientX - current.startX
    const dy = event.clientY - current.startY
    if (!current.moved && Math.hypot(dx, dy) < 5) return
    if (!current.moved) {
      current.moved = true
      blockClick = true
      element.value?.classList.add('window-dragging')
      if (current.native) {
        if (current.handle.hasPointerCapture(current.pointer)) current.handle.releasePointerCapture(current.pointer)
        void startBarDrag().catch(options.onError).finally(finish)
        return
      }
    }
    if (current.native) return
    const left = Math.min(Math.max(current.rect.left + dx, 8), Math.max(8, window.innerWidth - current.rect.width - 8))
    const top = Math.min(Math.max(current.rect.top + dy, 8), Math.max(8, window.innerHeight - current.rect.height - 8))
    apply(current.baseX + left - current.rect.left, current.baseY + top - current.rect.top)
  }

  function begin(event: PointerEvent): void {
    if (event.button !== 0 || !event.isPrimary || !element.value) return
    const handle = event.currentTarget as HTMLElement
    // 标题栏里的关闭按钮、输入框等继续执行自己的动作。
    const control = (event.target as Element).closest('button, a, input, select, textarea, [contenteditable="true"]')
    if (control && control !== handle) return
    finish()
    blockClick = false
    event.preventDefault()
    session = {
      pointer: event.pointerId, handle, startX: event.clientX, startY: event.clientY,
      baseX: offsetX, baseY: offsetY, rect: element.value.getBoundingClientRect(),
      moved: false, native: options.native()
    }
    handle.setPointerCapture(event.pointerId)
    options.onPress?.()
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    window.addEventListener('pointercancel', up)
    window.addEventListener('blur', finish)
  }

  function consumeClick(event: MouseEvent): boolean {
    if (!blockClick || event.detail === 0) return false
    blockClick = false
    event.preventDefault()
    event.stopPropagation()
    return true
  }

  function reset(): void { finish(); apply(0, 0); clamp() }

  onMounted(() => {
    if (options.native()) return
    observer = new ResizeObserver(clamp)
    if (element.value) observer.observe(element.value)
    window.addEventListener('resize', clamp)
  })
  onBeforeUnmount(() => {
    observer?.disconnect()
    window.removeEventListener('resize', clamp)
    finish()
  })
  return { begin, consumeClick, reset }
}
