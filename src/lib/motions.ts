/** 动态效果清单的唯一来源：设置页、配置恢复、预览页与灵动岛统一从这里读取。 */
export const MOTION_MODES = [
  'glide', 'bounce', 'sweep', 'focus', 'flicker'
] as const

export type MotionMode = typeof MOTION_MODES[number]

export const MOTION_LABELS: Record<MotionMode, string> = {
  glide: '丝滑流体',
  bounce: '果冻弹簧',
  sweep: '粘液蠕虫',
  focus: '景深聚拢',
  flicker: '极速瞬切'
}

export const MOTION_DESCRIPTIONS: Record<MotionMode, string> = {
  glide: '默认流体手感：强缓出顺滑开合，轻度开关拉伸与平滑分段滑块，兼顾细腻与效率。',
  bounce: '果冻弹簧手感：开关显著拉伸回弹、按压压扁回弹、书签收藏弹跳与轻盈物理余韵。',
  sweep: '粘液蠕虫手感：横展先导展开，分段滑块、导航蠕虫与灵动岛标签指示块移动时先拉长再收拢。',
  focus: '景深聚拢手感：展开与切页携带真实光学微焦虚化转清晰对焦与强磁吸附落位，凸显玻璃空间感。',
  flicker: '极速瞬切手感：面向高频操作，关闭拉伸与回弹，极短开合与纯透明瞬切，零等待响应。'
}

export const MOTION_TIMINGS: Record<MotionMode, { expandMs: number; collapseMs: number }> = {
  glide: { expandMs: 360, collapseMs: 240 },
  bounce: { expandMs: 520, collapseMs: 300 },
  sweep: { expandMs: 420, collapseMs: 280 },
  focus: { expandMs: 320, collapseMs: 200 },
  flicker: { expandMs: 45, collapseMs: 35 }
}

/** 兼容旧版配置与备份（unroll / step 均平滑映射至 glide）。 */
export const LEGACY_MOTION_MAP: Record<string, MotionMode> = {
  unroll: 'glide',
  step: 'glide'
}

export function isMotionMode(value: string): value is MotionMode {
  return (MOTION_MODES as readonly string[]).includes(value)
}

export function normalizeMotionMode(value: string): MotionMode {
  if (isMotionMode(value)) return value
  return LEGACY_MOTION_MAP[value] ?? 'glide'
}
