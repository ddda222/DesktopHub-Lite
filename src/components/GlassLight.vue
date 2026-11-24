<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { createGlassPass } from '@/features/island/motion/glassRenderer'

const props = withDefaults(defineProps<{
  lightSurface: boolean
  signal?: string
  active?: boolean
  quality?: 'full' | 'low'
  cornerRadius?: number
  palette?: 'cool' | 'liquid' | 'amber' | 'vermilion' | 'lime' | 'chalk' | 'neon' | 'candy'
}>(), { active: true, quality: 'full', palette: 'cool' })
const emit = defineEmits<{ unavailable: [] }>()
const canvas = ref<HTMLCanvasElement | null>(null)
let dispose: (() => void) | undefined

onMounted(() => {
  const element = canvas.value
  const surface = element?.parentElement
  if (!element || !surface) return
  const gl = element.getContext('webgl', {
    alpha: true, antialias: false, premultipliedAlpha: false,
    depth: false, stencil: false, powerPreference: 'low-power'
  })
  if (!gl) { emit('unavailable'); return }

  const media = matchMedia('(prefers-reduced-motion: reduce), (prefers-reduced-transparency: reduce), (prefers-contrast: more), (forced-colors: active)')
  let pass: ReturnType<typeof createGlassPass> = null
  let inView = true
  let disposed = false
  let frame = 0
  let previous = 0
  let latestPaint = -Infinity
  let width = element.clientWidth
  let height = element.clientHeight
  let corner = parseFloat(getComputedStyle(surface).borderTopLeftRadius) || 0
  let resizeTimer: ReturnType<typeof setTimeout> | undefined
  let trim = true
  let pointer: { clientX: number; clientY: number } | null = null
  let pressPending = false
  let targetX = .31, targetY = 1, targetEnergy = .15
  let lightX = targetX, lightY = targetY, energy = targetEnergy
  let tone = Number(props.lightSurface)
  let originX = .5, originY = 1
  let rippleStart = -Infinity
  const enabled = () => !disposed && !!pass && props.active && inView && !document.hidden && !media.matches

  function schedule(): void {
    if (!frame && enabled()) frame = requestAnimationFrame(draw)
  }

  function pause(): void {
    cancelAnimationFrame(frame)
    clearTimeout(resizeTimer)
    frame = previous = 0
    latestPaint = rippleStart = -Infinity
    trim = true
    pressPending = false
    pointer = null
    targetEnergy = energy = .15
  }

  function draw(now: number): void {
    frame = 0
    if (!pass || !enabled() || width <= 0 || height <= 0) return
    const full = props.quality === 'full'
    if (!full && now - latestPaint < 31) { schedule(); return }
    latestPaint = now
    const ratio = Math.min(window.devicePixelRatio || 1, full ? 1.5 : .75)
    const bucket = full ? 128 : 64
    const idealWidth = Math.max(1, Math.ceil(width * ratio / bucket) * bucket)
    const idealHeight = Math.max(1, Math.ceil(height * ratio / bucket) * bucket)
    const nextWidth = trim ? idealWidth : Math.max(idealWidth, element!.width)
    const nextHeight = trim ? idealHeight : Math.max(idealHeight, element!.height)
    if (nextWidth !== element!.width) element!.width = nextWidth
    if (nextHeight !== element!.height) element!.height = nextHeight
    trim = false
    if (pointer) {
      const box = surface!.getBoundingClientRect()
      if (box.width && box.height) {
        targetX = Math.min(1, Math.max(0, (pointer.clientX - box.left) / box.width))
        targetY = 1 - Math.min(1, Math.max(0, (pointer.clientY - box.top) / box.height))
      }
      pointer = null
      if (pressPending) { originX = targetX; originY = targetY; rippleStart = now; pressPending = false }
    }
    const follow = 1 - Math.exp(-Math.min(previous ? now - previous : 16, 64) / 52)
    lightX += (targetX - lightX) * follow
    lightY += (targetY - lightY) * follow
    energy += (targetEnergy - energy) * follow
    tone += (Number(props.lightSurface) - tone) * follow
    const rippleAge = Math.min((now - rippleStart) / 680, 1)
    const cornerNow = props.cornerRadius ?? (parseFloat(getComputedStyle(surface!).borderTopLeftRadius) || corner)
    const paletteMap: Record<string, { base: [number, number, number]; rim: [number, number, number] } | undefined> = {
      liquid: { base: [.48, .82, 1], rim: [.92, .78, 1] },
      amber: { base: [.96, .62, .08], rim: [1, .92, .56] },
      vermilion: { base: [.72, .24, .16], rim: [.92, .48, .36] },
      lime: { base: [.64, .9, .21], rim: [.98, .92, .25] },
      chalk: { base: [.62, .8, 1], rim: [1, 1, 1] },
      neon: { base: [1, .17, .84], rim: [0, .94, 1] },
      candy: { base: [.94, .38, .48], rim: [1, .86, .55] },
    }
    const colors = paletteMap[props.palette ?? 'cool']
    pass.draw({ width, height, corner: cornerNow, lightX, lightY, energy, tone, rippleX: originX, rippleY: originY,
      rippleAge, full, tintBase: colors?.base, tintRim: colors?.rim })
    const remaining = Math.abs(targetX-lightX) + Math.abs(targetY-lightY) + Math.abs(targetEnergy-energy) + Math.abs(Number(props.lightSurface)-tone)
    if (remaining > .005 || rippleAge < 1) {
      previous = now
      schedule()
    } else previous = 0
  }

  const sync = () => { if (enabled()) schedule(); else pause() }
  const follow = (event: PointerEvent) => {
    if (!enabled() || !event.isPrimary) return
    pointer = { clientX: event.clientX, clientY: event.clientY }
    targetEnergy = 1
    schedule()
  }
  const press = (event: PointerEvent) => {
    if (!enabled() || !event.isPrimary || event.button !== 0) return
    if (event.target instanceof Element && event.target.closest('.capsule, .collapse-action')) return
    pressPending = true
    follow(event)
  }
  const leave = () => { targetEnergy = .15; pointer = null; pressPending = false; schedule() }
  const restore = () => {
    if (disposed) return
    pass = createGlassPass(gl)
    if (!pass && !gl.isContextLost()) { emit('unavailable'); return }
    sync()
  }
  const lost = (event: Event) => { event.preventDefault(); pass = null; pause() }
  const resize = new ResizeObserver(([entry]) => {
    if (!entry) return
    width = entry.contentRect.width; height = entry.contentRect.height
    if (!enabled()) return
    if (props.cornerRadius === undefined) corner = parseFloat(getComputedStyle(surface).borderTopLeftRadius) || 0
    clearTimeout(resizeTimer)
    resizeTimer = setTimeout(() => { trim = true; schedule() }, 200)
    schedule()
  })
  const intersection = new IntersectionObserver(entries => {
    inView = entries.at(-1)?.isIntersecting ?? false
    sync()
  })
  const stopActive = watch(() => props.active, sync)
  const stopTone = watch(() => [props.lightSurface, props.cornerRadius, props.palette], schedule)
  const stopQuality = watch(() => props.quality, () => { trim = true; schedule() })
  const stopSignal = watch(() => props.signal, signal => {
    if (!enabled() || !signal || signal === 'idle') return
    originX = .5; originY = 1; rippleStart = performance.now(); schedule()
  })
  resize.observe(element)
  intersection.observe(element)
  media.addEventListener('change', sync)
  document.addEventListener('visibilitychange', sync)
  surface.addEventListener('pointermove', follow, { passive: true })
  surface.addEventListener('pointerdown', press, { passive: true, capture: true })
  surface.addEventListener('pointerleave', leave)
  element.addEventListener('webglcontextlost', lost)
  element.addEventListener('webglcontextrestored', restore)
  dispose = () => {
    disposed = true
    pause()
    resize.disconnect(); intersection.disconnect()
    stopActive(); stopTone(); stopQuality(); stopSignal()
    media.removeEventListener('change', sync)
    document.removeEventListener('visibilitychange', sync)
    surface.removeEventListener('pointermove', follow)
    surface.removeEventListener('pointerdown', press, true)
    surface.removeEventListener('pointerleave', leave)
    element.removeEventListener('webglcontextlost', lost)
    element.removeEventListener('webglcontextrestored', restore)
    if (!gl.isContextLost()) {
      pass?.dispose()
      gl.getExtension('WEBGL_lose_context')?.loseContext()
    }
    pass = null
  }
  restore()
})
onBeforeUnmount(() => dispose?.())
</script>

<template>
  <canvas v-show="active" ref="canvas" class="glass-light" aria-hidden="true" />
</template>

<style scoped>
.glass-light {
  position: absolute;
  z-index: 2;
  inset: 0;
  display: block;
  width: 100%;
  height: 100%;
  pointer-events: none;
}
@media (prefers-reduced-motion: reduce), (prefers-reduced-transparency: reduce), (prefers-contrast: more), (forced-colors: active) {
  .glass-light { display: none; }
}
</style>
