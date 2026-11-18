<script setup lang="ts">
import { computed, useId } from 'vue'

export type IslandVariant = 'standard' | 'telemetry' | 'mecha' | 'scroll' | 'pixel'

const props = withDefaults(defineProps<{
  width: number
  height: number
  shape: 'line' | 'compact' | 'expanded'
  variant?: IslandVariant
}>(), { variant: 'standard' })

const gradient = 'island-hem-' + useId().replace(/[^a-z0-9-]/gi, '')

const contours = computed(() => {
  const w = Math.max(4, props.width), h = Math.max(4, props.height)
  const expanded = props.shape === 'expanded', line = props.shape === 'line'
  const v = props.variant
  const midY = h / 2

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
  <svg class="liquid-skin" :class="`variant-${props.variant}`" aria-hidden="true">
    <defs>
      <linearGradient :id="gradient">
        <stop stop-color="var(--island-hem-a)" stop-opacity="0"/>
        <stop offset=".32" stop-color="var(--island-hem-a)" stop-opacity=".45"/>
        <stop offset=".68" stop-color="var(--island-hem-b)" stop-opacity=".75"/>
        <stop offset="1" stop-color="var(--island-hem-a)" stop-opacity="0"/>
      </linearGradient>
    </defs>
    <path class="skin" :style="{ d: contours.face }" />
    <path v-if="props.shape !== 'line'" class="sheen" :style="{ d: contours.sheen }" />
    <path v-if="props.shape !== 'line'" class="hem" :stroke="`url(#${gradient})`" :style="{ d: contours.hem }" />

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
      <rect x="6" :y="props.height / 2 - 2" width="5" height="4" fill="#fb7185" />
      <path class="pixel-slots" :d="contours.ruler" />
      <rect :x="Math.max(16, props.width - 11)" :y="props.height / 2 - 2" width="5" height="4" fill="#a3e635" />
    </template>
  </svg>
</template>

<style scoped>
.liquid-skin { position:absolute; inset:0; width:100%; height:100%; z-index:0; pointer-events:none; }
.liquid-skin path { transition:d var(--geometry-time) var(--geometry-ease); }
.skin { fill:var(--surface); stroke:var(--island-rim, var(--island-outline)); stroke-width:1px; }
.variant-mecha .skin { stroke-width:1.4px; }
.variant-scroll .skin { stroke-width:1.5px; }
.variant-pixel { shape-rendering:crispEdges; }
.variant-pixel .skin { stroke:#0f0d1a; stroke-width:2px; }
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
@media (forced-colors:active) {
  .skin { fill:Canvas; stroke:CanvasText; }
  .sheen, .hem, .reticles, .ruler, .mecha-rails, .mecha-seam, .scroll-frame, .scroll-tail, .scroll-line, .pixel-slots { display:none; }
}
</style>
