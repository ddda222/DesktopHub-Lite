import { computed } from 'vue'
import { MOTION_TIMINGS, normalizeMotionMode, type MotionMode } from './motions'
import { state } from '@/store'

export interface GsapTransitionSpec {
  duration: number
  ease: string
  delay?: number
}

export interface GsapPose {
  opacity?: number
  x?: number
  y?: number
  scale?: number
  scaleX?: number
  scaleY?: number
  rotate?: number
  filter?: string
  boxShadow?: string
}

export interface MotionPreset {
  mode: MotionMode
  /** 灵动岛展开：宽高与顶部距离曲线 */
  shellExpand: GsapTransitionSpec
  /** 粘液蠕虫分段延迟：先横向展开，慢半拍垂展高度 */
  shellExpandHeightDelay?: number
  /** 灵动岛收起：临界阻尼干脆归位，不拖尾、不反向回晃 */
  shellCollapse: GsapTransitionSpec
  /** 已展开状态切换功能页导致的高度调整 */
  shellResize: GsapTransitionSpec
  /** 共享指示器（分段滑块 / 导航蠕虫 / 顶部功能滑块 / 剪贴板模式条） */
  pill: GsapTransitionSpec
  /** 开关圆钮滑动曲线 */
  switchKnob: GsapTransitionSpec
  /** 开关按住时的轻微拉伸形变（flicker 与特殊几何主题下自动关闭） */
  switchPressScale: GsapPose
  /** 常规按钮、图标按钮、磁贴悬停 */
  tileHover: GsapPose
  /** 常规按钮按压触感 */
  buttonTap: GsapPose
  /** 图标卡片按压触感 */
  tileTap: GsapPose
  /** 悬浮弹层 / 菜单 / 提示进场 */
  popEnter: GsapPose
  popExit: GsapPose
  popTransition: GsapTransitionSpec
  /** 灵动岛切页进退场位移与形变参数 */
  paneTransition: GsapTransitionSpec
  paneOffsetX: number
  paneOffsetY: number
  paneInitialScale: number
  paneScaleX: number
  paneScaleY: number
  paneBlurPx: number
  /** 面板展开内容揭示曲线与初始偏移 */
  contentRevealTransition: GsapTransitionSpec
  contentOffsetY: number
  contentScaleX: number
  contentScaleY: number
  contentBlurPx: number
  /** 底部拉条松手弹性回位 */
  dismissSnap: GsapTransitionSpec
  /** 是否在展开瞬间需要给原生窗口预留微小弹簧过冲安全余量（px） */
  bounceHeadroom: number
}

const EASE_GLIDE = 'power3.out'
const EASE_BOUNCE = 'back.out(1.65)'
const EASE_SWEEP = 'elastic.out(0.95, 0.62)'
const EASE_FOCUS = 'expo.out'
const EASE_FLICKER = 'none'

export const MOTION_PRESETS: Record<MotionMode, MotionPreset> = {
  glide: {
    mode: 'glide',
    shellExpand: { duration: MOTION_TIMINGS.glide.expandMs / 1000, ease: EASE_GLIDE },
    shellCollapse: { duration: MOTION_TIMINGS.glide.collapseMs / 1000, ease: 'power2.out' },
    shellResize: { duration: 0.22, ease: EASE_GLIDE },
    pill: { duration: 0.28, ease: EASE_GLIDE },
    switchKnob: { duration: 0.22, ease: EASE_GLIDE },
    switchPressScale: { scaleX: 1.12, scaleY: 0.94 },
    tileHover: { scale: 1.04, y: -1.5 },
    buttonTap: { scale: 0.955 },
    tileTap: { scale: 0.96 },
    popEnter: { opacity: 1, y: 0, scale: 1 },
    popExit: { opacity: 0, y: 8, scale: 0.97 },
    popTransition: { duration: 0.22, ease: EASE_GLIDE },
    paneTransition: { duration: 0.24, ease: EASE_GLIDE },
    paneOffsetX: 28,
    paneOffsetY: 0,
    paneInitialScale: 0.985,
    paneScaleX: 0.985,
    paneScaleY: 0.985,
    paneBlurPx: 0,
    contentRevealTransition: { duration: 0.22, delay: 0.03, ease: EASE_GLIDE },
    contentOffsetY: 8,
    contentScaleX: 0.98,
    contentScaleY: 0.98,
    contentBlurPx: 0,
    dismissSnap: { duration: 0.3, ease: EASE_GLIDE },
    bounceHeadroom: 0
  },
  bounce: {
    mode: 'bounce',
    shellExpand: { duration: MOTION_TIMINGS.bounce.expandMs / 1000, ease: 'elastic.out(1.02, 0.56)' },
    shellCollapse: { duration: MOTION_TIMINGS.bounce.collapseMs / 1000, ease: 'back.inOut(1.15)' },
    shellResize: { duration: 0.24, ease: 'back.out(1.22)' },
    pill: { duration: 0.34, ease: EASE_BOUNCE },
    switchKnob: { duration: 0.3, ease: EASE_BOUNCE },
    switchPressScale: { scaleX: 1.32, scaleY: 0.8 },
    tileHover: { scale: 1.07, y: -2.5 },
    buttonTap: { scaleX: 1.1, scaleY: 0.86 },
    tileTap: { scale: 0.91 },
    popEnter: { opacity: 1, y: 0, scale: 1 },
    popExit: { opacity: 0, y: 12, scale: 0.9 },
    popTransition: { duration: 0.32, ease: EASE_BOUNCE },
    paneTransition: { duration: 0.34, ease: EASE_BOUNCE },
    paneOffsetX: 20,
    paneOffsetY: 16,
    paneInitialScale: 0.9,
    paneScaleX: 0.9,
    paneScaleY: 1.08,
    paneBlurPx: 0,
    contentRevealTransition: { duration: 0.34, delay: 0.04, ease: EASE_BOUNCE },
    contentOffsetY: 14,
    contentScaleX: 0.92,
    contentScaleY: 0.92,
    contentBlurPx: 0,
    dismissSnap: { duration: 0.42, ease: 'elastic.out(1.05, 0.58)' },
    bounceHeadroom: 22
  },
  sweep: {
    mode: 'sweep',
    // 粘液蠕虫特征：先横向拉开，慢半拍再垂展高度；收起先压扁，再缩窄。
    shellExpand: { duration: MOTION_TIMINGS.sweep.expandMs / 1000, ease: EASE_SWEEP },
    shellExpandHeightDelay: 0.085,
    shellCollapse: { duration: MOTION_TIMINGS.sweep.collapseMs / 1000, ease: 'power3.inOut' },
    shellResize: { duration: 0.24, ease: 'power3.out' },
    pill: { duration: 0.36, ease: EASE_SWEEP },
    switchKnob: { duration: 0.32, ease: EASE_SWEEP },
    switchPressScale: { scaleX: 1.24, scaleY: 0.86 },
    tileHover: { scaleX: 1.05, scaleY: 0.98, y: -1 },
    buttonTap: { scaleX: 1.04, scaleY: 0.93 },
    tileTap: { scale: 0.945 },
    popEnter: { opacity: 1, y: 0, scale: 1 },
    popExit: { opacity: 0, y: 8, scale: 0.95 },
    popTransition: { duration: 0.28, ease: 'power3.out' },
    paneTransition: { duration: 0.32, ease: 'back.out(1.2)' },
    paneOffsetX: 56,
    paneOffsetY: 0,
    paneInitialScale: 0.98,
    paneScaleX: 1.14,
    paneScaleY: 0.92,
    paneBlurPx: 0,
    contentRevealTransition: { duration: 0.28, delay: 0.06, ease: EASE_FOCUS },
    contentOffsetY: -8,
    contentScaleX: 1.06,
    contentScaleY: 0.92,
    contentBlurPx: 0,
    dismissSnap: { duration: 0.38, ease: 'elastic.out(0.95, 0.64)' },
    bounceHeadroom: 8
  },
  focus: {
    mode: 'focus',
    // 景深聚拢：高速磁吸起势 + 光学对焦缩放，适合分层玻璃空间层次。
    shellExpand: { duration: MOTION_TIMINGS.focus.expandMs / 1000, ease: EASE_FOCUS },
    shellCollapse: { duration: MOTION_TIMINGS.focus.collapseMs / 1000, ease: 'power3.out' },
    shellResize: { duration: 0.2, ease: EASE_FOCUS },
    pill: { duration: 0.22, ease: EASE_FOCUS },
    switchKnob: { duration: 0.2, ease: EASE_FOCUS },
    switchPressScale: { scaleX: 1.08, scaleY: 0.95 },
    tileHover: { scale: 1.055, y: -2.5 },
    buttonTap: { scale: 0.945, y: 1 },
    tileTap: { scale: 0.94, y: 1 },
    popEnter: { opacity: 1, y: 0, scale: 1 },
    popExit: { opacity: 0, y: 4, scale: 0.93 },
    popTransition: { duration: 0.22, ease: EASE_FOCUS },
    paneTransition: { duration: 0.24, ease: EASE_FOCUS },
    paneOffsetX: 0,
    paneOffsetY: 10,
    paneInitialScale: 0.93,
    paneScaleX: 0.93,
    paneScaleY: 0.93,
    paneBlurPx: 6,
    contentRevealTransition: { duration: 0.24, delay: 0.02, ease: EASE_FOCUS },
    contentOffsetY: 10,
    contentScaleX: 0.94,
    contentScaleY: 0.94,
    contentBlurPx: 5,
    dismissSnap: { duration: 0.24, ease: EASE_FOCUS },
    bounceHeadroom: 4
  },
  flicker: {
    mode: 'flicker',
    shellExpand: { duration: MOTION_TIMINGS.flicker.expandMs / 1000, ease: EASE_FLICKER },
    shellCollapse: { duration: MOTION_TIMINGS.flicker.collapseMs / 1000, ease: EASE_FLICKER },
    shellResize: { duration: 0.035, ease: EASE_FLICKER },
    pill: { duration: 0.04, ease: EASE_FLICKER },
    switchKnob: { duration: 0.04, ease: EASE_FLICKER },
    switchPressScale: { scaleX: 1, scaleY: 1 },
    tileHover: { scale: 1 },
    buttonTap: { scale: 0.975 },
    tileTap: { scale: 0.975 },
    popEnter: { opacity: 1, y: 0, scale: 1 },
    popExit: { opacity: 0, y: 0, scale: 1 },
    popTransition: { duration: 0.04, ease: EASE_FLICKER },
    paneTransition: { duration: 0.045, ease: EASE_FLICKER },
    paneOffsetX: 0,
    paneOffsetY: 0,
    paneInitialScale: 1,
    paneScaleX: 1,
    paneScaleY: 1,
    paneBlurPx: 0,
    contentRevealTransition: { duration: 0.04, delay: 0, ease: EASE_FLICKER },
    contentOffsetY: 0,
    contentScaleX: 1,
    contentScaleY: 1,
    contentBlurPx: 0,
    dismissSnap: { duration: 0.045, ease: EASE_FLICKER },
    bounceHeadroom: 0
  }
}

export const INSTANT_TRANSITION: GsapTransitionSpec = { duration: 0.001, ease: 'none' }

export function getMotionPreset(mode: string | MotionMode = state.ui.islandAppearance.motion): MotionPreset {
  return MOTION_PRESETS[normalizeMotionMode(mode)]
}

/** 响应式当前全局动效预设；所有组件直接读取，无需重复规范化。 */
export const activeMotionPreset = computed<MotionPreset>(() =>
  MOTION_PRESETS[normalizeMotionMode(state.ui.islandAppearance.motion)]
)

/** 结合系统 prefers-reduced-motion 的全局减少动效标志。 */
export const activeReducedMotion = computed<'always' | 'user'>(() =>
  state.ui.islandReducedMotion ? 'always' : 'user'
)
