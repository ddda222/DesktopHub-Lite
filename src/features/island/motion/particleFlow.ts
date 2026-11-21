import type { IslandCueKind } from './useIslandEffects'

export const FLOW_HEIGHT = 44
export const flowDuration: Record<IslandCueKind, number> = {
  launch: 820, notice: 960, loading: 1450, complete: 920,
}

interface Mote {
  index: number
  seed: number
  offsetX: number
  offsetY: number
  velocityX: number
  velocityY: number
}

export interface FlowPointer { x: number; y: number; pressed: boolean }

const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(max, value))

export function createMotes(kind: IslandCueKind, low: boolean): Mote[] {
  const count = low ? (kind === 'notice' ? 6 : 8) : { launch: 18, notice: 10, loading: 20, complete: 18 }[kind]
  return Array.from({ length: count }, (_, index) => ({
    index, seed: ((index + 1) * 0.61803398875) % 1,
    offsetX: 0, offsetY: 0, velocityX: 0, velocityY: 0,
  }))
}

const CANDY_COLORS = ['#ff4d7d', '#ffd166', '#06d6a0', '#38bdf8', '#c084fc']

export function paintFlow(
  ctx: CanvasRenderingContext2D, motes: Mote[], kind: IslandCueKind,
  progress: number, width: number, elapsed: number, pointer: FlowPointer | null, dark: boolean, color?: string,
  motion = 'glide',
): void {
  ctx.clearRect(0, 0, width, FLOW_HEIGHT)
  const drag = Math.exp(-elapsed * 16)
  const baseColor = color || (dark ? '#93c5fd' : '#2563eb')
  ctx.fillStyle = ctx.strokeStyle = baseColor
  ctx.lineCap = motion === 'step' ? 'square' : 'round'
  for (const mote of motes) {
    const seed = mote.seed
    const side = mote.index % 2 === 0 ? -1 : 1
    const fraction = mote.index / Math.max(1, motes.length - 1)
    const time = clamp((progress - seed * .14) / .86, 0, 1)
    const blend = time * time * (3 - 2 * time)
    let along: number
    let rise: number
    let tangent: number
    let alpha = Math.min(1, time * 9, (1 - time) * 4.2)
    switch (kind) {
      case 'launch':
        along = .5 + side * (.06 + (.18 + seed * .28) * blend)
        rise = 8 + Math.sin(Math.PI * time) * (8 + seed * 16)
        tangent = side * (6 + seed * 8)
        break
      case 'notice': {
        const beat = clamp((progress - (mote.index % 2) * .32 - seed * .06) / .52, 0, 1)
        const pulse = Math.sin(Math.PI * beat)
        along = .72 + seed * .14
        rise = 8 + (6 + seed * 14) * pulse
        tangent = 0
        alpha = pulse
        break
      }
      case 'loading': {
        const lane = mote.index % 2
        along = .08 + blend * .82 + fraction * .045 + lane * .018
        rise = 10 + lane * 8 + 5 * Math.sin(Math.PI * 2 * (along * 1.5 + time * .45))
        tangent = 6 + seed * 5
        break
      }
      case 'complete':
        along = .5 + side * (.04 + (.14 + seed * .34) * (1 - (1 - time) ** 2))
        rise = 8 + Math.sin(Math.PI * time) * (10 + seed * 18)
        tangent = side * (5 + seed * 7)
        break
    }
    const centerX = along * width
    const curl = Math.sin(along * 8 + time * 5 + seed * 3) * 3.2
    const centerY = rise + curl
    const dx = pointer ? pointer.x - centerX : 0
    const dy = pointer ? pointer.y - centerY : 0
    const distance = Math.hypot(dx, dy)
    const influence = pointer ? Math.max(0, 1 - distance / 64) ** 2 : 0
    const force = pointer?.pressed ? -2.4 : 1.6
    const fieldX = clamp(dx * force * influence, -18, 18)
    const fieldY = clamp(dy * force * influence, -12, 12)
    mote.velocityX = (mote.velocityX + (fieldX - mote.offsetX) * elapsed * 190) * drag
    mote.velocityY = (mote.velocityY + (fieldY - mote.offsetY) * elapsed * 190) * drag
    mote.offsetX += mote.velocityX * elapsed
    mote.offsetY += mote.velocityY * elapsed
    if (alpha < .015) continue
    const rawX = centerX + mote.offsetX
    const rawY = clamp(centerY + mote.offsetY, 4, FLOW_HEIGHT - 4)
    const x = motion === 'step' ? Math.round(rawX / 4) * 4 : rawX
    const y = motion === 'step' ? Math.round(rawY / 4) * 4 : rawY
    const radius = 1.7 + seed * 1.5

    if (motion === 'bounce') {
      const bounceWave = 1 + Math.sin(Math.PI * time * 1.5) * .35
      const r = (2.8 + seed * 2.4) * bounceWave
      ctx.fillStyle = CANDY_COLORS[mote.index % CANDY_COLORS.length]
      ctx.globalAlpha = alpha * .9
      ctx.beginPath()
      ctx.arc(x, y, r, 0, Math.PI * 2)
      ctx.fill()
      ctx.fillStyle = '#ffffff'
      ctx.globalAlpha = alpha * .85
      ctx.beginPath()
      ctx.arc(x - r * .32, y - r * .32, Math.max(.9, r * .3), 0, Math.PI * 2)
      ctx.fill()
      continue
    }

    if (motion === 'step') {
      const size = Math.max(4, Math.round(radius * 2.2))
      ctx.fillStyle = mote.index % 3 === 0 ? '#facc15' : baseColor
      ctx.globalAlpha = alpha * .92
      ctx.fillRect(x - size / 2, y - size / 2, size, size)
      if (mote.index % 2 === 0) {
        ctx.fillRect(x - size * 1.35, y - 1, size * 2.7, 2)
        ctx.fillRect(x - 1, y - size * 1.35, 2, size * 2.7)
      }
      ctx.globalAlpha = alpha * (.14 + influence * .22)
      ctx.beginPath()
      ctx.arc(x, y, size * 1.2, 0, Math.PI * 2)
      ctx.fill()
      continue
    }

    if (motion === 'flicker') {
      const arcColor = mote.index % 2 === 0 ? '#ff2bd6' : '#00f0ff'
      const jitter = Math.sin(time * 42 + mote.index * 3.1) > -.3 ? 1 : .2
      ctx.fillStyle = ctx.strokeStyle = arcColor
      ctx.globalAlpha = alpha * .95 * jitter
      ctx.lineWidth = Math.max(1.6, radius * .85)
      const zig = (mote.index % 2 === 0 ? 1 : -1) * (3 + seed * 4)
      ctx.beginPath()
      ctx.moveTo(x - tangent * 2.1, y - zig * .7)
      ctx.lineTo(x - tangent * .95, y + zig * .85)
      ctx.lineTo(x, y)
      ctx.stroke()
      ctx.fillStyle = '#ffffff'
      ctx.beginPath()
      ctx.arc(x, y, radius * .85, 0, Math.PI * 2)
      ctx.fill()
      ctx.fillStyle = arcColor
      ctx.globalAlpha = alpha * (.22 + influence * .25) * jitter
      ctx.beginPath()
      ctx.arc(x, y, radius * 2.8, 0, Math.PI * 2)
      ctx.fill()
      continue
    }

    if (motion === 'unroll') {
      const angle = Math.PI * .25 + time * 2.4 * side + seed * 3
      const petal = 3.2 + seed * 2.6
      ctx.fillStyle = mote.index % 3 === 0 ? '#f59e0b' : baseColor
      ctx.globalAlpha = alpha * .82
      ctx.save()
      ctx.translate(x, y)
      ctx.rotate(angle)
      ctx.beginPath()
      ctx.moveTo(0, -petal)
      ctx.lineTo(petal * .68, 0)
      ctx.lineTo(0, petal)
      ctx.lineTo(-petal * .68, 0)
      ctx.closePath()
      ctx.fill()
      ctx.restore()
      ctx.globalAlpha = alpha * (.12 + influence * .18)
      ctx.beginPath()
      ctx.arc(x, y, radius * 2.4, 0, Math.PI * 2)
      ctx.fill()
      continue
    }

    // glide & sweep
    ctx.fillStyle = ctx.strokeStyle = baseColor
    const streak = motion === 'sweep' ? tangent * 2.5 : tangent * 1.4
    ctx.globalAlpha = alpha * .85
    ctx.lineWidth = motion === 'sweep' ? radius * .95 : radius * .85
    ctx.beginPath()
    ctx.moveTo(x - streak, y)
    ctx.lineTo(x, y)
    ctx.stroke()
    if (motion === 'sweep') {
      const tick = 3.5 + seed * 2.5
      ctx.strokeStyle = '#ffffff'
      ctx.lineWidth = 1.2
      ctx.beginPath()
      ctx.moveTo(x, y - tick)
      ctx.lineTo(x, y + tick)
      ctx.stroke()
    }
    ctx.fillStyle = motion === 'sweep' ? '#ffffff' : baseColor
    ctx.beginPath()
    ctx.arc(x, y, radius, 0, Math.PI * 2)
    ctx.fill()
    ctx.fillStyle = baseColor
    ctx.globalAlpha = alpha * (.16 + influence * .22)
    ctx.beginPath()
    ctx.arc(x, y, radius * (2.6 + influence * 1.6), 0, Math.PI * 2)
    ctx.fill()
  }
  ctx.globalAlpha = 1
}
