<script setup lang="ts">
/**
 * 灵动岛胶囊微读数渲染组件：支持网络速率与 CPU/内存占用的多样式可视化，
 * 使用 GSAP 驱动数值滚动插值、弹性进度条、双环描边与脉冲呼吸动画。
 */
import { computed, nextTick, onBeforeUnmount, reactive, ref, watch } from 'vue'
import { gsap } from '@/lib/gsapMotion'
import type { TelemetryReadout, TelemetryStyle } from './telemetryReadout'

const RING_CIRCUMFERENCE = 2 * Math.PI * 5

const props = withDefaults(defineProps<{
  readout: TelemetryReadout
  styleMode: TelemetryStyle
  reduced?: boolean
}>(), {
  reduced: false
})

const rootEl = ref<HTMLElement | null>(null)
const pulseGridRefs = new Map<'cpu' | 'memory', HTMLElement>()
const pulseTweens = new Map<'cpu' | 'memory', gsap.core.Tween>()

const cpuTarget = computed(() =>
  props.readout.segments.find(item => item.metric === 'cpu')?.percent ?? null
)
const memoryTarget = computed(() =>
  props.readout.segments.find(item => item.metric === 'memory')?.percent ?? null
)

const animated = reactive({
  cpu: cpuTarget.value ?? 0,
  memory: memoryTarget.value ?? 0,
  cpuHasValue: cpuTarget.value !== null,
  memoryHasValue: memoryTarget.value !== null
})

function setPulseRef(metric: 'cpu' | 'memory', el: unknown): void {
  if (el instanceof HTMLElement) {
    pulseGridRefs.set(metric, el)
  } else {
    pulseGridRefs.delete(metric)
  }
}

function loadLevel(percent: number | null | undefined): 'normal' | 'warm' | 'hot' {
  if (percent === null || percent === undefined) return 'normal'
  if (percent >= 85) return 'hot'
  if (percent >= 65) return 'warm'
  return 'normal'
}

function formattedPercent(metric: 'cpu' | 'memory', rawValue: string): string {
  const hasVal = metric === 'cpu' ? animated.cpuHasValue : animated.memoryHasValue
  if (!hasVal) return rawValue
  const val = metric === 'cpu' ? animated.cpu : animated.memory
  return `${Math.round(Math.max(0, Math.min(100, val)))}%`
}

function progressRatio(metric: 'cpu' | 'memory'): number {
  const hasVal = metric === 'cpu' ? animated.cpuHasValue : animated.memoryHasValue
  if (!hasVal) return 0
  const val = metric === 'cpu' ? animated.cpu : animated.memory
  return Math.max(0.04, Math.min(1, val / 100))
}

function ringOffset(metric: 'cpu' | 'memory'): number {
  const hasVal = metric === 'cpu' ? animated.cpuHasValue : animated.memoryHasValue
  if (!hasVal) return RING_CIRCUMFERENCE
  const val = metric === 'cpu' ? animated.cpu : animated.memory
  const ratio = Math.max(0, Math.min(1, val / 100))
  return RING_CIRCUMFERENCE * (1 - ratio)
}

function activeBarsCount(percent: number | null | undefined): number {
  if (percent === null || percent === undefined) return 0
  return Math.max(1, Math.min(4, Math.ceil(percent / 25)))
}

function syncPulseAnimation(): void {
  pulseTweens.forEach(t => t.kill())
  pulseTweens.clear()
  if (props.styleMode !== 'pulse' || props.reduced) return
  for (const metric of ['cpu', 'memory'] as const) {
    const grid = pulseGridRefs.get(metric)
    const rawPct = metric === 'cpu' ? cpuTarget.value : memoryTarget.value
    if (!grid || rawPct === null) continue
    const activeIdx = activeBarsCount(rawPct) - 1
    const bars = grid.querySelectorAll<HTMLElement>('i')
    const peakBar = bars[activeIdx]
    if (!peakBar) continue
    const tween = gsap.fromTo(
      peakBar,
      { scaleY: 0.78, opacity: 0.72 },
      {
        scaleY: 1.18,
        opacity: 1,
        duration: rawPct >= 65 ? 0.55 : 0.9,
        yoyo: true,
        repeat: -1,
        ease: 'sine.inOut'
      }
    )
    pulseTweens.set(metric, tween)
  }
}

function updateMetricTween(key: 'cpu' | 'memory', target: number | null): void {
  const flagKey = key === 'cpu' ? 'cpuHasValue' : 'memoryHasValue'
  if (target === null) {
    animated[flagKey] = false
    return
  }
  const safeTarget = Math.max(0, Math.min(100, target))
  if (!animated[flagKey] || props.reduced) {
    animated[flagKey] = true
    animated[key] = safeTarget
    return
  }
  gsap.to(animated, {
    [key]: safeTarget,
    duration: 0.52,
    ease: 'power2.out',
    overwrite: 'auto'
  })
}

watch(cpuTarget, val => updateMetricTween('cpu', val), { immediate: true })
watch(memoryTarget, val => updateMetricTween('memory', val), { immediate: true })

watch(
  [() => props.styleMode, () => props.reduced, cpuTarget, memoryTarget],
  () => {
    void nextTick(() => syncPulseAnimation())
  },
  { immediate: true, flush: 'post' }
)

onBeforeUnmount(() => {
  gsap.killTweensOf(animated)
  pulseTweens.forEach(t => t.kill())
  pulseTweens.clear()
})
</script>

<template>
  <span ref="rootEl" class="telemetry-readout-wrap" :class="`style-${styleMode}`">
    <template v-for="(seg, idx) in readout.segments" :key="`${seg.metric || 'net'}-${idx}`">
      <i v-if="idx > 0" class="tel-sep">·</i>
      <span
        class="tel-seg"
        :class="[
          seg.metric ? `metric-${seg.metric}` : '',
          seg.metric && seg.metric !== 'net' ? `level-${loadLevel(seg.percent)}` : ''
        ]"
      >
        <em class="tel-pre" :class="{ accent: seg.accent }">{{ seg.prefix }}</em>

        <!-- 处理器与内存视觉样式 -->
        <template v-if="seg.metric === 'cpu' || seg.metric === 'memory'">
          <span
            v-if="styleMode === 'bar'"
            class="tel-bar-track"
            aria-hidden="true"
          >
            <i
              class="tel-bar-fill"
              :style="{ transform: `scaleX(${progressRatio(seg.metric)})` }"
            />
          </span>

          <svg
            v-else-if="styleMode === 'ring'"
            class="tel-ring-svg"
            viewBox="0 0 14 14"
            aria-hidden="true"
          >
            <circle class="tel-ring-bg" cx="7" cy="7" r="5" />
            <circle
              class="tel-ring-arc"
              cx="7"
              cy="7"
              r="5"
              :stroke-dasharray="RING_CIRCUMFERENCE"
              :stroke-dashoffset="ringOffset(seg.metric)"
            />
          </svg>

          <span
            v-else-if="styleMode === 'pulse'"
            :ref="el => setPulseRef(seg.metric as 'cpu' | 'memory', el)"
            class="tel-pulse-grid"
            aria-hidden="true"
          >
            <i
              v-for="barIdx in 4"
              :key="barIdx"
              class="tel-pulse-bar"
              :class="{ 'is-lit': barIdx <= activeBarsCount(seg.percent) }"
              :style="{ '--bar-h': `${3 + barIdx * 1.6}px` }"
            />
          </span>

          <span class="tel-val">{{ formattedPercent(seg.metric, seg.value) }}</span>
        </template>

        <!-- 网速等基础读数 -->
        <template v-else>
          <span class="tel-val">{{ seg.value }}</span>
        </template>
      </span>
    </template>
  </span>
</template>

<style scoped>
.telemetry-readout-wrap {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-variant-numeric: tabular-nums;
}
.tel-sep {
  font-style: normal;
  opacity: 0.4;
  margin: 0 1px;
}
.tel-seg {
  display: inline-flex;
  align-items: center;
  gap: 3.5px;
  --tel-color: var(--accent);
}
.tel-seg.level-warm {
  --tel-color: #f59e0b;
}
.tel-seg.level-hot {
  --tel-color: var(--danger);
}
.tel-pre {
  font-style: normal;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-2);
}
.tel-pre.accent {
  color: var(--accent);
  font-weight: 700;
}
.tel-val {
  color: var(--text);
  font-weight: 600;
  transition: color 180ms ease;
}
.tel-seg.level-warm .tel-val,
.tel-seg.level-hot .tel-val {
  color: var(--tel-color);
}

/* 能量微条 */
.tel-bar-track {
  position: relative;
  display: inline-flex;
  align-items: center;
  width: 24px;
  height: 5px;
  border-radius: 999px;
  background: color-mix(in srgb, var(--text) 14%, transparent);
  overflow: hidden;
  box-shadow: inset 0 0 0 0.6px color-mix(in srgb, var(--text) 12%, transparent);
}
.tel-bar-fill {
  display: block;
  width: 100%;
  height: 100%;
  border-radius: 999px;
  background: var(--tel-color);
  transform-origin: left center;
  transition: background-color 220ms ease;
}

/* 双环仪表 */
.tel-ring-svg {
  width: 13px;
  height: 13px;
  flex: none;
  transform: rotate(-90deg);
}
.tel-ring-bg {
  fill: none;
  stroke: color-mix(in srgb, var(--text) 18%, transparent);
  stroke-width: 2.1;
}
.tel-ring-arc {
  fill: none;
  stroke: var(--tel-color);
  stroke-width: 2.1;
  stroke-linecap: round;
  transition: stroke 220ms ease;
}

/* 律动格栅 */
.tel-pulse-grid {
  display: inline-flex;
  align-items: flex-end;
  gap: 1.5px;
  height: 10px;
}
.tel-pulse-bar {
  display: block;
  width: 2.5px;
  height: var(--bar-h, 6px);
  border-radius: 1px;
  background: color-mix(in srgb, var(--text) 18%, transparent);
  transform-origin: bottom center;
  transition: background-color 180ms ease;
}
.tel-pulse-bar.is-lit {
  background: var(--tel-color);
}
</style>
