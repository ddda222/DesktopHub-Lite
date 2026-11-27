<script setup lang="ts">
import { watch } from 'vue'
import { gsap } from '@/lib/gsapMotion'
import { activeReducedMotion } from '@/lib/motionPresets'

/**
 * 全局 GSAP 动效上下文根包装：
 * - 将软件内「减少动画」状态实时同步给 GSAP 全局时间轴倍率（减少动画开启时瞬间收敛）；
 * - 统一保持页面挂载根接口不变。
 */
watch(
  activeReducedMotion,
  (mode) => {
    gsap.globalTimeline.timeScale(mode === 'always' ? 100 : 1)
  },
  { immediate: true }
)
</script>

<template>
  <slot />
</template>
