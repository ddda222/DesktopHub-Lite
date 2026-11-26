import type { ThemeMode } from '@/lib/themes'
import type { MotionMode } from '@/lib/motions'

/** 3D 互动设置空间支持的模块机位。 */
export type SceneMode =
  | 'appearance'
  | 'shortcuts'
  | 'clipboard'
  | 'apps'
  | 'translation'
  | 'audio'
  | 'integrations'

/** 外部设置页面实时驱动 3D 场景的完整状态快照。 */
export interface Reactive3DState {
  mode: SceneMode
  theme: ThemeMode
  motion: MotionMode
  anchor: 'left' | 'center' | 'right'
  scale: number
  webglGlow: boolean
  reducedMotion: boolean
  avoidFullscreen: boolean
  fullscreenSim: boolean
  hideFromCapture: boolean
  telemetryMode: 'smart' | 'always' | 'off'
  telemetryNet: boolean
  telemetryCpu: boolean
  telemetryMemory: boolean
  netLevel: number
  cpuLevel: number
  memLevel: number
  clipboardPaused: boolean
  clipCount: number
  clipLimit: number
  translateFlipped: boolean
  audioMuted: boolean
  islandExpanded: boolean
}

/** 从前景按钮射向 3D 场景的光波脉冲。 */
interface ButtonPulse {
  x: number
  y: number
  age: number
  duration: number
  color: string
  label: string
}

/** 3D 空间中正在消散的碎片/粒子（用于清空历史、试译光束等一次性物理反馈）。 */
interface BurstParticle {
  x: number
  y: number
  z: number
  vx: number
  vy: number
  vz: number
  size: number
  age: number
  life: number
  color: string
}

interface Vec3 {
  x: number
  y: number
  z: number
}

interface ThemePalette {
  bgTop: string
  bgBottom: string
  grid: string
  primary: string
  secondary: string
  glow: string
  faceFill: string
  faceStroke: string
  isLight: boolean
}

const THEME_PALETTES: Record<ThemeMode, ThemePalette> = {
  system: {
    bgTop: '#101520',
    bgBottom: '#1c2536',
    grid: 'rgba(130, 168, 220, 0.14)',
    primary: '#38bdf8',
    secondary: '#a78bfa',
    glow: '#38bdf8',
    faceFill: 'rgba(26, 39, 62, 0.72)',
    faceStroke: 'rgba(125, 211, 252, 0.65)',
    isLight: false
  },
  light: {
    bgTop: '#eef2f8',
    bgBottom: '#dce3ef',
    grid: 'rgba(30, 41, 59, 0.09)',
    primary: '#2563eb',
    secondary: '#f97316',
    glow: '#3b82f6',
    faceFill: 'rgba(255, 255, 255, 0.82)',
    faceStroke: 'rgba(37, 99, 235, 0.45)',
    isLight: true
  },
  dark: {
    bgTop: '#0d1016',
    bgBottom: '#181e2a',
    grid: 'rgba(203, 213, 225, 0.1)',
    primary: '#e2e8f0',
    secondary: '#60a5fa',
    glow: '#93c5fd',
    faceFill: 'rgba(30, 36, 48, 0.82)',
    faceStroke: 'rgba(226, 232, 240, 0.42)',
    isLight: false
  },
  liquid: {
    bgTop: '#070b14',
    bgBottom: '#111a2e',
    grid: 'rgba(125, 211, 252, 0.16)',
    primary: '#7dd3fc',
    secondary: '#d8b4fe',
    glow: '#bae6fd',
    faceFill: 'rgba(18, 30, 52, 0.72)',
    faceStroke: 'rgba(214, 240, 255, 0.74)',
    isLight: false
  },
  midnight: {
    bgTop: '#050c19',
    bgBottom: '#0c1d38',
    grid: 'rgba(109, 182, 255, 0.17)',
    primary: '#6db6ff',
    secondary: '#38f2ff',
    glow: '#6db6ff',
    faceFill: 'rgba(12, 28, 54, 0.78)',
    faceStroke: 'rgba(109, 182, 255, 0.76)',
    isLight: false
  },
  mecha: {
    bgTop: '#14100c',
    bgBottom: '#261b11',
    grid: 'rgba(245, 158, 11, 0.16)',
    primary: '#f59e0b',
    secondary: '#fb7185',
    glow: '#f59e0b',
    faceFill: 'rgba(42, 29, 18, 0.82)',
    faceStroke: 'rgba(245, 158, 11, 0.75)',
    isLight: false
  },
  scroll: {
    bgTop: '#f5ede0',
    bgBottom: '#e7d8c1',
    grid: 'rgba(153, 56, 42, 0.1)',
    primary: '#b94132',
    secondary: '#9a6b39',
    glow: '#d9534f',
    faceFill: 'rgba(252, 247, 238, 0.85)',
    faceStroke: 'rgba(185, 65, 50, 0.56)',
    isLight: true
  },
  pixel: {
    bgTop: '#0b140d',
    bgBottom: '#142618',
    grid: 'rgba(132, 204, 22, 0.18)',
    primary: '#84cc16',
    secondary: '#22c55e',
    glow: '#a3e635',
    faceFill: 'rgba(20, 38, 24, 0.84)',
    faceStroke: 'rgba(163, 230, 53, 0.8)',
    isLight: false
  },
  blueprint: {
    bgTop: '#082040',
    bgBottom: '#12386b',
    grid: 'rgba(224, 242, 254, 0.2)',
    primary: '#e0f2fe',
    secondary: '#38bdf8',
    glow: '#7dd3fc',
    faceFill: 'rgba(15, 52, 102, 0.65)',
    faceStroke: 'rgba(224, 242, 254, 0.82)',
    isLight: false
  },
  neon: {
    bgTop: '#10071e',
    bgBottom: '#230d3b',
    grid: 'rgba(244, 114, 182, 0.18)',
    primary: '#f472b6',
    secondary: '#22d3ee',
    glow: '#ec4899',
    faceFill: 'rgba(32, 14, 56, 0.78)',
    faceStroke: 'rgba(34, 211, 238, 0.8)',
    isLight: false
  },
  clay: {
    bgTop: '#f9edf4',
    bgBottom: '#e5f0fb',
    grid: 'rgba(236, 72, 153, 0.1)',
    primary: '#ec4899',
    secondary: '#38bdf8',
    glow: '#f472b6',
    faceFill: 'rgba(255, 255, 255, 0.88)',
    faceStroke: 'rgba(236, 72, 153, 0.5)',
    isLight: true
  }
}

/** 轻量 3D 互动场景引擎：所有三维机关直接绑定到设置状态，零外部重资产依赖。 */
export class Reactive3DEngine {
  private canvas: HTMLCanvasElement
  private ctx: CanvasRenderingContext2D
  private state: Reactive3DState
  private raf = 0
  private lastTs = 0
  private elapsed = 0
  private width = 1
  private height = 1
  private dpr = 1

  // 平滑阻尼追踪的物理量
  private pointerX = 0
  private pointerY = 0
  private smoothPointerX = 0
  private smoothPointerY = 0
  private camYaw = 0
  private camPitch = -0.22
  private anchorOffsetX = 0
  private coreScale = 1
  private expandProgress = 0
  private tuckProgress = 0
  private cloakProgress = 0
  private translateSpin = 0
  private impactShake = 0
  private motionDemoTimer = 0
  private motionDemoPhase = 0
  private translateBeamAge = -1
  private radarWaveAge = -1

  // 3D 机械键帽受压状态（键名 -> 0..1 下沉深度）
  private keyPressDepth: Record<string, number> = {
    Ctrl: 0,
    Alt: 0,
    Shift: 0,
    Space: 0,
    V: 0,
    F: 0
  }

  private pulses: ButtonPulse[] = []
  private bursts: BurstParticle[] = []
  private ambientStars: Vec3[] = []

  constructor(canvas: HTMLCanvasElement, initialState: Reactive3DState) {
    this.canvas = canvas
    const ctx = canvas.getContext('2d')
    if (!ctx) throw new Error('无法初始化 2D/3D 画布上下文')
    this.ctx = ctx
    this.state = { ...initialState }

    // 预生成轻盈的 3D 空间星尘点阵
    for (let i = 0; i < 64; i++) {
      this.ambientStars.push({
        x: (Math.random() - 0.5) * 14,
        y: (Math.random() - 0.5) * 8,
        z: (Math.random() - 0.5) * 10
      })
    }

    this.resize()
    this.start()
  }

  public updateState(next: Reactive3DState): void {
    const prev = this.state
    this.state = { ...next }

    if (prev.motion !== next.motion) {
      this.playMotionDemo()
    }
    if (prev.theme !== next.theme) {
      this.impactShake = Math.min(1, this.impactShake + 0.55)
      this.spawnRingBurst(THEME_PALETTES[next.theme].primary, 18)
    }
    if (prev.translateFlipped !== next.translateFlipped) {
      this.impactShake = Math.min(1, this.impactShake + 0.45)
    }
    if (prev.clipLimit !== next.clipLimit && next.clipLimit < prev.clipCount) {
      this.spawnRingBurst('#f97316', 10)
    }
  }

  public setPointer(clientX: number, clientY: number): void {
    const rect = this.canvas.getBoundingClientRect()
    if (!rect.width || !rect.height) return
    this.pointerX = ((clientX - rect.left) / rect.width) * 2 - 1
    this.pointerY = ((clientY - rect.top) / rect.height) * 2 - 1
  }

  /** 从前景按钮发射一道光波击中 3D 核心。 */
  public emitButtonPulse(clientX: number, clientY: number, label = ''): void {
    const rect = this.canvas.getBoundingClientRect()
    const x = clientX - rect.left
    const y = clientY - rect.top
    const palette = THEME_PALETTES[this.state.theme]
    this.pulses.push({
      x,
      y,
      age: 0,
      duration: 0.65,
      color: palette.primary,
      label
    })
    this.impactShake = Math.min(1, this.impactShake + 0.42)
  }

  /** 触发 3D 核心演示当前开合动效。 */
  public playMotionDemo(): void {
    this.motionDemoTimer = 1.65
    this.motionDemoPhase = 0
  }

  /** 按下某个 3D 键帽（供快捷键录制与试按联动）。 */
  public pressKeycaps(caps: string[]): void {
    for (const raw of caps) {
      const normalized = raw.trim()
      if (normalized in this.keyPressDepth) {
        this.keyPressDepth[normalized] = 1
      } else {
        // 未在固定 6 键内的主按键，映射到右侧高光键帽反馈
        this.keyPressDepth.Space = 1
      }
    }
    this.impactShake = Math.min(1, this.impactShake + 0.35)
    this.spawnRingBurst(THEME_PALETTES[this.state.theme].secondary, 12)
  }

  /** 触发一次性 3D 特效（如翻译光束、剪贴板清空炸裂、雷达信标波）。 */
  public triggerEvent(kind: 'translate-beam' | 'clipboard-clear' | 'radar-ping' | 'copied'): void {
    const palette = THEME_PALETTES[this.state.theme]
    if (kind === 'translate-beam') {
      this.translateBeamAge = 0
      this.spawnRingBurst(palette.secondary, 16)
    } else if (kind === 'clipboard-clear') {
      this.spawnRingBurst('#ef4444', 26)
      this.impactShake = 0.8
    } else if (kind === 'radar-ping') {
      this.radarWaveAge = 0
      this.spawnRingBurst(palette.primary, 14)
    } else {
      this.spawnRingBurst(palette.glow, 14)
      this.impactShake = 0.35
    }
  }

  public resize(): void {
    const rect = this.canvas.getBoundingClientRect()
    this.dpr = Math.min(window.devicePixelRatio || 1, 2)
    this.width = Math.max(1, rect.width)
    this.height = Math.max(1, rect.height)
    this.canvas.width = Math.round(this.width * this.dpr)
    this.canvas.height = Math.round(this.height * this.dpr)
  }

  public dispose(): void {
    cancelAnimationFrame(this.raf)
    this.raf = 0
  }

  private start(): void {
    const tick = (ts: number) => {
      const dt = this.lastTs ? Math.min(0.05, (ts - this.lastTs) / 1000) : 0.016
      this.lastTs = ts
      this.step(dt)
      this.render()
      this.raf = requestAnimationFrame(tick)
    }
    this.raf = requestAnimationFrame(tick)
  }

  private spawnRingBurst(color: string, count: number): void {
    if (this.state.reducedMotion) return
    for (let i = 0; i < count; i++) {
      const angle = (Math.PI * 2 * i) / count + Math.random() * 0.3
      const speed = 1.6 + Math.random() * 2.2
      this.bursts.push({
        x: this.anchorOffsetX,
        y: 0,
        z: 0,
        vx: Math.cos(angle) * speed,
        vy: (Math.random() - 0.5) * 1.8,
        vz: Math.sin(angle) * speed,
        size: 2.2 + Math.random() * 2.4,
        age: 0,
        life: 0.7 + Math.random() * 0.45,
        color
      })
    }
  }

  private step(dt: number): void {
    const reduced = this.state.reducedMotion
    const lerp = (cur: number, target: number, speed: number) =>
      reduced ? target : cur + (target - cur) * Math.min(1, dt * speed)

    if (!reduced) {
      this.elapsed += dt
    }

    this.smoothPointerX = lerp(this.smoothPointerX, this.pointerX, 5)
    this.smoothPointerY = lerp(this.smoothPointerY, this.pointerY, 5)

    // 不同设置模块切换 3D 机位角度
    const modeAngle: Record<SceneMode, { yaw: number; pitch: number }> = {
      appearance: { yaw: 0.16, pitch: -0.22 },
      shortcuts: { yaw: -0.22, pitch: -0.44 },
      clipboard: { yaw: 0.34, pitch: -0.24 },
      apps: { yaw: -0.32, pitch: -0.26 },
      translation: { yaw: 0.08, pitch: -0.2 },
      audio: { yaw: -0.14, pitch: -0.3 },
      integrations: { yaw: 0.24, pitch: -0.36 }
    }
    const targetCam = modeAngle[this.state.mode]
    this.camYaw = lerp(this.camYaw, targetCam.yaw + this.smoothPointerX * 0.24, 4.5)
    this.camPitch = lerp(this.camPitch, targetCam.pitch + this.smoothPointerY * 0.14, 4.5)

    // 停靠位置映射为 3D 主装置水平偏移
    const targetAnchorX = this.state.anchor === 'left' ? -1.45 : this.state.anchor === 'right' ? 1.45 : 0
    this.anchorOffsetX = lerp(this.anchorOffsetX, targetAnchorX, 6.5)

    // 显示大小映射为 3D 核心缩放
    const targetScale = 0.72 + ((this.state.scale - 75) / 50) * 0.56
    this.coreScale = lerp(this.coreScale, targetScale, 8)

    // 全屏避让收为细线
    const shouldTuck = this.state.avoidFullscreen && this.state.fullscreenSim
    this.tuckProgress = lerp(this.tuckProgress, shouldTuck ? 1 : 0, 7)

    // 隐私隐藏光学罩
    this.cloakProgress = lerp(this.cloakProgress, this.state.hideFromCapture ? 1 : 0, 6.5)

    // 翻译双塔 180° 空间翻转
    this.translateSpin = lerp(this.translateSpin, this.state.translateFlipped ? Math.PI : 0, 6)

    // 开合动效演示进度
    if (this.motionDemoTimer > 0) {
      this.motionDemoTimer = Math.max(0, this.motionDemoTimer - dt)
      this.motionDemoPhase += dt
      const openWindow = this.motionDemoTimer > 0.45 ? 1 : 0
      this.expandProgress = lerp(this.expandProgress, openWindow, this.state.motion === 'flicker' ? 18 : 8)
    } else {
      this.expandProgress = lerp(this.expandProgress, this.state.islandExpanded ? 1 : 0, 7)
    }

    this.impactShake = Math.max(0, this.impactShake - dt * 2.4)

    // 键帽回弹
    for (const k of Object.keys(this.keyPressDepth)) {
      this.keyPressDepth[k] = Math.max(0, this.keyPressDepth[k] - dt * 2.8)
    }

    // 翻译光束与雷达波
    if (this.translateBeamAge >= 0) {
      this.translateBeamAge += dt
      if (this.translateBeamAge > 1.1) this.translateBeamAge = -1
    }
    if (this.radarWaveAge >= 0) {
      this.radarWaveAge += dt
      if (this.radarWaveAge > 1.6) this.radarWaveAge = -1
    }

    // 更新按钮冲击脉冲
    for (let i = this.pulses.length - 1; i >= 0; i--) {
      this.pulses[i].age += dt
      if (this.pulses[i].age >= this.pulses[i].duration) {
        this.pulses.splice(i, 1)
      }
    }

    // 更新 3D 爆发微粒
    for (let i = this.bursts.length - 1; i >= 0; i--) {
      const p = this.bursts[i]
      p.age += dt
      p.x += p.vx * dt
      p.y += p.vy * dt
      p.z += p.vz * dt
      p.vx *= 0.96
      p.vy *= 0.96
      p.vz *= 0.96
      if (p.age >= p.life) this.bursts.splice(i, 1)
    }
  }

  /** 将 3D 世界坐标投影到 2D 屏幕（舞台中心刻意向右偏移，避让左侧半透明控制抽屉）。 */
  private project(p: Vec3): { x: number; y: number; z: number; s: number } {
    const shakeX = Math.sin(this.elapsed * 42) * this.impactShake * 0.05
    const shakeY = Math.cos(this.elapsed * 37) * this.impactShake * 0.04

    // 绕 Y 轴偏航
    const cy = Math.cos(this.camYaw)
    const sy = Math.sin(this.camYaw)
    const x1 = p.x * cy - p.z * sy
    const z1 = p.x * sy + p.z * cy

    // 绕 X 轴俯仰
    const cx = Math.cos(this.camPitch)
    const sx = Math.sin(this.camPitch)
    const y2 = p.y * cx - z1 * sx
    const z2 = p.y * sx + z1 * cx

    const cameraDist = 7.4
    const depth = Math.max(1.2, cameraDist - z2)
    const fov = Math.min(this.width, this.height) * 0.86
    const scale = fov / depth

    // 宽屏时舞台焦点偏右（约 66% 处），给左侧半透明设置面板留出清晰互动舞台
    const centerRatio = this.width >= 960 ? 0.67 : 0.5
    return {
      x: this.width * centerRatio + (x1 + shakeX) * scale,
      y: this.height * 0.52 - (y2 + shakeY) * scale,
      z: z2,
      s: scale
    }
  }

  private drawBox(
    cx: number,
    cy: number,
    cz: number,
    w: number,
    h: number,
    d: number,
    fill: string,
    stroke: string,
    rotY = 0,
    topLabel?: string
  ): void {
    const hw = w / 2
    const hh = h / 2
    const hd = d / 2
    const cos = Math.cos(rotY)
    const sin = Math.sin(rotY)
    const local: Vec3[] = [
      { x: -hw, y: -hh, z: -hd },
      { x: hw, y: -hh, z: -hd },
      { x: hw, y: hh, z: -hd },
      { x: -hw, y: hh, z: -hd },
      { x: -hw, y: -hh, z: hd },
      { x: hw, y: -hh, z: hd },
      { x: hw, y: hh, z: hd },
      { x: -hw, y: hh, z: hd }
    ]

    const pts = local.map(v => {
      const rx = v.x * cos - v.z * sin
      const rz = v.x * sin + v.z * cos
      return this.project({ x: cx + rx, y: cy + v.y, z: cz + rz })
    })

    const faces: { idx: number[]; shade: number; isTop?: boolean }[] = [
      { idx: [0, 1, 2, 3], shade: 0.78 },
      { idx: [4, 5, 6, 7], shade: 1.0 },
      { idx: [0, 4, 7, 3], shade: 0.84 },
      { idx: [1, 5, 6, 2], shade: 0.92 },
      { idx: [3, 2, 6, 7], shade: 1.12, isTop: true },
      { idx: [0, 1, 5, 4], shade: 0.68 }
    ]

    faces.sort((a, b) => {
      const za = a.idx.reduce((acc, i) => acc + pts[i].z, 0) / 4
      const zb = b.idx.reduce((acc, i) => acc + pts[i].z, 0) / 4
      return za - zb
    })

    const ctx = this.ctx
    for (const face of faces) {
      ctx.beginPath()
      face.idx.forEach((i, order) => {
        const pt = pts[i]
        if (order === 0) ctx.moveTo(pt.x, pt.y)
        else ctx.lineTo(pt.x, pt.y)
      })
      ctx.closePath()
      ctx.fillStyle = fill
      ctx.fill()
      ctx.strokeStyle = stroke
      ctx.lineWidth = 1.15
      ctx.stroke()

      if (face.isTop && topLabel) {
        const tx = face.idx.reduce((acc, i) => acc + pts[i].x, 0) / 4
        const ty = face.idx.reduce((acc, i) => acc + pts[i].y, 0) / 4
        ctx.save()
        ctx.fillStyle = stroke
        ctx.font = '700 12px "Cascadia Mono", monospace'
        ctx.textAlign = 'center'
        ctx.textBaseline = 'middle'
        ctx.fillText(topLabel, tx, ty)
        ctx.restore()
      }
    }
  }

  private drawOrbitRing(
    cx: number,
    cy: number,
    cz: number,
    radiusX: number,
    radiusZ: number,
    tiltX: number,
    rotY: number,
    color: string,
    lineWidth = 1.4,
    dashed = false
  ): void {
    const ctx = this.ctx
    const steps = 64
    ctx.save()
    ctx.beginPath()
    if (dashed) ctx.setLineDash([5, 5])
    for (let i = 0; i <= steps; i++) {
      const a = (i / steps) * Math.PI * 2 + rotY
      const lx = Math.cos(a) * radiusX
      const lz = Math.sin(a) * radiusZ
      const ly = lz * Math.sin(tiltX)
      const tz = lz * Math.cos(tiltX)
      const pt = this.project({ x: cx + lx, y: cy + ly, z: cz + tz })
      if (i === 0) ctx.moveTo(pt.x, pt.y)
      else ctx.lineTo(pt.x, pt.y)
    }
    ctx.strokeStyle = color
    ctx.lineWidth = lineWidth
    ctx.stroke()
    ctx.restore()
  }

  private render(): void {
    const ctx = this.ctx
    const palette = THEME_PALETTES[this.state.theme]

    ctx.save()
    ctx.scale(this.dpr, this.dpr)

    // 1. 主题大气天穹背景
    const bgGrad = ctx.createLinearGradient(0, 0, this.width, this.height)
    bgGrad.addColorStop(0, palette.bgTop)
    bgGrad.addColorStop(1, palette.bgBottom)
    ctx.fillStyle = bgGrad
    ctx.fillRect(0, 0, this.width, this.height)

    // 聚焦环境体积光晕（跟随 3D 主装置位置移动）
    const focusPt = this.project({ x: this.anchorOffsetX, y: 0.2, z: 0 })
    const glowRadius = Math.max(this.width, this.height) * 0.42
    const radGrad = ctx.createRadialGradient(focusPt.x, focusPt.y, 12, focusPt.x, focusPt.y, glowRadius)
    radGrad.addColorStop(0, `${palette.glow}${palette.isLight ? '28' : '38'}`)
    radGrad.addColorStop(0.5, `${palette.secondary}${palette.isLight ? '10' : '16'}`)
    radGrad.addColorStop(1, 'transparent')
    ctx.fillStyle = radGrad
    ctx.fillRect(0, 0, this.width, this.height)

    // 2. 透视建筑地网（随视角倾斜与主题色呼应）
    this.drawPerspectiveFloor(palette)

    // 3. 空间漂浮星尘
    this.drawAmbientDust(palette)

    // 4. 按当前设置模块渲染专属 3D 物理互动机关
    switch (this.state.mode) {
      case 'appearance':
        this.drawAppearanceCore(palette)
        break
      case 'shortcuts':
        this.drawShortcutMatrix(palette)
        break
      case 'clipboard':
        this.drawClipboardStack(palette)
        break
      case 'apps':
        this.drawAppsConstellation(palette)
        break
      case 'translation':
        this.drawTranslationTowers(palette)
        break
      case 'audio':
        this.drawAudioEqualizer(palette)
        break
      case 'integrations':
        this.drawRadarBeacon(palette)
        break
    }

    // 5. 3D 爆发微粒
    for (const p of this.bursts) {
      const pt = this.project(p)
      const alpha = Math.max(0, 1 - p.age / p.life)
      ctx.save()
      ctx.globalAlpha = alpha
      ctx.fillStyle = p.color
      ctx.beginPath()
      ctx.arc(pt.x, pt.y, p.size, 0, Math.PI * 2)
      ctx.fill()
      ctx.restore()
    }

    // 6. 前景按钮发射的互动光丝与涟漪
    this.drawButtonPulses(focusPt.x, focusPt.y)

    ctx.restore()
  }

  private drawPerspectiveFloor(palette: ThemePalette): void {
    const ctx = this.ctx
    const y = -1.95
    const range = 6
    ctx.save()
    ctx.strokeStyle = palette.grid
    ctx.lineWidth = 1
    for (let i = -range; i <= range; i++) {
      const a = this.project({ x: i * 0.9, y, z: -range * 0.9 })
      const b = this.project({ x: i * 0.9, y, z: range * 0.9 })
      ctx.beginPath()
      ctx.moveTo(a.x, a.y)
      ctx.lineTo(b.x, b.y)
      ctx.stroke()

      const c = this.project({ x: -range * 0.9, y, z: i * 0.9 })
      const d = this.project({ x: range * 0.9, y, z: i * 0.9 })
      ctx.beginPath()
      ctx.moveTo(c.x, c.y)
      ctx.lineTo(d.x, d.y)
      ctx.stroke()
    }
    ctx.restore()
  }

  private drawAmbientDust(palette: ThemePalette): void {
    const ctx = this.ctx
    const speed = this.state.reducedMotion ? 0 : 0.22
    ctx.save()
    for (let i = 0; i < this.ambientStars.length; i++) {
      const star = this.ambientStars[i]
      const pt = this.project({
        x: star.x + Math.sin(this.elapsed * speed + i) * 0.2,
        y: star.y + Math.cos(this.elapsed * speed * 0.8 + i) * 0.16,
        z: star.z
      })
      const r = Math.max(0.8, Math.min(2.6, pt.s * 0.014))
      ctx.fillStyle = i % 3 === 0 ? palette.primary : palette.secondary
      ctx.globalAlpha = palette.isLight ? 0.28 : 0.42
      ctx.beginPath()
      ctx.arc(pt.x, pt.y, r, 0, Math.PI * 2)
      ctx.fill()
    }
    ctx.restore()
  }

  /** 模块 01：外观与灵动岛 3D 核心棱镜机构（响应主题、动效、缩放、停靠、光轨、全屏细线、隐私罩、微读数）。 */
  private drawAppearanceCore(palette: ThemePalette): void {
    const cx = this.anchorOffsetX
    const s = this.coreScale
    const bob = this.state.reducedMotion ? 0 : Math.sin(this.elapsed * 1.7) * 0.08
    const tuck = this.tuckProgress
    const cloak = this.cloakProgress

    // 全屏避让时升起一片玻璃全屏幕墙示意
    if (tuck > 0.02) {
      this.drawBox(
        0,
        -0.25,
        -1.2,
        5.8,
        3.2 * tuck,
        0.06,
        palette.isLight ? 'rgba(37, 99, 235, 0.08)' : 'rgba(56, 189, 248, 0.09)',
        palette.primary
      )
    }

    // 按不同动效计算 3D 翼片张开形态
    const e = this.expandProgress
    let wingX = 1.05 * e
    let wingY = 0.65 * e
    if (this.state.motion === 'sweep') {
      wingX = 1.42 * Math.min(1, e * 1.55)
      wingY = 0.72 * Math.max(0, (e - 0.25) / 0.75)
    } else if (this.state.motion === 'bounce') {
      const overshoot = e > 0.1 && e < 0.99 ? Math.sin(e * Math.PI) * 0.28 : 0
      wingX = (1.1 + overshoot) * e
      wingY = (0.76 + overshoot * 1.35) * e
    } else if (this.state.motion === 'step') {
      const crisp = Math.min(1, e * 1.35)
      wingX = 1.16 * crisp
      wingY = 0.72 * crisp
    } else if (this.state.motion === 'flicker') {
      const snap = Math.min(1, e * 1.45)
      wingX = 1.15 * snap
      wingY = 0.7 * snap
    }

    // 停靠与细线压缩态变换
    const coreY = bob + tuck * 1.45
    const boxW = (1.55 + wingX) * s
    const boxH = Math.max(0.11, (0.48 + wingY) * (1 - tuck * 0.84)) * s
    const boxD = (0.52 + e * 0.25) * (1 - tuck * 0.55) * s
    const rot = this.state.reducedMotion ? 0.12 : this.elapsed * 0.45

    // 光轨舱门（WebGL）粒子环
    if (this.state.webglGlow && !this.state.reducedMotion) {
      this.drawOrbitRing(cx, coreY, 0, (1.85 + e * 0.4) * s, (1.15 + e * 0.3) * s, 0.36, this.elapsed * 0.9, palette.primary, 1.8)
      this.drawOrbitRing(cx, coreY, 0, (2.25 + e * 0.35) * s, (1.45 + e * 0.25) * s, -0.42, -this.elapsed * 0.65, palette.secondary, 1.2, true)

      // 沿光轨巡航的高亮能量珠
      const ctx = this.ctx
      for (let i = 0; i < 4; i++) {
        const a = this.elapsed * 1.8 + (i * Math.PI) / 2
        const px = cx + Math.cos(a) * (1.85 + e * 0.4) * s
        const pz = Math.sin(a) * (1.15 + e * 0.3) * s
        const py = coreY + pz * Math.sin(0.36)
        const pt = this.project({ x: px, y: py, z: pz * Math.cos(0.36) })
        ctx.save()
        ctx.fillStyle = i % 2 === 0 ? palette.primary : palette.secondary
        ctx.shadowColor = palette.glow
        ctx.shadowBlur = 12
        ctx.beginPath()
        ctx.arc(pt.x, pt.y, 3.6, 0, Math.PI * 2)
        ctx.fill()
        ctx.restore()
      }
    } else {
      // 关闭光轨时仅保留素雅标尺静环
      this.drawOrbitRing(cx, coreY, 0, 1.75 * s, 1.05 * s, 0.24, 0, palette.grid, 1, true)
    }

    // 中央 3D 核心机身（隐私隐藏模式下变为虚影轮廓）
    const fill = cloak > 0.3 ? 'transparent' : palette.faceFill
    const stroke = cloak > 0.3 ? `${palette.secondary}88` : palette.faceStroke
    this.drawBox(cx, coreY, 0, boxW, boxH, boxD, fill, stroke, rot * 0.35)

    // 展开时伸出的左右立体副翼
    if (e > 0.08 && tuck < 0.7) {
      const wingOffset = (boxW / 2 + 0.38 * s) * e
      this.drawBox(cx - wingOffset, coreY, 0.12, 0.42 * s, boxH * 0.78, 0.28 * s, fill, palette.primary, rot * 0.4)
      this.drawBox(cx + wingOffset, coreY, -0.12, 0.42 * s, boxH * 0.78, 0.28 * s, fill, palette.secondary, -rot * 0.4)
    }

    // 隐私光学隐形罩
    if (cloak > 0.05) {
      this.drawBox(
        cx,
        coreY,
        0,
        boxW + 0.55 * cloak,
        boxH + 0.45 * cloak,
        boxD + 0.55 * cloak,
        'rgba(148, 163, 184, 0.08)',
        palette.secondary,
        -rot * 0.2,
        'CLOAKED'
      )
    }

    // 胶囊微读数 3 组立体全息柱（响应 网速 / CPU / 内存 勾选）
    if (this.state.telemetryMode !== 'off') {
      const bars: { enabled: boolean; x: number; level: number; label: string; color: string }[] = [
        { enabled: this.state.telemetryNet, x: cx - 1.55 * s, level: this.state.netLevel, label: 'NET', color: palette.primary },
        { enabled: this.state.telemetryCpu, x: cx, level: this.state.cpuLevel, label: 'CPU', color: palette.secondary },
        { enabled: this.state.telemetryMemory, x: cx + 1.55 * s, level: this.state.memLevel, label: 'MEM', color: palette.glow }
      ]
      for (const bar of bars) {
        if (!bar.enabled) continue
        const h = Math.max(0.25, bar.level * 1.35)
        this.drawBox(bar.x, -1.9 + h / 2, 1.35, 0.34, h, 0.34, `${bar.color}33`, bar.color, 0, bar.label)
      }
    }
  }

  /** 模块 02：快捷键 3D 机械键帽阵（真实按键按下即物理下沉并喷射光柱）。 */
  private drawShortcutMatrix(palette: ThemePalette): void {
    const keys: { label: string; x: number; z: number; w: number }[] = [
      { label: 'Ctrl', x: -1.75, z: 0.75, w: 0.92 },
      { label: 'Alt', x: -0.68, z: 0.75, w: 0.84 },
      { label: 'Shift', x: -1.45, z: -0.4, w: 1.18 },
      { label: 'Space', x: 0.58, z: 0.75, w: 1.48 },
      { label: 'V', x: 0.05, z: -0.4, w: 0.84 },
      { label: 'F', x: 1.08, z: -0.4, w: 0.84 }
    ]

    // 机械键盘金属底盘
    this.drawBox(0, -0.58, 0.18, 4.9, 0.24, 2.55, palette.faceFill, palette.grid)

    for (const k of keys) {
      const press = this.keyPressDepth[k.label] ?? 0
      const y = -0.18 - press * 0.24
      const active = press > 0.05
      this.drawBox(
        k.x,
        y,
        k.z,
        k.w,
        0.46,
        0.84,
        active ? `${palette.primary}55` : palette.faceFill,
        active ? palette.primary : palette.faceStroke,
        0,
        k.label
      )
      // 被按下时向上迸发全息光柱
      if (active) {
        this.drawBox(k.x, y + 0.9 * press, k.z, k.w * 0.45, 1.4 * press, 0.22, `${palette.secondary}28`, palette.secondary)
      }
    }
  }

  /** 模块 03：剪贴板 3D 立体卡片堆栈（暂停冻结 / 上限实时增减层数 / 溢出预警）。 */
  private drawClipboardStack(palette: ThemePalette): void {
    const visibleCards = Math.max(3, Math.min(12, Math.round(this.state.clipLimit / 80) + 2))
    const paused = this.state.clipboardPaused
    const overflow = this.state.clipCount > this.state.clipLimit

    for (let i = 0; i < visibleCards; i++) {
      const ratio = i / visibleCards
      const y = -1.35 + i * 0.27
      const floatWave = paused || this.state.reducedMotion ? 0 : Math.sin(this.elapsed * 2.1 + i * 0.55) * 0.06
      const rot = paused ? 0 : Math.sin(this.elapsed * 0.7 + i * 0.25) * 0.16
      const isOverflowLayer = overflow && i === 0
      const stroke = isOverflowLayer ? '#ef4444' : paused ? '#f59e0b' : ratio > 0.7 ? palette.primary : palette.faceStroke
      const fill = isOverflowLayer ? 'rgba(239, 68, 68, 0.22)' : paused ? 'rgba(245, 158, 11, 0.14)' : palette.faceFill
      this.drawBox(0, y + floatWave, 0, 2.35 - ratio * 0.25, 0.11, 1.45 - ratio * 0.15, fill, stroke, rot)
    }

    // 暂停态笼罩冻结框
    if (paused) {
      this.drawBox(0, 0.1, 0, 2.85, 3.2, 1.9, 'rgba(245, 158, 11, 0.05)', '#f59e0b', 0, 'PAUSED')
    }
  }

  /** 模块 04：应用管理 3D 悬浮星阵方格。 */
  private drawAppsConstellation(palette: ThemePalette): void {
    const cols = 4
    const rows = 3
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const idx = r * cols + c
        const x = (c - 1.5) * 0.95
        const z = (r - 1) * 0.95
        const y = this.state.reducedMotion ? 0 : Math.sin(this.elapsed * 2 + idx * 0.6) * 0.16
        const highlight = idx % 3 === 0
        this.drawBox(
          x,
          y,
          z,
          0.56,
          0.56,
          0.56,
          highlight ? `${palette.primary}38` : palette.faceFill,
          highlight ? palette.primary : palette.faceStroke,
          this.state.reducedMotion ? 0 : this.elapsed * 0.5 + idx * 0.2
        )
      }
    }
  }

  /** 模块 05：翻译 3D 双语晶体塔（点击方向翻转时 180° 空间换位 + 试译光束对射）。 */
  private drawTranslationTowers(palette: ThemePalette): void {
    const r = 1.65
    const angle = this.translateSpin
    const leftX = -Math.cos(angle) * r
    const leftZ = -Math.sin(angle) * r * 0.7
    const rightX = Math.cos(angle) * r
    const rightZ = Math.sin(angle) * r * 0.7
    const bob = this.state.reducedMotion ? 0 : Math.sin(this.elapsed * 1.8) * 0.08

    // 源语塔 & 目标语塔
    this.drawBox(leftX, bob, leftZ, 0.82, 2.1, 0.82, `${palette.primary}33`, palette.primary, this.elapsed * 0.4, 'ZH')
    this.drawBox(rightX, -bob, rightZ, 0.82, 2.1, 0.82, `${palette.secondary}33`, palette.secondary, -this.elapsed * 0.4, 'EN')

    // 中央转译棱镜
    this.drawBox(0, 0, 0, 0.72, 0.72, 0.72, palette.faceFill, palette.glow, this.elapsed * 1.1)
    this.drawOrbitRing(0, 0, 0, 1.9, 1.35, 0.18, this.elapsed * 0.6, palette.grid, 1.2, true)

    // 试译触发的能量粒子束
    if (this.translateBeamAge >= 0) {
      const p1 = this.project({ x: leftX, y: 0.2, z: leftZ })
      const p2 = this.project({ x: rightX, y: 0.2, z: rightZ })
      const progress = Math.min(1, this.translateBeamAge / 0.85)
      const ctx = this.ctx
      ctx.save()
      ctx.strokeStyle = palette.glow
      ctx.lineWidth = 3.2
      ctx.shadowColor = palette.glow
      ctx.shadowBlur = 16
      ctx.beginPath()
      ctx.moveTo(p1.x, p1.y)
      ctx.lineTo(p1.x + (p2.x - p1.x) * progress, p1.y + (p2.y - p1.y) * progress)
      ctx.stroke()
      ctx.restore()
    }
  }

  /** 模块 06：调音台 3D 立体音频频谱柱阵。 */
  private drawAudioEqualizer(palette: ThemePalette): void {
    const count = 9
    for (let i = 0; i < count; i++) {
      const x = (i - (count - 1) / 2) * 0.56
      const wave = this.state.audioMuted || this.state.reducedMotion
        ? 0.22
        : 0.35 + Math.abs(Math.sin(this.elapsed * 3.4 + i * 0.72)) * 1.75
      const col = i % 2 === 0 ? palette.primary : palette.secondary
      this.drawBox(x, -1.6 + wave / 2, 0, 0.34, wave, 0.38, `${col}3d`, col)
    }
  }

  /** 模块 07：通知与智能体 3D 通信雷达信标。 */
  private drawRadarBeacon(palette: ThemePalette): void {
    this.drawBox(0, -0.7, 0, 0.65, 1.85, 0.65, palette.faceFill, palette.primary, this.elapsed * 0.5, 'HUB')
    this.drawOrbitRing(0, -0.2, 0, 1.2, 1.2, 0, this.elapsed, palette.primary, 1.5)
    this.drawOrbitRing(0, -0.2, 0, 2.1, 2.1, 0, -this.elapsed * 0.6, palette.secondary, 1.2, true)
    this.drawOrbitRing(0, -0.2, 0, 3.0, 3.0, 0, this.elapsed * 0.35, palette.grid, 1)

    if (this.radarWaveAge >= 0) {
      const r = 0.6 + this.radarWaveAge * 2.4
      const alpha = Math.max(0, 1 - this.radarWaveAge / 1.6)
      const ctx = this.ctx
      ctx.save()
      ctx.globalAlpha = alpha
      this.drawOrbitRing(0, -0.2, 0, r, r, 0, 0, palette.glow, 2.8)
      ctx.restore()
    }
  }

  /** 绘制从按钮点击处射向 3D 舞台核心的实时反馈光丝与扩散涟漪。 */
  private drawButtonPulses(targetX: number, targetY: number): void {
    const ctx = this.ctx
    for (const pulse of this.pulses) {
      const t = Math.min(1, pulse.age / pulse.duration)
      const inv = 1 - t

      // 按钮原位涟漪圈
      ctx.save()
      ctx.strokeStyle = pulse.color
      ctx.globalAlpha = inv * 0.75
      ctx.lineWidth = 2
      ctx.beginPath()
      ctx.arc(pulse.x, pulse.y, 10 + t * 54, 0, Math.PI * 2)
      ctx.stroke()

      // 连线光束飞向 3D 核心
      const head = Math.min(1, t * 1.45)
      const tail = Math.max(0, t * 1.45 - 0.45)
      const x1 = pulse.x + (targetX - pulse.x) * tail
      const y1 = pulse.y + (targetY - pulse.y) * tail
      const x2 = pulse.x + (targetX - pulse.x) * head
      const y2 = pulse.y + (targetY - pulse.y) * head

      ctx.strokeStyle = pulse.color
      ctx.globalAlpha = inv * 0.9
      ctx.lineWidth = 2.4
      ctx.shadowColor = pulse.color
      ctx.shadowBlur = 12
      ctx.beginPath()
      ctx.moveTo(x1, y1)
      ctx.lineTo(x2, y2)
      ctx.stroke()

      // 核心命中闪环
      if (t > 0.45) {
        const hitT = (t - 0.45) / 0.55
        ctx.globalAlpha = (1 - hitT) * 0.7
        ctx.beginPath()
        ctx.arc(targetX, targetY, 16 + hitT * 78, 0, Math.PI * 2)
        ctx.stroke()
      }
      ctx.restore()
    }
  }
}
