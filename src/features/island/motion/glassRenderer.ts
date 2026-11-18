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
  openingAge: number
  direction: number
  full: boolean
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
uniform float corner, energy, tone, opening, ripple, direction, full;
float bell(float distance, float width) {
  float x = distance / width;
  return exp(-x*x);
}
void main() {
  vec2 uv = gl_FragCoord.xy / bufferSize;
  vec2 point = uv * size;
  vec2 box = abs(point - .5*size) - .5*size + corner + .6;
  float sdf = length(max(box, 0.)) + min(max(box.x, box.y), 0.) - corner;
  float depth = -sdf;
  float band = mix(5.5, 11., smoothstep(42., 125., size.y));
  if (depth < -1. || depth > band) { gl_FragColor = vec4(0.); return; }
  float coverage = smoothstep(-.75,.35,depth) * (1.-smoothstep(.65*band,band,depth));
  float reach = clamp(min(size.x,size.y)*.42,48.,142.);
  float proximity = bell(length(point-light*size),reach) * energy;
  float progress = clamp(opening,0.,1.);
  float crest = 1.-(1.-progress)*(1.-progress);
  float angle = atan(uv.x-.5,uv.y-.5);
  float pathAngle = direction > 0. ? abs(angle) : 3.14159-abs(angle);
  float front = bell(pathAngle-crest*3.14159,.62) * (1.-smoothstep(.60,1.,progress));
  float wake = ripple < 1. ? bell(length(point-origin*size)-ripple*250.,12.)*(1.-ripple)*smoothstep(0.,.06,ripple) : 0.;
  float focus = min(1.,proximity+front*.85+wake*.65);
  float rim = bell(depth-.95,.72);
  float ribbon = 0.;
  if (full>.5) {
    float curvature = sin(angle*2.-progress*3.8+light.x*1.7);
    float ridge = 1.6 + (1.+curvature)*(.65+front*1.1) + focus*.55;
    ribbon = bell(depth-ridge,.78+front*.48) * (.3+front*.5);
  }
  float halo = bell(depth,band*.46);
  float alpha = coverage * min(.86,(rim*.62+ribbon+halo*.26)*focus+rim*.08);
  vec3 reflection = mix(mix(vec3(.48,.73,.67),vec3(.84,.92,.84),rim),
                        mix(vec3(.24,.41,.38),vec3(.50,.62,.53),rim),tone);
  gl_FragColor = vec4(reflection,alpha);
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
    opening: uniform('opening'), ripple: uniform('ripple'), direction: uniform('direction'), full: uniform('full')
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
      gl.uniform1f(location.opening, frame.openingAge)
      gl.uniform1f(location.ripple, frame.rippleAge)
      gl.uniform1f(location.direction, frame.direction)
      gl.uniform1f(location.full, Number(frame.full))
      gl.drawArrays(gl.TRIANGLES, 0, 3)
    },
    dispose(): void {
      gl.useProgram(null)
      gl.deleteBuffer(triangle)
      gl.deleteProgram(program)
    }
  }
}
