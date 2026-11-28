import gsap from 'gsap'
import { CustomEase } from 'gsap/CustomEase'
import { CustomBounce } from 'gsap/CustomBounce'
import { CustomWiggle } from 'gsap/CustomWiggle'
import { Draggable } from 'gsap/Draggable'
import { DrawSVGPlugin } from 'gsap/DrawSVGPlugin'
import { Flip } from 'gsap/Flip'
import { InertiaPlugin } from 'gsap/InertiaPlugin'
import { MorphSVGPlugin } from 'gsap/MorphSVGPlugin'
import { MotionPathPlugin } from 'gsap/MotionPathPlugin'
import { Physics2DPlugin } from 'gsap/Physics2DPlugin'
import { ScrambleTextPlugin } from 'gsap/ScrambleTextPlugin'
import { SplitText } from 'gsap/SplitText'
import type { Directive } from 'vue'
import type { GsapPose, GsapTransitionSpec } from './motionPresets'

let registered = false

/** 全局注册 GSAP 核心与全套官方插件（含 MorphSVG / SplitText / ScrambleText / Physics2D / Flip / Draggable 等）。 */
export function ensureGsapRegistered(): typeof gsap {
  if (!registered) {
    gsap.registerPlugin(
      CustomEase,
      CustomBounce,
      CustomWiggle,
      Draggable,
      DrawSVGPlugin,
      Flip,
      InertiaPlugin,
      MorphSVGPlugin,
      MotionPathPlugin,
      Physics2DPlugin,
      ScrambleTextPlugin,
      SplitText
    )
    CustomBounce.create('hubBounceDrop', { strength: 0.42, squash: 2, squashID: 'hubBounceDropSquash' })
    CustomWiggle.create('hubWiggleReject', { wiggles: 6, type: 'anticipate' })
    CustomWiggle.create('hubWiggleJitter', { wiggles: 5, type: 'easeOut' })
    // 失焦切窗口等场景 WebView2 偶发 140–240ms 单帧调度抖动，放低 lagSmoothing 阈值防止短收起补间被单帧吞光。
    gsap.ticker.lagSmoothing(120, 16)
    registered = true
  }
  return gsap
}

ensureGsapRegistered()

export {
  gsap,
  Flip,
  Draggable,
  InertiaPlugin,
  MorphSVGPlugin,
  DrawSVGPlugin,
  MotionPathPlugin,
  Physics2DPlugin,
  ScrambleTextPlugin,
  SplitText,
  CustomEase,
  CustomBounce,
  CustomWiggle
}

interface InteractiveBindingState {
  pose?: GsapPose
  hover?: GsapPose
  pressed: boolean
  hovered: boolean
  tween: gsap.core.Tween | null
  onPointerDown: () => void
  onPointerUp: () => void
  onPointerEnter: () => void
  onPointerLeave: () => void
}

const interactiveMap = new WeakMap<HTMLElement, InteractiveBindingState>()

function isDisabled(el: HTMLElement): boolean {
  return (el as HTMLButtonElement).disabled === true || el.getAttribute('aria-disabled') === 'true'
}

function applyInteractiveTarget(el: HTMLElement, state: InteractiveBindingState): void {
  const target = state.pressed && state.pose
    ? state.pose
    : state.hovered && state.hover
      ? state.hover
      : { scale: 1, scaleX: 1, scaleY: 1, x: 0, y: 0, boxShadow: 'none' }

  // 仅清理实际由交互驱动的属性，避免清掉组件自身 transform/boxShadow
  const vars: gsap.TweenVars = {
    duration: state.pressed ? 0.11 : 0.24,
    ease: state.pressed ? 'power2.out' : 'back.out(1.6)',
    overwrite: 'auto'
  }
  if (state.pressed && state.pose) {
    Object.assign(vars, {
      scale: state.pose.scale ?? 1,
      scaleX: state.pose.scaleX ?? state.pose.scale ?? 1,
      scaleY: state.pose.scaleY ?? state.pose.scale ?? 1,
      y: state.pose.y ?? 0
    })
  } else if (state.hovered && state.hover) {
    Object.assign(vars, {
      scale: state.hover.scale ?? 1,
      scaleX: state.hover.scaleX ?? state.hover.scale ?? 1,
      scaleY: state.hover.scaleY ?? state.hover.scale ?? 1,
      y: state.hover.y ?? 0
    })
  } else {
    Object.assign(vars, { scale: 1, scaleX: 1, scaleY: 1, y: 0 })
  }
  state.tween?.kill()
  state.tween = gsap.to(el, vars)
}

function ensureInteractive(el: HTMLElement): InteractiveBindingState {
  let entry = interactiveMap.get(el)
  if (!entry) {
    entry = {
      pressed: false,
      hovered: false,
      tween: null,
      onPointerDown: () => {
        if (!entry?.pose || isDisabled(el)) return
        entry.pressed = true
        applyInteractiveTarget(el, entry)
      },
      onPointerUp: () => {
        if (!entry || !entry.pressed) return
        entry.pressed = false
        applyInteractiveTarget(el, entry)
      },
      onPointerEnter: () => {
        if (!entry?.hover || isDisabled(el)) return
        entry.hovered = true
        if (!entry.pressed) applyInteractiveTarget(el, entry)
      },
      onPointerLeave: () => {
        if (!entry) return
        const wasActive = entry.hovered || entry.pressed
        entry.hovered = false
        entry.pressed = false
        if (wasActive && (entry.hover || entry.pose)) applyInteractiveTarget(el, entry)
      }
    }
    el.addEventListener('pointerdown', entry.onPointerDown)
    el.addEventListener('pointerup', entry.onPointerUp)
    el.addEventListener('pointercancel', entry.onPointerUp)
    el.addEventListener('pointerenter', entry.onPointerEnter)
    el.addEventListener('pointerleave', entry.onPointerLeave)
    interactiveMap.set(el, entry)
  }
  return entry
}

function cleanupInteractiveIfUnused(el: HTMLElement): void {
  const entry = interactiveMap.get(el)
  if (!entry || entry.pose || entry.hover) return
  entry.tween?.kill()
  entry.tween = null
  el.removeEventListener('pointerdown', entry.onPointerDown)
  el.removeEventListener('pointerup', entry.onPointerUp)
  el.removeEventListener('pointercancel', entry.onPointerUp)
  el.removeEventListener('pointerenter', entry.onPointerEnter)
  el.removeEventListener('pointerleave', entry.onPointerLeave)
  interactiveMap.delete(el)
}

/** 按压弹簧反馈指令（替代 motion-v `:while-press`） */
export const vGsapPress: Directive<HTMLElement, GsapPose | undefined> = {
  mounted(el, binding) {
    const entry = ensureInteractive(el)
    entry.pose = binding.value
  },
  updated(el, binding) {
    const entry = ensureInteractive(el)
    entry.pose = binding.value
    if (!binding.value && entry.pressed) {
      entry.pressed = false
      applyInteractiveTarget(el, entry)
    }
  },
  unmounted(el) {
    // 仅销毁指令自身的按压补间，绝不调用 killTweensOf(el)，避免打断 GsapPresence @leave 退场动画。
    const entry = interactiveMap.get(el)
    if (entry) {
      entry.pressed = false
      entry.pose = undefined
      cleanupInteractiveIfUnused(el)
    }
  }
}

/** 悬停抬升指令（替代 motion-v `:while-hover`） */
export const vGsapHover: Directive<HTMLElement, GsapPose | undefined> = {
  mounted(el, binding) {
    const entry = ensureInteractive(el)
    entry.hover = binding.value
  },
  updated(el, binding) {
    const entry = ensureInteractive(el)
    entry.hover = binding.value
    if (!binding.value && entry.hovered) {
      entry.hovered = false
      applyInteractiveTarget(el, entry)
    }
  },
  unmounted(el) {
    const entry = interactiveMap.get(el)
    if (entry) {
      entry.hovered = false
      entry.hover = undefined
      cleanupInteractiveIfUnused(el)
    }
  }
}

/** 执行一次基于 GSAP Flip 的指示滑块位移动画。 */
export function animateFlipFromState(
  prevState: Flip.FlipState | null | undefined,
  targets: gsap.DOMTarget,
  transition: GsapTransitionSpec,
  options: { scale?: boolean; absolute?: boolean } = {}
): gsap.core.Timeline | null {
  if (!prevState) return null
  return Flip.from(prevState, {
    targets,
    duration: Math.max(0.001, transition.duration),
    ease: transition.ease,
    delay: transition.delay ?? 0,
    scale: options.scale ?? false,
    absolute: options.absolute ?? false,
    prune: true,
    simple: true
  })
}

const splitInstanceMap = new WeakMap<HTMLElement, SplitText>()

/** SplitText 轻量逐字入场（仅用于简短装饰标题或胶囊反馈，不用于可选中复制正文）。 */
export function animateSplitReveal(
  el: HTMLElement | null | undefined,
  reduced: boolean,
  options: { y?: number; stagger?: number; duration?: number; ease?: string } = {}
): void {
  if (!el || reduced) return
  const prev = splitInstanceMap.get(el)
  if (prev) {
    gsap.killTweensOf(prev.chars)
    prev.revert()
    splitInstanceMap.delete(el)
  }
  const split = SplitText.create(el, { type: 'chars' })
  splitInstanceMap.set(el, split)
  gsap.fromTo(
    split.chars,
    { opacity: 0, y: options.y ?? 7, scale: 0.9 },
    {
      opacity: 1,
      y: 0,
      scale: 1,
      duration: options.duration ?? 0.24,
      stagger: options.stagger ?? 0.015,
      ease: options.ease ?? 'back.out(1.6)',
      clearProps: 'transform,opacity',
      onComplete: () => {
        if (splitInstanceMap.get(el) === split) {
          split.revert()
          splitInstanceMap.delete(el)
        }
      }
    }
  )
}

/** ScrambleText 字符矩阵解码动效（用于验证码与胶囊短反馈）。 */
export function animateScrambleText(
  el: HTMLElement | null | undefined,
  nextText: string,
  reduced: boolean,
  options: { chars?: string; duration?: number; speed?: number } = {}
): void {
  if (!el) return
  gsap.killTweensOf(el)
  if (reduced || !nextText) {
    el.textContent = nextText
    return
  }
  gsap.to(el, {
    duration: options.duration ?? 0.42,
    scrambleText: {
      text: nextText,
      chars: options.chars ?? '0123456789ABCDEF#*',
      speed: options.speed ?? 0.72,
      revealDelay: 0.04
    },
    ease: 'none'
  })
}

/** 捕获容器内匹配元素在 DOM 更新前的 Flip 布局快照，供后续 playFlipLayout 使用。 */
export function captureFlipSnapshot(
  container: HTMLElement | null | undefined,
  selector: string,
  reduced: boolean,
  maxItems = 36
): Flip.FlipState | null {
  if (!container || reduced) return null
  const targets = Array.from(container.querySelectorAll<HTMLElement>(selector))
  if (!targets.length || targets.length > maxItems) return null
  return Flip.getState(targets)
}

/** 在 DOM 更新后播放 Flip 位置重排动画。 */
export function playFlipSnapshot(
  prevState: Flip.FlipState | null | undefined,
  container: HTMLElement | null | undefined,
  selector: string,
  spec: GsapTransitionSpec,
  options: { absolute?: boolean; nested?: boolean; stagger?: number; maxItems?: number } = {}
): void {
  if (!prevState || !container) return
  const targets = Array.from(container.querySelectorAll<HTMLElement>(selector))
  if (!targets.length || targets.length > (options.maxItems ?? 36)) return
  Flip.from(prevState, {
    targets,
    duration: Math.max(0.08, Math.min(0.45, spec.duration)),
    ease: spec.ease === 'none' ? 'power2.out' : spec.ease,
    stagger: options.stagger ?? 0.014,
    absolute: options.absolute ?? false,
    nested: options.nested ?? false,
    prune: true,
    onEnter: (els) =>
      gsap.fromTo(els, { opacity: 0, scale: 0.9 }, { opacity: 1, scale: 1, duration: 0.22, clearProps: 'transform,opacity' })
  })
}

/** 脉冲呼吸动画辅助（替代 motion-v 无限循环 :animate 数组关键帧）。 */
export interface GsapPulseConfig {
  opacity?: [number, number]
  scale?: [number, number]
  scaleY?: [number, number]
  duration?: number
  delay?: number
}

export function startPulseLoop(
  el: HTMLElement | null | undefined,
  vars: GsapPulseConfig
): gsap.core.Tween | null {
  if (!el) return null
  const fromVars: gsap.TweenVars = {}
  const toVars: gsap.TweenVars = {
    duration: (vars.duration ?? 1.2) * 0.5,
    repeat: -1,
    yoyo: true,
    ease: 'sine.inOut',
    delay: vars.delay ?? 0
  }
  if (vars.opacity) {
    fromVars.opacity = vars.opacity[0]
    toVars.opacity = vars.opacity[1]
  }
  if (vars.scale) {
    fromVars.scale = vars.scale[0]
    toVars.scale = vars.scale[1]
  }
  if (vars.scaleY) {
    fromVars.scaleY = vars.scaleY[0]
    toVars.scaleY = vars.scaleY[1]
  }
  return gsap.fromTo(el, fromVars, toVars)
}

const pulseTweenMap = new WeakMap<HTMLElement, { key: string; tween: gsap.core.Tween }>()

function syncPulseDirective(el: HTMLElement, cfg: GsapPulseConfig | false | null | undefined): void {
  const prev = pulseTweenMap.get(el)
  if (!cfg) {
    if (prev) {
      prev.tween.kill()
      pulseTweenMap.delete(el)
    }
    gsap.set(el, { clearProps: 'transform,opacity' })
    return
  }
  const key = JSON.stringify(cfg)
  if (prev?.key === key) return
  prev?.tween.kill()
  const tween = startPulseLoop(el, cfg)
  if (tween) pulseTweenMap.set(el, { key, tween })
}

/** 脉冲呼吸指令（替代循环关键帧数组）。 */
export const vGsapPulse: Directive<HTMLElement, GsapPulseConfig | false | null | undefined> = {
  mounted(el, binding) {
    syncPulseDirective(el, binding.value)
  },
  updated(el, binding) {
    syncPulseDirective(el, binding.value)
  },
  unmounted(el) {
    const prev = pulseTweenMap.get(el)
    if (prev) {
      prev.tween.kill()
      pulseTweenMap.delete(el)
    }
  }
}
