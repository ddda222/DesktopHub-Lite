<script setup lang="ts">
/**
 * 07 · MotionPath 灵动岛边缘任务巡航光粒（ResizeObserver 缓存轮廓，静止态零强制回流）：
 * - 通过 ResizeObserver 订阅宿主 `.island` 尺寸变化并缓存宽高与实时圆角，
 *   静止态 `tickFrame` 仅消费缓存路径并更新 transform，零 `getComputedStyle` DOM 读操作；
 * - 展开、切换、收起过程由 ResizeObserver 实时刷新路径，保证光粒紧贴外轮廓不漂移。
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
let hostObserver: ResizeObserver | null = null
let cachedBox = {
  w: Math.max(40, props.width),
  h: Math.max(12, props.height),
  r: Math.max(2, props.cornerRadius)
}

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

/** 从宿主内联样式或已知属性读取实时圆角，避免在帧循环里触发 getComputedStyle 强制回流。 */
function readInlineCornerRadius(host: HTMLElement | null): number {
  if (host) {
    const inlineR = parseFloat(host.style.borderRadius || host.style.borderTopLeftRadius)
    if (Number.isFinite(inlineR) && inlineR >= 0) return inlineR
  }
  return Math.max(2, props.cornerRadius)
}

function syncCachedBoxFromHost(useComputedFallback = false): void {
  const host = overlayEl.value?.parentElement
  if (!host) {
    cachedBox = {
      w: Math.max(40, props.width),
      h: Math.max(12, props.height),
      r: Math.max(2, props.cornerRadius)
    }
    return
  }
  let r = readInlineCornerRadius(host)
  if (useComputedFallback && !host.style.borderRadius && !host.style.borderTopLeftRadius) {
    const parsedR = parseFloat(getComputedStyle(host).borderTopLeftRadius)
    if (Number.isFinite(parsedR) && parsedR >= 0) r = parsedR
  }
  const w = Math.max(40, host.clientWidth || props.width)
  const h = Math.max(12, host.clientHeight || props.height)
  cachedBox = { w, h, r }
}

function attachHostObserver(): void {
  hostObserver?.disconnect()
  hostObserver = null
  const host = overlayEl.value?.parentElement
  if (!host || typeof ResizeObserver === 'undefined') return
  hostObserver = new ResizeObserver((entries) => {
    const entry = entries[entries.length - 1]
    if (!entry) return
    const borderBox = Array.isArray(entry.borderBoxSize) ? entry.borderBoxSize[0] : entry.borderBoxSize
    const nextW = borderBox?.inlineSize || entry.contentRect.width || props.width
    const nextH = borderBox?.blockSize || entry.contentRect.height || props.height
    cachedBox = {
      w: Math.max(40, nextW),
      h: Math.max(12, nextH),
      r: readInlineCornerRadius(host)
    }
  })
  hostObserver.observe(host)
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

  const { w, h, r } = cachedBox
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
  syncCachedBoxFromHost(true)
  if (!hostObserver) attachHostObserver()
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
  hostObserver?.disconnect()
  hostObserver = null
  lastTickMs = 0
}

watch(() => [props.width, props.height, props.cornerRadius], () => {
  cachedBox = {
    w: cachedBox.w || Math.max(40, props.width),
    h: cachedBox.h || Math.max(12, props.height),
    r: readInlineCornerRadius(overlayEl.value?.parentElement ?? null)
  }
})
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
  --orbit-c2: var(--accent-hover);
}

.orbit-overlay.tone-success {
  --orbit-c1: var(--success);
  --orbit-c2: color-mix(in srgb, var(--success) 72%, #fff);
}

.orbit-overlay.tone-warning {
  --orbit-c1: var(--warning);
  --orbit-c2: color-mix(in srgb, var(--warning) 72%, #fff);
}

.orbit-overlay.tone-error {
  --orbit-c1: var(--danger);
  --orbit-c2: color-mix(in srgb, var(--danger) 72%, #fff);
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
