<script setup lang="ts">
/**
 * 02 · MorphSVG + DrawSVG 灵动岛胶囊状态矢量徽标：
 * 在闲置 / 异步任务 / 识字 / 完成 / 异常之间使用单一 SVG 矢量路径变形，
 * 支持传入真实进度（0..1）由 DrawSVG 实时闭合描绘圆弧，或无预估时旋转呼吸。
 */
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { gsap } from '@/lib/gsapMotion'

const props = withDefaults(defineProps<{
  kind: string
  error?: boolean
  progress?: number | null
  reduced?: boolean
}>(), {
  error: false,
  progress: null,
  reduced: false
})

const corePathEl = ref<SVGPathElement | null>(null)
const arcPathEl = ref<SVGPathElement | null>(null)
const activeColor = ref('var(--accent)')
let spinTween: gsap.core.Tween | null = null

const SHAPES: Record<string, { d: string; arc: string; spin: boolean; color: string }> = {
  idle: {
    d: 'M12 8.5 A3.5 3.5 0 1 1 11.99 8.5 Z',
    arc: '0% 0%',
    spin: false,
    color: 'var(--accent)'
  },
  working: {
    d: 'M12 5 L13.8 10.2 L19 12 L13.8 13.8 L12 19 L10.2 13.8 L5 12 L10.2 10.2 Z',
    arc: '0% 42%',
    spin: true,
    color: 'var(--accent)'
  },
  'quick-ocr': {
    d: 'M7 7 H17 V17 H7 Z',
    arc: '10% 85%',
    spin: false,
    color: 'var(--accent)'
  },
  approval: {
    d: 'M12 5.5 L18.5 12 L12 18.5 L5.5 12 Z',
    arc: '0% 75%',
    spin: true,
    color: 'var(--accent)'
  },
  success: {
    d: 'M6.8 12.2 L8.8 10.2 L11 12.4 L15.8 7.6 L17.6 9.4 L11 16 Z',
    arc: '0% 100%',
    spin: false,
    color: 'var(--success)'
  },
  error: {
    d: 'M13.2 5.5 L7.5 13 H12 L10.8 18.5 L16.5 11 H12 Z',
    arc: '0% 100%',
    spin: false,
    color: 'var(--danger)'
  }
}

function resolveKey(): string {
  if (props.kind === 'feedback') return props.error ? 'error' : 'success'
  if (props.kind === 'working' || props.kind === 'quick-ocr' || props.kind === 'approval') return props.kind
  return 'idle'
}

function syncVisual(instant = false): void {
  const core = corePathEl.value
  const arc = arcPathEl.value
  if (!core || !arc) return
  const cfg = SHAPES[resolveKey()] ?? SHAPES.idle
  const dur = instant || props.reduced ? 0.001 : 0.36
  activeColor.value = cfg.color

  const hasDeterminateProgress =
    props.kind === 'working' && typeof props.progress === 'number' && Number.isFinite(props.progress)
  const targetArc = hasDeterminateProgress
    ? `0% ${Math.max(8, Math.min(100, Math.round((props.progress as number) * 100)))}%`
    : cfg.arc

  gsap.to(core, {
    morphSVG: cfg.d,
    duration: dur,
    ease: 'elastic.out(0.82, 0.72)'
  })

  gsap.to(arc, {
    drawSVG: targetArc,
    duration: instant || props.reduced ? 0.001 : hasDeterminateProgress ? 0.45 : 0.42,
    ease: 'power3.out'
  })

  const shouldSpin = cfg.spin && !hasDeterminateProgress && !props.reduced
  if (shouldSpin) {
    if (!spinTween) {
      spinTween = gsap.to(arc, {
        rotation: 360,
        transformOrigin: '50% 50%',
        duration: 1.25,
        repeat: -1,
        ease: 'none'
      })
    }
  } else {
    spinTween?.kill()
    spinTween = null
    gsap.set(arc, { rotation: 0, transformOrigin: '50% 50%' })
  }
}

watch(() => [props.kind, props.error, props.progress, props.reduced], () => syncVisual(false))

onMounted(() => syncVisual(true))
onBeforeUnmount(() => {
  spinTween?.kill()
  if (corePathEl.value) gsap.killTweensOf(corePathEl.value)
  if (arcPathEl.value) gsap.killTweensOf(arcPathEl.value)
})
</script>

<template>
  <svg
    class="capsule-morph-glyph"
    :style="{ color: activeColor }"
    viewBox="0 0 24 24"
    width="15"
    height="15"
    aria-hidden="true"
  >
    <circle cx="12" cy="12" r="9.5" fill="none" stroke="currentColor" stroke-opacity="0.18" stroke-width="1.6" />
    <path
      ref="arcPathEl"
      d="M12 2.5 A9.5 9.5 0 1 1 11.99 2.5"
      fill="none"
      stroke="currentColor"
      stroke-width="1.8"
      stroke-linecap="round"
    />
    <path
      ref="corePathEl"
      d="M12 8.5 A3.5 3.5 0 1 1 11.99 8.5 Z"
      fill="currentColor"
    />
  </svg>
</template>

<style scoped>
.capsule-morph-glyph {
  flex: none;
  display: block;
  transition: color 220ms ease;
}
</style>
