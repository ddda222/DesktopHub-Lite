<script setup lang="ts">
/**
 * 基于 GSAP 的挂载/卸载过渡容器（替代 motion-v AnimatePresence）：
 * - 支持 `mode="out-in"`（对应 wait）与默认并存模式；
 * - 支持初态/终态/退场态以及 `filter: blur(...)` 等 CSS 滤镜插值；
 * - 支持 `@after-enter` / `@animation-complete` 回调，便于灵动岛测高与弹层同步窗口。
 */
import type { GsapPose, GsapTransitionSpec } from '@/lib/motionPresets'
import { gsap } from '@/lib/gsapMotion'

const props = withDefaults(defineProps<{
  initial?: GsapPose | false
  animate?: GsapPose
  exit?: GsapPose
  transition?: GsapTransitionSpec
  mode?: 'out-in' | 'in-out' | 'default'
  appear?: boolean
  group?: boolean
  tag?: string
}>(), {
  initial: undefined,
  animate: () => ({ opacity: 1, x: 0, y: 0, scale: 1, scaleX: 1, scaleY: 1, filter: 'blur(0px)' }),
  exit: undefined,
  transition: () => ({ duration: 0.22, ease: 'power3.out' }),
  mode: 'default',
  appear: false,
  group: false,
  tag: 'div'
})

const emit = defineEmits<{
  'animation-complete': []
}>()

function buildFromVars(pose: GsapPose): gsap.TweenVars {
  const vars: gsap.TweenVars = {}
  if (pose.opacity !== undefined) vars.opacity = pose.opacity
  if (pose.x !== undefined) vars.x = pose.x
  if (pose.y !== undefined) vars.y = pose.y
  if (pose.scale !== undefined) vars.scale = pose.scale
  if (pose.scaleX !== undefined) vars.scaleX = pose.scaleX
  if (pose.scaleY !== undefined) vars.scaleY = pose.scaleY
  if (pose.rotate !== undefined) vars.rotate = pose.rotate
  if (pose.filter !== undefined) vars.filter = pose.filter
  return vars
}

function buildToVars(pose: GsapPose): gsap.TweenVars {
  const vars: gsap.TweenVars = {}
  if (pose.opacity !== undefined) vars.opacity = pose.opacity
  if (pose.x !== undefined) vars.x = pose.x
  if (pose.y !== undefined) vars.y = pose.y
  if (pose.scale !== undefined) vars.scale = pose.scale
  if (pose.scaleX !== undefined) vars.scaleX = pose.scaleX
  if (pose.scaleY !== undefined) vars.scaleY = pose.scaleY
  if (pose.rotate !== undefined) vars.rotate = pose.rotate
  if (pose.filter !== undefined) vars.filter = pose.filter
  return vars
}

function onEnter(el: Element, done: () => void): void {
  const target = el as HTMLElement
  gsap.killTweensOf(target)
  const spec = props.transition
  if (props.initial === false) {
    gsap.set(target, { clearProps: 'transform,opacity,filter' })
    done()
    emit('animation-complete')
    return
  }
  const fromPose = props.initial ?? { opacity: 0, y: 6, scale: 0.98 }
  const toPose: GsapPose = {
    opacity: 1,
    x: 0,
    y: 0,
    scale: 1,
    scaleX: 1,
    scaleY: 1,
    filter: 'blur(0px)',
    ...props.animate
  }
  gsap.fromTo(target, buildFromVars(fromPose), {
    ...buildToVars(toPose),
    duration: Math.max(0.001, spec.duration),
    delay: spec.delay ?? 0,
    ease: spec.ease,
    clearProps: 'transform,filter',
    onComplete: () => {
      done()
      emit('animation-complete')
    }
  })
}

function onLeave(el: Element, done: () => void): void {
  const target = el as HTMLElement
  gsap.killTweensOf(target)
  const spec = props.transition
  const exitPose = props.exit ?? { opacity: 0 }
  gsap.to(target, {
    ...buildToVars(exitPose),
    duration: Math.max(0.001, Math.min(0.24, spec.duration)),
    ease: 'power2.in',
    onComplete: done
  })
}

function onCancel(el: Element): void {
  gsap.killTweensOf(el)
}
</script>

<template>
  <TransitionGroup
    v-if="group"
    :tag="tag"
    :css="false"
    :appear="appear"
    @enter="onEnter"
    @leave="onLeave"
    @enter-cancelled="onCancel"
    @leave-cancelled="onCancel"
  >
    <slot />
  </TransitionGroup>
  <Transition
    v-else
    :css="false"
    :mode="mode === 'default' ? undefined : mode"
    :appear="appear"
    @enter="onEnter"
    @leave="onLeave"
    @enter-cancelled="onCancel"
    @leave-cancelled="onCancel"
  >
    <slot />
  </Transition>
</template>
