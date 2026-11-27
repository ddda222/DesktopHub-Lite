<script setup lang="ts" generic="T extends string">
import { computed, nextTick, onBeforeUnmount, ref, useId, watch } from 'vue'
import { Flip, animateFlipFromState } from '@/lib/gsapMotion'
import { activeMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
import { state } from '@/store'

/**
 * 基于 GSAP Flip 的分段单选滑块：
 * 选中项切换或换行时由 Flip 自动捕获前后几何状态，平滑滑移至新位置。
 */
const props = defineProps<{
  options: { value: T; label: string }[]
  modelValue: T
  name: string
  label: string
}>()

const emit = defineEmits<{ 'update:modelValue': [T] }>()
const uid = useId()
const groupEl = ref<HTMLElement | null>(null)
const flipId = `seg-thumb-${uid}`
let thumbTween: gsap.core.Timeline | null = null

const thumbTransition = computed(() =>
  state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.value.pill
)

watch(
  () => props.modelValue,
  () => {
    const root = groupEl.value
    if (!root || state.ui.islandReducedMotion) return
    if (thumbTween) {
      thumbTween.progress(1).kill()
      thumbTween = null
    }
    const prev = root.querySelector<HTMLElement>('.seg-thumb')
    if (!prev) return
    const flipState = Flip.getState(prev)
    void nextTick(() => {
      const next = root.querySelector<HTMLElement>('.seg-thumb')
      if (next) {
        thumbTween = animateFlipFromState(flipState, next, thumbTransition.value, { absolute: true })
      }
    })
  },
  { flush: 'pre' }
)

onBeforeUnmount(() => {
  thumbTween?.kill()
  thumbTween = null
})
</script>

<template>
  <span ref="groupEl" class="seg has-thumb" role="radiogroup" :aria-label="label">
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
      <span
        v-if="modelValue === opt.value"
        :data-flip-id="flipId"
        class="seg-thumb"
        aria-hidden="true"
      />
      <span class="seg-text">{{ opt.label }}</span>
    </label>
  </span>
</template>
