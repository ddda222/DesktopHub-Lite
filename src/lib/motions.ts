/** 动态效果清单的唯一来源：设置页、配置恢复、预览页与灵动岛统一从这里读取。 */
export const MOTION_MODES = [
  'glide', 'bounce', 'flicker'
] as const

export type MotionMode = typeof MOTION_MODES[number]

export const MOTION_LABELS: Record<MotionMode, string> = {
  glide: '流体灵动',
  bounce: '轻弹物理',
  flicker: '极速脆响'
}

export const MOTION_DESCRIPTIONS: Record<MotionMode, string> = {
  glide: '默认流体效率手感：强缓出顺滑展开，临界阻尼干脆收回，切页零拖尾。',
  bounce: '带有克制物理微弹的开合手感，收回保持果断零回晃，适合喜欢弹性反馈。',
  flicker: '极致响应速度：极短开合与瞬切过渡，高频操作几乎零等待。'
}

export const MOTION_TIMINGS: Record<MotionMode, { expandMs: number; collapseMs: number }> = {
  glide: { expandMs: 220, collapseMs: 155 },
  bounce: { expandMs: 285, collapseMs: 165 },
  flicker: { expandMs: 130, collapseMs: 95 }
}

/** 兼容旧版 6 套动效配置与备份，自动映射到对应的核心手感。 */
export const LEGACY_MOTION_MAP: Record<string, MotionMode> = {
  sweep: 'glide',
  unroll: 'glide',
  step: 'flicker'
}

export function isMotionMode(value: string): value is MotionMode {
  return (MOTION_MODES as readonly string[]).includes(value)
}

export function normalizeMotionMode(value: string): MotionMode {
  if (isMotionMode(value)) return value
  return LEGACY_MOTION_MAP[value] ?? 'glide'
}
