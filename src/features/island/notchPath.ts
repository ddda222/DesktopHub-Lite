/**
 * 灵动岛贴顶形态（notch / split 母舰顶栏）单路径数学连续 SVG 轮廓生成器。
 *
 * 设计约束：
 * - 严禁使用多段拼接 SVG 或独立耳朵 div；填充轮廓（fillPath）与外描边轮廓（strokePath）
 *   共享同一条数学连续路径，彻底消除拼接黑缝、切线断层与双边框重叠。
 * - 顶边（y = 0）与靠左/靠右停靠时的贴屏侧边无多余外描边，自然融入屏幕边框。
 * - 兼容全部主题族：圆弧（standard / telemetry / neon）、45° 重工装甲（mecha）、
 *   8-bit 全整数像素阶梯（pixel）、工程虚线斜肩（blueprint）。
 */

export type NotchVariant = 'standard' | 'telemetry' | 'mecha' | 'pixel' | 'blueprint' | 'neon'
export type NotchAnchor = 'left' | 'center' | 'right'

export interface NotchEarMetrics {
  /** 左融合肩宽（靠左贴屏边时为 0） */
  earLeft: number
  /** 右融合肩宽（靠右贴屏边时为 0） */
  earRight: number
  /** 肩部垂直过渡高度（用于 Win32 HRGN 顶带与 GlassLight 顶部渐隐避让） */
  earHeight: number
  /** 底部圆角 / 倒角 / 像素阶梯角尺寸 */
  bottomCorner: number
}

export interface NotchMetrics extends NotchEarMetrics {
  /** 去除左右肩部后的主体有效内容宽度 */
  bodyWidth: number
}

export interface NotchPathParams {
  /** 外壳总宽（含左右融合肩部 earLeft + bodyWidth + earRight） */
  width: number
  /** 外壳总高 */
  height: number
  /** 主题轮廓变体 */
  variant: NotchVariant
  /** 停靠位置（决定左右肩部是否贴屏平直） */
  anchor: NotchAnchor
  /** 展开进度 0..1（用于圆角/倒角随高度平滑过渡） */
  expandRatio?: number
  /** 全屏避让细线态 */
  line?: boolean
  /** 外描边中心线内缩像素（默认按主题描边宽的一半计算） */
  strokeInset?: number
}

export interface NotchPathResult {
  /** 闭合单路径：贴紧 y = 0 用于底色与图纸网格填充 */
  fillPath: string
  /** 连续外描边单路径：与 fillPath 完全共线，顶边与贴屏边无描边 */
  strokePath: string
  /** 底部微光高光线（圆弧与重工主题复用） */
  hemPath: string
  /** 几何度量 */
  metrics: NotchMetrics
}

export function normalizeNotchAnchor(value: string | null | undefined): NotchAnchor {
  if (value === 'left' || value === 'right') return value
  return 'center'
}

function fmt(n: number): string {
  const clean = Math.abs(n) < 1e-6 ? 0 : n
  const rounded = Math.round(clean * 100) / 100
  return Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(2).replace(/0+$/, '').replace(/\.$/, '')
}

function defaultStrokeInset(variant: NotchVariant): number {
  if (variant === 'pixel') return 1
  if (variant === 'neon') return 0.9
  if (variant === 'blueprint') return 0.75
  if (variant === 'mecha') return 0.7
  return 0.5
}

/**
 * 依据当前高度、主题与停靠边计算贴顶形态的肩部与底角几何尺寸。
 * 供 IslandEntry 在确定外壳总宽（bodyWidth + earLeft + earRight）与内容内边距时前置调用。
 */
export function computeNotchEarMetrics(
  height: number,
  variant: NotchVariant,
  anchor: NotchAnchor,
  expandRatio = 0,
  line = false
): NotchEarMetrics {
  const safeAnchor = normalizeNotchAnchor(anchor)
  const t = Math.max(0, Math.min(1, Number.isFinite(expandRatio) ? expandRatio : 0))
  const h = Math.max(4, Number.isFinite(height) ? height : 32)
  const s = defaultStrokeInset(variant)

  if (variant === 'pixel') {
    const intH = Math.max(6, Math.round(h))
    const step = line ? 2 : 3
    const usable = Math.max(2, intH - 2)
    const maxSteps = Math.max(1, Math.floor((usable * 0.44) / step))
    const topSteps = Math.min(line ? 1 : t > 0.45 ? 4 : 3, maxSteps)
    const botSteps = Math.min(line ? 1 : 2, maxSteps)
    const ear = topSteps * step
    const bot = botSteps * step
    return {
      earLeft: safeAnchor === 'left' ? 0 : ear,
      earRight: safeAnchor === 'right' ? 0 : ear,
      earHeight: ear,
      bottomCorner: bot
    }
  }

  const usableH = Math.max(1, h - 2 * s)
  let ear: number
  let bot: number

  if (variant === 'mecha') {
    ear = Math.min(line ? 3 : 10 + 4 * t, usableH * 0.44)
    bot = Math.min(line ? 2 : 8 + 5 * t, usableH * 0.46)
  } else if (variant === 'blueprint') {
    ear = Math.min(line ? 3 : 10 + 4 * t, usableH * 0.44)
    bot = Math.min(line ? 2 : 6 + 4 * t, usableH * 0.44)
  } else if (variant === 'telemetry') {
    ear = Math.min(line ? 3 : 10 + 4 * t, usableH * 0.42)
    bot = Math.min(line ? 3 : 11 + 3 * t, usableH * 0.46)
  } else if (variant === 'neon') {
    ear = Math.min(line ? 3 : 12 + 4 * t, usableH * 0.44)
    bot = Math.min(line ? 3 : 15 + 4 * t, usableH * 0.48)
  } else {
    // standard（深色 / 浅色）：G1 切线连续 S 曲线，严格满足 rTop <= H*0.48 与 rBot <= H*0.48
    ear = Math.min(line ? 3 : 12 + 4 * t, usableH * 0.44)
    bot = Math.min(line ? 3 : 14 + 4 * t, usableH * 0.48)
  }

  return {
    earLeft: safeAnchor === 'left' ? 0 : Math.round(ear * 100) / 100,
    earRight: safeAnchor === 'right' ? 0 : Math.round(ear * 100) / 100,
    earHeight: Math.round(ear * 100) / 100,
    bottomCorner: Math.round(bot * 100) / 100
  }
}

/**
 * 构建单条数学连续 SVG 贴顶轮廓（填充与描边严格共线，0px 接缝）。
 */
export function buildNotchPath(params: NotchPathParams): NotchPathResult {
  const anchor = normalizeNotchAnchor(params.anchor)
  const variant = params.variant
  const line = Boolean(params.line)
  const expandRatio = Math.max(0, Math.min(1, Number.isFinite(params.expandRatio ?? 0) ? (params.expandRatio ?? 0) : 0))
  const s = Math.max(0.25, params.strokeInset ?? defaultStrokeInset(variant))

  const rawW = Math.max(8, Number.isFinite(params.width) ? params.width : 140)
  const rawH = Math.max(4, Number.isFinite(params.height) ? params.height : 32)
  const base = computeNotchEarMetrics(rawH, variant, anchor, expandRatio, line)

  if (variant === 'pixel') {
    return buildPixelNotchPath(Math.round(rawW), Math.round(rawH), anchor, line, expandRatio)
  }

  const w = rawW
  const h = rawH
  let earL = base.earLeft
  let earR = base.earRight
  let botL = anchor === 'left' ? 0 : base.bottomCorner
  let botR = anchor === 'right' ? 0 : base.bottomCorner

  // 水平安全防越界缩放，确保极窄画布下也不会出现负段宽或路径自交。
  const maxSpan = Math.max(2, w - 4)
  const needed = earL + earR + botL + botR
  if (needed > maxSpan) {
    const ratio = maxSpan / needed
    earL *= ratio
    earR *= ratio
    botL *= ratio
    botR *= ratio
  }

  const yTop = s
  const yBot = Math.max(yTop + 1, h - s)
  const usableH = yBot - yTop
  const earHLeft = Math.min(earL, usableH * 0.48)
  const earHRight = Math.min(earR, usableH * 0.48)
  const botHLeft = Math.min(botL, usableH * 0.48)
  const botHRight = Math.min(botR, usableH * 0.48)
  const earHeight = Math.max(earHLeft, earHRight)
  const bottomCorner = Math.max(botHLeft, botHRight)

  const leftWallX = earL
  const rightWallX = w - earR
  const bodyWidth = Math.max(2, rightWallX - leftWallX)

  if (variant === 'mecha' || variant === 'blueprint') {
    // 45° 外斜肩 + 竖直侧壁 + 底部 45° 倒角，单条连续折线一笔画完。
    const segs: string[] = []
    if (anchor === 'left') {
      segs.push(`M 0 ${fmt(yBot)}`)
    } else {
      segs.push(
        `M 0 ${fmt(yTop)}`,
        `L ${fmt(leftWallX)} ${fmt(yTop + earHLeft)}`,
        `V ${fmt(yBot - botHLeft)}`,
        `L ${fmt(leftWallX + botL)} ${fmt(yBot)}`
      )
    }
    segs.push(`H ${fmt(rightWallX - botR)}`)
    if (anchor !== 'right') {
      segs.push(
        `L ${fmt(rightWallX)} ${fmt(yBot - botHRight)}`,
        `V ${fmt(yTop + earHRight)}`,
        `L ${fmt(w)} ${fmt(yTop)}`
      )
    }
    const strokePath = segs.join(' ')

    const fillSegs: string[] = []
    if (anchor === 'left') {
      fillSegs.push(
        'M 0 0',
        `V ${fmt(yBot)}`,
        `H ${fmt(rightWallX - botR)}`,
        `L ${fmt(rightWallX)} ${fmt(yBot - botHRight)}`,
        `V ${fmt(yTop + earHRight)}`,
        `L ${fmt(w)} ${fmt(yTop)}`,
        `L ${fmt(w)} 0`,
        'Z'
      )
    } else if (anchor === 'right') {
      fillSegs.push(
        'M 0 0',
        `L 0 ${fmt(yTop)}`,
        `L ${fmt(leftWallX)} ${fmt(yTop + earHLeft)}`,
        `V ${fmt(yBot - botHLeft)}`,
        `L ${fmt(leftWallX + botL)} ${fmt(yBot)}`,
        `H ${fmt(w)}`,
        'V 0',
        'Z'
      )
    } else {
      fillSegs.push(
        'M 0 0',
        `L 0 ${fmt(yTop)}`,
        `L ${fmt(leftWallX)} ${fmt(yTop + earHLeft)}`,
        `V ${fmt(yBot - botHLeft)}`,
        `L ${fmt(leftWallX + botL)} ${fmt(yBot)}`,
        `H ${fmt(rightWallX - botR)}`,
        `L ${fmt(rightWallX)} ${fmt(yBot - botHRight)}`,
        `V ${fmt(yTop + earHRight)}`,
        `L ${fmt(w)} ${fmt(yTop)}`,
        `L ${fmt(w)} 0`,
        'Z'
      )
    }

    const hemX1 = leftWallX + botL + 6
    const hemX2 = Math.max(hemX1, rightWallX - botR - 6)
    const hemPath = variant === 'mecha' && !line && hemX2 > hemX1
      ? `M ${fmt(hemX1)} ${fmt(yBot - 0.8)} H ${fmt(hemX2)}`
      : ''

    return {
      fillPath: fillSegs.join(' '),
      strokePath,
      hemPath,
      metrics: { earLeft: earL, earRight: earR, earHeight, bottomCorner, bodyWidth }
    }
  }

  // 圆弧族（standard / telemetry / neon）：
  // 上凹圆弧（圆心在 (0, yTop + earHLeft) 与 (w, yTop + earHRight)）在底端切线严格垂直，
  // 与竖直侧壁及底部凸圆弧实现 G1 切线连续，零台阶零接缝。
  const segs: string[] = []
  if (anchor === 'left') {
    segs.push(`M 0 ${fmt(yBot)}`)
  } else {
    segs.push(
      `M 0 ${fmt(yTop)}`,
      `A ${fmt(earL)} ${fmt(earHLeft)} 0 0 1 ${fmt(leftWallX)} ${fmt(yTop + earHLeft)}`,
      `V ${fmt(yBot - botHLeft)}`,
      `A ${fmt(botL)} ${fmt(botHLeft)} 0 0 0 ${fmt(leftWallX + botL)} ${fmt(yBot)}`
    )
  }
  segs.push(`H ${fmt(rightWallX - botR)}`)
  if (anchor !== 'right') {
    segs.push(
      `A ${fmt(botR)} ${fmt(botHRight)} 0 0 0 ${fmt(rightWallX)} ${fmt(yBot - botHRight)}`,
      `V ${fmt(yTop + earHRight)}`,
      `A ${fmt(earR)} ${fmt(earHRight)} 0 0 1 ${fmt(w)} ${fmt(yTop)}`
    )
  }
  const strokePath = segs.join(' ')

  const fillSegs: string[] = []
  if (anchor === 'left') {
    fillSegs.push(
      'M 0 0',
      `V ${fmt(yBot)}`,
      `H ${fmt(rightWallX - botR)}`,
      `A ${fmt(botR)} ${fmt(botHRight)} 0 0 0 ${fmt(rightWallX)} ${fmt(yBot - botHRight)}`,
      `V ${fmt(yTop + earHRight)}`,
      `A ${fmt(earR)} ${fmt(earHRight)} 0 0 1 ${fmt(w)} ${fmt(yTop)}`,
      `L ${fmt(w)} 0`,
      'Z'
    )
  } else if (anchor === 'right') {
    fillSegs.push(
      'M 0 0',
      `L 0 ${fmt(yTop)}`,
      `A ${fmt(earL)} ${fmt(earHLeft)} 0 0 1 ${fmt(leftWallX)} ${fmt(yTop + earHLeft)}`,
      `V ${fmt(yBot - botHLeft)}`,
      `A ${fmt(botL)} ${fmt(botHLeft)} 0 0 0 ${fmt(leftWallX + botL)} ${fmt(yBot)}`,
      `H ${fmt(w)}`,
      'V 0',
      'Z'
    )
  } else {
    fillSegs.push(
      'M 0 0',
      `L 0 ${fmt(yTop)}`,
      `A ${fmt(earL)} ${fmt(earHLeft)} 0 0 1 ${fmt(leftWallX)} ${fmt(yTop + earHLeft)}`,
      `V ${fmt(yBot - botHLeft)}`,
      `A ${fmt(botL)} ${fmt(botHLeft)} 0 0 0 ${fmt(leftWallX + botL)} ${fmt(yBot)}`,
      `H ${fmt(rightWallX - botR)}`,
      `A ${fmt(botR)} ${fmt(botHRight)} 0 0 0 ${fmt(rightWallX)} ${fmt(yBot - botHRight)}`,
      `V ${fmt(yTop + earHRight)}`,
      `A ${fmt(earR)} ${fmt(earHRight)} 0 0 1 ${fmt(w)} ${fmt(yTop)}`,
      `L ${fmt(w)} 0`,
      'Z'
    )
  }

  const hemStart = Math.max(leftWallX + botL, leftWallX + bodyWidth * 0.14)
  const hemEnd = Math.min(rightWallX - botR, leftWallX + bodyWidth * 0.86)
  const hemPath = !line && variant !== 'neon' && hemEnd > hemStart
    ? `M ${fmt(hemStart)} ${fmt(yBot - 0.35)} H ${fmt(hemEnd)}`
    : ''

  return {
    fillPath: fillSegs.join(' '),
    strokePath,
    hemPath,
    metrics: { earLeft: earL, earRight: earR, earHeight, bottomCorner, bodyWidth }
  }
}

/** 像素掌机（pixel）全整数 8-bit 阶梯卡带肩 + 底部像素阶梯角单路径生成。 */
function buildPixelNotchPath(
  w: number,
  h: number,
  anchor: NotchAnchor,
  line: boolean,
  expandRatio: number
): NotchPathResult {
  const intW = Math.max(12, Math.round(w))
  const intH = Math.max(6, Math.round(h))
  const step = line ? 2 : 3
  const yTop = 1
  const yBot = intH - 1
  const usable = Math.max(2, yBot - yTop)
  const maxSteps = Math.max(1, Math.floor((usable * 0.44) / step))
  let topSteps = Math.min(line ? 1 : expandRatio > 0.45 ? 4 : 3, maxSteps)
  let botSteps = Math.min(line ? 1 : 2, maxSteps)

  // 水平方向保证阶梯宽度总和不越界，始终保留主体整数宽。
  const leftSides = anchor === 'left' ? 0 : 1
  const rightSides = anchor === 'right' ? 0 : 1
  const sideFactor = leftSides + rightSides
  while (sideFactor * (topSteps + botSteps) * step > intW - 6 && (topSteps > 1 || botSteps > 1)) {
    if (topSteps >= botSteps && topSteps > 1) topSteps--
    else if (botSteps > 1) botSteps--
    else break
  }

  const earL = anchor === 'left' ? 0 : topSteps * step
  const earR = anchor === 'right' ? 0 : topSteps * step
  const botL = anchor === 'left' ? 0 : botSteps * step
  const botR = anchor === 'right' ? 0 : botSteps * step
  const leftWallX = earL
  const rightWallX = intW - earR
  const bodyWidth = Math.max(2, rightWallX - leftWallX)

  const contour: string[] = []
  if (anchor === 'left') {
    contour.push(`M 0 ${yBot}`)
  } else {
    contour.push(`M 0 ${yTop}`)
    // 左上 8-bit 阶梯卡带肩（全整数像素坐标）
    for (let k = 1; k <= topSteps; k++) {
      contour.push(`H ${k * step}`, `V ${yTop + k * step}`)
    }
    // 左侧竖壁
    contour.push(`V ${yBot - botL}`)
    // 左下像素阶梯角
    for (let k = 1; k <= botSteps; k++) {
      contour.push(`H ${leftWallX + k * step}`, `V ${yBot - botL + k * step}`)
    }
  }

  // 底边
  contour.push(`H ${rightWallX - botR}`)

  if (anchor !== 'right') {
    // 右下像素阶梯角（与左下严格镜像对称）
    for (let k = botSteps - 1; k >= 0; k--) {
      contour.push(`V ${yBot - botR + k * step}`, `H ${rightWallX - k * step}`)
    }
    // 右侧竖壁
    contour.push(`V ${yTop + earR}`)
    // 右上 8-bit 阶梯卡带肩（与左上严格镜像对称）
    for (let k = topSteps - 1; k >= 0; k--) {
      contour.push(`V ${yTop + k * step}`, `H ${intW - k * step}`)
    }
  }

  const strokePath = contour.join(' ')
  let fillPath: string
  if (anchor === 'left') {
    fillPath = `M 0 0 V ${yBot} ${contour.slice(1).join(' ')} L ${intW} 0 Z`
  } else if (anchor === 'right') {
    fillPath = `M 0 0 L 0 ${yTop} ${contour.slice(1).join(' ')} V 0 Z`
  } else {
    fillPath = `M 0 0 L 0 ${yTop} ${contour.slice(1).join(' ')} L ${intW} 0 Z`
  }

  const hemX1 = leftWallX + botL
  const hemX2 = Math.max(hemX1, rightWallX - botR)
  const hemPath = !line && hemX2 > hemX1 ? `M ${hemX1} ${yBot - 2} H ${hemX2}` : ''

  return {
    fillPath,
    strokePath,
    hemPath,
    metrics: {
      earLeft: earL,
      earRight: earR,
      earHeight: Math.max(earL, earR),
      bottomCorner: Math.max(botL, botR),
      bodyWidth
    }
  }
}
