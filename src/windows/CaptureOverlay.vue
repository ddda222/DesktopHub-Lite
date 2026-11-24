<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'

interface CaptureSessionInfo {
  sessionId: number
  dataUri: string
  width: number
  height: number
  screenX: number
  screenY: number
}

const session = ref<CaptureSessionInfo | null>(null)
const errorText = ref('')
const recognizing = ref(false)
let cancelling = false
const canvasEl = ref<HTMLElement | null>(null)
const drag = ref<{
  pointerId: number
  startX: number
  startY: number
  currentX: number
  currentY: number
} | null>(null)

const boxRect = computed(() => {
  if (!drag.value) return null
  const left = Math.min(drag.value.startX, drag.value.currentX)
  const top = Math.min(drag.value.startY, drag.value.currentY)
  const width = Math.abs(drag.value.currentX - drag.value.startX)
  const height = Math.abs(drag.value.currentY - drag.value.startY)
  return { left, top, width, height }
})

const pixelBadge = computed(() => {
  const rect = boxRect.value
  const host = canvasEl.value?.getBoundingClientRect()
  const info = session.value
  if (!rect || !host || !info || host.width <= 0 || host.height <= 0) return ''
  const pw = Math.round((rect.width / host.width) * info.width)
  const ph = Math.round((rect.height / host.height) * info.height)
  return `${pw} × ${ph} px`
})

function buildBrowserDemoUri(): string {
  const canvas = document.createElement('canvas')
  canvas.width = 1280
  canvas.height = 720
  const ctx = canvas.getContext('2d')
  if (ctx) {
    ctx.fillStyle = '#0f172a'
    ctx.fillRect(0, 0, canvas.width, canvas.height)
    ctx.fillStyle = '#f8fafc'
    ctx.fillRect(280, 200, 720, 260)
    ctx.fillStyle = '#0f172a'
    ctx.font = 'bold 34px sans-serif'
    ctx.fillText('截图识字预览示例 DesktopHub Lite 0.1.3', 320, 295)
    ctx.font = '24px sans-serif'
    ctx.fillStyle = '#334155'
    ctx.fillText('按住鼠标左键拖拉框选这段文字，松手即可识别', 320, 365)
  }
  return canvas.toDataURL('image/png')
}

async function cancel(): Promise<void> {
  if (recognizing.value || cancelling) return
  if (!isDesktop) {
    errorText.value = '预览模式：已模拟取消截图'
    return
  }
  cancelling = true
  try {
    await invokeHost('cancel_screen_ocr')
  } catch {
    cancelling = false
    // 窗口关闭过程忽略竞态失败
  }
}

function clampLocal(event: PointerEvent): { x: number; y: number; vw: number; vh: number } {
  const rect = canvasEl.value?.getBoundingClientRect()
  if (!rect || rect.width <= 0 || rect.height <= 0) {
    return { x: event.clientX, y: event.clientY, vw: window.innerWidth, vh: window.innerHeight }
  }
  return {
    x: Math.max(0, Math.min(rect.width, event.clientX - rect.left)),
    y: Math.max(0, Math.min(rect.height, event.clientY - rect.top)),
    vw: rect.width,
    vh: rect.height
  }
}

function beginDrag(event: PointerEvent): void {
  if (recognizing.value || !session.value) return
  if (event.button === 2) {
    void cancel()
    return
  }
  if (event.button !== 0 || !event.isPrimary) return
  const pos = clampLocal(event)
  drag.value = {
    pointerId: event.pointerId,
    startX: pos.x,
    startY: pos.y,
    currentX: pos.x,
    currentY: pos.y
  }
  canvasEl.value?.setPointerCapture(event.pointerId)
}

function moveDrag(event: PointerEvent): void {
  if (!drag.value || event.pointerId !== drag.value.pointerId || recognizing.value) return
  const pos = clampLocal(event)
  drag.value.currentX = pos.x
  drag.value.currentY = pos.y
}

async function endDrag(event: PointerEvent, cancelled = false): Promise<void> {
  if (!drag.value || event.pointerId !== drag.value.pointerId) return
  if (canvasEl.value?.hasPointerCapture(event.pointerId)) {
    canvasEl.value.releasePointerCapture(event.pointerId)
  }
  const pos = clampLocal(event)
  drag.value.currentX = pos.x
  drag.value.currentY = pos.y
  const rect = boxRect.value
  const info = session.value
  if (cancelled || !rect || !info || rect.width < 8 || rect.height < 8 || pos.vw <= 0 || pos.vh <= 0) {
    drag.value = null
    return
  }

  const selection = {
    x: Math.max(0, Math.min(1, rect.left / pos.vw)),
    y: Math.max(0, Math.min(1, rect.top / pos.vh)),
    width: Math.max(0, Math.min(1, rect.width / pos.vw)),
    height: Math.max(0, Math.min(1, rect.height / pos.vh))
  }
  recognizing.value = true
  errorText.value = ''
  try {
    await invokeHost('run_screen_ocr', {
      sessionId: info.sessionId,
      selection
    })
  } catch (error) {
    recognizing.value = false
    drag.value = null
    errorText.value = String(error)
  }
}

function onKeyDown(event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.preventDefault()
    void cancel()
  }
}

onMounted(async () => {
  window.addEventListener('keydown', onKeyDown)
  if (!isDesktop) {
    session.value = {
      sessionId: 1,
      dataUri: buildBrowserDemoUri(),
      width: 1280,
      height: 720,
      screenX: 0,
      screenY: 0
    }
    return
  }
  try {
    session.value = await invokeHost<CaptureSessionInfo>('get_capture_session')
  } catch (error) {
    errorText.value = String(error)
  }
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKeyDown)
})
</script>

<template>
  <main
    ref="canvasEl"
    class="capture-overlay"
    aria-label="屏幕截图框选区域"
    @pointerdown="beginDrag"
    @pointermove="moveDrag"
    @pointerup="endDrag($event, false)"
    @pointercancel="endDrag($event, true)"
    @contextmenu.prevent="cancel"
  >
    <img v-if="session" class="frozen-screen" :src="session.dataUri" alt="" draggable="false" />
    <div v-if="!boxRect" class="full-mask" aria-hidden="true" />
    <div
      v-if="boxRect"
      class="selection-box"
      :style="{
        left: `${boxRect.left}px`,
        top: `${boxRect.top}px`,
        width: `${boxRect.width}px`,
        height: `${boxRect.height}px`
      }"
    >
      <span v-if="pixelBadge" class="size-badge">{{ pixelBadge }}</span>
    </div>

    <div class="hint-pill" role="status" aria-live="polite">
      <template v-if="recognizing">正在识别框选区域的文字…</template>
      <template v-else-if="errorText">{{ errorText }} · 右键或按 Esc 退出</template>
      <template v-else>拖拉框选文字区域，松手即识字 · 右键或 Esc 取消</template>
    </div>
  </main>
</template>

<style scoped>
.capture-overlay {
  position: relative;
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  cursor: crosshair;
  user-select: none;
  background: #000;
}
.frozen-screen {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: fill;
  pointer-events: none;
}
.full-mask {
  position: absolute;
  inset: 0;
  background: rgb(0 0 0 / .38);
  pointer-events: none;
}
.selection-box {
  position: absolute;
  border: 2px solid #38bdf8;
  box-shadow: 0 0 0 9999px rgb(0 0 0 / .44);
  background: transparent;
  pointer-events: none;
}
.size-badge {
  position: absolute;
  left: 0;
  top: -26px;
  padding: 2px 8px;
  border-radius: 6px;
  background: rgb(15 23 42 / .88);
  color: #f8fafc;
  font-family: var(--font-mono);
  font-size: 11px;
  white-space: nowrap;
  font-variant-numeric: tabular-nums;
}
.hint-pill {
  position: fixed;
  top: 18px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 5;
  padding: 7px 14px;
  border-radius: 999px;
  background: rgb(15 23 42 / .84);
  color: #f8fafc;
  border: 1px solid rgb(255 255 255 / .18);
  font-size: 12.5px;
  white-space: nowrap;
  pointer-events: none;
  box-shadow: 0 6px 20px rgb(0 0 0 / .32);
}
</style>
