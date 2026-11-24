<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Reactive3DEngine, type Reactive3DState } from './reactive3d'

/** 全屏 3D 互动背景画布：直接绑定所有设置状态，并暴露按钮脉冲与机关触发接口。 */
const props = defineProps<{
  sceneState: Reactive3DState
}>()

const canvasEl = ref<HTMLCanvasElement | null>(null)
let engine: Reactive3DEngine | null = null
let resizeObserver: ResizeObserver | null = null

function onPointerMove(event: PointerEvent): void {
  engine?.setPointer(event.clientX, event.clientY)
}

function pulseFromEvent(event?: Event, label = ''): void {
  if (!engine) return
  if (event && 'clientX' in event && 'clientY' in event) {
    const pointer = event as MouseEvent
    if (pointer.clientX || pointer.clientY) {
      engine.emitButtonPulse(pointer.clientX, pointer.clientY, label)
      return
    }
  }
  // 键盘触发时，从左侧面板中段发射脉冲
  engine.emitButtonPulse(window.innerWidth * 0.25, window.innerHeight * 0.45, label)
}

function playMotionDemo(): void {
  engine?.playMotionDemo()
}

function pressKeycaps(caps: string[]): void {
  engine?.pressKeycaps(caps)
}

function triggerEvent(kind: 'translate-beam' | 'clipboard-clear' | 'radar-ping' | 'copied'): void {
  engine?.triggerEvent(kind)
}

defineExpose({
  pulseFromEvent,
  playMotionDemo,
  pressKeycaps,
  triggerEvent
})

watch(
  () => props.sceneState,
  next => {
    engine?.updateState(next)
  },
  { deep: true }
)

onMounted(() => {
  if (!canvasEl.value) return
  engine = new Reactive3DEngine(canvasEl.value, props.sceneState)
  resizeObserver = new ResizeObserver(() => engine?.resize())
  resizeObserver.observe(canvasEl.value)
  window.addEventListener('pointermove', onPointerMove, { passive: true })
})

onBeforeUnmount(() => {
  window.removeEventListener('pointermove', onPointerMove)
  resizeObserver?.disconnect()
  engine?.dispose()
  engine = null
})
</script>

<template>
  <canvas ref="canvasEl" class="reactive-3d-canvas" aria-hidden="true" />
</template>

<style scoped>
.reactive-3d-canvas {
  position: fixed;
  inset: 0;
  width: 100%;
  height: 100%;
  display: block;
  pointer-events: none;
  z-index: 0;
}
</style>
