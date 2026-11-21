type Spring = { value: number; velocity: number; target: number }
type Visual = HTMLElement | SVGElement
type Effect = {
  control: HTMLButtonElement; visual: Visual; original: string; restTransform: string; card: boolean
  x: Spring; y: Spring; press: Spring
}

const controls = '.capsule, .app-tile, .mode-nav button, .round-action'
const spring = (): Spring => ({ value: 0, velocity: 0, target: 0 })
const bound = (value: number, limit: number) => Math.max(-limit, Math.min(limit, value))

type MotionPhysicsProfile = { damping: number; omega: number; pressScale: number; stepPx: number }
const MOTION_PROFILES: Record<string, MotionPhysicsProfile> = {
  glide: { damping: 18, omega: 12, pressScale: 0.07, stepPx: 0 },
  sweep: { damping: 24, omega: 16, pressScale: 0.05, stepPx: 0 },
  bounce: { damping: 10, omega: 18, pressScale: 0.11, stepPx: 0 },
  step: { damping: 26, omega: 14, pressScale: 0.08, stepPx: 1.5 },
  flicker: { damping: 15, omega: 15, pressScale: 0.07, stepPx: 0 },
  unroll: { damping: 20, omega: 9, pressScale: 0.05, stepPx: 0 }
}

export function advanceSpring(axis: Spring, seconds: number, damping = 18, omega = 12): boolean {
  const offset = axis.value - axis.target
  const decay = Math.exp(-damping * seconds)
  const cos = Math.cos(omega * seconds), sin = Math.sin(omega * seconds)
  const phase = (axis.velocity + damping * offset) / omega
  axis.value = axis.target + decay * (offset * cos + phase * sin)
  axis.velocity = decay * (axis.velocity * cos - (damping * phase + omega * offset) * sin)
  if (Math.abs(axis.value - axis.target) > .002 || Math.abs(axis.velocity) > .03) return true
  axis.value = axis.target
  axis.velocity = 0
  return false
}

export function createIslandPhysics(panel: HTMLElement) {
  const effects = new Map<Visual, Effect>()
  const listeners = new AbortController()
  const passive = { capture: true, passive: true, signal: listeners.signal }
  let enabled = false, visible = true, disposed = false, frame = 0, previous = 0
  let current: Effect | undefined
  let pending: { effect: Effect; x: number; y: number } | undefined
  let press: { effect: Effect; id: number; drag: boolean; x: number; y: number;
    lastX: number; lastY: number; time: number; vx: number; vy: number } | undefined

  const releaseEffect = (effect: Effect) => {
    effect.x.target = effect.y.target = effect.press.target = 0
  }
  function reset() {
    cancelAnimationFrame(frame)
    frame = previous = 0
    pending = current = press = undefined
    for (const effect of effects.values()) effect.visual.style.transform = effect.original
    effects.clear()
  }
  const awake = () => enabled && visible && !disposed && !document.hidden
  function tick(now: number) {
    frame = 0
    if (!awake()) { reset(); return }
    if (pending) {
      const { effect, x, y } = pending
      pending = undefined
      const rect = effect.control.getBoundingClientRect()
      if (rect.width && rect.height) {
        effect.x.target = bound((x - rect.left) / rect.width * 2 - 1, 1) * 2.5
        effect.y.target = bound((y - rect.top) / rect.height * 2 - 1, 1) * 2
      }
    }
    const seconds = Math.min(.064, Math.max(0, now - previous) / 1000)
    previous = now
    const profile = MOTION_PROFILES[panel.dataset.motion || 'glide'] ?? MOTION_PROFILES.glide
    let moving = false
    for (const effect of effects.values()) {
      const { visual, x, y, press: depth } = effect
      if (!panel.contains(visual)) {
        visual.style.transform = effect.original
        effects.delete(visual)
        if (current === effect) current = undefined
        if (press?.effect === effect) press = undefined
        continue
      }
      const mx = advanceSpring(x, seconds, profile.damping, profile.omega)
      const my = advanceSpring(y, seconds, profile.damping, profile.omega)
      const mp = advanceSpring(depth, seconds, profile.damping, profile.omega)
      if (!mx && !my && !mp && !x.value && !y.value && !depth.value) {
        visual.style.transform = effect.original
        if (current !== effect && press?.effect !== effect) effects.delete(visual)
        continue
      }
      const tx = profile.stepPx > 0 ? Math.round(x.value / profile.stepPx) * profile.stepPx : x.value
      const ty = profile.stepPx > 0 ? Math.round(y.value / profile.stepPx) * profile.stepPx : y.value
      const tilt = effect.card ? 'perspective(120px) rotateX(' + (-ty * 2).toFixed(2) + 'deg) rotateY(' + (tx * 1.6).toFixed(2) + 'deg) ' : ''
      visual.style.transform = 'translate3d(' + tx.toFixed(2) + 'px,' + ty.toFixed(2) + 'px,0) ' + tilt + 'scale(' + (1 - depth.value * profile.pressScale).toFixed(3) + ')' + effect.restTransform
      moving ||= mx || my || mp
    }
    if (moving) frame = requestAnimationFrame(tick)
  }
  function wake() {
    if (!frame && effects.size && awake()) {
      previous = performance.now()
      frame = requestAnimationFrame(tick)
    }
  }
  function select(event: PointerEvent) {
    const control = event.target instanceof Element ? event.target.closest<HTMLButtonElement>(controls) : null
    const valid = control && panel.contains(control) && !control.disabled && !control.closest('[inert]')
    const selector = control?.matches('.capsule') ? (panel.classList.contains('expanded') && !control.classList.contains('has-feedback') ? '.grip' : '.capsule-text') : ':scope > svg, :scope > img, :scope > .fallback'
    const visual = valid ? control.querySelector<Visual>(selector) : null
    if (current && visual === current.visual) return current
    if (current) releaseEffect(current)
    pending = current = undefined
    if (!visual || !control) { wake(); return }
    let effect = effects.get(visual)
    if (!effect) {
      const base = getComputedStyle(visual).transform
      effect = { control, visual, original: visual.style.transform, restTransform: base === 'none' ? '' : ' ' + base,
        card: control.classList.contains('app-tile'), x: spring(), y: spring(), press: spring() }
      effects.set(visual, effect)
    }
    current = effect
    return effect
  }
  function move(event: PointerEvent) {
    if (!awake() || !event.isPrimary) return
    if (press) {
      if (press.id !== event.pointerId) return
      if (!(event.buttons & 1)) { end(event); return }
      if (press.drag) {
        const { effect } = press
        const dt = Math.max(1, event.timeStamp - press.time)
        const x = bound((event.screenX - press.x) * .13, 4), y = bound((event.screenY - press.y) * .13, 3)
        const vx = (x - press.lastX) * 1000 / dt, vy = (y - press.lastY) * 1000 / dt
        press.vx = bound(vx, 54); press.vy = bound(vy, 54)
        press.lastX = x; press.lastY = y; press.time = event.timeStamp
        effect.x.target = effect.x.value = x; effect.y.target = effect.y.value = y
        effect.x.velocity = effect.y.velocity = 0
        pending = undefined
      }
    } else if (event.pointerType !== 'touch') {
      const effect = select(event)
      if (effect) pending = { effect, x: event.clientX, y: event.clientY }
    }
    wake()
  }
  function down(event: PointerEvent) {
    if (!awake() || !event.isPrimary || event.button !== 0 || press) return
    const effect = select(event)
    if (!effect) return
    press = { effect, id: event.pointerId, drag: effect.control.matches('.capsule'), x: event.screenX, y: event.screenY, lastX: 0, lastY: 0, time: event.timeStamp, vx: 0, vy: 0 }
    effect.press.target = 1
    wake()
  }
  function end(event: PointerEvent) {
    if (!press || press.id !== event.pointerId) return
    const { effect, time, vx, vy, drag } = press
    if (drag && event.type !== 'pointercancel' && event.timeStamp - time < 80) {
      effect.x.velocity = vx
      effect.y.velocity = vy
    }
    releaseEffect(effect)
    pending = current = press = undefined
    wake()
  }
  function leave(event: PointerEvent) {
    if (event.target !== panel || press || !current) return
    releaseEffect(current)
    current = pending = undefined
    wake()
  }
  panel.addEventListener('pointermove', move, passive)
  panel.addEventListener('pointerdown', down, passive)
  panel.addEventListener('pointerleave', leave, passive)
  panel.addEventListener('lostpointercapture', end, passive)
  window.addEventListener('pointerup', end, passive)
  window.addEventListener('pointercancel', end, passive)
  window.addEventListener('blur', reset, { signal: listeners.signal })
  document.addEventListener('visibilitychange', reset, { signal: listeners.signal })
  const observer = new IntersectionObserver(entries => {
    visible = entries[entries.length - 1].isIntersecting
    if (!visible) reset()
  })
  observer.observe(panel)
  return {
    setEnabled(value: boolean) { enabled = value; if (!value) reset() },
    reset,
    dispose() { disposed = true; reset(); listeners.abort(); observer.disconnect() }
  }
}
