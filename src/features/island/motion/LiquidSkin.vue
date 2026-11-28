<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, shallowRef, useId, watch } from 'vue'
import { gsap } from '@/lib/gsapMotion'
import { buildNotchPath, type NotchAnchor } from '../notchPath'

export type IslandVariant = 'standard' | 'telemetry' | 'mecha' | 'pixel' | 'blueprint' | 'neon'

/** 悬浮形态下，标准圆角底座在几何运动时交由 CSS 图层承担；贴顶刘海形态与所有器物主题均逐帧绘制单路径 SVG。 */
const SUSPENDED_FLOATING_VARIANTS = new Set<IslandVariant>(['standard'])

const props = withDefaults(defineProps<{
  width: number
  height: number
  shape: 'line' | 'compact' | 'expanded'
  variant?: IslandVariant
  moving?: boolean
  /** 外壳轮廓模式：floating 为经典悬浮胶囊/卡片，notch 为单路径零接缝贴顶刘海 */
  contourMode?: 'floating' | 'notch'
  /** 停靠位置（贴顶模式下决定左右肩部是否平直贴屏边） */
  anchor?: NotchAnchor
  /** split 模式下方悬浮卡片样式（pixel 主题呈现暗绿液晶内屏硬边框） */
  splitCard?: boolean
}>(), {
  variant: 'standard',
  moving: false,
  contourMode: 'floating',
  anchor: 'center',
  splitCard: false
})

const isNotchContour = computed(() => props.contourMode === 'notch')
const suspendDuringMotion = computed(() =>
  !isNotchContour.value && props.moving && SUSPENDED_FLOATING_VARIANTS.has(props.variant)
)

// 外壳的宽和高在动画期间同帧跟踪实际 DOM 尺寸，使单路径轮廓、仪表准星与霓虹灯管严丝合缝跟随缩放。
const svg = ref<SVGSVGElement | null>(null)
const reticlesEl = ref<SVGPathElement | null>(null)
const neonTubeEl = ref<SVGPathElement | null>(null)
const drawnSize = shallowRef({ width: Math.max(4, props.width), height: Math.max(4, props.height) })
let resize: ResizeObserver | undefined
let hasObserved = false

function isMotionReduced(): boolean {
  return document.documentElement.dataset.reducedMotion === '1' ||
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

function applyObservedSize(nextW: number, nextH: number): void {
  if (nextW <= 0 || nextH <= 0) return
  if (suspendDuringMotion.value) return
  const threshold = props.moving ? 0.35 : 0.2
  if (
    Math.abs(nextW - drawnSize.value.width) >= threshold ||
    Math.abs(nextH - drawnSize.value.height) >= threshold
  ) {
    drawnSize.value = { width: nextW, height: nextH }
  }
}

watch(
  () => [props.width, props.height] as const,
  ([w, h]) => {
    if (!hasObserved && w > 0 && h > 0) {
      drawnSize.value = { width: w, height: h }
    }
  }
)

watch(
  () => props.moving,
  isMoving => {
    if (!isMoving && svg.value) {
      const box = svg.value.getBoundingClientRect()
      if (box.width > 0 && box.height > 0) {
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
  const initialBox = svg.value.getBoundingClientRect()
  if (initialBox.width > 0 && initialBox.height > 0) {
    hasObserved = true
    drawnSize.value = { width: initialBox.width, height: initialBox.height }
  }
  resize = new ResizeObserver(([entry]) => {
    if (!entry) return
    hasObserved = true
    // ResizeObserver 在浏览器每帧绘制前触发，直接同步 drawnSize 可实现零帧滞后的单路径变形。
    applyObservedSize(entry.contentRect.width, entry.contentRect.height)
  })
  resize.observe(svg.value)
})
onBeforeUnmount(() => {
  resize?.disconnect()
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
  const w = Math.max(4, drawnSize.value.width)
  const h = Math.max(4, drawnSize.value.height)
  const expanded = props.shape === 'expanded'
  const line = props.shape === 'line'
  const v = props.variant
  const midY = h / 2
  // 基于实际高度平滑插值 0..1 展开进度，消除展开/收起临界高度的边角参数阶跃。
  const expandRatio = Math.max(0, Math.min(1, (h - 34) / 86))

  if (isNotchContour.value) {
    const notch = buildNotchPath({
      width: w,
      height: h,
      variant: v,
      anchor: props.anchor,
      expandRatio,
      line
    })
    const { earLeft, earRight, earHeight, bodyWidth } = notch.metrics
    const bodyX1 = earLeft
    const bodyX2 = Math.max(bodyX1 + 4, w - earRight)
    const bodyMidX = bodyX1 + bodyWidth / 2

    let extra = ''
    let ruler = ''

    if (v === 'blueprint') {
      if (line) {
        ruler = `M ${bodyX1 + 4} ${midY} H ${Math.max(bodyX1 + 4, bodyX2 - 4)}`
      } else {
        const i = expanded ? 7 : 5
        const a = expanded ? 4 : 2.5
        const cross = (x: number, y: number) => `M ${x - a} ${y} H ${x + a} M ${x} ${y - a} V ${y + a} `
        const topCrossY = Math.min(h - i - 2, Math.max(i + 2, 6))
        extra = cross(bodyX1 + i, topCrossY) + cross(bodyX2 - i, topCrossY) + cross(bodyX1 + i, h - i - 2) + cross(bodyX2 - i, h - i - 2)
        if (expanded && h >= 80) {
          // 尺寸标注线放在底部拉条上方独立带（h - 22），彻底错开底部 dismiss-handle
          const y = h - 22
          const x1 = bodyX1 + 16
          const x2 = Math.max(x1 + 20, bodyX2 - 16)
          const gap = 38
          ruler = `M ${x1} ${y - 3.5} V ${y + 3.5} M ${x2} ${y - 3.5} V ${y + 3.5} ` +
            `M ${x1} ${y} H ${bodyMidX - gap} M ${bodyMidX + gap} ${y} H ${x2} ` +
            `M ${x1 + 5} ${y - 2.5} L ${x1} ${y} L ${x1 + 5} ${y + 2.5} M ${x2 - 5} ${y - 2.5} L ${x2} ${y} L ${x2 - 5} ${y + 2.5}`
        } else {
          const parts: string[] = []
          for (let x = bodyX1 + 14, n = 0; x < bodyX2 - 13; x += 6, n++) {
            parts.push(`M ${x} ${h - 1.5} V ${h - (n % 5 === 0 ? 5.5 : 3.5)}`)
          }
          ruler = parts.join(' ')
        }
      }
    } else if (v === 'neon') {
      if (line) {
        ruler = `M ${bodyX1 + 6} ${midY} H ${Math.max(bodyX1 + 6, bodyX2 - 6)}`
      } else {
        const inset = 4 + expandRatio
        const innerR = Math.max(2, notch.metrics.bottomCorner - inset + 1)
        const ix1 = bodyX1 + inset
        const ix2 = Math.max(ix1 + 4, bodyX2 - inset)
        const iy1 = Math.min(h - inset - 2, Math.max(4, earHeight * 0.45))
        const iy2 = Math.max(iy1 + 2, h - inset)
        extra = `M ${ix1} ${iy1} V ${Math.max(iy1, iy2 - innerR)} A ${innerR} ${innerR} 0 0 0 ${Math.min(ix2, ix1 + innerR)} ${iy2} H ${Math.max(ix1 + innerR, ix2 - innerR)} A ${innerR} ${innerR} 0 0 0 ${ix2} ${Math.max(iy1, iy2 - innerR)} V ${iy1}`
      }
    } else if (v === 'mecha') {
      if (line) {
        ruler = `M ${bodyX1 + 6} ${midY} H ${Math.max(bodyX1 + 6, bodyMidX - 6)} M ${bodyMidX - 3} ${midY - 2.2} V ${midY + 2.2} M ${bodyMidX + 3} ${midY - 2.2} V ${midY + 2.2} M ${Math.min(bodyX2 - 6, bodyMidX + 6)} ${midY} H ${bodyX2 - 6}`
      } else if (expanded && h >= 80) {
        const railTop = Math.max(18, earHeight + 6)
        const railBot = Math.max(railTop, h - 18)
        extra = `M ${bodyX1 + 4.5} ${railTop} V ${railBot} M ${bodyX2 - 4.5} ${railTop} V ${railBot}`
      }
    } else if (v === 'pixel') {
      if (line) {
        ruler = `M ${bodyMidX - 12} ${midY} H ${bodyMidX - 5} M ${bodyMidX - 2} ${midY} H ${bodyMidX + 5} M ${bodyMidX + 8} ${midY} H ${bodyMidX + 15}`
      }
    } else if (v === 'telemetry') {
      if (line) {
        ruler = `M ${bodyX1 + 9} ${midY - 2} V ${midY + 2} M ${bodyX1 + 14} ${midY} H ${Math.max(bodyX1 + 14, bodyX2 - 14)} M ${Math.max(bodyX1 + 14, bodyX2 - 9)} ${midY - 2} V ${midY + 2}`
      } else {
        const inset = 4 + 1.5 * expandRatio
        const arm = 5 + 2.5 * expandRatio
        const rx1 = bodyX1 + inset
        const rx2 = Math.max(rx1 + arm, bodyX2 - inset)
        const ry1 = Math.max(4, inset)
        const ry2 = Math.max(ry1 + arm, h - inset)
        extra = `M ${rx1} ${ry1 + arm} V ${ry1} H ${rx1 + arm} M ${rx2 - arm} ${ry1} H ${rx2} V ${ry1 + arm} M ${rx1} ${ry2 - arm} V ${ry2} H ${rx1 + arm} M ${rx2 - arm} ${ry2} H ${rx2} V ${ry2 - arm}`
      }
    }

    return {
      isNotch: true,
      fillPath: notch.fillPath,
      strokePath: notch.strokePath,
      face: '',
      sheen: '',
      hem: notch.hemPath,
      extra,
      ruler,
      bodyMidX,
      bodyX1,
      bodyX2,
      showDim: v === 'blueprint' && expanded && h >= 80
    }
  }

  // ==================== 悬浮形态（floating 与 split 下方悬浮卡片） ====================
  if (v === 'blueprint') {
    const face = roundedRect(.75, .75, w - 1.5, h - 1.5, line ? 1 : 2)
    if (line) {
      return { isNotch: false, fillPath: '', strokePath: '', face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: '', ruler: `M 4 ${midY} H ${Math.max(4, w - 4)}`, bodyMidX: w / 2, bodyX1: 0, bodyX2: w, showDim: false }
    }
    const i = expanded ? 7 : 5, a = expanded ? 4 : 2.5
    const cross = (x: number, y: number) => `M ${x - a} ${y} H ${x + a} M ${x} ${y - a} V ${y + a} `
    const marks = cross(i, i) + cross(w - i, i) + cross(i, h - i) + cross(w - i, h - i)
    let ruler = ''
    if (expanded) {
      const y = h - 22, x1 = 16, x2 = Math.max(x1 + 20, w - 16), gap = 38, cx = w / 2
      ruler = `M ${x1} ${y - 3.5} V ${y + 3.5} M ${x2} ${y - 3.5} V ${y + 3.5} ` +
        `M ${x1} ${y} H ${cx - gap} M ${cx + gap} ${y} H ${x2} ` +
        `M ${x1 + 5} ${y - 2.5} L ${x1} ${y} L ${x1 + 5} ${y + 2.5} M ${x2 - 5} ${y - 2.5} L ${x2} ${y} L ${x2 - 5} ${y + 2.5}`
    } else {
      const parts: string[] = []
      for (let x = 14, n = 0; x < w - 13; x += 6, n++) parts.push(`M ${x} ${h - 1.5} V ${h - (n % 5 === 0 ? 5.5 : 3.5)}`)
      ruler = parts.join(' ')
    }
    return { isNotch: false, fillPath: '', strokePath: '', face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: marks, ruler, bodyMidX: w / 2, bodyX1: 0, bodyX2: w, showDim: expanded }
  }

  if (v === 'neon') {
    const r = Math.min(line ? 3 : 24 - 4 * expandRatio, h / 2, w / 2)
    const face = roundedRect(1, 1, w - 2, h - 2, r)
    if (line) {
      return { isNotch: false, fillPath: '', strokePath: '', face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: '', ruler: `M 6 ${midY} H ${Math.max(6, w - 6)}`, bodyMidX: w / 2, bodyX1: 0, bodyX2: w, showDim: false }
    }
    const inset = 4 + expandRatio
    const tube = roundedRect(inset, inset, Math.max(2, w - inset * 2), Math.max(2, h - inset * 2), Math.max(0, r - inset + 1))
    return { isNotch: false, fillPath: '', strokePath: '', face: `path('${face}')`, sheen: `path('M 0 0 H 0')`, hem: `path('M 0 0 H 0')`, extra: tube, ruler: '', bodyMidX: w / 2, bodyX1: 0, bodyX2: w, showDim: false }
  }

  if (v === 'mecha') {
    const cut = Math.min(line ? 2 : 8 + 5 * expandRatio, h / 2 - .5, w / 2 - .5)
    const face = `M ${cut} .75 H ${w - cut} L ${w - .75} ${cut} V ${h - cut} L ${w - cut} ${h - .75} H ${cut} L .75 ${h - cut} V ${cut} Z`
    const sheen = `M ${cut + 2} 1.8 H ${Math.max(cut + 2, w - cut - 2)}`
    const hem = `M ${cut + 6} ${h - 1.5} H ${Math.max(cut + 6, w - cut - 6)}`
    const rails = `M 4.5 18 V ${Math.max(18, h - 18)} M ${w - 4.5} 18 V ${Math.max(18, h - 18)}`
    const seam = `M 6 ${midY} H ${Math.max(6, w / 2 - 6)} M ${w / 2 - 3} ${midY - 2.2} V ${midY + 2.2} M ${w / 2 + 3} ${midY - 2.2} V ${midY + 2.2} M ${Math.min(w - 6, w / 2 + 6)} ${midY} H ${w - 6}`
    return { isNotch: false, fillPath: '', strokePath: '', face: `path('${face}')`, sheen: `path('${sheen}')`, hem: `path('${hem}')`, extra: rails, ruler: seam, bodyMidX: w / 2, bodyX1: 0, bodyX2: w, showDim: false }
  }

  if (v === 'pixel') {
    if (props.splitCard) {
      // split 悬浮卡片：暗绿液晶内屏硬边框风格，直角硬边 + 外框可见亮边
      const x1 = 1, y1 = 1, x2 = Math.round(w) - 1, y2 = Math.round(h) - 1
      const face = `M ${x1} ${y1} H ${x2} V ${y2} H ${x1} Z`
      const sheen = `M ${x1 + 2} 3 H ${Math.max(x1 + 2, x2 - 2)}`
      const hem = `M ${x1 + 2} ${y2 - 2} H ${Math.max(x1 + 2, x2 - 2)}`
      return { isNotch: false, fillPath: '', strokePath: '', face: `path('${face}')`, sheen: `path('${sheen}')`, hem: `path('${hem}')`, extra: '', ruler: '', bodyMidX: w / 2, bodyX1: 0, bodyX2: w, showDim: false }
    }
    const s = line ? 1.5 : 3
    const x1 = 1, y1 = 1, x2 = w - 1, y2 = h - 1
    const face = `M ${x1 + 2 * s} ${y1} H ${x2 - 2 * s} V ${y1 + s} H ${x2 - s} V ${y1 + 2 * s} H ${x2} V ${y2 - 2 * s} H ${x2 - s} V ${y2 - s} H ${x2 - 2 * s} V ${y2} H ${x1 + 2 * s} V ${y2 - s} H ${x1 + s} V ${y2 - 2 * s} H ${x1} V ${y1 + 2 * s} H ${x1 + s} V ${y1 + s} H ${x1 + 2 * s} Z`
    const sheen = `M ${x1 + 2 * s} 2.5 H ${x2 - 2 * s}`
    const hem = `M ${x1 + 2 * s} ${y2 - 1.5} H ${x2 - 2 * s}`
    const slots = `M ${w / 2 - 12} ${midY} H ${w / 2 - 5} M ${w / 2 - 2} ${midY} H ${w / 2 + 5} M ${w / 2 + 8} ${midY} H ${w / 2 + 15}`
    return { isNotch: false, fillPath: '', strokePath: '', face: `path('${face}')`, sheen: `path('${sheen}')`, hem: `path('${hem}')`, extra: '', ruler: slots, bodyMidX: w / 2, bodyX1: 0, bodyX2: w, showDim: false }
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
  return { isNotch: false, fillPath: '', strokePath: '', face: `path('${face}')`, sheen: `path('${sheen}')`, hem: `path('${hem}')`, extra: reticles, ruler, bodyMidX: w / 2, bodyX1: 0, bodyX2: w, showDim: false }
})
</script>

<template>
  <svg
    ref="svg"
    class="liquid-skin"
    :class="[
      `variant-${props.variant}`,
      `contour-${props.contourMode}`,
      { 'is-suspended': suspendDuringMotion, 'is-split-card': props.splitCard }
    ]"
    :viewBox="`0 0 ${drawnSize.width} ${drawnSize.height}`"
    preserveAspectRatio="none"
    aria-hidden="true"
  >
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

    <!-- 贴顶单路径：底色填充与外描边共享同一数学连续曲线，顶边无描边，零接缝 -->
    <template v-if="contours.isNotch">
      <path class="skin-fill" :d="contours.fillPath" />
      <path v-if="props.variant === 'blueprint' && props.shape !== 'line'" class="bp-grid" :fill="`url(#${gridPattern})`" :d="contours.fillPath" />
      <path
        class="skin-stroke"
        :stroke="props.variant === 'neon' ? `url(#${neonGradient})` : undefined"
        :d="contours.strokePath"
      />
      <path v-if="contours.hem" class="hem" :stroke="`url(#${gradient})`" :d="contours.hem" />
    </template>

    <!-- 悬浮形态（保持既有渲染零回归） -->
    <template v-else>
      <path class="skin"
        :stroke="props.variant === 'neon' ? `url(#${neonGradient})` : undefined"
        :style="{ d: contours.face }" />
      <path v-if="props.variant === 'blueprint' && props.shape !== 'line'" class="bp-grid" :fill="`url(#${gridPattern})`" :style="{ d: contours.face }" />
      <path v-if="props.shape !== 'line' && props.variant !== 'blueprint' && props.variant !== 'neon'" class="sheen" :style="{ d: contours.sheen }" />
      <path v-if="props.shape !== 'line' && props.variant !== 'blueprint' && props.variant !== 'neon'" class="hem" :stroke="`url(#${gradient})`" :style="{ d: contours.hem }" />
    </template>

    <!-- 星舰仪表准星与标尺 -->
    <path v-if="props.variant === 'telemetry' && props.shape !== 'line'" ref="reticlesEl" class="reticles" :d="contours.extra" />
    <path v-if="props.variant === 'telemetry' && props.shape === 'line'" class="ruler" :d="contours.ruler" />

    <!-- 琥珀重工导轨与合拢缝 -->
    <path v-if="props.variant === 'mecha' && props.shape === 'expanded' && contours.extra" class="mecha-rails" :d="contours.extra" />
    <path v-if="props.variant === 'mecha' && props.shape === 'line'" class="mecha-seam" :d="contours.ruler" />

    <!-- 像素掌机卡带细线指示灯 -->
    <template v-if="props.variant === 'pixel' && props.shape === 'line'">
      <rect :x="contours.bodyX1 + 6" :y="drawnSize.height / 2 - 2" width="5" height="4" fill="#fb7185" />
      <path class="pixel-slots" :d="contours.ruler" />
      <rect :x="Math.max(contours.bodyX1 + 16, contours.bodyX2 - 11)" :y="drawnSize.height / 2 - 2" width="5" height="4" fill="#a3e635" />
    </template>

    <!-- 蓝晒图纸套准十字、比例尺与展开尺寸标注（上移避开底部收起拉条） -->
    <path v-if="props.variant === 'blueprint' && props.shape !== 'line'" class="bp-marks" :d="contours.extra" />
    <path v-if="props.variant === 'blueprint'" class="bp-ruler" :class="{ 'is-line': props.shape === 'line' }" :d="contours.ruler" />
    <text v-if="contours.showDim" :x="contours.bodyMidX" :y="drawnSize.height - 19" text-anchor="middle" class="bp-dim">{{ Math.round(props.width) }} × {{ Math.round(props.height) }}</text>

    <!-- 霓虹夜行双色灯管与细线流光 -->
    <path v-if="props.variant === 'neon' && props.shape !== 'line' && contours.extra" ref="neonTubeEl" class="neon-tube" :stroke="`url(#${neonGradient})`" :d="contours.extra" />
    <path v-if="props.variant === 'neon' && props.shape === 'line'" class="neon-line" :stroke="`url(#${neonGradient})`" :d="contours.ruler" />
  </svg>
</template>

<style scoped>
.liquid-skin { position:absolute; inset:0; width:100%; height:100%; z-index:0; pointer-events:none; overflow:hidden; }
.liquid-skin.is-suspended { visibility: hidden; }
.skin { fill:var(--surface); stroke:var(--island-rim, var(--island-outline)); stroke-width:1px; }
.contour-floating.variant-standard .skin { fill:transparent; stroke:none; }
.skin-fill { fill:var(--surface); stroke:none; }
.skin-stroke { fill:none; stroke:var(--island-rim, var(--island-outline)); stroke-width:1px; stroke-linejoin:round; stroke-linecap:butt; }
.variant-mecha .skin,
.variant-mecha .skin-stroke { stroke-width:1.4px; stroke-linejoin:miter; }
.variant-pixel { shape-rendering:crispEdges; }
.variant-pixel .skin,
.variant-pixel .skin-stroke { stroke:var(--island-rim, #7c72ad); stroke-width:2px; stroke-linejoin:miter; }
.variant-pixel.is-split-card .skin { fill:#1c221e; stroke:var(--island-rim, #7c72ad); stroke-width:2px; }
.variant-blueprint .skin,
.variant-blueprint .skin-stroke { stroke:rgba(219, 238, 255, .74); stroke-width:1.25px; stroke-dasharray:7 3; stroke-linejoin:miter; }
.variant-neon .skin,
.variant-neon .skin-stroke { stroke-width:1.8px; }
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
  .skin, .skin-fill { fill:Canvas; }
  .skin, .skin-stroke { stroke:CanvasText; }
  .sheen, .hem, .reticles, .ruler, .mecha-rails, .mecha-seam, .pixel-slots,
  .bp-grid, .bp-marks, .bp-ruler, .bp-dim, .neon-tube, .neon-line { display:none; }
}
</style>
