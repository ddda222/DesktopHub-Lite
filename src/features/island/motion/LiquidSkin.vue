<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, useId, watch } from 'vue'

export type IslandVariant = 'standard' | 'liquid' | 'telemetry' | 'mecha' | 'scroll' | 'pixel' | 'blueprint' | 'neon' | 'clay'

const ROUND_VARIANTS = new Set<IslandVariant>(['standard', 'telemetry', 'liquid', 'neon', 'clay'])

const props = withDefaults(defineProps<{
  width: number
  height: number
  shape: 'line' | 'compact' | 'expanded'
  variant?: IslandVariant
  moving?: boolean
}>(), { variant: 'standard', moving: false })

const suspendDuringMotion = computed(() => props.moving && ROUND_VARIANTS.has(props.variant))

// 外壳的宽和高在动画期间对常规圆角主题交给 CSS GPU 图层，静止后再精确绘制饰线；异形主题按帧节流更新。
const svg = ref<SVGSVGElement | null>(null)
const drawnSize = shallowRef({ width: props.width, height: props.height })
let resize: ResizeObserver | undefined
let pendingWidth = props.width
let pendingHeight = props.height
let syncRaf = 0

function flushSize(): void {
  syncRaf = 0
  if (props.moving && ROUND_VARIANTS.has(props.variant)) return
  const threshold = props.moving ? 2 : 0.5
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

onMounted(() => {
  if (!svg.value) return
  resize = new ResizeObserver(([entry]) => {
    if (!entry) return
    pendingWidth = entry.contentRect.width
    pendingHeight = entry.contentRect.height
    if (props.moving && ROUND_VARIANTS.has(props.variant)) return
    if (!syncRaf) syncRaf = requestAnimationFrame(flushSize)
  })
  resize.observe(svg.value)
})
onBeforeUnmount(() => {
  resize?.disconnect()
  cancelAnimationFrame(syncRaf)
})

const uid = useId().replace(/[^a-z0-9-]/gi, '')
const gradient = 'island-hem-' + uid
const gridPattern = 'island-grid-' + uid
const neonGradient = 'island-neon-' + uid
const clayShade = 'island-clay-' + uid
const clayRim = 'island-clay-rim-' + uid
const liquidFill = 'island-liquid-fill-' + uid
const liquidRim = 'island-liquid-rim-' + uid

/** 圆角矩形轮廓；三套新主题共用，半径按可用尺寸收紧。 */
function roundedRect(x: number, y: number, w: number, h: number, r: number): string {
  const rr = Math.max(0, Math.min(r, w / 2, h / 2))
  return `M ${x + rr} ${y} H ${x + w - rr} A ${rr} ${rr} 0 0 1 ${x + w} ${y + rr} V ${y + h - rr} A ${rr} ${rr} 0 0 1 ${x + w - rr} ${y + h} H ${x + rr} A ${rr} ${rr} 0 0 1 ${x} ${y + h - rr} V ${y + rr} A ${rr} ${rr} 0 0 1 ${x + rr} ${y} Z`
}

const contours = computed(() => {
  const w = Math.max(4, drawnSize.value.width), h = Math.max(4, drawnSize.value.height)
  const expanded = props.shape === 'expanded', line = props.shape === 'line'
  const v = props.variant
  const midY = h / 2

  if (v === 'blueprint') {
    const face = roundedRect(.75, .75, w - 1.5, h - 1.5, line ? 1 : 2)
    if (line) {
      return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: '', ruler: `M 4 ${midY} H ${Math.max(4, w - 4)}`, tail: '' }
    }
    // 四角套准十字
    const i = expanded ? 7 : 5, a = expanded ? 4 : 2.5
    const cross = (x: number, y: number) => `M ${x - a} ${y} H ${x + a} M ${x} ${y - a} V ${y + a} `
    const marks = cross(i, i) + cross(w - i, i) + cross(i, h - i) + cross(w - i, h - i)
    let ruler = ''
    if (expanded) {
      // 底部尺寸线：两端界线 + 箭头，中间留出标注文字的位置
      const y = h - 8, x1 = 16, x2 = Math.max(x1 + 20, w - 16), gap = 34, cx = w / 2
      ruler = `M ${x1} ${y - 4} V ${y + 4} M ${x2} ${y - 4} V ${y + 4} ` +
        `M ${x1} ${y} H ${cx - gap} M ${cx + gap} ${y} H ${x2} ` +
        `M ${x1 + 5} ${y - 2.5} L ${x1} ${y} L ${x1 + 5} ${y + 2.5} M ${x2 - 5} ${y - 2.5} L ${x2} ${y} L ${x2 - 5} ${y + 2.5}`
    } else {
      // 收起时沿底边画一把比例尺
      const parts: string[] = []
      for (let x = 14, n = 0; x < w - 13; x += 6, n++) parts.push(`M ${x} ${h - 1.5} V ${h - (n % 5 === 0 ? 5.5 : 3.5)}`)
      ruler = parts.join(' ')
    }
    return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: marks, ruler, tail: '' }
  }

  if (v === 'neon') {
    const r = Math.min(expanded ? 20 : line ? 3 : 24, h / 2, w / 2)
    const face = roundedRect(1, 1, w - 2, h - 2, r)
    if (line) {
      return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: '', ruler: `M 6 ${midY} H ${Math.max(6, w - 6)}`, tail: '' }
    }
    const inset = expanded ? 5 : 4
    const tube = roundedRect(inset, inset, w - inset * 2, h - inset * 2, Math.max(0, r - inset + 1))
    return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: tube, ruler: '', tail: '' }
  }

  if (v === 'clay') {
    const r = Math.min(expanded ? 28 : 24, h / 2, w / 2)
    const face = roundedRect(.5, .5, w - 1, h - 1, r)
    if (line) {
      return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: '', ruler: '', tail: '' }
    }
    const i = expanded ? 10 : 6
    const rim = roundedRect(2.5, 2.5, w - 5, h - 5, Math.max(0, r - 2))
    const gloss = `M ${i + 2} ${Math.min(h * .45, r + 2)} Q ${i + 2} ${i} ${Math.min(w * .22, r * 1.7 + i)} ${i}`
    return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: rim, ruler: gloss, tail: '' }
  }
  if (v === 'mecha') {
    const cut = Math.min(expanded ? 13 : line ? 2 : 8, h / 2 - .5, w / 2 - .5)
    const face = `M ${cut} .75 H ${w - cut} L ${w - .75} ${cut} V ${h - cut} L ${w - cut} ${h - .75} H ${cut} L .75 ${h - cut} V ${cut} Z`
    const sheen = `M ${cut + 2} 1.8 H ${Math.max(cut + 2, w - cut - 2)}`
    const hem = `M ${cut + 6} ${h - 1.5} H ${Math.max(cut + 6, w - cut - 6)}`
    const rails = `M 4.5 18 V ${Math.max(18, h - 18)} M ${w - 4.5} 18 V ${Math.max(18, h - 18)}`
    const seam = `M 6 ${midY} H ${Math.max(6, w / 2 - 6)} M ${w / 2 - 3} ${midY - 2.2} V ${midY + 2.2} M ${w / 2 + 3} ${midY - 2.2} V ${midY + 2.2} M ${Math.min(w - 6, w / 2 + 6)} ${midY} H ${w - 6}`
    return { face: `path('${face}')`, sheen: `path('${sheen}')`, hem: `path('${hem}')`, extra: rails, ruler: seam, tail: '' }
  }

  if (v === 'scroll') {
    if (line) {
      const face = `M 2 1 H ${w - 16} V ${h - 1} H 2 Z`
      const tail = `M ${w - 16} 1 H ${w - 1} L ${w - 5.5} ${midY} L ${w - 1} ${h - 1} H ${w - 16} Z`
      const ruler = `M 5 ${midY} H ${Math.max(5, w - 18)}`
      return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: '', ruler, tail }
    }
    const r = Math.min(expanded ? 11 : 7, h / 2.3, w / 4)
    const face = `M ${r} .75 H ${w - r} A ${r} ${r} 0 0 0 ${w - .75} ${r} V ${h - r} A ${r} ${r} 0 0 0 ${w - r} ${h - .75} H ${r} A ${r} ${r} 0 0 0 .75 ${h - r} V ${r} A ${r} ${r} 0 0 0 ${r} .75 Z`
    const pad = expanded ? 5.5 : 3.8
    const frameOuter = `M ${r + 2} ${pad} H ${w - r - 2} V ${h - pad} H ${r + 2} Z`
    const frameInner = expanded ? ` M ${r + 4.5} ${pad + 2.5} H ${w - r - 4.5} V ${h - pad - 2.5} H ${r + 4.5} Z` : ''
    return { face: `path('${face}')`, sheen: `path('M ${r + 2} 1.8 H ${w - r - 2}')`, hem: `path('M ${r + 8} ${h - 1.6} H ${w - r - 8}')`, extra: frameOuter + frameInner, ruler: '', tail: '' }
  }

  if (v === 'pixel') {
    const s = line ? 1.5 : 3
    const x1 = 1, y1 = 1, x2 = w - 1, y2 = h - 1
    const face = `M ${x1 + 2 * s} ${y1} H ${x2 - 2 * s} V ${y1 + s} H ${x2 - s} V ${y1 + 2 * s} H ${x2} V ${y2 - 2 * s} H ${x2 - s} V ${y2 - s} H ${x2 - 2 * s} V ${y2} H ${x1 + 2 * s} V ${y2 - s} H ${x1 + s} V ${y2 - 2 * s} H ${x1} V ${y1 + 2 * s} H ${x1 + s} V ${y1 + s} H ${x1 + 2 * s} Z`
    const sheen = `M ${x1 + 2 * s} 2.5 H ${x2 - 2 * s}`
    const hem = `M ${x1 + 2 * s} ${y2 - 1.5} H ${x2 - 2 * s}`
    const slots = `M ${w / 2 - 12} ${midY} H ${w / 2 - 5} M ${w / 2 - 2} ${midY} H ${w / 2 + 5} M ${w / 2 + 8} ${midY} H ${w / 2 + 15}`
    return { face: `path('${face}')`, sheen: `path('${sheen}')`, hem: `path('${hem}')`, extra: '', ruler: slots, tail: '' }
  }

  if (v === 'liquid') {
    const r = Math.min(expanded ? 22 : line ? 4 : 26, h / 2, w / 2)
    const face = roundedRect(0.8, 0.8, w - 1.6, h - 1.6, r)
    if (line) {
      return { face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: '', ruler: `M 6 ${midY} H ${Math.max(6, w - 6)}`, tail: '' }
    }
    const inner = roundedRect(2.6, 2.6, w - 5.2, h - 5.2, Math.max(2, r - 2))
    const arcX1 = Math.min(w * 0.22, r + 10)
    const arcX2 = Math.max(arcX1 + 12, w - arcX1)
    const gloss = `M ${arcX1} 2.2 H ${arcX2}`
    return { face: `path('${face}')`, sheen: `path('${gloss}')`, hem: `path('M ${Math.max(r, w * 0.14)} ${h - 1.4} H ${Math.min(w - r, w * 0.86)}')`, extra: inner, ruler: gloss, tail: '' }
  }

  // standard / telemetry
  const baseRadius = v === 'telemetry' ? (expanded ? 12 : line ? 4 : 10) : (expanded ? 18 : line ? 5 : 24)
  const radius = Math.min(baseRadius, h / 2, w / 2)
  const right = Math.max(radius, w - radius)
  const bottomY = Math.max(radius, h - radius)
  const low = h - .5
  const face = `M ${radius} .5 H ${right} Q ${w - .5} .5 ${w - .5} ${radius} V ${bottomY} Q ${w - .5} ${low} ${right} ${low} H ${radius} Q .5 ${low} .5 ${bottomY} V ${radius} Q .5 .5 ${radius} .5 Z`
  const sheen = `M ${radius} 1.35 H ${right}`
  const hem = `M ${Math.max(radius, w * .16)} ${low - .2} H ${Math.min(right, w * .84)}`

  const inset = expanded ? 5.5 : 4
  const arm = expanded ? 7.5 : 5
  const rx1 = inset, ry1 = inset, rx2 = Math.max(inset + arm, w - inset), ry2 = Math.max(inset + arm, h - inset)
  const reticles = `M ${rx1} ${ry1 + arm} V ${ry1} H ${rx1 + arm} M ${rx2 - arm} ${ry1} H ${rx2} V ${ry1 + arm} M ${rx1} ${ry2 - arm} V ${ry2} H ${rx1 + arm} M ${rx2 - arm} ${ry2} H ${rx2} V ${ry2 - arm}`
  const ruler = `M 9 ${midY - 2} V ${midY + 2} M 14 ${midY} H ${Math.max(14, w - 14)} M ${Math.max(14, w - 9)} ${midY - 2} V ${midY + 2}`
  return { face: `path('${face}')`, sheen: `path('${sheen}')`, hem: `path('${hem}')`, extra: reticles, ruler, tail: '' }
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
      <linearGradient v-if="props.variant === 'clay'" :id="clayShade" x1="0%" y1="0%" x2="0%" y2="100%">
        <stop offset="0%" stop-color="#fffdf9" />
        <stop offset="68%" stop-color="#f7efe3" />
        <stop offset="100%" stop-color="#eadbc8" />
      </linearGradient>
      <linearGradient v-if="props.variant === 'clay'" :id="clayRim" x1="0%" y1="0%" x2="0%" y2="100%">
        <stop offset="0%" stop-color="#ffffff" stop-opacity=".95" />
        <stop offset="55%" stop-color="#ffffff" stop-opacity=".2" />
        <stop offset="100%" stop-color="#c4ab90" stop-opacity=".55" />
      </linearGradient>
      <linearGradient v-if="props.variant === 'liquid'" :id="liquidFill" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#14223a" />
        <stop offset="52%" stop-color="#0d1526" />
        <stop offset="100%" stop-color="#131930" />
      </linearGradient>
      <linearGradient v-if="props.variant === 'liquid'" :id="liquidRim" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#e0f2fe" stop-opacity=".88" />
        <stop offset="46%" stop-color="#7dd3fc" stop-opacity=".52" />
        <stop offset="100%" stop-color="#d8b4fe" stop-opacity=".78" />
      </linearGradient>
    </defs>
    <path class="skin"
      :fill="props.variant === 'clay' ? `url(#${clayShade})` : props.variant === 'liquid' ? `url(#${liquidFill})` : undefined"
      :stroke="props.variant === 'neon' ? `url(#${neonGradient})` : props.variant === 'liquid' ? `url(#${liquidRim})` : undefined"
      :style="{ d: contours.face }" />
    <path v-if="props.variant === 'blueprint' && props.shape !== 'line'" class="bp-grid" :fill="`url(#${gridPattern})`" :style="{ d: contours.face }" />
    <path v-if="props.shape !== 'line' && props.variant !== 'blueprint' && props.variant !== 'neon' && props.variant !== 'clay'" class="sheen" :style="{ d: contours.sheen }" />
    <path v-if="props.shape !== 'line' && props.variant !== 'blueprint' && props.variant !== 'neon' && props.variant !== 'clay'" class="hem" :stroke="`url(#${gradient})`" :style="{ d: contours.hem }" />

    <!-- 星舰仪表准星与标尺 -->
    <path v-if="props.variant === 'telemetry' && props.shape !== 'line'" class="reticles" :d="contours.extra" />
    <path v-if="props.variant === 'telemetry' && props.shape === 'line'" class="ruler" :d="contours.ruler" />

    <!-- 琥珀重工导轨与合拢缝 -->
    <path v-if="props.variant === 'mecha' && props.shape === 'expanded'" class="mecha-rails" :d="contours.extra" />
    <path v-if="props.variant === 'mecha' && props.shape === 'line'" class="mecha-seam" :d="contours.ruler" />

    <!-- 海棠书卷乌丝双栏与燕尾红签 -->
    <path v-if="props.variant === 'scroll' && props.shape !== 'line'" class="scroll-frame" :d="contours.extra" />
    <path v-if="props.variant === 'scroll' && props.shape === 'line'" class="scroll-tail" :d="contours.tail" />
    <path v-if="props.variant === 'scroll' && props.shape === 'line'" class="scroll-line" :d="contours.ruler" />

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
    <path v-if="props.variant === 'neon' && props.shape !== 'line'" class="neon-tube" :stroke="`url(#${neonGradient})`" :d="contours.extra" />
    <path v-if="props.variant === 'neon' && props.shape === 'line'" class="neon-line" :stroke="`url(#${neonGradient})`" :d="contours.ruler" />

    <!-- 软糖黏土立体高光圈与左上果冻反光 -->
    <path v-if="props.variant === 'clay' && props.shape !== 'line'" class="clay-rim" :stroke="`url(#${clayRim})`" :d="contours.extra" />
    <path v-if="props.variant === 'clay' && props.shape !== 'line'" class="clay-gloss" :d="contours.ruler" />

    <!-- 液态琉璃菲涅尔折射内环与细线冷光丝 -->
    <path v-if="props.variant === 'liquid' && props.shape !== 'line'" class="liquid-inner-rim" :stroke="`url(#${liquidRim})`" :d="contours.extra" />
    <path v-if="props.variant === 'liquid' && props.shape === 'line'" class="liquid-line" :stroke="`url(#${liquidRim})`" :d="contours.ruler" />
  </svg>
</template>

<style scoped>
.liquid-skin { position:absolute; inset:0; width:100%; height:100%; z-index:0; pointer-events:none; }
.liquid-skin.is-suspended { visibility: hidden; }
.skin { fill:var(--surface); stroke:var(--island-rim, var(--island-outline)); stroke-width:1px; }
.variant-mecha .skin { stroke-width:1.4px; }
.variant-scroll .skin { stroke-width:1.5px; }
.variant-pixel { shape-rendering:crispEdges; }
.variant-pixel .skin { stroke:#0f0d1a; stroke-width:2px; }
.variant-blueprint .skin { stroke:rgba(219, 238, 255, .72); stroke-width:1.25px; stroke-dasharray:7 3; }
.variant-neon .skin { stroke-width:1.8px; }
.variant-clay .skin { stroke:#d4bea6; stroke-width:1.6px; }
.sheen { fill:none; stroke:var(--island-top-sheen); stroke-width:1px; opacity:.85; }
.hem { fill:none; stroke-width:1.2px; }
.reticles { fill:none; stroke:var(--accent); stroke-width:1.35px; opacity:.82; stroke-linecap:square; }
.ruler { fill:none; stroke:var(--accent); stroke-width:1.25px; opacity:.9; stroke-linecap:round; }
.mecha-rails { fill:none; stroke:var(--accent); stroke-width:2px; stroke-dasharray:5 4; opacity:.72; }
.mecha-seam { fill:none; stroke:var(--accent); stroke-width:1.4px; opacity:.92; }
.scroll-frame { fill:none; stroke:rgba(31, 36, 33, .22); stroke-width:.9px; }
.scroll-tail { fill:var(--accent); stroke:none; }
.scroll-line { fill:none; stroke:#2c302e; stroke-width:1.6px; }
.pixel-slots { fill:none; stroke:#94a3b8; stroke-width:2px; }
.bp-marks { fill:none; stroke:#fde047; stroke-width:1.1px; opacity:.88; }
.bp-ruler { fill:none; stroke:rgba(224, 242, 254, .62); stroke-width:1px; }
.bp-ruler.is-line { stroke:#fde047; stroke-dasharray:4 3; stroke-width:1.2px; }
.bp-dim { fill:#fde047; font-family:var(--font-mono); font-size:9px; font-weight:700; letter-spacing:.08em; }
.neon-tube { fill:none; stroke-width:1.1px; stroke-dasharray:18 6; opacity:.72; }
.neon-line { fill:none; stroke-width:2.2px; stroke-linecap:round; }
.clay-rim { fill:none; stroke-width:2.2px; }
.clay-gloss { fill:none; stroke:rgba(255, 255, 255, .88); stroke-width:2.8px; stroke-linecap:round; }
.variant-liquid .skin { stroke-width:1.35px; }
.liquid-inner-rim { fill:none; stroke-width:1px; opacity:.34; }
.liquid-line { fill:none; stroke-width:2px; stroke-linecap:round; }
@media (forced-colors:active) {
  .skin { fill:Canvas; stroke:CanvasText; }
  .sheen, .hem, .reticles, .ruler, .mecha-rails, .mecha-seam, .scroll-frame, .scroll-tail, .scroll-line, .pixel-slots,
  .bp-grid, .bp-marks, .bp-ruler, .bp-dim, .neon-tube, .neon-line, .clay-rim, .clay-gloss { display:none; }
}
</style>
