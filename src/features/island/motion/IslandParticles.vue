<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { IslandCue } from './useIslandEffects'
import { createMotes, FLOW_HEIGHT, flowDuration, paintFlow, type FlowPointer } from './particleFlow'

const props = defineProps<{ cue: IslandCue | null; active: boolean; dark: boolean; quality: 'full' | 'low' }>()
const canvas = ref<HTMLCanvasElement | null>(null)
let ctx: CanvasRenderingContext2D | null = null
let frame = 0
let lastSequence: number | null = null
let interaction: AbortController | null = null
let observer: ResizeObserver | null = null
let width = 1

function stop(): void {
  cancelAnimationFrame(frame)
  frame = 0
  interaction?.abort()
  interaction = null
  observer?.disconnect()
  observer = null
  if (ctx && canvas.value) {
    ctx.setTransform(1, 0, 0, 1, 0, 0)
    ctx.clearRect(0, 0, canvas.value.width, canvas.value.height)
  }
}

function start(cue: IslandCue | null): void {
  const surface = canvas.value
  const host = surface?.parentElement
  if (!cue || !surface || !host || !props.active || document.hidden || cue.sequence === lastSequence) return
  stop()
  lastSequence = cue.sequence
  const context = ctx ??= surface.getContext('2d', { alpha: true })
  if (!context) return
  const low = props.quality === 'low'
  const density = low ? 1 : Math.min(window.devicePixelRatio || 1, 1.5)
  const bufferWidth = low ? 384 : 768
  const bufferHeight = Math.ceil(FLOW_HEIGHT * density)
  if (surface.width !== bufferWidth) surface.width = bufferWidth
  if (surface.height !== bufferHeight) surface.height = bufferHeight
  width = Math.max(1, surface.clientWidth)
  observer = new ResizeObserver(entries => { width = Math.max(1, entries[0].contentRect.width) })
  observer.observe(surface)

  const motes = createMotes(cue.kind, low)
  const started = performance.now()
  let previous = started
  let pointer: FlowPointer | null = null
  let event: PointerEvent | null = null
  interaction = new AbortController()
  const options = { passive: true, capture: true, signal: interaction.signal }
  const track = (next: PointerEvent) => { if (next.isPrimary) event = next }
  const leave = () => { event = null; pointer = null }
  host.addEventListener('pointermove', track, options)
  host.addEventListener('pointerdown', track, options)
  host.addEventListener('pointerup', track, options)
  host.addEventListener('pointerleave', leave, { passive: true, signal: options.signal })
  host.addEventListener('pointercancel', leave, options)

  function render(now: number): void {
    const progress = (now - started) / flowDuration[cue!.kind]
    if (!props.active || document.hidden || progress >= 1) { stop(); return }
    if (!low || now - previous >= 30 || previous === started) {
      if (event) {
        const box = surface!.getBoundingClientRect()
        pointer = { x: event.clientX - box.left, y: event.clientY - box.top, pressed: Boolean(event.buttons & 1) }
        event = null
      }
      context!.setTransform(bufferWidth / width, 0, 0, density, 0, 0)
      paintFlow(context!, motes, cue!.kind, Math.max(0, progress), width, Math.min(.05, (now - previous) / 1000), pointer, props.dark)
      previous = now
    }
    frame = requestAnimationFrame(render)
  }
  frame = requestAnimationFrame(render)
}

function visibility(): void { if (document.hidden) stop() }
watch(() => props.cue, cue => { if (cue) start(cue); else stop() }, { flush: 'post' })
watch(() => props.active, enabled => { if (!enabled) stop() }, { flush: 'sync' })
onMounted(() => {
  document.addEventListener('visibilitychange', visibility)
  start(props.cue)
})
onBeforeUnmount(() => {
  document.removeEventListener('visibilitychange', visibility)
  stop()
  if (canvas.value) { canvas.value.width = 1; canvas.value.height = 1 }
  ctx = null
})
</script>

<template>
  <canvas ref="canvas" class="island-particles" width="1" height="1" aria-hidden="true" />
</template>

<style scoped>
.island-particles {
  position: absolute;
  inset: 0 0 auto;
  z-index: 2;
  width: 100%;
  height: 22px;
  pointer-events: none;
}
</style>
