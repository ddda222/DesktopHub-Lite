import type { IslandCueKind } from './useIslandEffects'

export const FLOW_HEIGHT = 22
export const flowDuration: Record<IslandCueKind, number> = {
  launch: 720, notice: 880, loading: 1350, complete: 780,
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
  const count = low ? (kind === 'notice' ? 4 : 5) : { launch: 12, notice: 6, loading: 14, complete: 12 }[kind]
  return Array.from({ length: count }, (_, index) => ({
    index, seed: ((index + 1) * 0.61803398875) % 1,
    offsetX: 0, offsetY: 0, velocityX: 0, velocityY: 0,
  }))
}

export function paintFlow(
  ctx: CanvasRenderingContext2D, motes: Mote[], kind: IslandCueKind,
  progress: number, width: number, elapsed: number, pointer: FlowPointer | null, dark: boolean, color?: string,
): void {
  ctx.clearRect(0, 0, width, FLOW_HEIGHT)
  const drag = Math.exp(-elapsed * 16)
  ctx.fillStyle = ctx.strokeStyle = color || (dark ? '#b8d6ff' : '#2558c4')
  ctx.lineCap = 'round'
  for (const mote of motes) {
    const seed = mote.seed
    const side = mote.index % 2 === 0 ? -1 : 1
    const fraction = mote.index / Math.max(1, motes.length - 1)
    const time = clamp((progress - seed * .13) / .87, 0, 1)
    const blend = time * time * (3 - 2 * time)
    let along: number
    let rise: number
    let tangent: number
    let alpha = Math.min(1, time * 10, (1 - time) * 5)
    switch (kind) {
      case 'launch':
        along = .5 + side * (.16 + seed * .23) * (1 - blend)
        rise = 4 + Math.sin(Math.PI * time) * (1.5 + seed * 3)
        tangent = -side * (2 + seed * 3)
        break
      case 'notice': {
        const beat = clamp((progress - (mote.index % 2) * .34 - seed * .06) / .48, 0, 1)
        const pulse = Math.sin(Math.PI * beat)
        along = .73 + seed * .07
        rise = 4 + (2 + seed * 5) * pulse
        tangent = 0
        alpha = pulse
        break
      }
      case 'loading': {
        const lane = mote.index % 2
        along = .11 + blend * .73 + fraction * .045 + lane * .018
        rise = 4 + lane * 3 + 2 * Math.sin(Math.PI * 2 * (along * 1.5 + time * .35))
        tangent = 2 + seed * 2
        break
      }
      case 'complete':
        along = .5 + side * (.035 + (.10 + seed * .27) * (1 - (1 - time) ** 2))
        rise = 4 + Math.sin(Math.PI * time) * (2 + seed * 5)
        tangent = side * (1.5 + seed * 1.8)
        break
    }
    const centerX = along * width
    const curl = Math.sin(along * 9 + time * 4 + seed * 2) * 1.4
    const centerY = rise + curl
    const dx = pointer ? pointer.x - centerX : 0
    const dy = pointer ? pointer.y - centerY : 0
    const distance = Math.hypot(dx, dy)
    const influence = pointer ? Math.max(0, 1 - distance / 56) ** 2 : 0
    const force = pointer?.pressed ? -2.3 : 1.5
    const fieldX = clamp(dx * force * influence, -14, 14)
    const fieldY = clamp(dy * force * influence, -7, 7)
    mote.velocityX = (mote.velocityX + (fieldX - mote.offsetX) * elapsed * 190) * drag
    mote.velocityY = (mote.velocityY + (fieldY - mote.offsetY) * elapsed * 190) * drag
    mote.offsetX += mote.velocityX * elapsed
    mote.offsetY += mote.velocityY * elapsed
    if (alpha < .015) continue
    const x = centerX + mote.offsetX
    const y = clamp(centerY + mote.offsetY, 2.4, FLOW_HEIGHT - 2)
    const radius = .85 + seed * .65
    ctx.globalAlpha = alpha * (.56 + seed * .22)
    ctx.lineWidth = radius
    ctx.beginPath()
    ctx.moveTo(x - tangent, y)
    ctx.lineTo(x, y)
    ctx.stroke()
    ctx.beginPath()
    ctx.arc(x, y, radius, 0, Math.PI * 2)
    ctx.fill()
    ctx.globalAlpha = alpha * (.045 + influence * .17)
    ctx.beginPath()
    ctx.arc(x, y, radius * (2.3 + influence * 1.6), 0, Math.PI * 2)
    ctx.fill()
  }
  ctx.globalAlpha = 1
}
