/** 灵动岛全局形态清单的唯一来源：设置页、配置恢复、预览栏与灵动岛统一从这里读取。 */
export const ISLAND_SHAPES = ['floating', 'notch', 'split'] as const

export type IslandShape = typeof ISLAND_SHAPES[number]

export const ISLAND_SHAPE_LABELS: Record<IslandShape, string> = {
  floating: '悬浮胶囊',
  notch: '贴顶刘海',
  split: '贴顶分层'
}

export const ISLAND_SHAPE_DESCRIPTIONS: Record<IslandShape, string> = {
  floating: '经典顶部悬浮胶囊，保留轻微顶边距与四周完整轮廓，适配全部主题。',
  notch: '彻底贴合屏幕顶沿，左右带主题专属数学连续融合肩部，展开时整块刘海顺滑垂展。',
  split: '顶部保持紧凑贴顶母舰导航栏，展开时下方主功能面板作为独立悬浮硬件卡片错峰浮现。'
}

export function isIslandShape(value: string): value is IslandShape {
  return (ISLAND_SHAPES as readonly string[]).includes(value)
}

export function normalizeIslandShape(value: string | null | undefined): IslandShape {
  if (value && isIslandShape(value)) return value
  return 'floating'
}
