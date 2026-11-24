/** 动态效果清单的唯一来源：设置页、配置恢复、预览页与灵动岛统一从这里读取。 */
export const MOTION_MODES = [
  'glide', 'sweep', 'bounce', 'step', 'flicker', 'unroll'
] as const

export type MotionMode = typeof MOTION_MODES[number]

export const MOTION_LABELS: Record<MotionMode, string> = {
  glide: '柔和舒展',
  sweep: '先宽后高',
  bounce: '轻弹回落',
  step: '逐级开合',
  flicker: '快速开合',
  unroll: '垂直抽展'
}

export const MOTION_DESCRIPTIONS: Record<MotionMode, string> = {
  glide: '胶囊向两侧舒展，底边随即放下。收起时，内容随底边一起退回。',
  sweep: '胶囊先拉宽成横条，再向下打开。收起时先折回高度，最后收窄。',
  bounce: '底边略过目标位置，再轻轻弹回。收起时，图标随内容一起收拢。',
  step: '宽和高交替进位，面板分四级打开，再逐级收回。',
  flicker: '宽和高同时快速到位，工具栏和内容一起显现。',
  unroll: '底边先向下伸展，两侧随后打开。收起时，两侧先向中间靠拢。'
}

export const MOTION_TIMINGS: Record<MotionMode, { expandMs: number; collapseMs: number }> = {
  glide: { expandMs: 270, collapseMs: 180 },
  sweep: { expandMs: 310, collapseMs: 200 },
  bounce: { expandMs: 360, collapseMs: 195 },
  step: { expandMs: 260, collapseMs: 175 },
  flicker: { expandMs: 145, collapseMs: 110 },
  unroll: { expandMs: 300, collapseMs: 195 }
}

export function isMotionMode(value: string): value is MotionMode {
  return (MOTION_MODES as readonly string[]).includes(value)
}
