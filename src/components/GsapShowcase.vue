<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import Icon from '@/components/Icon.vue'
import {
  Draggable,
  Flip,
  gsap,
  SplitText,
  vGsapPress
} from '@/lib/gsapMotion'
import { activeMotionPreset } from '@/lib/motionPresets'
import { state } from '@/store'

const rootEl = ref<HTMLElement | null>(null)
const reduced = computed(() => state.ui.islandReducedMotion)

// =========================================================================
// 01 · Timeline 多轨分镜编排 + 实时 Scrub 进度条
// =========================================================================
const tlStageEl = ref<HTMLElement | null>(null)
const tlProgress = ref(0)
const tlSpeed = ref<0.5 | 1 | 2>(1)
const tlPlaying = ref(false)
let demoTl: gsap.core.Timeline | null = null

function setupTimelineDemo(): void {
  const stage = tlStageEl.value
  if (!stage) return
  demoTl?.kill()
  const shell = stage.querySelector('.tl-shell')
  const ring = stage.querySelector('.tl-ring')
  const badge = stage.querySelector('.tl-badge')
  const cards = stage.querySelectorAll('.tl-mini-card')
  const caption = stage.querySelector('.tl-caption')

  gsap.set(shell, { width: 120, height: 32, borderRadius: 16 })
  gsap.set(ring, { opacity: 0, scaleX: 0.6 })
  gsap.set(badge, { opacity: 0, scale: 0.4 })
  gsap.set(cards, { opacity: 0, y: 14, scale: 0.85 })
  gsap.set(caption, { opacity: 0.55 })

  demoTl = gsap.timeline({
    paused: true,
    onUpdate: () => {
      if (demoTl) tlProgress.value = Math.round(demoTl.progress() * 100)
    },
    onComplete: () => {
      tlPlaying.value = false
    },
    onReverseComplete: () => {
      tlPlaying.value = false
    }
  })

  demoTl
    .to(ring, { opacity: 1, scaleX: 1, duration: 0.22, ease: 'power2.out' }, 0)
    .to(shell, { width: 290, duration: 0.34, ease: 'back.out(1.4)' }, 0.08)
    .to(shell, { height: 108, borderRadius: 18, duration: 0.42, ease: 'elastic.out(0.95, 0.65)' }, 0.2)
    .to(ring, { opacity: 0.2, duration: 0.3 }, 0.38)
    .to(badge, { opacity: 1, scale: 1, duration: 0.28, ease: 'back.out(2.2)' }, 0.32)
    .to(cards, { opacity: 1, y: 0, scale: 1, duration: 0.32, stagger: 0.07, ease: 'back.out(1.7)' }, 0.34)
    .to(caption, { opacity: 1, duration: 0.25 }, 0.45)

  demoTl.timeScale(tlSpeed.value)
}

function playTimeline(reverse = false): void {
  if (!demoTl) return
  demoTl.timeScale(tlSpeed.value)
  tlPlaying.value = true
  if (reverse) {
    if (demoTl.progress() === 0) demoTl.progress(1)
    demoTl.reverse()
  } else {
    if (demoTl.progress() === 1) demoTl.progress(0)
    demoTl.play()
  }
}

function togglePauseTimeline(): void {
  if (!demoTl) return
  if (demoTl.paused()) {
    tlPlaying.value = true
    demoTl.play()
  } else {
    tlPlaying.value = false
    demoTl.pause()
  }
}

function scrubTimeline(event: Event): void {
  if (!demoTl) return
  const val = Number((event.target as HTMLInputElement).value)
  tlProgress.value = val
  tlPlaying.value = false
  demoTl.pause()
  demoTl.progress(val / 100)
}

function setTimelineSpeed(speed: 0.5 | 1 | 2): void {
  tlSpeed.value = speed
  demoTl?.timeScale(speed)
}

// =========================================================================
// 02 · MorphSVG + DrawSVG 矢量图标变形与激光环绕描边
// =========================================================================
type MorphStateId = 'mic' | 'spark' | 'check' | 'bolt'
const MORPH_STATES: { id: MorphStateId; label: string; color: string; arc: string; d: string }[] = [
  {
    id: 'mic',
    label: '待命唤醒',
    color: '#3b82f6',
    arc: '0% 28%',
    d: 'M24 10 C27.3 10 30 12.7 30 16 L30 24 C30 27.3 27.3 30 24 30 C20.7 30 18 27.3 18 24 L18 16 C18 12.7 20.7 10 24 10 Z'
  },
  {
    id: 'spark',
    label: 'AI 思考',
    color: '#8b5cf6',
    arc: '15% 72%',
    d: 'M24 8 L28.2 19.8 L40 24 L28.2 28.2 L24 40 L19.8 28.2 L8 24 L19.8 19.8 Z'
  },
  {
    id: 'check',
    label: '执行完成',
    color: '#10b981',
    arc: '0% 100%',
    d: 'M11 24.5 L15 20.5 L21 26.5 L33.5 14 L37.5 18 L21 34.5 Z'
  },
  {
    id: 'bolt',
    label: '高压脉冲',
    color: '#f59e0b',
    arc: '55% 98%',
    d: 'M26 8 L13 26 L23 26 L20 40 L35 21 L25 21 Z'
  }
]
const morphStateId = ref<MorphStateId>('mic')
const morphCorePathEl = ref<SVGPathElement | null>(null)
const morphRingCircleEl = ref<SVGPathElement | null>(null)

function triggerMorph(id: MorphStateId): void {
  morphStateId.value = id
  const cfg = MORPH_STATES.find(item => item.id === id)
  if (!cfg) return
  if (morphCorePathEl.value) {
    gsap.to(morphCorePathEl.value, {
      morphSVG: cfg.d,
      fill: cfg.color,
      duration: reduced.value ? 0.01 : 0.48,
      ease: 'elastic.out(0.85, 0.7)'
    })
  }
  if (morphRingCircleEl.value) {
    gsap.to(morphRingCircleEl.value, {
      drawSVG: cfg.arc,
      stroke: cfg.color,
      duration: reduced.value ? 0.01 : 0.56,
      ease: 'power3.out'
    })
  }
}

// =========================================================================
// 03 · Draggable + InertiaPlugin 惯性甩动 & 停靠槽磁吸
// =========================================================================
const railArenaEl = ref<HTMLElement | null>(null)
const railPuckEl = ref<HTMLElement | null>(null)
const activeDock = ref<'left' | 'center' | 'right'>('center')
const throwSpeed = ref(0)
let railDraggable: Draggable | null = null

function dockTo(slot: 'left' | 'center' | 'right'): void {
  const arena = railArenaEl.value
  const puck = railPuckEl.value
  if (!arena || !puck) return
  const maxX = Math.max(0, arena.clientWidth - puck.offsetWidth - 16)
  const targetX = slot === 'left' ? 0 : slot === 'right' ? maxX : Math.round(maxX / 2)
  activeDock.value = slot
  gsap.to(puck, {
    x: targetX,
    duration: reduced.value ? 0.01 : 0.42,
    ease: 'back.out(1.5)'
  })
}

function setupDraggableRail(): void {
  const arena = railArenaEl.value
  const puck = railPuckEl.value
  if (!arena || !puck) return
  railDraggable?.kill()
  const maxX = Math.max(0, arena.clientWidth - puck.offsetWidth - 16)
  gsap.set(puck, { x: Math.round(maxX / 2) })
  const instances = Draggable.create(puck, {
    type: 'x',
    bounds: { minX: 0, maxX },
    inertia: true,
    edgeResistance: 0.65,
    snap: {
      x: (endVal: number) => {
        const curMax = Math.max(0, arena.clientWidth - puck.offsetWidth - 16)
        const points = [0, Math.round(curMax / 2), curMax]
        return points.reduce((best, p) => (Math.abs(p - endVal) < Math.abs(best - endVal) ? p : best), points[0])
      }
    },
    onDrag() {
      const curMax = Math.max(1, arena.clientWidth - puck.offsetWidth - 16)
      const ratio = this.x / curMax
      activeDock.value = ratio < 0.3 ? 'left' : ratio > 0.7 ? 'right' : 'center'
    },
    onThrowUpdate() {
      throwSpeed.value = Math.round(Math.abs(this.deltaX * 60))
    },
    onThrowComplete() {
      const curMax = Math.max(1, arena.clientWidth - puck.offsetWidth - 16)
      const ratio = this.x / curMax
      activeDock.value = ratio < 0.3 ? 'left' : ratio > 0.7 ? 'right' : 'center'
      throwSpeed.value = 0
    }
  })
  railDraggable = instances[0] ?? null
}

// =========================================================================
// 04 · SplitText 逐字波浪 + ScrambleText 赛博乱码解码
// =========================================================================
const TEXT_SAMPLES = [
  'DesktopHub 光学玻璃动效引擎就绪',
  'AI Translation Ready · Latency 42ms',
  'Cloud Build #482 · APK Verified OK'
]
const textIndex = ref(0)
const textFxMode = ref<'split' | 'scramble'>('split')
const fxTextEl = ref<HTMLElement | null>(null)
let activeSplit: SplitText | null = null

function playTextFx(nextIdx = textIndex.value, mode = textFxMode.value): void {
  textIndex.value = nextIdx
  textFxMode.value = mode
  const el = fxTextEl.value
  if (!el) return
  gsap.killTweensOf(el)
  activeSplit?.revert()
  activeSplit = null

  const text = TEXT_SAMPLES[nextIdx]
  if (reduced.value) {
    el.textContent = text
    return
  }

  if (mode === 'scramble') {
    gsap.to(el, {
      duration: 0.75,
      scrambleText: {
        text,
        chars: '01█▓▒AI_HUB#*$',
        speed: 0.65,
        revealDelay: 0.1
      },
      ease: 'none'
    })
  } else {
    el.textContent = text
    activeSplit = SplitText.create(el, { type: 'chars' })
    gsap.fromTo(
      activeSplit.chars,
      { opacity: 0, y: 20, scale: 0.7, rotationX: -55 },
      {
        opacity: 1,
        y: 0,
        scale: 1,
        rotationX: 0,
        duration: 0.44,
        stagger: 0.022,
        ease: 'back.out(1.9)',
        clearProps: 'transform'
      }
    )
  }
}

// =========================================================================
// 05 · GSAP Flip 跨布局形态切换 + 筛选 + 洗牌
// =========================================================================
interface FlipCard {
  id: string
  name: string
  cat: 'core' | 'smart'
  tone: string
  desc: string
}
const flipFilter = ref<'all' | 'core' | 'smart'>('all')
const flipCompact = ref(false)
const flipExpandedId = ref<string | null>(null)
const flipContainerEl = ref<HTMLElement | null>(null)
const flipCards = ref<FlipCard[]>([
  { id: 'apps', name: '应用启动', cat: 'core', tone: '#3b82f6', desc: '全键盘直达筛选与毫秒级拉起。' },
  { id: 'clip', name: '剪贴历史', cat: 'core', tone: '#8b5cf6', desc: '图文分组检索与常驻短语模板。' },
  { id: 'trans', name: '流式翻译', cat: 'smart', tone: '#06b6d4', desc: '独立模型接入与剪贴板联动。' },
  { id: 'ocr', name: '离线识字', cat: 'smart', tone: '#10b981', desc: '本地截屏秒级识别，无需上传云端。' }
])

const visibleFlipCards = computed(() =>
  flipFilter.value === 'all' ? flipCards.value : flipCards.value.filter(c => c.cat === flipFilter.value)
)

async function runFlipMutation(mutate: () => void): Promise<void> {
  const container = flipContainerEl.value
  const targets = container ? container.querySelectorAll<HTMLElement>('.flip-item') : null
  const flipState = targets?.length && !reduced.value ? Flip.getState(targets) : null
  mutate()
  await nextTick()
  if (!flipState || !container) return
  const nextTargets = container.querySelectorAll<HTMLElement>('.flip-item')
  if (nextTargets.length) {
    Flip.from(flipState, {
      targets: nextTargets,
      duration: 0.45,
      ease: 'power3.inOut',
      stagger: 0.025,
      absolute: true,
      nested: true,
      onEnter: els => gsap.fromTo(els, { opacity: 0, scale: 0.85 }, { opacity: 1, scale: 1, duration: 0.3 }),
      onLeave: els => gsap.to(els, { opacity: 0, scale: 0.85, duration: 0.2 })
    })
  }
}

function shuffleFlipCards(): void {
  void runFlipMutation(() => {
    const copy = [...flipCards.value]
    for (let i = copy.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1))
      ;[copy[i], copy[j]] = [copy[j], copy[i]]
    }
    flipCards.value = copy
  })
}

// =========================================================================
// 06 · CustomBounce & CustomWiggle 专属物理曲线对比台
// =========================================================================
const bounceDropEl = ref<HTMLElement | null>(null)
const wiggleRejectEl = ref<HTMLElement | null>(null)
const wiggleJitterEl = ref<HTMLElement | null>(null)

function fireCustomBounce(): void {
  if (!bounceDropEl.value || reduced.value) return
  gsap.killTweensOf(bounceDropEl.value)
  gsap.fromTo(
    bounceDropEl.value,
    { y: -42, scaleX: 1, scaleY: 1 },
    { y: 0, duration: 0.82, ease: 'hubBounceDrop' }
  )
  gsap.fromTo(
    bounceDropEl.value,
    { scaleX: 1, scaleY: 1 },
    { scaleX: 1.24, scaleY: 0.78, duration: 0.82, ease: 'hubBounceDropSquash' }
  )
}

function fireWiggle(type: 'reject' | 'jitter'): void {
  const el = type === 'reject' ? wiggleRejectEl.value : wiggleJitterEl.value
  if (!el || reduced.value) return
  gsap.killTweensOf(el)
  gsap.fromTo(
    el,
    { x: 0, rotation: 0 },
    type === 'reject'
      ? { x: 10, rotation: 3, duration: 0.56, ease: 'hubWiggleReject', clearProps: 'transform' }
      : { x: 7, duration: 0.42, ease: 'hubWiggleJitter', clearProps: 'transform' }
  )
}

// =========================================================================
// 07 · MotionPathPlugin 沿胶囊边缘轨道巡航粒子
// =========================================================================
const orbitDot1El = ref<HTMLElement | null>(null)
const orbitDot2El = ref<HTMLElement | null>(null)
const orbitPathId = '#gsap-capsule-orbit-path'
const orbitFast = ref(false)
let orbitTween1: gsap.core.Tween | null = null
let orbitTween2: gsap.core.Tween | null = null

function setupOrbit(): void {
  orbitTween1?.kill()
  orbitTween2?.kill()
  if (!orbitDot1El.value || !orbitDot2El.value) return
  orbitTween1 = gsap.to(orbitDot1El.value, {
    motionPath: { path: orbitPathId, align: orbitPathId, alignOrigin: [0.5, 0.5], autoRotate: true },
    duration: 3.4,
    repeat: -1,
    ease: 'none'
  })
  orbitTween2 = gsap.to(orbitDot2El.value, {
    motionPath: { path: orbitPathId, align: orbitPathId, alignOrigin: [0.5, 0.5], autoRotate: true, start: 0.5, end: 1.5 },
    duration: 3.4,
    repeat: -1,
    ease: 'none'
  })
}

function boostOrbit(): void {
  orbitFast.value = !orbitFast.value
  const scale = orbitFast.value ? 2.8 : 1
  if (orbitTween1) gsap.to(orbitTween1, { timeScale: scale, duration: 0.4 })
  if (orbitTween2) gsap.to(orbitTween2, { timeScale: scale, duration: 0.4 })
}

// =========================================================================
// 08 · 2D 网格径向 Stagger 涟漪 + Physics2D 重力粒子喷射
// =========================================================================
const MATRIX_LABELS = [
  '⌘ 唤醒', '⌥ 译文', '⌃ 复制', '⇧ 截图', '⎋ 退隐',
  '🔊 音量', '📁 中转', '✉️ 邮件', '⚡ 打包', '🤖 审批',
  '✨ 光晕', '🫧 粘液', '🎯 景深', '🪩 弹性', '🚀 GSAP'
]
const matrixEl = ref<HTMLElement | null>(null)
const burstLayerEl = ref<HTMLElement | null>(null)

function triggerMatrixRipple(fromIdx: number, event: MouseEvent): void {
  if (reduced.value) return
  const tiles = matrixEl.value?.querySelectorAll<HTMLElement>('.matrix-cap')
  if (tiles?.length) {
    gsap.killTweensOf(tiles)
    gsap.fromTo(
      tiles,
      { scale: 1, y: 0 },
      {
        scale: 0.84,
        y: 3,
        duration: 0.16,
        yoyo: true,
        repeat: 1,
        ease: 'power2.out',
        stagger: { amount: 0.36, from: fromIdx, grid: [3, 5] },
        clearProps: 'transform'
      }
    )
  }

  // Physics2D 重力彩色粒子喷射
  const layer = burstLayerEl.value
  const btn = event.currentTarget as HTMLElement | null
  if (!layer || !btn) return
  const layerRect = layer.getBoundingClientRect()
  const btnRect = btn.getBoundingClientRect()
  const originX = btnRect.left - layerRect.left + btnRect.width / 2
  const originY = btnRect.top - layerRect.top + btnRect.height / 2
  const palette = ['#3b82f6', '#06b6d4', '#8b5cf6', '#10b981', '#f59e0b']

  for (let i = 0; i < 14; i++) {
    const dot = document.createElement('span')
    dot.className = 'p2d-spark'
    dot.style.left = `${originX}px`
    dot.style.top = `${originY}px`
    dot.style.background = palette[i % palette.length]
    layer.appendChild(dot)
    gsap.to(dot, {
      duration: 0.85,
      opacity: 0,
      scale: 0.2,
      physics2D: {
        velocity: gsap.utils.random(95, 220),
        angle: gsap.utils.random(-145, -35),
        gravity: 420,
        friction: 0.02
      },
      onComplete: () => dot.remove()
    })
  }
}

onMounted(() => {
  setupTimelineDemo()
  triggerMorph('mic')
  setupDraggableRail()
  playTextFx(0, 'split')
  setupOrbit()
})

onBeforeUnmount(() => {
  demoTl?.kill()
  railDraggable?.kill()
  activeSplit?.revert()
  orbitTween1?.kill()
  orbitTween2?.kill()
  if (rootEl.value) gsap.killTweensOf(rootEl.value.querySelectorAll('*'))
})
</script>

<template>
  <section ref="rootEl" class="gsap-lab" aria-label="GSAP 全套动画特点组件实验室">
    <header class="lab-head">
      <div>
        <span class="lab-eyebrow">GSAP 3.15 全能力实验室（现已全套内置并驱动整站）</span>
        <h2>点一点下面 8 个专项组件，看你最想把哪些特性接入灵动岛</h2>
      </div>
      <span class="lab-chip">Timeline · MorphSVG · Draggable+Inertia · Flip · SplitText · ScrambleText · CustomBounce/Wiggle · MotionPath · Physics2D</span>
    </header>

    <div class="lab-grid">
      <!-- 01. Timeline 分镜编排 + 实时拖动 Scrub -->
      <article class="lab-card">
        <div class="card-head">
          <strong>01 · Timeline 多轨分镜 &amp; 进度条实时拖动</strong>
          <div class="btn-row">
            <button v-gsap-press="activeMotionPreset.buttonTap" class="mini-btn is-primary" type="button" @click="playTimeline(false)">正放</button>
            <button v-gsap-press="activeMotionPreset.buttonTap" class="mini-btn" type="button" @click="playTimeline(true)">倒放</button>
            <button class="mini-btn" type="button" @click="togglePauseTimeline">{{ tlPlaying ? '暂停' : '继续' }}</button>
            <button class="mini-btn" type="button" @click="setTimelineSpeed(tlSpeed === 1 ? 0.5 : tlSpeed === 0.5 ? 2 : 1)">{{ tlSpeed }}x</button>
          </div>
        </div>
        <p class="card-desc">光环先扫 -> 胶囊横向张开 -> 纵向弹性垂展 -> 子图标依次浮现；拖动下方进度条可一帧帧回看。</p>
        <div ref="tlStageEl" class="tl-stage">
          <div class="tl-shell">
            <span class="tl-ring" aria-hidden="true" />
            <span class="tl-badge">LIVE</span>
            <div class="tl-mini-row">
              <span class="tl-mini-card"><Icon name="grid" :size="14" />应用</span>
              <span class="tl-mini-card"><Icon name="clipboard" :size="14" />历史</span>
              <span class="tl-mini-card"><Icon name="translate" :size="14" />翻译</span>
            </div>
            <span class="tl-caption">GSAP Multi-Track Choreography</span>
          </div>
        </div>
        <label class="scrub-row">
          <span>进度 {{ tlProgress }}%</span>
          <input :value="tlProgress" type="range" min="0" max="100" @input="scrubTimeline" />
        </label>
      </article>

      <!-- 02. MorphSVG + DrawSVG -->
      <article class="lab-card">
        <div class="card-head">
          <strong>02 · MorphSVG 路径变形 &amp; DrawSVG 激光描边</strong>
        </div>
        <p class="card-desc">同一个 SVG 矢量节点直接在 4 种几何形状间丝滑变形，外圈随状态百分比动态描画。</p>
        <div class="morph-stage">
          <svg class="morph-svg" viewBox="0 0 48 48" width="92" height="92" aria-hidden="true">
            <circle cx="24" cy="24" r="21" fill="none" stroke="currentColor" stroke-opacity="0.12" stroke-width="2.2" />
            <path
              ref="morphRingCircleEl"
              d="M24 3 A21 21 0 1 1 23.99 3"
              fill="none"
              stroke="#3b82f6"
              stroke-width="2.6"
              stroke-linecap="round"
            />
            <path
              ref="morphCorePathEl"
              d="M24 10 C27.3 10 30 12.7 30 16 L30 24 C30 27.3 27.3 30 24 30 C20.7 30 18 27.3 18 24 L18 16 C18 12.7 20.7 10 24 10 Z"
              fill="#3b82f6"
            />
          </svg>
          <div class="morph-actions">
            <button
              v-for="item in MORPH_STATES"
              :key="item.id"
              v-gsap-press="activeMotionPreset.buttonTap"
              type="button"
              class="mini-btn"
              :class="{ 'is-primary': morphStateId === item.id }"
              @click="triggerMorph(item.id)"
            >
              {{ item.label }}
            </button>
          </div>
        </div>
      </article>

      <!-- 03. Draggable + InertiaPlugin -->
      <article class="lab-card">
        <div class="card-head">
          <strong>03 · Draggable + Inertia 惯性甩动停靠</strong>
          <span>{{ throwSpeed > 0 ? `脱手速度 ${throwSpeed} px/s` : `当前停靠：${activeDock === 'left' ? '左上' : activeDock === 'right' ? '右上' : '居中'}` }}</span>
        </div>
        <p class="card-desc">像真实桌面浮条一样用力向左右一甩：松手后带着物理动量滑行，并自动磁吸落入三档槽位。</p>
        <div ref="railArenaEl" class="rail-arena">
          <div class="rail-slots" aria-hidden="true">
            <span :class="{ active: activeDock === 'left' }">左上槽</span>
            <span :class="{ active: activeDock === 'center' }">居中槽</span>
            <span :class="{ active: activeDock === 'right' }">右上槽</span>
          </div>
          <div ref="railPuckEl" class="rail-puck">
            <Icon name="pin" :size="14" />
            <span>甩动胶囊</span>
          </div>
        </div>
        <div class="btn-row">
          <button class="mini-btn" type="button" @click="dockTo('left')">吸到左边</button>
          <button class="mini-btn" type="button" @click="dockTo('center')">吸到居中</button>
          <button class="mini-btn" type="button" @click="dockTo('right')">吸到右边</button>
        </div>
      </article>

      <!-- 04. SplitText + ScrambleText -->
      <article class="lab-card">
        <div class="card-head">
          <strong>04 · SplitText 逐字波浪 &amp; ScrambleText 乱码解码</strong>
          <div class="btn-row">
            <button class="mini-btn" :class="{ 'is-primary': textFxMode === 'split' }" type="button" @click="playTextFx(textIndex, 'split')">逐字弹跳</button>
            <button class="mini-btn" :class="{ 'is-primary': textFxMode === 'scramble' }" type="button" @click="playTextFx(textIndex, 'scramble')">赛博解码</button>
          </div>
        </div>
        <p class="card-desc">胶囊切换任务文案、AI 译文出现或版本号提示时，可用逐字波浪或矩阵字符扫描效果。</p>
        <div class="text-fx-stage">
          <p ref="fxTextEl" class="fx-headline">{{ TEXT_SAMPLES[textIndex] }}</p>
        </div>
        <div class="btn-row">
          <button
            v-for="(sample, idx) in TEXT_SAMPLES"
            :key="sample"
            v-gsap-press="activeMotionPreset.buttonTap"
            class="mini-btn"
            :class="{ 'is-primary': textIndex === idx }"
            type="button"
            @click="playTextFx(idx, textFxMode)"
          >
            文案 {{ idx + 1 }}
          </button>
        </div>
      </article>

      <!-- 05. Flip 跨布局形态切换与滤波重排 -->
      <article class="lab-card lab-wide">
        <div class="card-head">
          <strong>05 · Flip 全自动跨容器形变（筛选 / 排版切换 / 点卡放大）</strong>
          <div class="btn-row">
            <button class="mini-btn" :class="{ 'is-primary': flipFilter === 'all' }" type="button" @click="runFlipMutation(() => { flipFilter = 'all' })">全部</button>
            <button class="mini-btn" :class="{ 'is-primary': flipFilter === 'core' }" type="button" @click="runFlipMutation(() => { flipFilter = 'core' })">核心</button>
            <button class="mini-btn" :class="{ 'is-primary': flipFilter === 'smart' }" type="button" @click="runFlipMutation(() => { flipFilter = 'smart' })">智能</button>
            <button class="mini-btn" type="button" @click="runFlipMutation(() => { flipCompact = !flipCompact })">{{ flipCompact ? '切网格' : '切单行条' }}</button>
            <button class="mini-btn is-primary" type="button" @click="shuffleFlipCards">洗牌重排</button>
          </div>
        </div>
        <div ref="flipContainerEl" class="flip-grid" :class="{ 'is-compact': flipCompact }">
          <button
            v-for="card in visibleFlipCards"
            :key="card.id"
            type="button"
            class="flip-item"
            :class="{ 'is-expanded': flipExpandedId === card.id }"
            @click="runFlipMutation(() => { flipExpandedId = flipExpandedId === card.id ? null : card.id })"
          >
            <span class="flip-dot" :style="{ background: card.tone }" />
            <span class="flip-copy">
              <strong>{{ card.name }}</strong>
              <small>{{ flipExpandedId === card.id ? card.desc + '（再点收起）' : '点击可原地 FLIP 放大' }}</small>
            </span>
          </button>
        </div>
      </article>

      <!-- 06. CustomBounce & CustomWiggle -->
      <article class="lab-card">
        <div class="card-head">
          <strong>06 · CustomBounce 挤压弹跳 &amp; CustomWiggle 微震</strong>
        </div>
        <p class="card-desc">超越 CSS 贝塞尔的物理形变曲线：落地压扁、校验拒绝蓄力晃动、轻提醒震颤。</p>
        <div class="curves-stage">
          <button ref="bounceDropEl" type="button" class="curve-pill tone-blue" @click="fireCustomBounce">
            点我 · 落地挤压回弹
          </button>
          <button ref="wiggleRejectEl" type="button" class="curve-pill tone-red" @click="fireWiggle('reject')">
            点我 · 拒绝摇头震颤
          </button>
          <button ref="wiggleJitterEl" type="button" class="curve-pill tone-amber" @click="fireWiggle('jitter')">
            点我 · 提醒微抖
          </button>
        </div>
      </article>

      <!-- 07. MotionPathPlugin 沿胶囊边界巡航 -->
      <article class="lab-card">
        <div class="card-head">
          <strong>07 · MotionPath 沿胶囊边界轨道巡航</strong>
          <button class="mini-btn is-primary" type="button" @click="boostOrbit">{{ orbitFast ? '恢复巡航' : '切换超频倍速' }}</button>
        </div>
        <p class="card-desc">把翻译中/下载中/打包中的状态光点绑定在圆角胶囊矢量轨上精准跑圈。</p>
        <div class="orbit-stage">
          <svg class="orbit-svg" viewBox="0 0 260 66" width="260" height="66">
            <path
              id="gsap-capsule-orbit-path"
              d="M36 9 H224 A24 24 0 0 1 224 57 H36 A24 24 0 0 1 36 9 Z"
              fill="none"
              stroke="currentColor"
              stroke-opacity="0.16"
              stroke-width="2"
            />
          </svg>
          <span class="orbit-label">任务执行中 · Orbital Telemetry</span>
          <span ref="orbitDot1El" class="orbit-dot dot-blue" />
          <span ref="orbitDot2El" class="orbit-dot dot-cyan" />
        </div>
      </article>

      <!-- 08. 2D 网格径向 Stagger 涟漪 + Physics2D 重力粒子 -->
      <article class="lab-card lab-wide">
        <div class="card-head">
          <strong>08 · 2D 网格径向涟漪 Stagger + Physics2D 重力彩色粒子</strong>
          <span>点击下方任意一颗毛玻璃键帽触发以它为圆心的水波纹 + 重力粒子！</span>
        </div>
        <div class="matrix-wrap">
          <div ref="burstLayerEl" class="burst-layer" aria-hidden="true" />
          <div ref="matrixEl" class="matrix-grid">
            <button
              v-for="(label, idx) in MATRIX_LABELS"
              :key="label"
              type="button"
              class="matrix-cap"
              @click="triggerMatrixRipple(idx, $event)"
            >
              {{ label }}
            </button>
          </div>
        </div>
      </article>
    </div>
  </section>
</template>

<style scoped>
.gsap-lab {
  width: min(860px, 100%);
  padding: 20px;
  border-radius: 22px;
  border: 1px solid var(--line-strong);
  background: var(--surface-glass);
  backdrop-filter: var(--glass-backdrop);
  box-shadow: var(--island-drop-shadow);
  color: var(--text);
}

.lab-head {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  justify-content: space-between;
  gap: 8px 14px;
  margin-bottom: 16px;
}

.lab-eyebrow {
  display: inline-block;
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.08em;
  color: var(--accent);
}

.lab-head h2 {
  margin: 4px 0 0;
  font-size: 15px;
  font-weight: 650;
}

.lab-chip {
  font-size: 10.5px;
  color: var(--text-3);
}

.lab-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.lab-card {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
  border-radius: 16px;
  border: 1px solid var(--line);
  background: var(--surface-2);
  box-shadow: var(--control-shadow);
}

.lab-wide {
  grid-column: 1 / -1;
}

.card-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.card-head strong {
  font-size: 12.5px;
  font-weight: 650;
}

.card-head > span {
  font-size: 11px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}

.card-desc {
  margin: 0;
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--text-2);
}

.btn-row {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.mini-btn {
  height: 26px;
  padding: 0 10px;
  border-radius: 8px;
  border: 1px solid var(--line);
  background: var(--tile-surface);
  color: var(--text);
  font-size: 11px;
  font-weight: 600;
  box-shadow: var(--tile-shadow);
}

.mini-btn:hover {
  border-color: var(--line-strong);
  box-shadow: var(--tile-hover-shadow);
}

.mini-btn.is-primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-text);
}

/* 01 Timeline Stage */
.tl-stage {
  display: grid;
  place-items: center;
  min-height: 132px;
  padding: 10px;
  border-radius: 12px;
  background: var(--recess-bg);
  border: 1px solid var(--line);
  box-shadow: var(--recess-shadow);
}

.tl-shell {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  overflow: hidden;
  padding: 10px;
  background: var(--surface);
  border: 1px solid var(--line-strong);
  box-shadow: var(--island-drop-shadow);
}

.tl-ring {
  position: absolute;
  inset: 0;
  border-radius: inherit;
  box-shadow: inset 0 0 0 2px var(--accent);
  pointer-events: none;
}

.tl-badge {
  position: absolute;
  top: 7px;
  right: 8px;
  padding: 1px 6px;
  border-radius: 99px;
  font-size: 9px;
  font-weight: 700;
  background: var(--accent-weak);
  color: var(--accent);
}

.tl-mini-row {
  display: flex;
  gap: 8px;
}

.tl-mini-card {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 5px 9px;
  border-radius: 9px;
  background: var(--tile-surface);
  border: 1px solid var(--line);
  font-size: 11px;
  font-weight: 600;
}

.tl-caption {
  font-size: 10px;
  color: var(--text-3);
}

.scrub-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 11px;
  color: var(--text-2);
  font-variant-numeric: tabular-nums;
}

.scrub-row input {
  flex: 1;
  accent-color: var(--accent);
}

/* 02 MorphSVG Stage */
.morph-stage {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 14px;
  border-radius: 12px;
  background: var(--recess-bg);
  border: 1px solid var(--line);
  box-shadow: var(--recess-shadow);
}

.morph-svg {
  flex: none;
}

.morph-actions {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px;
  flex: 1;
}

/* 03 Draggable Rail */
.rail-arena {
  position: relative;
  display: flex;
  align-items: center;
  height: 76px;
  padding: 8px;
  border-radius: 14px;
  background: var(--recess-bg);
  border: 1px solid var(--line);
  box-shadow: var(--recess-shadow);
  overflow: hidden;
}

.rail-slots {
  position: absolute;
  inset: 8px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px;
  pointer-events: none;
}

.rail-slots span {
  font-size: 10.5px;
  color: var(--text-3);
  transition: color 140ms ease;
}

.rail-slots span.active {
  color: var(--accent);
  font-weight: 700;
}

.rail-puck {
  position: relative;
  z-index: 2;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 44px;
  padding: 0 14px;
  border-radius: 22px;
  border: 1px solid var(--line-strong);
  background: var(--tile-surface);
  color: var(--text);
  font-size: 12px;
  font-weight: 650;
  box-shadow: var(--tile-shadow);
  cursor: grab;
  user-select: none;
  touch-action: none;
}

/* 04 SplitText + ScrambleText */
.text-fx-stage {
  display: grid;
  place-items: center;
  min-height: 76px;
  padding: 12px;
  border-radius: 12px;
  background: var(--recess-bg);
  border: 1px solid var(--line);
  box-shadow: var(--recess-shadow);
  overflow: hidden;
}

.fx-headline {
  margin: 0;
  font-size: 15px;
  font-weight: 700;
  text-align: center;
  letter-spacing: 0.02em;
  color: var(--text);
  font-family: var(--font-mono);
}

/* 05 Flip Grid */
.flip-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

.flip-grid.is-compact {
  display: flex;
  flex-wrap: wrap;
}

.flip-grid.is-compact .flip-item {
  flex: 1 1 140px;
}

.flip-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid var(--line);
  background: var(--tile-surface);
  box-shadow: var(--tile-shadow);
  text-align: left;
}

.flip-item.is-expanded {
  grid-column: 1 / -1;
  border-color: var(--accent);
  background: var(--accent-weak);
}

.flip-dot {
  flex: none;
  width: 10px;
  height: 10px;
  border-radius: 50%;
}

.flip-copy {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.flip-copy strong {
  font-size: 12.5px;
}

.flip-copy small {
  font-size: 11px;
  color: var(--text-2);
}

/* 06 CustomBounce / CustomWiggle */
.curves-stage {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  justify-content: center;
  gap: 10px;
  min-height: 96px;
  padding: 16px 10px 12px;
  border-radius: 12px;
  background: var(--recess-bg);
  border: 1px solid var(--line);
  box-shadow: var(--recess-shadow);
}

.curve-pill {
  padding: 7px 12px;
  border-radius: 99px;
  font-size: 11.5px;
  font-weight: 650;
  border: 1px solid var(--line-strong);
  background: var(--tile-surface);
  box-shadow: var(--tile-shadow);
  transform-origin: center bottom;
}

.curve-pill.tone-blue { color: #2563eb; }
.curve-pill.tone-red { color: #dc2626; }
.curve-pill.tone-amber { color: #d97706; }

/* 07 MotionPath */
.orbit-stage {
  position: relative;
  display: grid;
  place-items: center;
  min-height: 96px;
  border-radius: 12px;
  background: var(--recess-bg);
  border: 1px solid var(--line);
  box-shadow: var(--recess-shadow);
  overflow: hidden;
}

.orbit-svg {
  display: block;
}

.orbit-label {
  position: absolute;
  font-size: 11.5px;
  font-weight: 650;
  color: var(--text-2);
  pointer-events: none;
}

.orbit-dot {
  position: absolute;
  top: 0;
  left: 0;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  pointer-events: none;
}

.dot-blue {
  background: #3b82f6;
  box-shadow: 0 0 12px #3b82f6;
}

.dot-cyan {
  background: #06b6d4;
  box-shadow: 0 0 12px #06b6d4;
}

/* 08 Matrix Ripple + Physics2D */
.matrix-wrap {
  position: relative;
  overflow: hidden;
  border-radius: 12px;
  padding: 8px;
  background: var(--recess-bg);
  border: 1px solid var(--line);
  box-shadow: var(--recess-shadow);
}

.burst-layer {
  position: absolute;
  inset: 0;
  pointer-events: none;
  z-index: 3;
}

.matrix-grid {
  position: relative;
  z-index: 2;
  display: grid;
  grid-template-columns: repeat(5, minmax(0, 1fr));
  gap: 8px;
}

.matrix-cap {
  height: 36px;
  border-radius: 10px;
  border: 1px solid var(--line);
  background: var(--tile-surface);
  color: var(--text);
  font-size: 11.5px;
  font-weight: 600;
  box-shadow: var(--tile-shadow);
}

.matrix-cap:hover {
  border-color: var(--accent);
}

:deep(.p2d-spark) {
  position: absolute;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  margin-left: -3.5px;
  margin-top: -3.5px;
  pointer-events: none;
}

@media (max-width: 720px) {
  .lab-grid {
    grid-template-columns: 1fr;
  }
  .matrix-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }
}
</style>
