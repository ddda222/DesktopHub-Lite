<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, useId, watch } from 'vue'
import { gsap } from '@/lib/gsapMotion'

export type IslandVariant = 'standard' | 'telemetry' | 'mecha' | 'pixel' | 'blueprint' | 'neon'

/** 只有标准圆角底座在几何运动时交由 CSS 图层隐藏 SVG 轮廓；其余器物与仪表主题保持逐帧贴合。 */
const SUSPENDED_VARIANTS = new Set<IslandVariant>(['standard'])

const props = withDefaults(defineProps<{
  width: number
  height: number
  shape: 'line' | 'compact' | 'expanded'
  variant?: IslandVariant
  moving?: boolean
}>(), { variant: 'standard', moving: false })

const suspendDuringMotion = computed(() => props.moving && SUSPENDED_VARIANTS.has(props.variant))

// 外壳的宽和高在动画期间按帧跟踪实际 DOM 尺寸，使仪表准星与霓虹灯管平滑跟随缩放。
const svg = ref<SVGSVGElement | null>(null)
const reticlesEl = ref<SVGPathElement | null>(null)
const neonTubeEl = ref<SVGPathElement | null>(null)
const drawnSize = shallowRef({ width: props.width, height: props.height })
let resize: ResizeObserver | undefined
let pendingWidth = props.width
let pendingHeight = props.height
let syncRaf = 0

function isMotionReduced(): boolean {
  return document.documentElement.dataset.reducedMotion === '1' ||
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

function flushSize(): void {
  syncRaf = 0
  if (props.moving && SUSPENDED_VARIANTS.has(props.variant)) return
  const threshold = props.moving ? 0.5 : 0.25
  if (
    Math.abs(pendingWidth - drawnSize.value.width) >= threshold ||
    Math.abs(pendingHeight - drawnSize.value.height) >= threshold
  ) {
    drawnSize.value = { width: pendingWidth, height: pendingHeight }
  }
}

watch(
  () => props.moving,
  isMoving => {
    if (!isMoving && svg.value) {
      const box = svg.value.getBoundingClientRect()
      if (box.width > 0 && box.height > 0) {
        pendingWidth = box.width
        pendingHeight = box.height
        drawnSize.value = { width: box.width, height: box.height }
      }
    }
  },
  { flush: 'post' }
)

watch(
  () => props.shape,
  (nextShape, prevShape) => {
    if (nextShape === prevShape || nextShape === 'line' || prevShape === 'line') return
    if (isMotionReduced()) return
    // 展开与收起过渡时为星舰准星与霓虹内环增加顺滑呼吸流光，避免硬切换感。
    if (props.variant === 'telemetry' && reticlesEl.value) {
      gsap.fromTo(
        reticlesEl.value,
        { opacity: 0.48 },
        { opacity: 0.86, duration: 0.34, ease: 'power2.out', overwrite: 'auto' }
      )
    } else if (props.variant === 'neon' && neonTubeEl.value) {
      gsap.fromTo(
        neonTubeEl.value,
        { strokeDashoffset: nextShape === 'expanded' ? 24 : -24, opacity: 0.48 },
        { strokeDashoffset: 0, opacity: 0.76, duration: 0.38, ease: 'power2.out', overwrite: 'auto' }
      )
    }
  },
  { flush: 'post' }
)

onMounted(() => {
  if (!svg.value) return
  resize = new ResizeObserver(([entry]) => {
    if (!entry) return
    pendingWidth = entry.contentRect.width
    pendingHeight = entry.contentRect.height
    if (props.moving && SUSPENDED_VARIANTS.has(props.variant)) return
    if (!syncRaf) syncRaf = requestAnimationFrame(flushSize)
  })
  resize.observe(svg.value)
})
onBeforeUnmount(() => {
  resize?.disconnect()
  cancelAnimationFrame(syncRaf)
  if (reticlesEl.value) gsap.killTweensOf(reticlesEl.value)
  if (neonTubeEl.value) gsap.killTweensOf(neonTubeEl.value)
})

const uid = useId().replace(/[^a-z0-9-]/gi, '')
const gradient = 'island-hem-' + uid
const gridPattern = 'island-grid-' + uid
const neonGradient = 'island-neon-' + uid

/** 圆角矩形轮廓；霓虹灯管等圆角样式共用，半径按可用尺寸收紧。 */
function roundedRect(x: number, y: number, w: number, h: number, r: number): string {
  const rr = Math.max(0, Math.min(r, w / 2, h / 2))
  return `M ${x + rr} ${y} H ${x + w - rr} A ${rr} ${rr} 0 0 1 ${x + w} ${y + rr} V ${y + h - rr} A ${rr} ${rr} 0 0 1 ${x + w - rr} ${y + h} H ${x + rr} A ${rr} ${rr} 0 0 1 ${x} ${y + h - rr} V ${y + rr} A ${rr} ${rr} 0 0 1 ${x + rr} ${y} Z`
}

const contours = computed(() => {
  const w = Math.max(4, drawnSize.value.width), h = Math.max(4, drawnSize.value.height)
  const expanded = props.shape === 'expanded', line = props.shape === 'line'
  const v = props.variant
  const midY = h / 2
  // 基于实际高度平滑插值 0..1 展开进度，消除展开/收起临界高度的边角参数阶跃。
  const expandRatio = Math.max(0, Math.min(1, (h - 34) / 86))

  if (v === 'blueprint') {
    const face = roundedRect(.75, .75, w - 1.5, h - 1.5, line ? 1 : 2)
    if (line) {
      return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: '', ruler: `M 4 ${midY} H ${Math.max(4, w - 4)}` }
    }
    // 四角套准十字
    const i = expanded ? 7 : 5, a = expanded ? 4 : 2.5
    const cross = (x: number, y: number) => `M ${x - a} ${y} H ${x + a} M ${x} ${y - a} V ${y + a} `
    const marks = cross(i, i) + cross(w - i, i) + cross(i, h - i) + cross(w - i, h - i)
    let ruler = ''
    if (expanded) {
      const y = h - 8, x1 = 16, x2 = Math.max(x1 + 20, w - 16), gap = 34, cx = w / 2
      ruler = `M ${x1} ${y - 4} V ${y + 4} M ${x2} ${y - 4} V ${y + 4} ` +
        `M ${x1} ${y} H ${cx - gap} M ${cx + gap} ${y} H ${x2} ` +
        `M ${x1 + 5} ${y - 2.5} L ${x1} ${y} L ${x1 + 5} ${y + 2.5} M ${x2 - 5} ${y - 2.5} L ${x2} ${y} L ${x2 - 5} ${y + 2.5}`
    } else {
      const parts: string[] = []
      for (let x = 14, n = 0; x < w - 13; x += 6, n++) parts.push(`M ${x} ${h - 1.5} V ${h - (n % 5 === 0 ? 5.5 : 3.5)}`)
      ruler = parts.join(' ')
    }
    return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: marks, ruler }
  }

  if (v === 'neon') {
    const r = Math.min(line ? 3 : 24 - 4 * expandRatio, h / 2, w / 2)
    const face = roundedRect(1, 1, w - 2, h - 2, r)
    if (line) {
      return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: '', ruler: `M 6 ${midY} H ${Math.max(6, w - 6)}` }
    }
    const inset = 4 + expandRatio
    const tube = roundedRect(inset, inset, Math.max(2, w - inset * 2), Math.max(2, h - inset * 2), Math.max(0, r - inset + 1))
    return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: tube, ruler: '' }
  }

  if (v === 'mecha') {
    // 基于实际高度平滑插值倒角尺寸，避免展开/收起临界高度切角阶跃。
    const cut = Math.min(line ? 2 : 8 + 5 * expandRatio, h / 2 - .5, w / 2 - .5)
    const face = `M ${cut} .75 H ${w - cut} L ${w - .75} ${cut} V ${h - cut} L ${w - cut} ${h - .75} H ${cut} L .75 ${h - cut} V ${cut} Z`
    const sheen = `M ${cut + 2} 1.8 H ${Math.max(cut + 2, w - cut - 2)}`
    const hem = `M ${cut + 6} ${h - 1.5} H ${Math.max(cut + 6, w - cut - 6)}`
    const rails = `M 4.5 18 V ${Math.max(18, h - 18)} M ${w - 4.5} 18 V ${Math.max(18, h - 18)}`
    const seam = `M 6 ${midY} H ${Math.max(6, w / 2 - 6)} M ${w / 2 - 3} ${midY - 2.2} V ${midY + 2.2} M ${w / 2 + 3} ${midY - 2.2} V ${midY + 2.2} M ${Math.min(w - 6, w / 2 + 6)} ${midY} H ${w - 6}`
    return { face: `path('${face}')`, sheen: `path('${sheen}')`, hem: `path('${hem}')`, extra: rails, ruler: seam }
  }

  if (v === 'pixel') {
    const s = line ? 1.5 : 3
    const x1 = 1, y1 = 1, x2 = w - 1, y2 = h - 1
    const face = `M ${x1 + 2 * s} ${y1} H ${x2 - 2 * s} V ${y1 + s} H ${x2 - s} V ${y1 + 2 * s} H ${x2} V ${y2 - 2 * s} H ${x2 - s} V ${y2 - s} H ${x2 - 2 * s} V ${y2} H ${x1 + 2 * s} V ${y2 - s} H ${x1 + s} V ${y2 - 2 * s} H ${x1} V ${y1 + 2 * s} H ${x1 + s} V ${y1 + s} H ${x1 + 2 * s} Z`
    const sheen = `M ${x1 + 2 * s} 2.5 H ${x2 - 2 * s}`
    const hem = `M ${x1 + 2 * s} ${y2 - 1.5} H ${x2 - 2 * s}`
    const slots = `M ${w / 2 - 12} ${midY} H ${w / 2 - 5} M ${w / 2 - 2} ${midY} H ${w / 2 + 5} M ${w / 2 + 8} ${midY} H ${w / 2 + 15}`
    return { face: `path('${face}')`, sheen: `path('${sheen}')`, hem: `path('${hem}')`, extra: '', ruler: slots }
  }

  // standard / telemetry
  const baseRadius = v === 'telemetry'
    ? (line ? 4 : 10 + 2 * expandRatio)
    : (expanded ? 18 : line ? 5 : 24)
  const radius = Math.min(baseRadius, h / 2, w / 2)
  const right = Math.max(radius, w - radius)
  const bottomY = Math.max(radius, h - radius)
  const low = h - .5
  const face = `M ${radius} .5 H ${right} Q ${w - .5} .5 ${w - .5} ${radius} V ${bottomY} Q ${w - .5} ${low} ${right} ${low} H ${radius} Q .5 ${low} .5 ${bottomY} V ${radius} Q .5 .5 ${radius} .5 Z`
  const sheen = `M ${radius} 1.35 H ${right}`
  const hem = `M ${Math.max(radius, w * .16)} ${low - .2} H ${Math.min(right, w * .84)}`

  const inset = 4 + 1.5 * expandRatio
  const arm = 5 + 2.5 * expandRatio
  const rx1 = inset, ry1 = inset, rx2 = Math.max(inset + arm, w - inset), ry2 = Math.max(inset + arm, h - inset)
  const reticles = `M ${rx1} ${ry1 + arm} V ${ry1} H ${rx1 + arm} M ${rx2 - arm} ${ry1} H ${rx2} V ${ry1 + arm} M ${rx1} ${ry2 - arm} V ${ry2} H ${rx1 + arm} M ${rx2 - arm} ${ry2} H ${rx2} V ${ry2 - arm}`
  const ruler = `M 9 ${midY - 2} V ${midY + 2} M 14 ${midY} H ${Math.max(14, w - 14)} M ${Math.max(14, w - 9)} ${midY - 2} V ${midY + 2}`
  return { face: `path('${face}')`, sheen: `path('${sheen}')`, hem: `path('${hem}')`, extra: reticles, ruler }
})
</script>

<template>
  <svg ref="svg" class="liquid-skin" :class="[`variant-${props.variant}`, { 'is-suspended': suspendDuringMotion }]"
    :viewBox="`0 0 ${drawnSize.width} ${drawnSize.height}`" preserveAspectRatio="none" aria-hidden="true">
    <defs>
      <linearGradient :id="gradient">
        <stop stop-color="var(--island-hem-a)" stop-opacity="0"/>
        <stop offset=".32" stop-color="var(--island-hem-a)" stop-opacity=".45"/>
        <stop offset=".68" stop-color="var(--island-hem-b)" stop-opacity=".75"/>
        <stop offset="1" stop-color="var(--island-hem-a)" stop-opacity="0"/>
      </linearGradient>
      <pattern v-if="props.variant === 'blueprint'" :id="gridPattern" width="24" height="24" patternUnits="userSpaceOnUse">
        <path d="M 8 0 V 24 M 16 0 V 24 M 0 8 H 24 M 0 16 H 24" stroke="rgba(186, 220, 255, 0.08)" stroke-width="0.6" />
        <path d="M 24 0 V 24 M 0 24 H 24" stroke="rgba(186, 220, 255, 0.19)" stroke-width="0.85" />
      </pattern>
      <linearGradient v-if="props.variant === 'neon'" :id="neonGradient" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#ff2bd6" />
        <stop offset="52%" stop-color="#a855f7" />
        <stop offset="100%" stop-color="#00f0ff" />
      </linearGradient>
    </defs>
    <path class="skin"
      :stroke="props.variant === 'neon' ? `url(#${neonGradient})` : undefined"
      :style="{ d: contours.face }" />
    <path v-if="props.variant === 'blueprint' && props.shape !== 'line'" class="bp-grid" :fill="`url(#${gridPattern})`" :style="{ d: contours.face }" />
    <path v-if="props.shape !== 'line' && props.variant !== 'blueprint' && props.variant !== 'neon'" class="sheen" :style="{ d: contours.sheen }" />
    <path v-if="props.shape !== 'line' && props.variant !== 'blueprint' && props.variant !== 'neon'" class="hem" :stroke="`url(#${gradient})`" :style="{ d: contours.hem }" />

    <!-- 星舰仪表准星与标尺 -->
    <path v-if="props.variant === 'telemetry' && props.shape !== 'line'" ref="reticlesEl" class="reticles" :d="contours.extra" />
    <path v-if="props.variant === 'telemetry' && props.shape === 'line'" class="ruler" :d="contours.ruler" />

    <!-- 琥珀重工导轨与合拢缝 -->
    <path v-if="props.variant === 'mecha' && props.shape === 'expanded'" class="mecha-rails" :d="contours.extra" />
    <path v-if="props.variant === 'mecha' && props.shape === 'line'" class="mecha-seam" :d="contours.ruler" />

    <!-- 像素掌机卡带细线指示灯 -->
    <template v-if="props.variant === 'pixel' && props.shape === 'line'">
      <rect x="6" :y="drawnSize.height / 2 - 2" width="5" height="4" fill="#fb7185" />
      <path class="pixel-slots" :d="contours.ruler" />
      <rect :x="Math.max(16, drawnSize.width - 11)" :y="drawnSize.height / 2 - 2" width="5" height="4" fill="#a3e635" />
    </template>

    <!-- 蓝晒图纸套准十字、比例尺与展开尺寸标注 -->
    <path v-if="props.variant === 'blueprint' && props.shape !== 'line'" class="bp-marks" :d="contours.extra" />
    <path v-if="props.variant === 'blueprint'" class="bp-ruler" :class="{ 'is-line': props.shape === 'line' }" :d="contours.ruler" />
    <text v-if="props.variant === 'blueprint' && props.shape === 'expanded'" :x="drawnSize.width / 2" :y="drawnSize.height - 5" text-anchor="middle" class="bp-dim">{{ Math.round(props.width) }} × {{ Math.round(props.height) }}</text>

    <!-- 霓虹夜行双色灯管与细线流光 -->
    <path v-if="props.variant === 'neon' && props.shape !== 'line'" ref="neonTubeEl" class="neon-tube" :stroke="`url(#${neonGradient})`" :d="contours.extra" />
    <path v-if="props.variant === 'neon' && props.shape === 'line'" class="neon-line" :stroke="`url(#${neonGradient})`" :d="contours.ruler" />
  </svg>
</template>

<style scoped>
.liquid-skin { position:absolute; inset:0; width:100%; height:100%; z-index:0; pointer-events:none; }
.liquid-skin.is-suspended { visibility: hidden; }
.skin { fill:var(--surface); stroke:var(--island-rim, var(--island-outline)); stroke-width:1px; }
.variant-standard .skin { fill:transparent; stroke:none; }
.variant-mecha .skin { stroke-width:1.4px; }
.variant-pixel { shape-rendering:crispEdges; }
.variant-pixel .skin { stroke:#0f0d1a; stroke-width:2px; }
.variant-blueprint .skin { stroke:rgba(219, 238, 255, .72); stroke-width:1.25px; stroke-dasharray:7 3; }
.variant-neon .skin { stroke-width:1.8px; }
.sheen { fill:none; stroke:var(--island-top-sheen); stroke-width:1px; opacity:.85; }
.hem { fill:none; stroke-width:1.2px; }
.reticles { fill:none; stroke:var(--accent); stroke-width:1.35px; opacity:.82; stroke-linecap:square; }
.ruler { fill:none; stroke:var(--accent); stroke-width:1.25px; opacity:.9; stroke-linecap:round; }
.mecha-rails { fill:none; stroke:var(--accent); stroke-width:2px; stroke-dasharray:5 4; opacity:.72; }
.mecha-seam { fill:none; stroke:var(--accent); stroke-width:1.4px; opacity:.92; }
.pixel-slots { fill:none; stroke:#94a3b8; stroke-width:2px; }
.bp-marks { fill:none; stroke:#fde047; stroke-width:1.1px; opacity:.88; }
.bp-ruler { fill:none; stroke:rgba(224, 242, 254, .62); stroke-width:1px; }
.bp-ruler.is-line { stroke:#fde047; stroke-dasharray:4 3; stroke-width:1.2px; }
.bp-dim { fill:#fde047; font-family:var(--font-mono); font-size:9px; font-weight:700; letter-spacing:.08em; }
.neon-tube { fill:none; stroke-width:1.1px; stroke-dasharray:18 6; opacity:.72; }
.neon-line { fill:none; stroke-width:2.2px; stroke-linecap:round; }
@media (forced-colors:active) {
  .skin { fill:Canvas; stroke:CanvasText; }
  .sheen, .hem, .reticles, .ruler, .mecha-rails, .mecha-seam, .pixel-slots,
  .bp-grid, .bp-marks, .bp-ruler, .bp-dim, .neon-tube, .neon-line { display:none; }
}
</style>
