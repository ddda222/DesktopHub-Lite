<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'

const props = defineProps<{ lightSurface: boolean; signal?: string }>()
const emit = defineEmits<{ unavailable: [] }>()
const canvas = ref<HTMLCanvasElement | null>(null)
let dispose: (() => void) | undefined

const vertexSource = `
attribute vec2 position;
void main() {
  gl_Position = vec4(position, 0.0, 1.0);
}`

const fragmentSource = `
precision mediump float;
uniform vec2 resolution;
uniform vec2 light;
uniform float radius;
uniform float pixel;
uniform float intensity;
uniform float lightSurface;
uniform vec2 rippleOrigin;
uniform float rippleAge;
void main() {
  vec2 point = gl_FragCoord.xy;
  vec2 box = abs(point - resolution * .5) - resolution * .5 + radius + pixel * .75;
  float edge = (length(max(box, 0.)) + min(max(box.x, box.y), 0.) - radius) / pixel;
  float depth = max(-edge, 0.);
  // 胶囊限制为 6px，面板扩展至 11px 柔光晕，正文区域仍完全透明。
  float band = mix(6., 11., smoothstep(48., 120., resolution.y / pixel));
  float edgeMask = 1. - smoothstep(band * .45, band, depth);
  float rim = exp(-pow((depth - 1.) / .9, 2.));
  float glow = exp(-pow(depth / (band * .55), 2.));
  float reach = clamp(min(resolution.x, resolution.y) * .3, 32. * pixel, 140. * pixel);
  float focus = exp(-pow(length(point - light * resolution) / reach, 2.) * 1.35);
  float ring = exp(-pow((length((point - rippleOrigin * resolution) / pixel) - rippleAge * 280.) / 8., 2.));
  float wave = ring * pow(max(1. - rippleAge / .9, 0.), 2.) * smoothstep(0., .055, rippleAge);
  // 无全周常亮项：未被光斑照到的边缘交给 CSS 中性描边。
  float spot = smoothstep(.03, .85, focus) * intensity;
  float halo = glow * spot * mix(.72, .58, lightSurface);
  float alpha = (rim * spot * .65 + halo + wave * .5) * edgeMask;
  vec3 darkLight = mix(vec3(.28, .65, 1.), vec3(.65, .95, 1.), spot);
  vec3 paleLight = mix(vec3(.08, .38, .78), vec3(.02, .54, .85), spot);
  gl_FragColor = vec4(mix(darkLight, paleLight, lightSurface), min(alpha, .9) * (1. - smoothstep(-.4, .7, edge)));
}`

function compile(gl: WebGLRenderingContext, type: number, source: string): WebGLShader | null {
  const shader = gl.createShader(type)
  if (!shader) return null
  gl.shaderSource(shader, source)
  gl.compileShader(shader)
  if (gl.getShaderParameter(shader, gl.COMPILE_STATUS)) return shader
  console.warn('边缘光效着色器编译失败：', gl.getShaderInfoLog(shader))
  gl.deleteShader(shader)
  return null
}

function createProgram(gl: WebGLRenderingContext): WebGLProgram | null {
  const vertex = compile(gl, gl.VERTEX_SHADER, vertexSource)
  const fragment = compile(gl, gl.FRAGMENT_SHADER, fragmentSource)
  const program = vertex && fragment ? gl.createProgram() : null
  if (program && vertex && fragment) {
    gl.attachShader(program, vertex)
    gl.attachShader(program, fragment)
    gl.linkProgram(program)
  }
  gl.deleteShader(vertex)
  gl.deleteShader(fragment)
  if (program && gl.getProgramParameter(program, gl.LINK_STATUS)) return program
  if (program) console.warn('边缘光效着色器链接失败：', gl.getProgramInfoLog(program))
  gl.deleteProgram(program)
  return null
}

onMounted(() => {
  const element = canvas.value
  const surface = element?.parentElement
  // 按需绘制后保留最后一帧，避免其他悬停样式合成时取到已清空的缓冲区。
  const gl = element?.getContext('webgl', {
    alpha: true, antialias: false, premultipliedAlpha: false,
    preserveDrawingBuffer: true, depth: false, stencil: false, powerPreference: 'low-power'
  })
  if (!element || !surface || !gl) {
    emit('unavailable')
    return
  }

  const program = createProgram(gl)
  const triangle = gl.createBuffer()
  if (!program || !triangle) {
    gl.deleteProgram(program)
    gl.deleteBuffer(triangle)
    emit('unavailable')
    return
  }

  const position = gl.getAttribLocation(program, 'position')
  const resolution = gl.getUniformLocation(program, 'resolution')
  const light = gl.getUniformLocation(program, 'light')
  const radius = gl.getUniformLocation(program, 'radius')
  const pixel = gl.getUniformLocation(program, 'pixel')
  const intensity = gl.getUniformLocation(program, 'intensity')
  const lightSurface = gl.getUniformLocation(program, 'lightSurface')
  const rippleOrigin = gl.getUniformLocation(program, 'rippleOrigin')
  const rippleAge = gl.getUniformLocation(program, 'rippleAge')
  gl.useProgram(program)
  gl.bindBuffer(gl.ARRAY_BUFFER, triangle)
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW)
  gl.enableVertexAttribArray(position)
  gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0)

  let frame = 0
  let previousTime = 0
  let targetX = 0.27
  let targetY = 1
  let targetEnergy = 0.18
  let lightX = targetX
  let lightY = targetY
  let energy = targetEnergy
  let rippleStart = -Infinity
  let rippleX = targetX
  let rippleY = targetY
  let surfaceTone = Number(props.lightSurface)

  const requestDraw = (): void => {
    if (!frame) frame = requestAnimationFrame(draw)
  }

  const draw = (now: number): void => {
    frame = 0
    const ratio = Math.min(window.devicePixelRatio, 1.5)
    const width = Math.round(element.clientWidth * ratio)
    const height = Math.round(element.clientHeight * ratio)
    if (!width || !height) return
    if (element.width !== width || element.height !== height) {
      element.width = width
      element.height = height
      gl.viewport(0, 0, width, height)
    }

    const elapsed = previousTime ? Math.min(now - previousTime, 50) : 16
    const follow = 1 - Math.exp(-elapsed / 55)
    lightX += (targetX - lightX) * follow
    lightY += (targetY - lightY) * follow
    energy += (targetEnergy - energy) * follow
    surfaceTone += (Number(props.lightSurface) - surfaceTone) * follow
    const age = Math.min((now - rippleStart) / 1000, 1)

    const corner = parseFloat(getComputedStyle(surface).borderTopLeftRadius) || 0
    gl.uniform2f(resolution, width, height)
    gl.uniform2f(light, lightX, lightY)
    gl.uniform1f(radius, Math.min(corner * ratio, width / 2, height / 2))
    gl.uniform1f(pixel, ratio)
    gl.uniform1f(intensity, energy)
    gl.uniform1f(lightSurface, surfaceTone)
    gl.uniform2f(rippleOrigin, rippleX, rippleY)
    gl.uniform1f(rippleAge, age)
    gl.drawArrays(gl.TRIANGLES, 0, 3)

    const remaining = Math.abs(targetX - lightX) + Math.abs(targetY - lightY) + Math.abs(targetEnergy - energy) + Math.abs(Number(props.lightSurface) - surfaceTone)
    previousTime = remaining > 0.006 || age < 0.9 ? now : 0
    if (previousTime) requestDraw()
  }

  const followPointer = (event: PointerEvent): void => {
    const box = surface.getBoundingClientRect()
    if (!box.width || !box.height) return
    const x = Math.max(0, Math.min(event.clientX - box.left, box.width))
    const y = Math.max(0, Math.min(event.clientY - box.top, box.height))
    // 连续跟随实际坐标，避免“最近边”变化时光源突然跳到另一条边。
    targetX = x / box.width
    targetY = 1 - y / box.height
    targetEnergy = 1
    requestDraw()
  }

  function flash(event: PointerEvent): void {
    if (!event.isPrimary || event.button !== 0) return
    followPointer(event)
    rippleX = targetX
    rippleY = targetY
    rippleStart = performance.now()
  }

  function dim(): void {
    targetEnergy = 0.18
    requestDraw()
  }

  const unavailable = () => emit('unavailable')
  const stopTone = watch(() => props.lightSurface, requestDraw)
  // 任务状态改变只扫过一次边缘，不新建画布，也不持续运行动画。
  const stopSignal = watch(() => props.signal, signal => {
    if (!signal || signal === 'idle') return
    rippleX = .5; rippleY = 1; rippleStart = performance.now()
    requestDraw()
  })
  const resize = new ResizeObserver(requestDraw)
  resize.observe(element)
  surface.addEventListener('pointermove', followPointer, { passive: true })
  surface.addEventListener('pointerdown', flash, { passive: true })
  surface.addEventListener('pointerleave', dim)
  element.addEventListener('webglcontextlost', unavailable)
  requestDraw()

  dispose = () => {
    cancelAnimationFrame(frame)
    resize.disconnect()
    stopTone()
    stopSignal()
    surface.removeEventListener('pointermove', followPointer)
    surface.removeEventListener('pointerdown', flash)
    surface.removeEventListener('pointerleave', dim)
    element.removeEventListener('webglcontextlost', unavailable)
    gl.useProgram(null)
    gl.deleteBuffer(triangle)
    gl.deleteProgram(program)
  }
})

onBeforeUnmount(() => dispose?.())
</script>

<template>
  <canvas ref="canvas" class="glass-light" aria-hidden="true" />
</template>

<style scoped>
.glass-light {
  position: absolute;
  z-index: 2;
  inset: 0;
  display: block;
  width: 100%;
  height: 100%;
  pointer-events: none;
}
@media (prefers-reduced-transparency: reduce), (prefers-contrast: more) {
  .glass-light { display: none; }
}
</style>
