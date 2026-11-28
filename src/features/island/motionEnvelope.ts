/**
 * 灵动岛展开/收起运动期的单调不缩包络与细线态动画防跳过判定。
 * 独立抽为纯函数模块，便于 IslandEntry 与离线单测共用并防止回归。
 */

export interface MotionEnvelope {
  width: number
  height: number
}

export interface MotionEnvelopeInput {
  /** 当前运动期内已累积的单调包络；稳态首次进入运动时为 null。 */
  current: MotionEnvelope | null
  /** 上一次响应式 bounds 的宽度（像素）。 */
  prevWidth?: number
  /** 上一次响应式 bounds 的高度（像素）。 */
  prevHeight?: number
  /** 本次目标尺寸的宽度（含可选回弹余量）。 */
  targetWidth: number
  /** 本次目标尺寸的高度（含可选回弹余量）。 */
  targetHeight: number
  /** 当前 DOM 实际渲染宽度（可选，兜底覆盖刚完成的布局）。 */
  domWidth?: number
  /** 当前 DOM 实际渲染高度（可选，兜底覆盖刚完成的布局）。 */
  domHeight?: number
}

/**
 * 推进运动期单调不缩包络：
 * - 在 geometryMoving 尚未 settle 前，包络宽高只增不减；
 * - 当收起态胶囊比展开态更宽，或收缩途中遥测/反馈增宽时，包络会在保持展开高度的前提下增宽，
 *   绝不向 Rust 下发 38px 矮 HRGN 腰斩尚未收完的展开面板。
 */
export function advanceMotionEnvelope(input: MotionEnvelopeInput): {
  envelope: MotionEnvelope
  needsExpandRegion: boolean
} {
  const prevW = input.prevWidth !== undefined ? Math.ceil(input.prevWidth) : 0
  const prevH = input.prevHeight !== undefined ? Math.ceil(input.prevHeight) : 0
  const nextWidth = Math.ceil(
    Math.max(
      input.current?.width ?? 0,
      prevW,
      input.targetWidth,
      input.domWidth ?? 0
    )
  )
  const nextHeight = Math.ceil(
    Math.max(
      input.current?.height ?? 0,
      prevH,
      input.targetHeight,
      input.domHeight ?? 0
    )
  )
  const envelope: MotionEnvelope = { width: nextWidth, height: nextHeight }
  const needsExpandRegion = input.current === null
    ? (input.prevWidth === undefined || input.prevHeight === undefined || nextWidth > prevW || nextHeight > prevH)
    : (nextWidth > input.current.width || nextHeight > input.current.height)
  return { envelope, needsExpandRegion }
}

export interface ShellMotionBypassInput {
  isDesktop: boolean
  tucked: boolean
  reducedMotion: boolean
  wasExpanded: boolean
  expanded: boolean
  closingFromExpanded: boolean
}

/**
 * 判断本次尺寸/形态变更是否应跳过外壳补间并直接下发终态 HRGN：
 * - 减弱动态模式始终跳过动画；
 * - 桌面端全屏细线态（tucked）下，仅在「非从展开态发起收起」且「不在展开→收起运动途中」时跳过；
 * - 只要是从展开态发起收起（wasExpanded && !expanded）或在展开→收起运动中收到 suppressed=true，
 *   都必须完整播放收起补间并保持大包络 HRGN，待 settleGeometry 后再下发 140×14 细线 HRGN。
 */
export function shouldBypassShellMotion(input: ShellMotionBypassInput): boolean {
  if (input.reducedMotion) return true
  if (!input.isDesktop || !input.tucked) return false
  if (input.wasExpanded && !input.expanded) return false
  if (input.closingFromExpanded && !input.expanded) return false
  return true
}
