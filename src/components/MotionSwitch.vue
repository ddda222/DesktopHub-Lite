<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import { Flip, animateFlipFromState, vGsapPress } from '@/lib/gsapMotion'
import { activeMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
import { state } from '@/store'

const props = withDefaults(defineProps<{
  checked: boolean
  disabled?: boolean
  label?: string
  asDecorative?: boolean
}>(), {
  disabled: false,
  label: undefined,
  asDecorative: false
})

const emit = defineEmits<{
  toggle: []
}>()

const knobEl = ref<HTMLElement | null>(null)

const preserveGeometryTheme = computed(() => state.settings.theme === 'pixel')

const knobTransition = computed(() => {
  if (state.ui.islandReducedMotion) return INSTANT_TRANSITION
  return activeMotionPreset.value.switchKnob
})

const pressPose = computed(() => {
  if (props.disabled || state.ui.islandReducedMotion || preserveGeometryTheme.value) return undefined
  return activeMotionPreset.value.switchPressScale
})

watch(
  () => props.checked,
  () => {
    const el = knobEl.value
    if (!el || state.ui.islandReducedMotion) return
    const flipState = Flip.getState(el)
    void nextTick(() => {
      if (knobEl.value) {
        animateFlipFromState(flipState, knobEl.value, knobTransition.value)
      }
    })
  },
  { flush: 'pre' }
)
</script>

<template>
  <span
    v-if="asDecorative"
    class="switch motion-switch"
    :class="{ 'is-on': checked }"
    aria-hidden="true"
  >
    <span ref="knobEl" class="switch-knob" />
  </span>
  <button
    v-else
    class="switch motion-switch"
    type="button"
    role="switch"
    :class="{ 'is-on': checked }"
    :aria-label="label"
    :aria-checked="checked"
    :disabled="disabled"
    @click="emit('toggle')"
  >
    <span
      ref="knobEl"
      v-gsap-press="pressPose"
      class="switch-knob"
      aria-hidden="true"
    />
  </button>
</template>
