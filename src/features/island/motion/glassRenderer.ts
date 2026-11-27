export interface GlassFrame {
  width: number
  height: number
  corner: number
  lightX: number
  lightY: number
  energy: number
  tone: number
  rippleX: number
  rippleY: number
  rippleAge: number
  full: boolean
  tintBase?: [number, number, number]
  tintRim?: [number, number, number]
}

const vertex = `
attribute vec2 position;
void main() { gl_Position = vec4(position, 0., 1.); }
`

const fragment = `
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
uniform vec2 size, bufferSize, light, origin;
uniform vec3 tintBase, tintRim;
uniform float corner, energy, tone, ripple, full;
float bell(float distance, float width) {
  float x = distance / width;
  return exp(-x*x);
}
void main() {
  vec2 uv = gl_FragCoord.xy / bufferSize;
  vec2 point = uv * size;
  vec2 box = abs(point - .5*size) - .5*size + corner + .15;
  float boxSdf = length(max(box, 0.)) + min(max(box.x, box.y), 0.) - corner;
  float depth = -boxSdf;
  // 严格贴合外框边缘线（<= 2.2px），只点亮轮廓边缘本身，不向面板内部产生任何内光晕。
  float band = 2.2;
  if (depth < -.5 || depth > band) { gl_FragColor = vec4(0.); return; }
  float coverage = smoothstep(-.45, .15, depth) * (1. - smoothstep(1.15, band, depth));
  float reach = clamp(min(size.x, size.y) * .48, 56., 160.);
  float proximity = bell(length(point - light * size), reach) * energy;
  float wake = ripple < 1. ? bell(length(point - origin * size) - ripple * 260., 12.) * (1. - ripple) * smoothstep(0., .06, ripple) : 0.;
  float focus = min(1., proximity + wake * .65);
  float rim = bell(depth - .65, .52);
  float alpha = coverage * min(.92, rim * (.26 + .68 * focus));
  vec3 reflection = mix(tintBase, tintRim, rim * .85);
  gl_FragColor = vec4(reflection, alpha);
}`

function shader(gl: WebGLRenderingContext, type: number, source: string): WebGLShader | null {
  const handle = gl.createShader(type)
  if (!handle) return null
  gl.shaderSource(handle, source)
  gl.compileShader(handle)
  if (gl.getShaderParameter(handle, gl.COMPILE_STATUS)) return handle
  gl.deleteShader(handle)
  return null
}

export function createGlassPass(gl: WebGLRenderingContext) {
  const vs = shader(gl, gl.VERTEX_SHADER, vertex)
  const fs = shader(gl, gl.FRAGMENT_SHADER, fragment)
  const program = vs && fs ? gl.createProgram() : null
  if (program && vs && fs) {
    gl.attachShader(program, vs)
    gl.attachShader(program, fs)
    gl.linkProgram(program)
    gl.detachShader(program, vs)
    gl.detachShader(program, fs)
  }
  gl.deleteShader(vs)
  gl.deleteShader(fs)
  const triangle = program && gl.getProgramParameter(program, gl.LINK_STATUS) ? gl.createBuffer() : null
  if (!program || !triangle) {
    gl.deleteProgram(program)
    return null
  }
  const uniform = (name: string) => gl.getUniformLocation(program, name)
  const location = {
    size: uniform('size'), bufferSize: uniform('bufferSize'), corner: uniform('corner'),
    light: uniform('light'), energy: uniform('energy'), tone: uniform('tone'), origin: uniform('origin'),
    ripple: uniform('ripple'), full: uniform('full'),
    tintBase: uniform('tintBase'), tintRim: uniform('tintRim')
  }
  gl.useProgram(program)
  gl.bindBuffer(gl.ARRAY_BUFFER, triangle)
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW)
  const position = gl.getAttribLocation(program, 'position')
  gl.enableVertexAttribArray(position)
  gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0)
  return {
    draw(frame: GlassFrame): void {
      gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight)
      gl.uniform2f(location.size, frame.width, frame.height)
      gl.uniform2f(location.bufferSize, gl.drawingBufferWidth, gl.drawingBufferHeight)
      gl.uniform1f(location.corner, Math.min(frame.corner, frame.width/2, frame.height/2))
      gl.uniform2f(location.light, frame.lightX, frame.lightY)
      gl.uniform1f(location.energy, frame.energy)
      gl.uniform1f(location.tone, frame.tone)
      gl.uniform2f(location.origin, frame.rippleX, frame.rippleY)
      gl.uniform1f(location.ripple, frame.rippleAge)
      gl.uniform1f(location.full, Number(frame.full))
      const defaultBase: [number, number, number] = frame.tone > .5 ? [.48, .74, 1.] : [.44, .76, 1.]
      const defaultRim: [number, number, number] = frame.tone > .5 ? [.98, .99, 1.] : [.94, .98, 1.]
      const base = frame.tintBase ?? defaultBase
      const rim = frame.tintRim ?? defaultRim
      gl.uniform3f(location.tintBase, base[0], base[1], base[2])
      gl.uniform3f(location.tintRim, rim[0], rim[1], rim[2])
      gl.drawArrays(gl.TRIANGLES, 0, 3)
    },
    dispose(): void {
      gl.useProgram(null)
      gl.deleteBuffer(triangle)
      gl.deleteProgram(program)
    }
  }
}
