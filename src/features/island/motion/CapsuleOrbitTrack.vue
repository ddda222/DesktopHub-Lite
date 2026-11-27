<script setup lang="ts">
/**
 * 07 · MotionPath 灵动岛边缘任务巡航光粒（逐帧锁定实时边界）：
 * - 通过 gsap.ticker 每帧读取宿主 `.island` 实测宽高与实时 border-radius，
 *   配合 MotionPathPlugin.getRawPath / cacheRawPathMeasurements / getPositionOnPath，
 *   保证展开、切换、收起过程光粒与边缘轨迹始终贴紧外轮廓，绝不内缩漂移或裁切丢失。
 */
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { gsap, MotionPathPlugin } from '@/lib/gsapMotion'

const props = withDefaults(defineProps<{
  width: number
  height: number
  cornerRadius: number
  active: boolean
  tone?: 'accent' | 'success' | 'error' | 'warning'
  speed?: number
  reduced?: boolean
}>(), {
  tone: 'accent',
  speed: 1,
  reduced: false
})

const overlayEl = ref<HTMLElement | null>(null)
const svgEl = ref<SVGSVGElement | null>(null)
const railPathEl = ref<SVGPathElement | null>(null)
const glint1El = ref<HTMLElement | null>(null)
const glint2El = ref<HTMLElement | null>(null)

let tickerBound = false
let phase1 = 0
let lastTickMs = 0
let cachedKey = ''
let cachedRawPath: ReturnType<typeof MotionPathPlugin.getRawPath> | null = null

const PATH_INSET = 1.75

function buildRoundedRectPath(w: number, h: number, cornerR: number): string {
  const innerW = Math.max(12, w - PATH_INSET * 2)
  const innerH = Math.max(6, h - PATH_INSET * 2)
  const r = Math.max(1.5, Math.min(cornerR - PATH_INSET * 0.4, innerW / 2, innerH / 2))
  const x0 = PATH_INSET
  const y0 = PATH_INSET
  const x1 = x0 + innerW
  const y1 = y0 + innerH
  return [
    `M ${x0 + r} ${y0}`,
    `H ${x1 - r}`,
    `A ${r} ${r} 0 0 1 ${x1} ${y0 + r}`,
    `V ${y1 - r}`,
    `A ${r} ${r} 0 0 1 ${x1 - r} ${y1}`,
    `H ${x0 + r}`,
    `A ${r} ${r} 0 0 1 ${x0} ${y1 - r}`,
    `V ${y0 + r}`,
    `A ${r} ${r} 0 0 1 ${x0 + r} ${y0}`,
    'Z'
  ].join(' ')
}

function readLiveShellBox(): { w: number; h: number; r: number } {
  const host = overlayEl.value?.parentElement
  if (!host) {
    return {
      w: Math.max(40, props.width),
      h: Math.max(12, props.height),
      r: Math.max(2, props.cornerRadius)
    }
  }
  const style = getComputedStyle(host)
  const parsedW = parseFloat(style.width)
  const parsedH = parseFloat(style.height)
  const parsedR = parseFloat(style.borderTopLeftRadius)
  const w = Math.max(40, Number.isFinite(parsedW) && parsedW > 0 ? parsedW : (host.clientWidth || props.width))
  const h = Math.max(12, Number.isFinite(parsedH) && parsedH > 0 ? parsedH : (host.clientHeight || props.height))
  const r = Number.isFinite(parsedR) ? parsedR : props.cornerRadius
  return { w, h, r }
}

function placeGlint(el: HTMLElement, rawPath: ReturnType<typeof MotionPathPlugin.getRawPath>, progress: number): void {
  const pt = MotionPathPlugin.getPositionOnPath(rawPath, ((progress % 1) + 1) % 1, true) as {
    x: number
    y: number
    angle: number
  }
  el.style.transform = `translate3d(${pt.x.toFixed(2)}px, ${pt.y.toFixed(2)}px, 0) translate(-50%, -50%) rotate(${pt.angle.toFixed(2)}deg)`
}

function tickFrame(): void {
  const d1 = glint1El.value
  const d2 = glint2El.value
  if (!d1 || !d2 || !props.active || props.reduced) return

  const now = performance.now()
  const dtSec = lastTickMs > 0 ? Math.min(0.064, Math.max(0.001, (now - lastTickMs) / 1000)) : 1 / 60
  lastTickMs = now

  const { w, h, r } = readLiveShellBox()
  const quantKey = `${w.toFixed(1)}:${h.toFixed(1)}:${r.toFixed(1)}`
  if (quantKey !== cachedKey || !cachedRawPath) {
    cachedKey = quantKey
    const pathD = buildRoundedRectPath(w, h, r)
    svgEl.value?.setAttribute('viewBox', `0 0 ${w.toFixed(2)} ${h.toFixed(2)}`)
    railPathEl.value?.setAttribute('d', pathD)
    cachedRawPath = MotionPathPlugin.getRawPath(pathD)
    MotionPathPlugin.cacheRawPathMeasurements(cachedRawPath)
  }

  const baseDur = Math.max(2.1, Math.min(4.6, (w + h) / 140))
  const speedDur = baseDur / Math.max(0.5, props.speed)
  phase1 = (phase1 + dtSec / speedDur) % 1
  const phase2 = (phase1 + 0.5) % 1

  placeGlint(d1, cachedRawPath, phase1)
  placeGlint(d2, cachedRawPath, phase2)
}

function startOrbit(): void {
  if (!props.active || props.reduced) {
    stopOrbit()
    return
  }
  if (!tickerBound) {
    lastTickMs = performance.now()
    gsap.ticker.add(tickFrame)
    tickerBound = true
  }
  tickFrame()
}

function stopOrbit(): void {
  if (tickerBound) {
    gsap.ticker.remove(tickFrame)
    tickerBound = false
  }
  lastTickMs = 0
}

watch(() => [props.active, props.reduced], startOrbit, { flush: 'post' })
onMounted(startOrbit)
onBeforeUnmount(stopOrbit)
</script>

<template>
  <div
    v-show="active && !reduced"
    ref="overlayEl"
    class="orbit-overlay"
    :class="`tone-${tone}`"
    aria-hidden="true"
  >
    <svg ref="svgEl" class="orbit-svg" preserveAspectRatio="none">
      <path ref="railPathEl" class="orbit-rail" fill="none" />
    </svg>
    <span ref="glint1El" class="orbit-glint is-primary" />
    <span ref="glint2El" class="orbit-glint is-secondary" />
  </div>
</template>

<style scoped>
.orbit-overlay {
  position: absolute;
  inset: 0;
  z-index: 4;
  pointer-events: none;
  overflow: hidden;
  border-radius: inherit;
  --orbit-c1: var(--accent);
  --orbit-c2: color-mix(in srgb, var(--accent) 65%, #38bdf8);
}

.orbit-overlay.tone-success {
  --orbit-c1: #10b981;
  --orbit-c2: #34d399;
}

.orbit-overlay.tone-warning {
  --orbit-c1: #f59e0b;
  --orbit-c2: #fbbf24;
}

.orbit-overlay.tone-error {
  --orbit-c1: #ef4444;
  --orbit-c2: #f87171;
}

.orbit-svg {
  display: block;
  width: 100%;
  height: 100%;
}

.orbit-rail {
  stroke: color-mix(in srgb, var(--orbit-c1) 24%, transparent);
  stroke-width: 1.25px;
}

.orbit-glint {
  position: absolute;
  top: 0;
  left: 0;
  width: 18px;
  height: 3.5px;
  border-radius: 99px;
  transform-origin: center center;
  will-change: transform;
  pointer-events: none;
}

.orbit-glint.is-primary {
  background: linear-gradient(90deg, transparent, var(--orbit-c1), #fff);
  box-shadow: 0 0 10px var(--orbit-c1);
}

.orbit-glint.is-secondary {
  width: 12px;
  opacity: 0.82;
  background: linear-gradient(90deg, transparent, var(--orbit-c2), #fff);
  box-shadow: 0 0 7px var(--orbit-c2);
}
</style>
