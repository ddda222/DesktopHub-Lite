<script setup lang="ts" generic="T extends string">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'

/**
 * 带平滑滑块的分段单选控件（参考 Leaf Tabs / Gooey Tab Bar）。
 * 滑块定位完全使用 transform + width，同行切换时支持粘液微拉伸，换行跳转自动跳过斜向穿屏。
 */
const props = defineProps<{
  options: { value: T; label: string }[]
  modelValue: T
  name: string
  label: string
}>()

const emit = defineEmits<{ 'update:modelValue': [T] }>()

const root = ref<HTMLElement | null>(null)
const thumb = ref({ x: 0, y: 0, w: 0, h: 0 })
const ready = ref(false)
const moving = ref(false)
const rowJump = ref(false)

let resizeObserver: ResizeObserver | null = null
let rafId = 0
let movingTimer = 0
let rowJumpTimer = 0

function measure(animateMove = true): void {
  const container = root.value
  const active = container?.querySelector<HTMLElement>('.seg-item.is-on')
  if (!container || !active) return
  const nextX = active.offsetLeft
  const nextY = active.offsetTop
  const nextW = active.offsetWidth
  const nextH = active.offsetHeight
  const prev = thumb.value
  const crossedRow = ready.value && animateMove && Math.abs(nextY - prev.y) > 4
  const shifted = ready.value && animateMove && !crossedRow && (Math.abs(nextX - prev.x) > 2 || Math.abs(nextW - prev.w) > 2)

  if (crossedRow) {
    rowJump.value = true
    window.clearTimeout(rowJumpTimer)
    rowJumpTimer = window.setTimeout(() => { rowJump.value = false }, 40)
  } else if (shifted) {
    moving.value = true
    window.clearTimeout(movingTimer)
    movingTimer = window.setTimeout(() => { moving.value = false }, 220)
  }

  thumb.value = { x: nextX, y: nextY, w: nextW, h: nextH }
  if (!ready.value && nextW > 0 && nextH > 0) {
    cancelAnimationFrame(rafId)
    rafId = requestAnimationFrame(() => { ready.value = true })
  }
}

function scheduleMeasure(animateMove = true): void {
  void nextTick(() => {
    measure(animateMove)
    cancelAnimationFrame(rafId)
    rafId = requestAnimationFrame(() => measure(false))
  })
}

const thumbStyle = computed(() => ({
  width: `${thumb.value.w}px`,
  height: `${thumb.value.h}px`,
  transform: `translate3d(${thumb.value.x}px, ${thumb.value.y}px, 0)`
}))

watch(() => [props.modelValue, props.options.length] as const, () => scheduleMeasure(true))

onMounted(() => {
  scheduleMeasure(false)
  if (root.value && typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver(() => measure(false))
    resizeObserver.observe(root.value)
  }
})

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  cancelAnimationFrame(rafId)
  window.clearTimeout(movingTimer)
  window.clearTimeout(rowJumpTimer)
})
</script>

<template>
  <span ref="root" class="seg has-thumb" role="radiogroup" :aria-label="label">
    <span
      class="seg-thumb"
      :class="{ 'is-ready': ready, 'is-moving': moving, 'is-row-jump': rowJump }"
      aria-hidden="true"
      :style="thumbStyle"
    />
    <label
      v-for="opt in options"
      :key="opt.value"
      class="seg-item"
      :class="{ 'is-on': modelValue === opt.value }"
    >
      <input
        class="sr-only"
        type="radio"
        :name="name"
        :value="opt.value"
        :checked="modelValue === opt.value"
        @change="emit('update:modelValue', opt.value)"
      />
      <span>{{ opt.label }}</span>
    </label>
  </span>
</template>
