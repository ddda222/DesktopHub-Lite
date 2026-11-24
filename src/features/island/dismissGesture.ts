/** 灵动岛收缩判定与流体手势物理纯函数（与 DOM / Tauri 解耦，可独立单测）。 */

export interface TranslationKeepContext {
  status: string
  source: string
  result: { text?: string } | null
  error: string
}

export interface WindowBlurContext {
  expanded: boolean
  pinned: boolean
  hasApproval: boolean
  shelfBusy: boolean
  receivingFiles: boolean
  composing: boolean
  translationLoading: boolean
  previewKeepOpen: boolean
}

export interface DismissPullResult {
  offsetY: number
  scaleY: number
  ready: boolean
}

/** 仅在翻译加载中、已有输入/译文或错误提示时保持面板防收起，空白页不再死锁。 */
export function shouldKeepTranslationOpen(translation: TranslationKeepContext): boolean {
  if (translation.status === 'loading') return true
  if (translation.source.trim().length > 0) return true
  if (translation.result?.text && translation.result.text.trim().length > 0) return true
  if (translation.error.trim().length > 0) return true
  return false
}

/** 桌面主窗失焦或点击岛外时判断是否应自动收缩，保护进行中的任务与主动钉住状态。 */
export function shouldAutoCollapseOnWindowBlur(ctx: WindowBlurContext): boolean {
  if (!ctx.expanded) return false
  if (ctx.pinned || ctx.hasApproval || ctx.shelfBusy || ctx.receivingFiles) return false
  if (ctx.composing || ctx.translationLoading || ctx.previewKeepOpen) return false
  return true
}

/** Apple 流体边界橡皮筋阻尼：越过边界后随距离渐进增强阻力。 */
export function rubberband(overshoot: number, dimension: number, constant = 0.55): number {
  if (overshoot === 0 || dimension <= 0) return 0
  return (overshoot * dimension * constant) / (dimension + constant * Math.abs(overshoot))
}

/** 指数衰减动量投影：把松手瞬间的滑动速度投影到未来落点（单位：px）。 */
export function projectMomentum(velocityPxPerSec: number, decelerationRate = 0.99): number {
  return (velocityPxPerSec / 1000) * decelerationRate / (1 - decelerationRate)
}

/** 计算底部握柄拖拽过程中的 1:1 跟手位移、微压缩比例与收起就绪状态。 */
export function computeDismissPull(rawDeltaY: number): DismissPullResult {
  if (rawDeltaY < 0) {
    const abs = Math.abs(rawDeltaY)
    const offsetY = abs <= 36 ? rawDeltaY : -36 - rubberband(abs - 36, 36, 0.55)
    const scaleY = Math.max(0.94, 1 + offsetY * 0.0011)
    return { offsetY, scaleY, ready: offsetY <= -28 }
  }
  const offsetY = rubberband(rawDeltaY, 26, 0.55)
  return { offsetY, scaleY: 1, ready: false }
}

/** 释放底部握柄时综合位移与动量投影判定是否提交收缩。 */
export function shouldCommitDismiss(
  offsetY: number,
  velocityPxPerSec: number,
  moved: boolean,
  reducedMotion = false
): boolean {
  if (!moved) return false
  if (offsetY <= -28) return true
  if (!reducedMotion) {
    const projectedY = offsetY + projectMomentum(velocityPxPerSec, 0.99)
    if (offsetY <= -8 && velocityPxPerSec <= -220 && projectedY <= -30) return true
  }
  return false
}
