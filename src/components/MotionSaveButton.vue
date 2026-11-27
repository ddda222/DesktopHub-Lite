<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import GsapPresence from '@/components/GsapPresence.vue'
import { gsap, vGsapPress } from '@/lib/gsapMotion'
import { activeMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
import { state } from '@/store'

const props = withDefaults(defineProps<{
  label: string
  busy?: boolean
  done?: boolean
  disabled?: boolean
  variant?: 'primary' | 'accent' | 'default'
}>(), {
  busy: false,
  done: false,
  disabled: false,
  variant: 'primary'
})

const emit = defineEmits<{ click: [MouseEvent] }>()

const checkPathEl = ref<SVGPathElement | null>(null)
const modeKey = computed(() => (props.busy ? 'busy' : props.done ? 'done' : 'idle'))

watch(modeKey, (mode) => {
  if (mode !== 'done') return
  void nextTick(() => {
    if (!checkPathEl.value) return
    if (state.ui.islandReducedMotion) {
      gsap.set(checkPathEl.value, { drawSVG: '0% 100%' })
    } else {
      gsap.fromTo(checkPathEl.value, { drawSVG: '0% 0%' }, { drawSVG: '0% 100%', duration: 0.28, ease: 'power3.out' })
    }
  })
})

const pressPose = computed(() => {
  if (props.disabled || state.ui.islandReducedMotion) return undefined
  return activeMotionPreset.value.buttonTap
})

const stateTransition = computed(() =>
  state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.value.popTransition
)

const stateInitial = computed(() => {
  if (state.ui.islandReducedMotion) return false as const
  if (modeKey.value === 'busy') return { opacity: 0, scale: 0.72 }
  if (modeKey.value === 'done') return { opacity: 0, scale: 0.6 }
  return { opacity: 0, y: 4 }
})

const stateExit = computed(() => {
  if (state.ui.islandReducedMotion) return { opacity: 0 }
  if (modeKey.value === 'busy') return { opacity: 0, scale: 0.72 }
  if (modeKey.value === 'done') return { opacity: 0, scale: 0.75 }
  return { opacity: 0, y: -4 }
})
</script>

<template>
  <button
    v-gsap-press="pressPose"
    type="button"
    class="btn btn-motion-stateful"
    :class="{
      'btn-primary': variant === 'primary',
      'btn-accent': variant === 'accent'
    }"
    :disabled="disabled"
    @click="emit('click', $event)"
  >
    <!-- 用隐藏的基准标签撑住按钮真实宽度，状态切换不抖动 -->
    <span class="btn-label-sizer" aria-hidden="true">{{ label }}</span>
    <GsapPresence
      mode="out-in"
      :initial="stateInitial"
      :exit="stateExit"
      :transition="modeKey === 'done' ? stateTransition : { duration: 0.12, ease: 'power2.out' }"
    >
      <span
        v-if="modeKey === 'busy'"
        key="busy"
        class="btn-state-layer"
        aria-hidden="true"
      >
        <span class="btn-spinner" />
      </span>
      <span
        v-else-if="modeKey === 'done'"
        key="done"
        class="btn-state-layer btn-check"
        aria-hidden="true"
      >
        <svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round">
          <path ref="checkPathEl" d="M5 12.5 L10 17.5 L19.5 7" />
        </svg>
      </span>
      <span
        v-else
        key="idle"
        class="btn-state-layer"
      >
        {{ label }}
      </span>
    </GsapPresence>
  </button>
</template>
