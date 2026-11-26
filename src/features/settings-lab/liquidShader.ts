import type { ThemeMode } from '@/lib/themes'
import type { MotionMode } from '@/lib/motions'
import type { SceneMode } from './reactive3d'

/** 驱动 3D 液态玻璃 GLSL 着色器的实时设置状态。 */
export interface LiquidSceneState {
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
  clipLimit: number
  translateFlipped: boolean
  audioMuted: boolean
  islandExpanded: boolean
}

type RGB = [number, number, number]

interface LiquidMaterial {
  bgA: RGB
  bgB: RGB
  core: RGB
  rim: RGB
  accent: RGB
  dispersion: number
  isLight: boolean
  inkStyle: number // 0 = 液态玻璃/水银，1 = 宣纸水墨晕染感
}

const MATERIALS: Record<ThemeMode, LiquidMaterial> = {
  system: {
    bgA: [0.06, 0.08, 0.13],
    bgB: [0.12, 0.16, 0.24],
    core: [0.22, 0.34, 0.52],
    rim: [0.68, 0.88, 1.0],
    accent: [0.45, 0.72, 1.0],
    dispersion: 0.78,
    isLight: false,
    inkStyle: 0
  },
  light: {
    bgA: [0.95, 0.96, 0.98],
    bgB: [0.85, 0.89, 0.96],
    core: [0.76, 0.86, 0.98],
    rim: [1.0, 1.0, 1.0],
    accent: [0.32, 0.56, 0.98],
    dispersion: 0.85,
    isLight: true,
    inkStyle: 0
  },
  dark: {
    bgA: [0.04, 0.05, 0.07],
    bgB: [0.10, 0.12, 0.17],
    core: [0.18, 0.21, 0.28],
    rim: [0.86, 0.91, 1.0],
    accent: [0.52, 0.68, 0.95],
    dispersion: 0.65,
    isLight: false,
    inkStyle: 0
  },
  liquid: {
    bgA: [0.03, 0.05, 0.11],
    bgB: [0.07, 0.11, 0.22],
    core: [0.16, 0.36, 0.62],
    rim: [0.88, 0.96, 1.0],
    accent: [0.76, 0.62, 1.0],
    dispersion: 1.35,
    isLight: false,
    inkStyle: 0
  },
  midnight: {
    bgA: [0.02, 0.05, 0.11],
    bgB: [0.05, 0.13, 0.25],
    core: [0.10, 0.32, 0.62],
    rim: [0.56, 0.86, 1.0],
    accent: [0.22, 0.92, 0.98],
    dispersion: 0.95,
    isLight: false,
    inkStyle: 0
  },
  mecha: {
    bgA: [0.07, 0.05, 0.04],
    bgB: [0.17, 0.11, 0.06],
    core: [0.58, 0.30, 0.07],
    rim: [1.0, 0.78, 0.36],
    accent: [1.0, 0.45, 0.18],
    dispersion: 0.72,
    isLight: false,
    inkStyle: 0
  },
  scroll: {
    bgA: [0.96, 0.93, 0.88],
    bgB: [0.89, 0.83, 0.74],
    core: [0.72, 0.23, 0.18],
    rim: [0.98, 0.92, 0.82],
    accent: [0.22, 0.20, 0.19],
    dispersion: 0.32,
    isLight: true,
    inkStyle: 1
  },
  pixel: {
    bgA: [0.04, 0.08, 0.05],
    bgB: [0.08, 0.17, 0.10],
    core: [0.18, 0.46, 0.14],
    rim: [0.72, 1.0, 0.34],
    accent: [0.30, 0.92, 0.48],
    dispersion: 0.60,
    isLight: false,
    inkStyle: 0
  },
  blueprint: {
    bgA: [0.03, 0.12, 0.25],
    bgB: [0.07, 0.23, 0.45],
    core: [0.14, 0.42, 0.74],
    rim: [0.88, 0.96, 1.0],
    accent: [0.46, 0.84, 1.0],
    dispersion: 0.52,
    isLight: false,
    inkStyle: 0
  },
  neon: {
    bgA: [0.06, 0.02, 0.12],
    bgB: [0.15, 0.05, 0.26],
    core: [0.46, 0.12, 0.56],
    rim: [0.98, 0.46, 0.78],
    accent: [0.14, 0.92, 0.98],
    dispersion: 1.25,
    isLight: false,
    inkStyle: 0
  },
  clay: {
    bgA: [0.97, 0.92, 0.95],
    bgB: [0.88, 0.93, 0.99],
    core: [0.95, 0.54, 0.72],
    rim: [1.0, 0.98, 1.0],
    accent: [0.44, 0.76, 0.98],
    dispersion: 0.78,
    isLight: true,
    inkStyle: 0
  }
}

const VERT_SRC = `
attribute vec2 aPos;
varying vec2 vUv;
void main() {
  vUv = aPos * 0.5 + 0.5;
  gl_Position = vec4(aPos, 0.0, 1.0);
}
`

const FRAG_SRC = `
precision highp float;

varying vec2 vUv;

uniform vec2 uRes;
uniform float uTime;
uniform vec2 uPointer;
uniform float uAnchorX;
uniform float uScale;
uniform float uExpand;
uniform float uTuck;
uniform float uFrost;
uniform float uGlow;
uniform float uFrozen;
uniform float uMode;        // 0外观 1快捷键 2剪贴板 3应用 4翻译 5调音台 6通知
uniform float uMotionStyle; // 0柔和 1先宽后高 2轻弹 3逐级 4快速 5垂直
uniform float uFlipAngle;
uniform float uClipDensity;
uniform float uAudioEnergy;
uniform vec3 uSatellites;   // x=net y=cpu z=mem (0 表示关闭，>0 表示强度)
uniform float uImpact;
uniform float uBeam;

uniform vec3 uBgA;
uniform vec3 uBgB;
uniform vec3 uCore;
uniform vec3 uRim;
uniform vec3 uAccent;
uniform float uDispersion;
uniform float uIsLight;
uniform float uInkStyle;

// 最多同时追踪 4 处按钮点击发射的光学折射涟漪 (x, y, age, active)
uniform vec4 uRipples[4];

mat2 rot2(float a) {
  float c = cos(a), s = sin(a);
  return mat2(c, -s, s, c);
}

// 多项式平滑并集：产生真实液态表面张力融合与拉丝感
float smin(float a, float b, float k) {
  float h = clamp(0.5 + 0.5 * (b - a) / k, 0.0, 1.0);
  return mix(b, a, h) - k * h * (1.0 - h);
}

float sdSphere(vec3 p, float r) {
  return length(p) - r;
}

float sdCapsule(vec3 p, vec3 a, vec3 b, float r) {
  vec3 pa = p - a, ba = b - a;
  float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
  return length(pa - ba * h) - r;
}

float sdTorus(vec3 p, vec2 t) {
  vec2 q = vec2(length(p.xz) - t.x, p.y);
  return length(q) - t.y;
}

float sdRoundBox(vec3 p, vec3 b, float r) {
  vec3 q = abs(p) - b;
  return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0) - r;
}

// 3D 液态雕塑有符号距离场：每一个设置项直接参与几何形态计算
float mapScene(vec3 p) {
  vec3 q = p;
  q.x -= uAnchorX;
  q.y -= uTuck * 0.92;

  // 鼠标轻微牵引与点击受击弹性晃动
  q.xz *= rot2(uPointer.x * 0.26 + sin(uTime * 0.35) * 0.14);
  q.yz *= rot2(uPointer.y * 0.16);

  float pulse = 1.0 + sin(uTime * 14.0) * uImpact * 0.065;
  float s = max(0.55, uScale * pulse);
  q /= s;

  float t = uTime;
  // 细微有机流体呼吸起伏
  float organic = (sin(q.x * 3.1 + t * 1.4) * cos(q.y * 2.9 - t * 1.1) * sin(q.z * 3.3 + t * 0.9)) * (0.045 + uImpact * 0.05) * (1.0 - uFrozen * 0.85);

  float d = 10.0;

  // 0: 外观与灵动岛 —— 多态液态琉璃核心 + 动效变形 + 卫星微读数液滴
  if (uMode < 0.5) {
    float e = uExpand;
    float stretchX = 0.26 + e * 0.72;
    float stretchY = 0.10 + e * 0.24;
    if (uMotionStyle > 0.5 && uMotionStyle < 1.5) {
      stretchX = 0.24 + min(1.0, e * 1.6) * 0.88;
      stretchY = 0.08 + max(0.0, e - 0.35) * 0.42;
    } else if (uMotionStyle > 4.5) {
      stretchX = 0.18 + max(0.0, e - 0.25) * 0.55;
      stretchY = 0.12 + min(1.0, e * 1.4) * 0.56;
    }

    // 全屏避让收为细线：将立体液球拉成顶部悬浮细光丝
    float radius = mix(0.68, 0.075, uTuck);
    float halfW = mix(stretchX, 1.32, uTuck);
    float halfH = mix(stretchY, 0.02, uTuck);

    float core = sdRoundBox(q, vec3(halfW, halfH, mix(0.28, 0.05, uTuck)), radius);
    d = core + organic;

    // 展开时分裂并保持表面张力拉丝的两颗翼滴
    if (uTuck < 0.75) {
      float wingDist = 0.78 + e * 0.62;
      vec3 w1 = q - vec3(-wingDist, sin(t * 1.5) * 0.14, cos(t * 1.1) * 0.16);
      vec3 w2 = q - vec3(wingDist, -sin(t * 1.3) * 0.14, -cos(t * 1.2) * 0.16);
      float dropR = (0.28 + e * 0.14) * (1.0 - uTuck);
      d = smin(d, sdSphere(w1, dropR), 0.36);
      d = smin(d, sdSphere(w2, dropR), 0.36);
    }

    // 光轨舱门开启时增加环绕液态玻璃光环
    if (uGlow > 0.05 && uTuck < 0.7) {
      vec3 rp = q;
      rp.xy *= rot2(0.32 + sin(t * 0.7) * 0.12);
      rp.xz *= rot2(t * 0.55);
      float ring = sdTorus(rp, vec2(1.18 + e * 0.22, 0.055 * uGlow));
      d = smin(d, ring, 0.14);
    }

    // 微读数卫星液滴（NET / CPU / MEM 实时呼吸）
    if (uSatellites.x > 0.01) {
      vec3 sp = q - vec3(-1.05, -0.78 + sin(t * 2.4) * 0.08, 0.35);
      d = smin(d, sdSphere(sp, 0.15 + uSatellites.x * 0.14), 0.22);
    }
    if (uSatellites.y > 0.01) {
      vec3 sp = q - vec3(0.0, -0.88 + cos(t * 3.1) * 0.09, 0.45);
      d = smin(d, sdSphere(sp, 0.15 + uSatellites.y * 0.14), 0.22);
    }
    if (uSatellites.z > 0.01) {
      vec3 sp = q - vec3(1.05, -0.78 + sin(t * 1.9 + 1.2) * 0.08, 0.35);
      d = smin(d, sdSphere(sp, 0.15 + uSatellites.z * 0.14), 0.22);
    }
  }
  // 1: 快捷键 —— 3 颗液态水晶按键浮岛（按键盘时受压涟漪）
  else if (uMode < 1.5) {
    float sink = uImpact * 0.22;
    float k1 = sdRoundBox(q - vec3(-0.92, -sink * 0.7, 0.1), vec3(0.32, 0.18, 0.32), 0.12);
    float k2 = sdRoundBox(q - vec3(0.0, -sink, -0.05), vec3(0.42, 0.22, 0.34), 0.12);
    float k3 = sdRoundBox(q - vec3(0.95, -sink * 0.5, 0.1), vec3(0.32, 0.18, 0.32), 0.12);
    float base = sdRoundBox(q - vec3(0.0, -0.52, 0.0), vec3(1.45, 0.06, 0.65), 0.14);
    d = smin(min(k1, min(k2, k3)), base, 0.18) + organic * 0.5;
  }
  // 2: 剪贴板 —— 纵向液态叠层水镜（暂停即冻结，上限控层数与厚度）
  else if (uMode < 2.5) {
    float spread = 0.26 + uClipDensity * 0.22;
    float wave = (1.0 - uFrozen) * sin(t * 1.8) * 0.08;
    float l1 = sdRoundBox(q - vec3(0.0, -spread + wave, 0.0), vec3(0.95, 0.06, 0.56), 0.14);
    float l2 = sdRoundBox(q - vec3(0.0, 0.0, 0.0), vec3(1.05, 0.07, 0.62), 0.14);
    float l3 = sdRoundBox(q - vec3(0.0, spread - wave, 0.0), vec3(0.88, 0.06, 0.52), 0.14);
    d = smin(smin(l1, l2, 0.16), l3, 0.16) + organic * (1.0 - uFrozen * 0.9);
  }
  // 3: 应用管理 —— 环绕悬浮液滴星阵
  else if (uMode < 3.5) {
    d = sdSphere(q, 0.44);
    for (int i = 0; i < 6; i++) {
      float fi = float(i);
      float a = fi * 1.0472 + t * 0.55;
      vec3 op = vec3(cos(a) * 1.05, sin(a * 2.0 + t) * 0.24, sin(a) * 0.72);
      d = smin(d, sdSphere(q - op, 0.24), 0.28);
    }
    d += organic * 0.6;
  }
  // 4: 翻译 —— 双生液滴太极互换 + 试译高能液桥
  else if (uMode < 4.5) {
    vec3 tp = q;
    tp.xz *= rot2(uFlipAngle);
    float sep = 0.95 - uBeam * 0.18;
    float leftOrb = sdSphere(tp - vec3(-sep, sin(t * 1.5) * 0.12, 0.0), 0.54);
    float rightOrb = sdSphere(tp - vec3(sep, -sin(t * 1.5) * 0.12, 0.0), 0.54);
    float bridge = sdCapsule(tp, vec3(-sep, 0.0, 0.0), vec3(sep, 0.0, 0.0), 0.09 + uBeam * 0.14);
    d = smin(smin(leftOrb, rightOrb, 0.35), bridge, 0.25) + organic;
  }
  // 5: 调音台 —— 声波驻波共振液态频谱体
  else if (uMode < 5.5) {
    float amp = uAudioEnergy;
    float h0 = 0.22 + amp * abs(sin(t * 3.8)) * 0.55;
    float h1 = 0.26 + amp * abs(sin(t * 4.4 + 1.0)) * 0.68;
    float h2 = 0.30 + amp * abs(sin(t * 3.2 + 2.1)) * 0.78;
    float h3 = 0.26 + amp * abs(sin(t * 4.8 + 3.2)) * 0.65;
    float h4 = 0.22 + amp * abs(sin(t * 3.5 + 4.3)) * 0.52;
    float b0 = sdCapsule(q, vec3(-1.12, -h0, 0.0), vec3(-1.12, h0, 0.0), 0.18);
    float b1 = sdCapsule(q, vec3(-0.56, -h1, 0.0), vec3(-0.56, h1, 0.0), 0.20);
    float b2 = sdCapsule(q, vec3(0.0, -h2, 0.0), vec3(0.0, h2, 0.0), 0.22);
    float b3 = sdCapsule(q, vec3(0.56, -h3, 0.0), vec3(0.56, h3, 0.0), 0.20);
    float b4 = sdCapsule(q, vec3(1.12, -h4, 0.0), vec3(1.12, h4, 0.0), 0.18);
    d = smin(smin(smin(b0, b1, 0.24), smin(b2, b3, 0.24), 0.24), b4, 0.24);
  }
  // 6: 通知与信标 —— 悬浮液态棱球 + 同心光学脉冲环
  else {
    float center = sdSphere(q, 0.56 + uBeam * 0.12);
    vec3 r1 = q; r1.xy *= rot2(0.45);
    vec3 r2 = q; r2.yz *= rot2(-0.55);
    float ringA = sdTorus(r1, vec2(1.05 + uBeam * 0.45, 0.055));
    float ringB = sdTorus(r2, vec2(1.38 + uBeam * 0.65, 0.04));
    d = smin(center, min(ringA, ringB), 0.18) + organic;
  }

  return d * s;
}

vec3 calcNormal(vec3 p) {
  vec2 e = vec2(0.0025, 0.0);
  return normalize(vec3(
    mapScene(p + e.xyy) - mapScene(p - e.xyy),
    mapScene(p + e.yxy) - mapScene(p - e.yxy),
    mapScene(p + e.yyx) - mapScene(p - e.yyx)
  ));
}

// 计算环境光学焦散背景（无杂乱模型，纯粹高级光场）
vec3 sampleBackdrop(vec2 uv, vec3 rd) {
  float grad = clamp(uv.y * 0.85 + uv.x * 0.25, 0.0, 1.0);
  vec3 col = mix(uBgB, uBgA, grad);

  // 柔和流动极光丝带
  float wave1 = sin(rd.x * 3.8 + uTime * 0.45 + rd.y * 2.2) * 0.5 + 0.5;
  float wave2 = cos(rd.y * 4.2 - uTime * 0.35 - rd.x * 2.6) * 0.5 + 0.5;
  float aurora = pow(wave1 * wave2, 2.4);
  col += mix(uCore, uAccent, wave1) * aurora * (uIsLight > 0.5 ? 0.18 : 0.34);

  // 聚焦环境柔光
  vec2 focus = vec2(uRes.x >= 960.0 ? 0.67 : 0.50, 0.50);
  float dist = length((uv - focus) * vec2(uRes.x / max(1.0, uRes.y), 1.0));
  col += uAccent * exp(-dist * 2.1) * (uIsLight > 0.5 ? 0.14 : 0.22);

  return col;
}

void main() {
  vec2 uv = vUv;
  float aspect = uRes.x / max(1.0, uRes.y);

  // 叠加来自前景按钮点击的光学折射波纹
  vec2 rippleOffset = vec2(0.0);
  float rippleFlash = 0.0;
  for (int i = 0; i < 4; i++) {
    vec4 rp = uRipples[i];
    if (rp.w > 0.5) {
      vec2 delta = (uv - rp.xy) * vec2(aspect, 1.0);
      float dist = length(delta);
      float waveRadius = rp.z * 1.15;
      float ring = exp(-pow((dist - waveRadius) * 14.0, 2.0)) * max(0.0, 1.0 - rp.z * 0.9);
      rippleOffset += normalize(delta + 0.0001) * ring * 0.018;
      rippleFlash += ring;
    }
  }

  vec2 warpedUv = uv + rippleOffset;
  // 宽屏把 3D 液态雕塑焦点放在右侧 67% 处，与左侧液态玻璃设置面板形成黄金构图
  vec2 center = vec2(uRes.x >= 960.0 ? 0.67 : 0.50, 0.50);
  vec2 p = (warpedUv - center) * vec2(aspect, 1.0) * 2.0;

  vec3 ro = vec3(0.0, 0.0, 3.65);
  vec3 rd = normalize(vec3(p, -1.95));

  vec3 col = sampleBackdrop(warpedUv, rd);

  // Raymarching 步进求交
  float t = 0.0;
  float hit = 0.0;
  for (int i = 0; i < 52; i++) {
    vec3 pos = ro + rd * t;
    float d = mapScene(pos);
    if (d < 0.0022) {
      hit = 1.0;
      break;
    }
    t += d * 0.85;
    if (t > 8.5) break;
  }

  if (hit > 0.5) {
    vec3 pos = ro + rd * t;
    vec3 n = calcNormal(pos);
    vec3 v = -rd;

    // 菲涅尔反射项（核心液态玻璃通透质感来源）
    float ndv = clamp(dot(n, v), 0.0, 1.0);
    float fresnel = pow(1.0 - ndv, 2.6);

    // 三向光学环境反射与 RGB 色散折射
    vec3 rDir = reflect(rd, n);
    float disp = uDispersion * (0.55 + uGlow * 0.65);
    vec3 refrR = sampleBackdrop(warpedUv + n.xy * (0.14 + disp * 0.05), refract(rd, n, 0.74));
    vec3 refrG = sampleBackdrop(warpedUv + n.xy * 0.14, refract(rd, n, 0.77));
    vec3 refrB = sampleBackdrop(warpedUv + n.xy * (0.14 - disp * 0.05), refract(rd, n, 0.80));
    vec3 refrCol = vec3(refrR.r, refrG.g, refrB.b);

    // 双高光灯（顶冷主光 + 侧暖补光）
    vec3 light1 = normalize(vec3(0.65, 0.85, 0.72));
    vec3 light2 = normalize(vec3(-0.75, -0.35, 0.55));
    float spec1 = pow(max(dot(reflect(-light1, n), v), 0.0), mix(46.0, 12.0, uFrost));
    float spec2 = pow(max(dot(reflect(-light2, n), v), 0.0), mix(24.0, 8.0, uFrost));
    float diff = max(dot(n, light1), 0.0) * 0.65 + 0.35;

    // 液态玻璃 / 水银本体着色
    vec3 bodyCol = mix(refrCol, uCore * diff, mix(0.42, 0.76, uInkStyle));
    // 虹彩边缘色散（光轨开启时更晶莹）
    vec3 irid = 0.5 + 0.5 * cos(6.2831 * (fresnel * 0.85 + vec3(0.0, 0.25, 0.55) + uTime * 0.08));
    vec3 rimGlow = mix(uRim, mix(uAccent, irid, 0.45 * uGlow), fresnel);

    vec3 glassCol = bodyCol + rimGlow * fresnel * (uIsLight > 0.5 ? 0.85 : 1.15);
    glassCol += uRim * spec1 * (uIsLight > 0.5 ? 0.95 : 1.1) + uAccent * spec2 * 0.55;

    // 宣纸水墨主题：柔和墨韵过渡
    if (uInkStyle > 0.5) {
      float inkEdge = smoothstep(0.15, 0.85, fresnel);
      glassCol = mix(uCore * (0.7 + 0.3 * diff), uAccent, inkEdge * 0.55) + uRim * spec1 * 0.35;
    }

    // 隐私隐藏（截图隐藏）开启时：模拟真实光学磨砂雾化与半透明隐身
    if (uFrost > 0.01) {
      vec3 frosted = mix(col, mix(uBgA, uRim, 0.35), 0.42);
      glassCol = mix(glassCol, frosted, uFrost * 0.76);
    }

    // 抗锯齿柔边融合
    col = mix(col, glassCol, 0.97);
  }

  // 叠加按钮点击激起的折射光环高光
  col += mix(uRim, uAccent, 0.55) * rippleFlash * (uIsLight > 0.5 ? 0.25 : 0.42);

  // 轻微胶片暗角收拢视觉中心
  float vig = 1.0 - dot((uv - 0.5) * 1.15, (uv - 0.5) * 1.15) * (uIsLight > 0.5 ? 0.14 : 0.32);
  col *= vig;

  gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
`

interface RippleSlot {
  x: number
  y: number
  age: number
  active: number
}

const MODE_INDEX: Record<SceneMode, number> = {
  appearance: 0,
  shortcuts: 1,
  clipboard: 2,
  apps: 3,
  translation: 4,
  audio: 5,
  integrations: 6
}

const MOTION_INDEX: Record<MotionMode, number> = {
  glide: 0,
  sweep: 1,
  bounce: 2,
  step: 3,
  flicker: 4
}

/** 原生 WebGL 3D 液态玻璃与光影流体引擎：零外部依赖，60fps 响应每一个设置项变化。 */
export class LiquidGlassEngine {
  private canvas: HTMLCanvasElement
  private gl: WebGLRenderingContext
  private program: WebGLProgram
  private state: LiquidSceneState
  private raf = 0
  private lastTs = 0
  private elapsed = 0
  private width = 1
  private height = 1
  private dpr = 1
  private disposed = false

  // 平滑插值状态（所有光学与几何量都做物理阻尼过渡，无生硬跳变）
  private pointerX = 0
  private pointerY = 0
  private smoothPointerX = 0
  private smoothPointerY = 0
  private anchorX = 0
  private coreScale = 1
  private expandVal = 0
  private tuckVal = 0
  private frostVal = 0
  private glowVal = 1
  private frozenVal = 0
  private flipAngle = 0
  private impactVal = 0
  private beamVal = 0
  private motionDemoTimer = 0

  private bgA: RGB
  private bgB: RGB
  private coreCol: RGB
  private rimCol: RGB
  private accentCol: RGB
  private dispersionVal: number
  private isLightVal: number
  private inkStyleVal: number

  private ripples: RippleSlot[] = [
    { x: 0.5, y: 0.5, age: 10, active: 0 },
    { x: 0.5, y: 0.5, age: 10, active: 0 },
    { x: 0.5, y: 0.5, age: 10, active: 0 },
    { x: 0.5, y: 0.5, age: 10, active: 0 }
  ]
  private nextRipple = 0

  private uniforms: Record<string, WebGLUniformLocation | null> = {}

  constructor(canvas: HTMLCanvasElement, initial: LiquidSceneState) {
    this.canvas = canvas
    const gl = canvas.getContext('webgl', { antialias: true, alpha: false, powerPreference: 'high-performance' })
    if (!gl) throw new Error('浏览器未开启 WebGL 上下文')
    this.gl = gl
    this.state = { ...initial }

    const mat = MATERIALS[initial.theme]
    this.bgA = [...mat.bgA]
    this.bgB = [...mat.bgB]
    this.coreCol = [...mat.core]
    this.rimCol = [...mat.rim]
    this.accentCol = [...mat.accent]
    this.dispersionVal = mat.dispersion
    this.isLightVal = mat.isLight ? 1 : 0
    this.inkStyleVal = mat.inkStyle

    this.program = this.buildProgram(gl, VERT_SRC, FRAG_SRC)
    gl.useProgram(this.program)

    // 全屏四边形顶点缓冲
    const buf = gl.createBuffer()
    gl.bindBuffer(gl.ARRAY_BUFFER, buf)
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]), gl.STATIC_DRAW)
    const loc = gl.getAttribLocation(this.program, 'aPos')
    gl.enableVertexAttribArray(loc)
    gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0)

    const names = [
      'uRes', 'uTime', 'uPointer', 'uAnchorX', 'uScale', 'uExpand', 'uTuck', 'uFrost', 'uGlow',
      'uFrozen', 'uMode', 'uMotionStyle', 'uFlipAngle', 'uClipDensity', 'uAudioEnergy',
      'uSatellites', 'uImpact', 'uBeam', 'uBgA', 'uBgB', 'uCore', 'uRim', 'uAccent',
      'uDispersion', 'uIsLight', 'uInkStyle',
      'uRipples[0]', 'uRipples[1]', 'uRipples[2]', 'uRipples[3]'
    ]
    for (const name of names) {
      this.uniforms[name] = gl.getUniformLocation(this.program, name)
    }

    this.resize()
    this.start()
  }

  private buildProgram(gl: WebGLRenderingContext, vsSrc: string, fsSrc: string): WebGLProgram {
    const compile = (type: number, src: string) => {
      const shader = gl.createShader(type)
      if (!shader) throw new Error('无法创建着色器')
      gl.shaderSource(shader, src)
      gl.compileShader(shader)
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
        const log = gl.getShaderInfoLog(shader) || '着色器编译失败'
        gl.deleteShader(shader)
        throw new Error(log)
      }
      return shader
    }
    const vs = compile(gl.VERTEX_SHADER, vsSrc)
    const fs = compile(gl.FRAGMENT_SHADER, fsSrc)
    const prog = gl.createProgram()
    if (!prog) throw new Error('无法创建着色器程序')
    gl.attachShader(prog, vs)
    gl.attachShader(prog, fs)
    gl.linkProgram(prog)
    gl.deleteShader(vs)
    gl.deleteShader(fs)
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      const log = gl.getProgramInfoLog(prog) || '着色器链接失败'
      gl.deleteProgram(prog)
      throw new Error(log)
    }
    return prog
  }

  public updateState(next: LiquidSceneState): void {
    const prev = this.state
    this.state = { ...next }
    if (prev.motion !== next.motion) this.playMotionDemo()
    if (prev.theme !== next.theme) this.impactVal = Math.min(1, this.impactVal + 0.85)
    if (prev.translateFlipped !== next.translateFlipped) this.impactVal = Math.min(1, this.impactVal + 0.65)
  }

  public setPointer(clientX: number, clientY: number): void {
    const rect = this.canvas.getBoundingClientRect()
    if (!rect.width || !rect.height) return
    this.pointerX = ((clientX - rect.left) / rect.width) * 2 - 1
    this.pointerY = ((clientY - rect.top) / rect.height) * 2 - 1
  }

  /** 从按钮屏幕坐标在液态光学场中激起真实折射涟漪。 */
  public emitRipple(clientX: number, clientY: number, strength = 0.72): void {
    const rect = this.canvas.getBoundingClientRect()
    const nx = rect.width ? (clientX - rect.left) / rect.width : 0.35
    const ny = rect.height ? 1.0 - (clientY - rect.top) / rect.height : 0.5
    this.ripples[this.nextRipple] = { x: nx, y: ny, age: 0, active: 1 }
    this.nextRipple = (this.nextRipple + 1) % this.ripples.length
    this.impactVal = Math.min(1.2, this.impactVal + strength)
  }

  public playMotionDemo(): void {
    this.motionDemoTimer = 1.6
    this.impactVal = Math.min(1.1, this.impactVal + 0.65)
  }

  public triggerBeam(): void {
    this.beamVal = 1.0
    this.impactVal = Math.min(1.1, this.impactVal + 0.85)
  }

  public resize(): void {
    const rect = this.canvas.getBoundingClientRect()
    // 控制最高 DPR 为 1.5，兼顾极高锐度与丝滑 60fps
    this.dpr = Math.min(window.devicePixelRatio || 1, 1.5)
    this.width = Math.max(1, rect.width)
    this.height = Math.max(1, rect.height)
    this.canvas.width = Math.round(this.width * this.dpr)
    this.canvas.height = Math.round(this.height * this.dpr)
    this.gl.viewport(0, 0, this.canvas.width, this.canvas.height)
  }

  public dispose(): void {
    if (this.disposed) return
    this.disposed = true
    cancelAnimationFrame(this.raf)
    this.gl.deleteProgram(this.program)
    this.gl.getExtension('WEBGL_lose_context')?.loseContext()
  }

  private start(): void {
    const loop = (ts: number) => {
      if (this.disposed) return
      const dt = this.lastTs ? Math.min(0.05, (ts - this.lastTs) / 1000) : 0.016
      this.lastTs = ts
      this.step(dt)
      this.render()
      this.raf = requestAnimationFrame(loop)
    }
    this.raf = requestAnimationFrame(loop)
  }

  private lerpVec3(cur: RGB, target: RGB, factor: number): void {
    cur[0] += (target[0] - cur[0]) * factor
    cur[1] += (target[1] - cur[1]) * factor
    cur[2] += (target[2] - cur[2]) * factor
  }

  private step(dt: number): void {
    const reduced = this.state.reducedMotion
    const mixRate = (speed: number) => (reduced ? 1 : Math.min(1, dt * speed))

    if (!reduced) {
      const flowSpeed = this.state.mode === 'clipboard' && this.state.clipboardPaused ? 0.08 : 1.0
      this.elapsed += dt * flowSpeed
    }

    this.smoothPointerX += (this.pointerX - this.smoothPointerX) * mixRate(4.5)
    this.smoothPointerY += (this.pointerY - this.smoothPointerY) * mixRate(4.5)

    const targetAnchor = this.state.anchor === 'left' ? -0.62 : this.state.anchor === 'right' ? 0.62 : 0
    this.anchorX += (targetAnchor - this.anchorX) * mixRate(5.5)

    const targetScale = 0.78 + ((this.state.scale - 75) / 50) * 0.44
    this.coreScale += (targetScale - this.coreScale) * mixRate(7)

    if (this.motionDemoTimer > 0) {
      this.motionDemoTimer = Math.max(0, this.motionDemoTimer - dt)
      const open = this.motionDemoTimer > 0.45 ? 1 : 0
      this.expandVal += (open - this.expandVal) * mixRate(this.state.motion === 'flicker' ? 16 : 6.5)
    } else {
      this.expandVal += ((this.state.islandExpanded ? 1 : 0) - this.expandVal) * mixRate(6)
    }

    const shouldTuck = this.state.avoidFullscreen && this.state.fullscreenSim ? 1 : 0
    this.tuckVal += (shouldTuck - this.tuckVal) * mixRate(6)
    this.frostVal += ((this.state.hideFromCapture ? 1 : 0) - this.frostVal) * mixRate(5.5)
    this.glowVal += ((this.state.webglGlow && !reduced ? 1 : 0) - this.glowVal) * mixRate(6)
    const frozenTarget = reduced || (this.state.mode === 'clipboard' && this.state.clipboardPaused) ? 1 : 0
    this.frozenVal += (frozenTarget - this.frozenVal) * mixRate(6)

    const targetFlip = this.state.translateFlipped ? Math.PI : 0
    this.flipAngle += (targetFlip - this.flipAngle) * mixRate(5.5)

    this.impactVal = Math.max(0, this.impactVal - dt * 2.2)
    this.beamVal = Math.max(0, this.beamVal - dt * 1.15)

    // 10 套光学材质之间丝滑插值过渡
    const mat = MATERIALS[this.state.theme]
    const mf = mixRate(4.8)
    this.lerpVec3(this.bgA, mat.bgA, mf)
    this.lerpVec3(this.bgB, mat.bgB, mf)
    this.lerpVec3(this.coreCol, mat.core, mf)
    this.lerpVec3(this.rimCol, mat.rim, mf)
    this.lerpVec3(this.accentCol, mat.accent, mf)
    this.dispersionVal += (mat.dispersion - this.dispersionVal) * mf
    this.isLightVal += ((mat.isLight ? 1 : 0) - this.isLightVal) * mf
    this.inkStyleVal += (mat.inkStyle - this.inkStyleVal) * mf

    for (const r of this.ripples) {
      if (r.active > 0.5) {
        r.age += dt
        if (r.age > 1.25) r.active = 0
      }
    }
  }

  private render(): void {
    const gl = this.gl
    const u = this.uniforms
    gl.uniform2f(u.uRes, this.canvas.width, this.canvas.height)
    gl.uniform1f(u.uTime, this.elapsed)
    gl.uniform2f(u.uPointer, this.smoothPointerX, this.smoothPointerY)
    gl.uniform1f(u.uAnchorX, this.anchorX)
    gl.uniform1f(u.uScale, this.coreScale)
    gl.uniform1f(u.uExpand, this.expandVal)
    gl.uniform1f(u.uTuck, this.tuckVal)
    gl.uniform1f(u.uFrost, this.frostVal)
    gl.uniform1f(u.uGlow, this.glowVal)
    gl.uniform1f(u.uFrozen, this.frozenVal)
    gl.uniform1f(u.uMode, MODE_INDEX[this.state.mode])
    gl.uniform1f(u.uMotionStyle, MOTION_INDEX[this.state.motion])
    gl.uniform1f(u.uFlipAngle, this.flipAngle)
    gl.uniform1f(u.uClipDensity, Math.min(1, this.state.clipLimit / 800))
    gl.uniform1f(u.uAudioEnergy, this.state.audioMuted || this.state.reducedMotion ? 0.04 : 1.0)

    const satOn = this.state.telemetryMode !== 'off'
    gl.uniform3f(
      u.uSatellites,
      satOn && this.state.telemetryNet ? Math.max(0.2, this.state.netLevel) : 0,
      satOn && this.state.telemetryCpu ? Math.max(0.2, this.state.cpuLevel) : 0,
      satOn && this.state.telemetryMemory ? Math.max(0.2, this.state.memLevel) : 0
    )

    gl.uniform1f(u.uImpact, this.impactVal)
    gl.uniform1f(u.uBeam, this.beamVal)
    gl.uniform3fv(u.uBgA, this.bgA)
    gl.uniform3fv(u.uBgB, this.bgB)
    gl.uniform3fv(u.uCore, this.coreCol)
    gl.uniform3fv(u.uRim, this.rimCol)
    gl.uniform3fv(u.uAccent, this.accentCol)
    gl.uniform1f(u.uDispersion, this.dispersionVal)
    gl.uniform1f(u.uIsLight, this.isLightVal)
    gl.uniform1f(u.uInkStyle, this.inkStyleVal)

    for (let i = 0; i < 4; i++) {
      const r = this.ripples[i]
      gl.uniform4f(u[`uRipples[${i}]`], r.x, r.y, r.age, r.active)
    }

    gl.drawArrays(gl.TRIANGLES, 0, 6)
  }
}
