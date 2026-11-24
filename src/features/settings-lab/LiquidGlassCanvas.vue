<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { LiquidGlassEngine, type LiquidSceneState } from './liquidShader'

/** 全屏液态玻璃与光影流体画布（WebGL GLSL Raymarching），不支持 WebGL 时自动柔美降级。 */
const props = defineProps<{
  sceneState: LiquidSceneState
}>()

const canvasEl = ref<HTMLCanvasElement | null>(null)
const fallbackMode = ref(false)
let engine: LiquidGlassEngine | null = null
let resizeObserver: ResizeObserver | null = null

function onPointerMove(event: PointerEvent): void {
  engine?.setPointer(event.clientX, event.clientY)
}

function pulseFromEvent(event?: Event, strength = 0.72): void {
  if (!engine) return
  if (event && 'clientX' in event && 'clientY' in event) {
    const pointer = event as MouseEvent
    if (pointer.clientX || pointer.clientY) {
      engine.emitRipple(pointer.clientX, pointer.clientY, strength)
      return
    }
  }
  engine.emitRipple(window.innerWidth * 0.35, window.innerHeight * 0.5, strength)
}

function playMotionDemo(): void {
  engine?.playMotionDemo()
}

function triggerBeam(): void {
  engine?.triggerBeam()
}

defineExpose({
  pulseFromEvent,
  playMotionDemo,
  triggerBeam
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
  try {
    engine = new LiquidGlassEngine(canvasEl.value, props.sceneState)
    resizeObserver = new ResizeObserver(() => engine?.resize())
    resizeObserver.observe(canvasEl.value)
    window.addEventListener('pointermove', onPointerMove, { passive: true })
  } catch (err) {
    console.warn('[LiquidGlassCanvas] WebGL 不可用，已切为柔和流体降级背景：', err)
    fallbackMode.value = true
  }
})

onBeforeUnmount(() => {
  window.removeEventListener('pointermove', onPointerMove)
  resizeObserver?.disconnect()
  engine?.dispose()
  engine = null
})
</script>

<template>
  <div v-if="fallbackMode" class="liquid-fallback" aria-hidden="true" />
  <canvas v-else ref="canvasEl" class="liquid-canvas" aria-hidden="true" />
</template>

<style scoped>
.liquid-canvas,
.liquid-fallback {
  position: fixed;
  inset: 0;
  width: 100%;
  height: 100%;
  display: block;
  pointer-events: none;
  z-index: 0;
}

.liquid-fallback {
  background:
    radial-gradient(circle at 67% 50%, rgba(96, 165, 250, 0.32), transparent 46%),
    linear-gradient(135deg, #071020 0%, #101d35 100%);
}
</style>
