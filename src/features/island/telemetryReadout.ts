/**
 * 胶囊轻量实时网速与资源微读数的纯规则计算（零 DOM / 宿主耦合，可独立回归测试）。
 */

export type TelemetryMode = 'off' | 'smart' | 'always'
export type TelemetryStyle = 'text' | 'bar' | 'ring' | 'pulse'

export interface TelemetryPrefs {
  telemetryMode: TelemetryMode
  telemetryNet: boolean
  telemetryCpu: boolean
  telemetryGpu: boolean
  telemetryMemory: boolean
}

export interface TelemetrySnapshot {
  netState: 'ready' | 'warmingUp' | 'unavailable' | 'disabled'
  downBps: number | null
  upBps: number | null
  cpuPercent: number | null
  gpuPercent: number | null
  gpuTempC?: number | null
  gpuVramUsedMb?: number | null
  gpuVramTotalMb?: number | null
  gpuVramPercent?: number | null
  memoryPercent: number | null
}

export type TelemetryAlertLevel = 'normal' | 'warm' | 'hot'

export interface TelemetrySegment {
  prefix: string
  value: string
  accent: boolean
  metric?: 'net' | 'cpu' | 'gpu' | 'memory'
  percent?: number | null
  /** 综合负载与温度后的分级告警（用于微读数变色提示）。 */
  alertLevel?: TelemetryAlertLevel
  /** 显卡高温预警标记（温度 >= 78°C）。 */
  thermalAlert?: boolean
}

export function normalizeTelemetryStyle(style: unknown): TelemetryStyle {
  return style === 'text' || style === 'bar' || style === 'ring' || style === 'pulse' ? style : 'bar'
}

export interface TelemetryReadout {
  segments: TelemetrySegment[]
  text: string
  ariaText: string
  tooltip: string
  metricCount: number
}

/** 智能浮现唤醒与回落阈值（字节/秒 或 百分比） */
export const SMART_NET_ENTER_BPS = 64 * 1024
export const SMART_NET_SUSTAIN_BPS = 32 * 1024
export const SMART_CPU_ENTER_PCT = 35
export const SMART_CPU_SUSTAIN_PCT = 25
export const SMART_GPU_ENTER_PCT = 40
export const SMART_GPU_SUSTAIN_PCT = 25
export const SMART_GPU_TEMP_ENTER_C = 80
export const SMART_GPU_TEMP_SUSTAIN_C = 74
export const GPU_TEMP_WARN_C = 78
export const GPU_TEMP_HOT_C = 85
export const SMART_MEM_ENTER_PCT = 80
export const SMART_MEM_SUSTAIN_PCT = 75
export const SMART_HOLD_TICKS = 3

export function normalizeTelemetryPrefs(raw?: Partial<TelemetryPrefs> | null): TelemetryPrefs {
  const mode = raw?.telemetryMode
  return {
    telemetryMode: mode === 'off' || mode === 'smart' || mode === 'always' ? mode : 'smart',
    telemetryNet: typeof raw?.telemetryNet === 'boolean' ? raw.telemetryNet : true,
    telemetryCpu: typeof raw?.telemetryCpu === 'boolean' ? raw.telemetryCpu : true,
    telemetryGpu: typeof raw?.telemetryGpu === 'boolean' ? raw.telemetryGpu : true,
    telemetryMemory: typeof raw?.telemetryMemory === 'boolean' ? raw.telemetryMemory : true
  }
}

export function countTelemetryMetrics(prefs: TelemetryPrefs): number {
  return (
    Number(prefs.telemetryNet) +
    Number(prefs.telemetryCpu) +
    Number(prefs.telemetryGpu) +
    Number(prefs.telemetryMemory)
  )
}

/**
 * 将字节/秒格式化为紧凑单行速率。
 * `verboseUnit = true` 用于单指标模式（如 `12.4 MB/s`），多指标并排使用短单位（如 `12.4M`）。
 */
export function formatCompactSpeed(bps: number, verboseUnit: boolean): string {
  const safe = Number.isFinite(bps) && bps > 0 ? bps : 0
  const kb = safe / 1024
  if (kb < 1000) {
    const rounded = Math.round(kb)
    return verboseUnit ? `${rounded} KB/s` : `${rounded}K`
  }
  const mb = kb / 1024
  if (mb < 100) {
    return verboseUnit ? `${mb.toFixed(1)} MB/s` : `${mb.toFixed(1)}M`
  }
  if (mb < 1000) {
    const rounded = Math.round(mb)
    return verboseUnit ? `${rounded} MB/s` : `${rounded}M`
  }
  const gb = mb / 1024
  return verboseUnit ? `${gb.toFixed(1)} GB/s` : `${gb.toFixed(1)}G`
}

/**
 * 推进一拍智能浮现/常驻状态机（带进入/回落迟滞与 3 秒最短保持期，避免每秒闪烁）。
 */
export function stepSmartTelemetry(
  sample: TelemetrySnapshot | null,
  prefs: TelemetryPrefs,
  holdRemaining: number
): { active: boolean; nextHold: number } {
  const metricCount = countTelemetryMetrics(prefs)
  if (prefs.telemetryMode === 'off' || metricCount === 0 || !sample) {
    return { active: false, nextHold: 0 }
  }
  if (prefs.telemetryMode === 'always') {
    return { active: true, nextHold: 0 }
  }

  const netReady = prefs.telemetryNet && sample.netState === 'ready'
  const down = sample.downBps ?? 0
  const up = sample.upBps ?? 0
  const cpu = sample.cpuPercent ?? -1
  const gpu = sample.gpuPercent ?? -1
  const gpuTemp = sample.gpuTempC ?? -1
  const mem = sample.memoryPercent ?? -1

  const enter =
    (netReady && (down >= SMART_NET_ENTER_BPS || up >= SMART_NET_ENTER_BPS)) ||
    (prefs.telemetryCpu && cpu >= SMART_CPU_ENTER_PCT) ||
    (prefs.telemetryGpu && (gpu >= SMART_GPU_ENTER_PCT || gpuTemp >= SMART_GPU_TEMP_ENTER_C)) ||
    (prefs.telemetryMemory && mem >= SMART_MEM_ENTER_PCT)

  if (enter) {
    return { active: true, nextHold: SMART_HOLD_TICKS }
  }

  if (holdRemaining > 0) {
    const sustain =
      (netReady && (down >= SMART_NET_SUSTAIN_BPS || up >= SMART_NET_SUSTAIN_BPS)) ||
      (prefs.telemetryCpu && cpu >= SMART_CPU_SUSTAIN_PCT) ||
      (prefs.telemetryGpu && (gpu >= SMART_GPU_SUSTAIN_PCT || gpuTemp >= SMART_GPU_TEMP_SUSTAIN_C)) ||
      (prefs.telemetryMemory && mem >= SMART_MEM_SUSTAIN_PCT)
    if (sustain) {
      return { active: true, nextHold: holdRemaining }
    }
    const nextHold = Math.max(0, holdRemaining - 1)
    return { active: nextHold > 0, nextHold }
  }

  return { active: false, nextHold: 0 }
}

/**
 * 组装胶囊单行微读数片段、无障碍播报文案与悬停完整提示。
 */
export function buildTelemetryReadout(
  sample: TelemetrySnapshot | null,
  prefs: TelemetryPrefs
): TelemetryReadout | null {
  const metricCount = countTelemetryMetrics(prefs)
  if (metricCount === 0 || !sample) return null

  const verbose = metricCount === 1
  const segments: TelemetrySegment[] = []
  const ariaParts: string[] = []
  const tipParts: string[] = []

  if (prefs.telemetryNet) {
    if (sample.netState === 'ready' && sample.downBps !== null && sample.upBps !== null) {
      const upDominant = sample.upBps > sample.downBps * 1.5 && sample.upBps >= SMART_NET_SUSTAIN_BPS
      const arrow = upDominant ? '↑' : '↓'
      const primary = upDominant ? sample.upBps : sample.downBps
      segments.push({ prefix: arrow, value: formatCompactSpeed(primary, verbose), accent: true, metric: 'net', percent: null })
      ariaParts.push(`${upDominant ? '上传' : '下载'} ${formatCompactSpeed(primary, true)}`)
      tipParts.push(`下载 ${formatCompactSpeed(sample.downBps, true)} · 上传 ${formatCompactSpeed(sample.upBps, true)}`)
    } else if (sample.netState === 'unavailable') {
      segments.push({ prefix: verbose ? '网速' : '↓', value: '未知', accent: false, metric: 'net', percent: null })
      ariaParts.push('网速未知')
      tipParts.push('未检测到活动的物理以太网或 Wi-Fi')
    } else {
      segments.push({ prefix: '↓', value: verbose ? '-- KB/s' : '--', accent: true, metric: 'net', percent: null })
      ariaParts.push('网速预热中')
      tipParts.push('网速采样预热中')
    }
  }

  if (prefs.telemetryCpu) {
    const pct = sample.cpuPercent !== null ? Math.round(sample.cpuPercent) : null
    const val = pct !== null ? `${pct}%` : '--%'
    segments.push({ prefix: 'CPU', value: val, accent: false, metric: 'cpu', percent: pct })
    ariaParts.push(`处理器 ${val}`)
    tipParts.push(`处理器 ${val}`)
  }

  if (prefs.telemetryGpu) {
    const pct = sample.gpuPercent !== null ? Math.round(sample.gpuPercent) : null
    const val = pct !== null ? `${pct}%` : '--%'
    const temp = typeof sample.gpuTempC === 'number' && Number.isFinite(sample.gpuTempC) ? Math.round(sample.gpuTempC) : null
    const vramUsed = typeof sample.gpuVramUsedMb === 'number' && Number.isFinite(sample.gpuVramUsedMb) ? sample.gpuVramUsedMb : null
    const vramTotal = typeof sample.gpuVramTotalMb === 'number' && Number.isFinite(sample.gpuVramTotalMb) && sample.gpuVramTotalMb > 0 ? sample.gpuVramTotalMb : null
    const vramPct = typeof sample.gpuVramPercent === 'number' && Number.isFinite(sample.gpuVramPercent) ? Math.round(sample.gpuVramPercent) : null

    const loadLevel: TelemetryAlertLevel = pct === null ? 'normal' : pct >= 85 ? 'hot' : pct >= 65 ? 'warm' : 'normal'
    const tempLevel: TelemetryAlertLevel = temp === null ? 'normal' : temp >= GPU_TEMP_HOT_C ? 'hot' : temp >= GPU_TEMP_WARN_C ? 'warm' : 'normal'
    const rank = (lvl: TelemetryAlertLevel) => (lvl === 'hot' ? 2 : lvl === 'warm' ? 1 : 0)
    const alertLevel = rank(tempLevel) >= rank(loadLevel) ? tempLevel : loadLevel
    const thermalAlert = temp !== null && temp >= GPU_TEMP_WARN_C

    segments.push({
      prefix: 'GPU',
      value: val,
      accent: false,
      metric: 'gpu',
      percent: pct,
      alertLevel,
      thermalAlert
    })

    const extraParts: string[] = []
    if (temp !== null) extraParts.push(`${temp}°C${temp >= GPU_TEMP_HOT_C ? ' 高温预警' : temp >= GPU_TEMP_WARN_C ? ' 偏高' : ''}`)
    if (vramUsed !== null && vramTotal !== null) {
      const usedGb = (vramUsed / 1024).toFixed(1)
      const totalGb = (vramTotal / 1024).toFixed(1)
      extraParts.push(`显存 ${usedGb}/${totalGb} GB${vramPct !== null ? ` (${vramPct}%)` : ''}`)
    }
    ariaParts.push(extraParts.length ? `显卡 ${val}，${extraParts.join('，')}` : `显卡 ${val}`)
    tipParts.push(extraParts.length ? `显卡 ${val}（${extraParts.join(' · ')}）` : `显卡 ${val}`)
  }

  if (prefs.telemetryMemory) {
    const pct = sample.memoryPercent !== null ? Math.round(sample.memoryPercent) : null
    const val = pct !== null ? `${pct}%` : '--%'
    segments.push({ prefix: '内存', value: val, accent: false, metric: 'memory', percent: pct })
    ariaParts.push(`内存 ${val}`)
    tipParts.push(`内存 ${val}`)
  }

  return {
    segments,
    text: segments.map((item) => `${item.prefix} ${item.value}`).join(' · '),
    ariaText: ariaParts.join('，'),
    tooltip: `实时读数：${tipParts.join(' · ')}`,
    metricCount
  }
}
