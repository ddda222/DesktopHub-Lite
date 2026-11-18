<script setup lang="ts">
import { computed, useId } from 'vue'
const props = defineProps<{ width: number; height: number; shape: 'line' | 'compact' | 'expanded' }>()
const gradient = 'island-hem-' + useId().replace(/[^a-z0-9-]/gi, '')
const contours = computed(() => {
  const w = props.width, h = props.height
  const expanded = props.shape === 'expanded', line = props.shape === 'line'
  const radius = Math.min(expanded ? 18 : 24, h / 2, w / 2)
  const lift = line ? .4 : expanded ? 3.5 : 1.8
  const seam = line ? .55 : expanded ? 2.8 : 1.5
  const low = h - .55, left = radius, right = w - radius
  const hem = `M ${left} ${low} C ${w*.34} ${low} ${w*.72} ${low-seam*.2} ${right} ${low-lift-seam}`
  const face = `M ${radius} .5 H ${right} Q ${w-.5} .5 ${w-.5} ${radius} V ${Math.max(radius,h-radius)} Q ${w-.5} ${low-lift} ${right} ${low-lift} C ${w*.72} ${low} ${w*.34} ${low} ${left} ${low} Q .5 ${low} .5 ${h-radius} V ${radius} Q .5 .5 ${radius} .5 Z`
  return { face: `path('${face}')`, hem: `path('${hem}')` }
})
</script>
<template>
  <svg class="liquid-skin" aria-hidden="true">
    <defs><linearGradient :id="gradient"><stop stop-color="var(--text-3)" stop-opacity="0"/><stop offset=".40" stop-color="var(--text-3)" stop-opacity=".12"/><stop offset=".77" stop-color="var(--text-2)" stop-opacity=".55"/><stop offset="1" stop-color="var(--text-3)" stop-opacity=".08"/></linearGradient></defs>
    <path class="skin" :style="{ d: contours.face }" />
    <path class="hem" :stroke="`url(#${gradient})`" :style="{ d: contours.hem }" />
  </svg>
</template>
<style scoped>
.liquid-skin { position:absolute; inset:0; width:100%; height:100%; z-index:0; pointer-events:none; }
.liquid-skin path { transition:d var(--geometry-time) var(--geometry-ease); }
.skin { fill:var(--surface); stroke:var(--island-outline); stroke-width:1px; }
.hem { fill:none; stroke-width:1.3px; }
@media (forced-colors:active) { .skin { fill:Canvas; stroke:CanvasText; } .hem { display:none; } }
</style>
