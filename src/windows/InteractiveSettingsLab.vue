<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { version as appVersion } from '../../package.json'
import HubMotionConfig from '@/components/HubMotionConfig.vue'
import Icon from '@/components/Icon.vue'
import IslandEntry from '@/features/island/IslandEntry.vue'
import LiquidGlassCanvas from '@/features/settings-lab/LiquidGlassCanvas.vue'
import type { LiquidSceneState } from '@/features/settings-lab/liquidShader'
import type { SceneMode } from '@/features/settings-lab/reactive3d'
import ShortcutRecorder, { type ShortcutModel } from '@/features/settings-lab/ShortcutRecorder.vue'
import AppManagement from '@/features/settings/AppManagement.vue'
import MixerSettings from '@/features/settings/MixerSettings.vue'
import MailSettings from '@/features/settings/MailSettings.vue'
import ApprovalSettings from '@/features/settings/ApprovalSettings.vue'
import ConfigBackup from '@/features/settings/ConfigBackup.vue'
import '@/features/settings/settings.css'
import '@/features/settings-lab/lab-theme.css'
import type { TelemetrySnapshot } from '@/features/island/telemetryReadout'
import { isDarkThemeMode, THEME_LABELS, THEME_MODES, type ThemeMode } from '@/lib/themes'
import {
  MOTION_LABELS,
  MOTION_MODES,
  MOTION_TIMINGS,
  normalizeMotionMode,
  type MotionMode
} from '@/lib/motions'
import { defaultIslandAppearance, invokeHost, isDesktop } from '@/lib/desktop/desktop'
import { translationProviderPresets } from '@/lib/desktop/providers'
import type { TranslateDirection } from '@/lib/desktop/types'
import { previewApps, previewIcons } from '@/mock/preview-icons'
import {
  clearClips,
  pushToast,
  saveClipHistoryLimit,
  saveShortcuts,
  saveTranslationSettings,
  setCaptureImages,
  setMotion,
  setPaused,
  setTheme,
  state
} from '@/store'

if (!isDesktop) {
  if (!state.apps.length) state.apps = previewApps
  if (!Object.keys(state.icons).length) state.icons = previewIcons
  state.ui.settingsOpen = true
}

const liquidRef = ref<InstanceType<typeof LiquidGlassCanvas> | null>(null)
const activeTab = ref<SceneMode>('appearance')
const peeking3D = ref(false)
const fullscreenSim = ref(false)
const audioMutedDemo = ref(false)
const reactionLog = ref('液态光学核心已就绪 · 轻触左侧任意设置，感受折射涟漪与形态流动')
const feedbackEvent = ref({ kind: '', sequence: 0 })

const tabs: { id: SceneMode; label: string; fluidHint: string }[] = [
  { id: 'appearance', label: '外观', fluidHint: '多体液态琉璃与卫星微读数液滴' },
  { id: 'shortcuts', label: '快捷键', fluidHint: '液态水晶按键浮岛' },
  { id: 'clipboard', label: '剪贴板', fluidHint: '叠层液态水镜（暂停即凝冰）' },
  { id: 'translation', label: '翻译', fluidHint: '双生液滴太极互换与虹彩液桥' },
  { id: 'audio', label: '调音台', fluidHint: '声波驻波共振液态体' },
  { id: 'apps', label: '应用', fluidHint: '环绕星轨液滴阵' },
  { id: 'integrations', label: '通知', fluidHint: '同心光学信标波环' }
]

const themeMaterials: Record<ThemeMode, { orb: string; desc: string }> = {
  system: { orb: 'radial-gradient(circle at 30% 30%, #ffffff, #64748b 58%, #0f172a)', desc: '钛灰液态银' },
  light: { orb: 'radial-gradient(circle at 30% 30%, #ffffff, #dbeafe 60%, #93c5fd)', desc: '冰透白水晶' },
  dark: { orb: 'radial-gradient(circle at 30% 30%, #e2e8f0, #334155 55%, #090d16)', desc: '曜石液态汞' },
  midnight: { orb: 'radial-gradient(circle at 30% 30%, #e0f2fe, #38bdf8 52%, #082042)', desc: '深海蓝琉璃' },
  mecha: { orb: 'radial-gradient(circle at 30% 30%, #fef3c7, #f59e0b 54%, #451a03)', desc: '熔融琥珀金' },
  pixel: { orb: 'radial-gradient(circle at 30% 30%, #ecfccb, #84cc16 54%, #14532d)', desc: '荧光绿琉璃' },
  blueprint: { orb: 'radial-gradient(circle at 30% 30%, #f0f9ff, #38bdf8 52%, #0c4a6e)', desc: '普鲁士光学蓝' },
  neon: { orb: 'radial-gradient(circle at 30% 30%, #fce7f3, #ec4899 50%, #22d3ee)', desc: '赛博色散铬' }
}

const motionCurves: Record<MotionMode, string> = {
  glide: 'M 2 18 C 18 18, 26 3, 78 3',
  bounce: 'M 2 18 C 16 18, 26 1, 44 2 C 56 3, 64 7, 78 3',
  sweep: 'M 2 18 L 24 18 C 36 18, 42 3, 78 3',
  focus: 'M 2 18 C 14 18, 20 2, 42 3 L 78 3',
  flicker: 'M 2 18 L 14 3 L 78 3'
}

const currentMotion = computed<MotionMode>(() =>
  normalizeMotionMode(state.ui.islandAppearance.motion)
)

const anchor = computed<'left' | 'center' | 'right'>({
  get: () => (state.ui.islandAnchor === 'left' || state.ui.islandAnchor === 'right' ? state.ui.islandAnchor : 'center'),
  set: value => { state.ui.islandAnchor = value }
})

const isLightScene = computed(() => {
  const mode = state.settings.theme
  if (mode === 'system') return false
  return !isDarkThemeMode(mode)
})

const islandRailJustify = computed(() =>
  anchor.value === 'left' ? 'flex-start' : anchor.value === 'right' ? 'flex-end' : 'center'
)

/* ---------- 模拟微读数实时脉动 ---------- */
const telemetrySnapshot = ref<TelemetrySnapshot>({
  netState: 'ready',
  downBps: 12_600_000,
  upBps: 640_000,
  cpuPercent: 44,
  memoryPercent: 68
})
let telemetryTimer: number | null = null
let replayTimer: number | null = null

function startTelemetry(): void {
  if (telemetryTimer !== null) return
  telemetryTimer = window.setInterval(() => {
    const prev = telemetrySnapshot.value
    telemetrySnapshot.value = {
      netState: 'ready',
      downBps: Math.round(Math.max(120_000, Math.min(48_000_000, (prev.downBps ?? 12_600_000) + (Math.random() - 0.46) * 5_600_000))),
      upBps: Math.round(Math.max(40_000, Math.min(6_000_000, (prev.upBps ?? 640_000) + (Math.random() - 0.5) * 380_000))),
      cpuPercent: Math.round(Math.max(12, Math.min(92, (prev.cpuPercent ?? 44) + (Math.random() - 0.48) * 14))),
      memoryPercent: Math.round(Math.max(40, Math.min(90, (prev.memoryPercent ?? 68) + (Math.random() - 0.5) * 5)))
    }
  }, 1300)
}

const shortcutDraft = ref<ShortcutModel>({
  bar: state.shortcuts.bar,
  clipboard: state.shortcuts.clipboard,
  translation: state.shortcuts.translation
})
const historyLimitDraft = ref<number>(state.clipboardPreferences.historyLimit ?? state.clipboardPreferences.defaultHistoryLimit)
const translationDraft = ref({ ...state.translationSettings })
const ordinaryClipCount = computed(() => state.clips.filter(item => !item.favorite).length)

/** 实时驱动全屏 WebGL 液态玻璃着色器的完整状态。 */
const liquidState = computed<LiquidSceneState>(() => ({
  mode: activeTab.value,
  theme: state.settings.theme,
  motion: currentMotion.value,
  anchor: anchor.value,
  scale: state.ui.islandAppearance.scale,
  webglGlow: state.ui.islandAppearance.webgl,
  reducedMotion: state.ui.islandReducedMotion,
  avoidFullscreen: state.ui.islandAppearance.avoidFullscreen,
  fullscreenSim: fullscreenSim.value,
  hideFromCapture: state.ui.islandAppearance.hideFromCapture,
  telemetryMode: state.ui.islandAppearance.telemetryMode,
  telemetryNet: state.ui.islandAppearance.telemetryNet,
  telemetryCpu: state.ui.islandAppearance.telemetryCpu,
  telemetryMemory: state.ui.islandAppearance.telemetryMemory,
  netLevel: Math.min(1, (telemetrySnapshot.value.downBps ?? 0) / 36_000_000),
  cpuLevel: (telemetrySnapshot.value.cpuPercent ?? 35) / 100,
  memLevel: (telemetrySnapshot.value.memoryPercent ?? 60) / 100,
  clipboardPaused: state.ui.paused,
  clipLimit: historyLimitDraft.value,
  translateFlipped: translationDraft.value.direction === 'en-zh',
  audioMuted: audioMutedDemo.value,
  islandExpanded: state.ui.islandExpanded
}))

function ripple(event: Event | undefined, summary: string, strength = 0.72): void {
  reactionLog.value = summary
  liquidRef.value?.pulseFromEvent(event, strength)
}

function switchTab(id: SceneMode, event: MouseEvent): void {
  activeTab.value = id
  const meta = tabs.find(t => t.id === id)
  ripple(event, `${meta?.label} · 液态场已重塑为${meta?.fluidHint}`, 0.8)
}

function pickTheme(mode: ThemeMode, event: MouseEvent): void {
  setTheme(mode)
  ripple(event, `${THEME_LABELS[mode]} · 光学材质切换为「${themeMaterials[mode].desc}」`, 0.9)
}

function replayFluidAndIsland(event?: Event): void {
  if (replayTimer !== null) window.clearTimeout(replayTimer)
  const timing = MOTION_TIMINGS[currentMotion.value]
  liquidRef.value?.playMotionDemo()
  state.ui.islandExpanded = false
  replayTimer = window.setTimeout(() => {
    state.ui.islandExpanded = true
    replayTimer = window.setTimeout(() => {
      state.ui.islandExpanded = false
      replayTimer = null
    }, timing.expandMs + 1050)
  }, timing.collapseMs + 40)
  if (event) ripple(event, `${MOTION_LABELS[currentMotion.value]} · 液态核心与顶部灵动岛同步拉丝开合`, 0.85)
}

function pickMotion(mode: MotionMode, event: MouseEvent): void {
  setMotion(mode)
  replayFluidAndIsland(event)
}

function pickAnchor(pos: 'left' | 'center' | 'right', event: MouseEvent): void {
  anchor.value = pos
  const label = pos === 'left' ? '左上' : pos === 'right' ? '右上' : '顶部居中'
  ripple(event, `停靠至${label} · 液态核心随表面张力漂移定位`, 0.65)
}

function onScaleInput(event: Event): void {
  reactionLog.value = `灵动岛缩放 ${state.ui.islandAppearance.scale}% · 液态核心体积实时膨胀收缩`
  if (event.target instanceof HTMLElement) {
    const r = event.target.getBoundingClientRect()
    liquidRef.value?.pulseFromEvent({ clientX: r.left + r.width / 2, clientY: r.top } as unknown as Event, 0.25)
  }
}

function toggleWebgl(event: MouseEvent): void {
  state.ui.islandAppearance.webgl = !state.ui.islandAppearance.webgl
  ripple(
    event,
    state.ui.islandAppearance.webgl
      ? '光轨舱门开启 · 液态琉璃环与虹彩边缘色散已激活'
      : '光轨舱门关闭 · 退回纯净无色散素玻璃',
    0.8
  )
}

function toggleReducedMotion(event: MouseEvent): void {
  state.ui.islandReducedMotion = !state.ui.islandReducedMotion
  ripple(
    event,
    state.ui.islandReducedMotion
      ? '减少动态效果开启 · 液态流体已凝固为静谧冰晶'
      : '减少动态效果关闭 · 恢复丝滑光学呼吸流动',
    0.7
  )
}

function toggleAvoidFullscreen(event: MouseEvent): void {
  state.ui.islandAppearance.avoidFullscreen = !state.ui.islandAppearance.avoidFullscreen
  fullscreenSim.value = state.ui.islandAppearance.avoidFullscreen
  state.ui.fullscreenSuppressed = fullscreenSim.value
  ripple(
    event,
    state.ui.islandAppearance.avoidFullscreen
      ? '全屏收为细线 · 液态核心已收束为顶部悬浮液态光丝'
      : '退出细线形态 · 液态光丝重新汇聚为饱满核心',
    0.85
  )
}

function toggleHideFromCapture(event: MouseEvent): void {
  state.ui.islandAppearance.hideFromCapture = !state.ui.islandAppearance.hideFromCapture
  ripple(
    event,
    state.ui.islandAppearance.hideFromCapture
      ? '截图与共享隐藏开启 · 液态核心已进入重度磨砂雾化隐身态'
      : '隐私雾化解除 · 恢复晶莹剔透光学质感',
    0.85
  )
}

function setTelemetryMode(mode: 'smart' | 'always' | 'off', event: MouseEvent): void {
  state.ui.islandAppearance.telemetryMode = mode
  ripple(
    event,
    mode === 'off'
      ? '微读数关闭 · 环绕卫星液滴已融回主核心'
      : `微读数「${mode === 'always' ? '常驻' : '智能'}」· 卫星液滴已分离并实时呼吸`,
    0.75
  )
}

function toggleMetric(key: 'telemetryNet' | 'telemetryCpu' | 'telemetryMemory', label: string, event: MouseEvent): void {
  state.ui.islandAppearance[key] = !state.ui.islandAppearance[key]
  ripple(event, `${label}卫星液滴已${state.ui.islandAppearance[key] ? '浮现' : '收回'}`, 0.55)
}

function onShortcutTest(key: keyof ShortcutModel, event: MouseEvent): void {
  const combo = shortcutDraft.value[key]
  state.ui.fullscreenSuppressed = false
  state.ui.islandPage = key === 'clipboard' ? 'clipboard' : key === 'translation' ? 'translation' : 'applications'
  state.ui.islandExpanded = true
  ripple(event, `触发 ${combo} · 水晶按键浮岛产生下沉折射波`, 0.9)
}

function onShortcutKeysPressed(caps: string[]): void {
  liquidRef.value?.pulseFromEvent(undefined, 0.75)
  reactionLog.value = `键入 ${caps.join(' + ')} · 液态镜面实时泛起折射波纹`
}

function toggleClipboardPause(event: MouseEvent): void {
  void setPaused(!state.ui.paused)
  ripple(
    event,
    !state.ui.paused
      ? '剪贴板恢复记录 · 叠层液态水镜重新流动'
      : '剪贴板暂停记录 · 叠层水镜瞬间凝结静止',
    0.8
  )
}

function onClearClips(event: MouseEvent): void {
  void clearClips()
  liquidRef.value?.triggerBeam()
  ripple(event, '清空剪贴板历史 · 触发全屏光学冲击涟漪', 1.0)
}

function flipTranslateDirection(event: MouseEvent): void {
  const next: TranslateDirection = translationDraft.value.direction === 'en-zh' ? 'zh-en' : 'en-zh'
  translationDraft.value.direction = next
  ripple(event, `方向翻转（${next === 'en-zh' ? '英 → 中' : '中 → 英'}）· 双生液滴太极绕转互换`, 0.85)
}

function fireTranslateBeam(event: MouseEvent): void {
  liquidRef.value?.triggerBeam()
  feedbackEvent.value = { kind: 'translated', sequence: feedbackEvent.value.sequence + 1 }
  ripple(event, '试译触发 · 双生液滴间拉起高能虹彩液桥', 0.95)
}

function toggleAudioMuteDemo(event: MouseEvent): void {
  audioMutedDemo.value = !audioMutedDemo.value
  ripple(
    event,
    audioMutedDemo.value ? '静音态 · 声波液态体归于镜面平静' : '恢复声波 · 液态驻波重新共振起伏',
    0.75
  )
}

function fireRadarPing(event: MouseEvent): void {
  liquidRef.value?.triggerBeam()
  feedbackEvent.value = { kind: 'copied', sequence: feedbackEvent.value.sequence + 1 }
  ripple(event, '信标测试 · 同心光学脉冲环已向外扩散', 0.95)
}

function toggleFullscreenSim(event: MouseEvent): void {
  fullscreenSim.value = !fullscreenSim.value
  state.ui.fullscreenSuppressed = state.ui.islandAppearance.avoidFullscreen && fullscreenSim.value
  ripple(event, fullscreenSim.value ? '模拟全屏 · 若已开启避让，液态核心收束为细光丝' : '退出模拟全屏 · 恢复常规液态体积', 0.75)
}

function resetDefaults(event: MouseEvent): void {
  Object.assign(state.ui.islandAppearance, defaultIslandAppearance())
  anchor.value = 'center'
  state.ui.islandReducedMotion = false
  fullscreenSim.value = false
  state.ui.fullscreenSuppressed = false
  setTheme('midnight')
  setMotion('glide')
  ripple(event, '已复位默认参数 · 液态光学核心恢复平衡态', 0.85)
}

async function saveCurrent(event: MouseEvent): Promise<void> {
  if (isDesktop) {
    await invokeHost('save_island_appearance', {
      appearance: { ...state.ui.islandAppearance },
      anchor: anchor.value,
      hoverMs: state.ui.islandHoverMs,
      leaveMs: state.ui.islandLeaveMs,
      reducedMotion: state.ui.islandReducedMotion
    })
    await saveShortcuts({ ...shortcutDraft.value, ocr: state.shortcuts.ocr, errors: [] })
    await saveTranslationSettings(translationDraft.value)
    await saveClipHistoryLimit(historyLimitDraft.value, Math.max(0, ordinaryClipCount.value - historyLimitDraft.value))
    pushToast('设置已保存到本机', 'success')
  } else {
    pushToast('浏览器预览模式 · 当前液态交互效果已实时生效（不修改本机文件）', 'success')
  }
  liquidRef.value?.triggerBeam()
  ripple(event, '配置已校准锁定', 0.9)
}

onMounted(() => {
  if (!isDesktop && state.settings.theme === 'dark') {
    setTheme('midnight')
  }
  startTelemetry()
})

onBeforeUnmount(() => {
  if (telemetryTimer !== null) window.clearInterval(telemetryTimer)
  if (replayTimer !== null) window.clearTimeout(replayTimer)
  state.ui.fullscreenSuppressed = false
})
</script>

<template>
  <HubMotionConfig>
    <div class="studio-deck" :class="{ 'is-light-scene': isLightScene }">
    <!-- 底层：全屏 WebGL GLSL 3D 液态玻璃与光影流体 -->
    <LiquidGlassCanvas ref="liquidRef" :scene-state="liquidState" />

    <!-- 顶部：真实可交互灵动岛 -->
    <div class="floating-island-rail" :style="{ justifyContent: islandRailJustify }">
      <div class="floating-island-slot">
        <IslandEntry
          :preview-feedback="feedbackEvent"
          :preview-telemetry="state.ui.islandAppearance.telemetryMode !== 'off' ? telemetrySnapshot : null"
        />
      </div>
    </div>

    <!-- 左侧：VisionOS 级悬浮液态玻璃设置面板 -->
    <aside class="glass-drawer" :class="{ 'is-peeking': peeking3D }" aria-label="液态玻璃交互设置">
      <header class="drawer-head">
        <div class="drawer-brand">
          <div class="drawer-orb" aria-hidden="true" />
          <div>
            <div class="drawer-title">DesktopHub 液态设置空间</div>
            <div class="drawer-sub">GLSL 实时折射流体 · v{{ appVersion }}</div>
          </div>
        </div>
        <div class="drawer-head-actions">
          <a class="deck-link-pill" href="./index.html">返回旧预览</a>
        </div>
      </header>

      <nav class="deck-tabs" aria-label="设置分区">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          type="button"
          class="deck-tab"
          :class="{ 'is-active': activeTab === tab.id }"
          @click="switchTab(tab.id, $event)"
        >
          {{ tab.label }}
        </button>
      </nav>

      <div class="deck-scroll">
        <!-- 01 外观 -->
        <div v-show="activeTab === 'appearance'" class="deck-section">
          <div class="deck-section-head">
            <h2>光学材质与液态开合</h2>
            <p>点选材质与开关，每一次轻触都会在背后液态琉璃中激起真实折射涟漪。</p>
          </div>

          <div class="deck-card">
            <div class="deck-card-head">
              <span class="deck-card-title">{{ THEME_MODES.length }} 套光学材质</span>
              <span class="deck-card-sub">{{ themeMaterials[state.settings.theme].desc }}</span>
            </div>
            <div class="theme-matrix">
              <button
                v-for="mode in THEME_MODES"
                :key="mode"
                type="button"
                class="theme-swatch"
                :class="{ 'is-active': state.settings.theme === mode }"
                @click="pickTheme(mode, $event)"
              >
                <span class="swatch-orb" :style="{ background: themeMaterials[mode].orb }" aria-hidden="true" />
                <span class="swatch-meta">
                  <span class="swatch-name">{{ THEME_LABELS[mode] }}</span>
                  <span class="swatch-tag">{{ themeMaterials[mode].desc }}</span>
                </span>
              </button>
            </div>
          </div>

          <div class="deck-card">
            <div class="deck-card-head">
              <span class="deck-card-title">开合动效</span>
              <button type="button" class="deck-btn is-sm" @click="replayFluidAndIsland($event)">▶ 演示开合</button>
            </div>
            <div class="motion-grid">
              <button
                v-for="mode in MOTION_MODES"
                :key="mode"
                type="button"
                class="motion-pad"
                :class="{ 'is-active': currentMotion === mode }"
                @click="pickMotion(mode, $event)"
              >
                <span class="motion-pad-top">
                  <span>{{ MOTION_LABELS[mode] }}</span>
                  <span class="motion-ms">{{ MOTION_TIMINGS[mode].expandMs }}ms</span>
                </span>
                <svg class="motion-curve" viewBox="0 0 80 20" aria-hidden="true">
                  <path :d="motionCurves[mode]" />
                </svg>
              </button>
            </div>
          </div>

          <div class="deck-card">
            <div class="deck-card-head">
              <span class="deck-card-title">停靠与体量</span>
            </div>
            <div class="deck-seg" role="radiogroup" aria-label="停靠位置">
              <button type="button" class="deck-seg-btn" :class="{ 'is-active': anchor === 'left' }" @click="pickAnchor('left', $event)">左上</button>
              <button type="button" class="deck-seg-btn" :class="{ 'is-active': anchor === 'center' }" @click="pickAnchor('center', $event)">顶部居中</button>
              <button type="button" class="deck-seg-btn" :class="{ 'is-active': anchor === 'right' }" @click="pickAnchor('right', $event)">右上</button>
            </div>
            <label class="fader-box">
              <span class="fader-head">
                <span>显示大小（实时改变液态体积）</span>
                <span class="fader-readout">{{ state.ui.islandAppearance.scale }}%</span>
              </span>
              <input
                v-model.number="state.ui.islandAppearance.scale"
                class="deck-slider"
                type="range"
                min="75"
                max="125"
                step="5"
                @input="onScaleInput"
              />
              <span class="fader-ticks"><span>75%</span><span>100%</span><span>125%</span></span>
            </label>
          </div>

          <div class="deck-card">
            <div class="deck-card-head">
              <span class="deck-card-title">光学与隐私开关</span>
            </div>
            <div class="toggle-list">
              <button
                type="button"
                class="mech-toggle"
                :class="{ 'is-on': state.ui.islandAppearance.showAppNames }"
                @click="state.ui.islandAppearance.showAppNames = !state.ui.islandAppearance.showAppNames; ripple($event, state.ui.islandAppearance.showAppNames ? '应用网格已开启显示名称' : '应用网格已切为紧凑纯图标')"
              >
                <span class="mech-copy">
                  <span class="mech-title">应用网格显示名称</span>
                  <span class="mech-desc">控制灵动岛应用页在图标下方显示文字名称或仅显示纯图标</span>
                </span>
                <span class="mech-switch"><span class="mech-knob" /></span>
              </button>
              <button type="button" class="mech-toggle" :class="{ 'is-on': state.ui.islandAppearance.webgl }" @click="toggleWebgl($event)">
                <span class="mech-copy">
                  <span class="mech-title">光轨舱门（虹彩折射环）</span>
                  <span class="mech-desc">开启边缘色散与环绕液态玻璃光环</span>
                </span>
                <span class="mech-switch"><span class="mech-knob" /></span>
              </button>
              <button type="button" class="mech-toggle" :class="{ 'is-on': state.ui.islandReducedMotion }" @click="toggleReducedMotion($event)">
                <span class="mech-copy">
                  <span class="mech-title">减少动态效果（冰晶凝静）</span>
                  <span class="mech-desc">流体流速归零，界面回归绝对安静</span>
                </span>
                <span class="mech-switch"><span class="mech-knob" /></span>
              </button>
              <button type="button" class="mech-toggle" :class="{ 'is-on': state.ui.islandAppearance.avoidFullscreen }" @click="toggleAvoidFullscreen($event)">
                <span class="mech-copy">
                  <span class="mech-title">全屏/最大化收为细线</span>
                  <span class="mech-desc">液态核心收束拉成顶部悬浮细光丝</span>
                </span>
                <span class="mech-switch"><span class="mech-knob" /></span>
              </button>
              <button type="button" class="mech-toggle" :class="{ 'is-on': state.ui.islandAppearance.hideFromCapture }" @click="toggleHideFromCapture($event)">
                <span class="mech-copy">
                  <span class="mech-title">截图与屏幕共享中隐藏</span>
                  <span class="mech-desc">液态玻璃实时起雾，呈现磨砂隐身态</span>
                </span>
                <span class="mech-switch"><span class="mech-knob" /></span>
              </button>
            </div>
          </div>

          <div class="deck-card">
            <div class="deck-card-head">
              <span class="deck-card-title">胶囊微读数（卫星液滴）</span>
            </div>
            <div class="deck-seg">
              <button type="button" class="deck-seg-btn" :class="{ 'is-active': state.ui.islandAppearance.telemetryMode === 'smart' }" @click="setTelemetryMode('smart', $event)">智能浮现</button>
              <button type="button" class="deck-seg-btn" :class="{ 'is-active': state.ui.islandAppearance.telemetryMode === 'always' }" @click="setTelemetryMode('always', $event)">常驻显示</button>
              <button type="button" class="deck-seg-btn" :class="{ 'is-active': state.ui.islandAppearance.telemetryMode === 'off' }" @click="setTelemetryMode('off', $event)">关闭</button>
            </div>
            <div v-if="state.ui.islandAppearance.telemetryMode !== 'off'" class="metric-row">
              <button type="button" class="deck-btn is-sm" :class="{ 'is-primary': state.ui.islandAppearance.telemetryNet }" @click="toggleMetric('telemetryNet', '网速', $event)">网速液滴</button>
              <button type="button" class="deck-btn is-sm" :class="{ 'is-primary': state.ui.islandAppearance.telemetryCpu }" @click="toggleMetric('telemetryCpu', '处理器', $event)">CPU 液滴</button>
              <button type="button" class="deck-btn is-sm" :class="{ 'is-primary': state.ui.islandAppearance.telemetryMemory }" @click="toggleMetric('telemetryMemory', '内存', $event)">内存液滴</button>
            </div>
          </div>
        </div>

        <!-- 02 快捷键 -->
        <div v-show="activeTab === 'shortcuts'" class="deck-section">
          <div class="deck-section-head">
            <h2>快捷键录制</h2>
            <p>点击录制直接敲击键盘，右侧液态水晶按键浮岛实时泛起下沉折射波。</p>
          </div>
          <div class="deck-card">
            <ShortcutRecorder
              v-model="shortcutDraft"
              @test-trigger="onShortcutTest"
              @keys-pressed="onShortcutKeysPressed"
            />
          </div>
        </div>

        <!-- 03 剪贴板 -->
        <div v-show="activeTab === 'clipboard'" class="deck-section">
          <div class="deck-section-head">
            <h2>剪贴板与叠层水镜</h2>
            <p>暂停记录瞬间凝冰静止，滑块实时拉宽叠层液镜，清空触发光学冲击波。</p>
          </div>
          <div class="deck-card">
            <div class="toggle-list">
              <button type="button" class="mech-toggle" :class="{ 'is-on': state.ui.paused }" @click="toggleClipboardPause($event)">
                <span class="mech-copy">
                  <span class="mech-title">暂停记录剪贴板</span>
                  <span class="mech-desc">右侧叠层液态水镜立即冻结静止</span>
                </span>
                <span class="mech-switch"><span class="mech-knob" /></span>
              </button>
              <button
                type="button"
                class="mech-toggle"
                :class="{ 'is-on': state.clipboardPreferences.captureImages }"
                @click="setCaptureImages(!state.clipboardPreferences.captureImages); ripple($event, '已切换新图片捕获状态')"
              >
                <span class="mech-copy">
                  <span class="mech-title">记录新图片</span>
                  <span class="mech-desc">控制是否记录后续复制的图像</span>
                </span>
                <span class="mech-switch"><span class="mech-knob" /></span>
              </button>
            </div>

            <label class="fader-box">
              <span class="fader-head">
                <span>普通记录数量上限</span>
                <span class="fader-readout">{{ historyLimitDraft }} 条</span>
              </span>
              <input
                v-model.number="historyLimitDraft"
                class="deck-slider"
                type="range"
                min="10"
                max="800"
                step="10"
                @input="reactionLog = `历史上限 ${historyLimitDraft} 条 · 叠层水镜间距已同步展开`"
              />
            </label>

            <button type="button" class="deck-btn is-danger" @click="onClearClips($event)">清空历史记录</button>
          </div>
        </div>

        <!-- 04 翻译 -->
        <div v-show="activeTab === 'translation'" class="deck-section">
          <div class="deck-section-head">
            <h2>翻译与双生液滴</h2>
            <p>点击方向翻转，两团液态球太极绕转互换；试译拉出虹彩液桥。</p>
          </div>
          <div class="deck-card">
            <div class="deck-card-head">
              <span class="deck-card-title">语言方向</span>
              <button type="button" class="deck-btn is-sm is-primary" @click="flipTranslateDirection($event)">
                {{ translationDraft.direction === 'en-zh' ? 'English ⇄ 中文' : '中文 ⇄ English' }}
              </button>
            </div>
            <div class="provider-list">
              <button
                v-for="preset in translationProviderPresets"
                :key="preset.id"
                type="button"
                class="deck-btn is-sm"
                :class="{ 'is-primary': translationDraft.providerId === preset.id }"
                @click="translationDraft.providerId = preset.id; translationDraft.baseUrl = preset.baseUrl; translationDraft.model = preset.model; ripple($event, `翻译服务已切至 ${preset.name}`)"
              >
                {{ preset.name }}
              </button>
            </div>
            <button type="button" class="deck-btn is-primary" @click="fireTranslateBeam($event)">▶ 触发虹彩液桥试译</button>
          </div>
        </div>

        <!-- 05 调音台 -->
        <div v-show="activeTab === 'audio'" class="deck-section">
          <div class="deck-section-head">
            <h2>调音台与声波液态体</h2>
            <p>切换静音瞬间令共振液态体回归镜面般平静。</p>
          </div>
          <div class="deck-card">
            <button type="button" class="deck-btn" :class="{ 'is-primary': audioMutedDemo }" @click="toggleAudioMuteDemo($event)">
              {{ audioMutedDemo ? '恢复声波共振' : '模拟静音（水面归于平静）' }}
            </button>
            <div class="embedded-wrap"><MixerSettings /></div>
          </div>
        </div>

        <!-- 06 应用管理 -->
        <div v-show="activeTab === 'apps'" class="deck-section">
          <div class="deck-section-head">
            <h2>应用管理</h2>
            <p>复用完整应用管理器，修改实时同步到顶部灵动岛。</p>
          </div>
          <div class="deck-card embedded-wrap"><AppManagement /></div>
        </div>

        <!-- 07 通知与备份 -->
        <div v-show="activeTab === 'integrations'" class="deck-section">
          <div class="deck-section-head">
            <h2>通知、智能体与备份</h2>
            <p>点击信标测试可向外释放同心光学脉冲环。</p>
          </div>
          <div class="deck-card">
            <button type="button" class="deck-btn is-primary" @click="fireRadarPing($event)">◉ 释放光学信标脉冲</button>
          </div>
          <div class="deck-card embedded-wrap"><MailSettings /></div>
          <div class="deck-card embedded-wrap"><ApprovalSettings /></div>
          <div class="deck-card embedded-wrap"><ConfigBackup :blocked="false" /></div>
        </div>
      </div>

      <footer class="drawer-footer">
        <span class="drawer-footer-note">{{ isDesktop ? '本机设置模式' : '网页液态预览 · 不改写本机配置' }}</span>
        <div class="drawer-head-actions">
          <button type="button" class="deck-btn is-sm" @click="resetDefaults($event)">复位</button>
          <button type="button" class="deck-btn is-sm is-primary" @click="saveCurrent($event)">应用</button>
        </div>
      </footer>
    </aside>

    <!-- 右下角：极简光学实况胶囊 -->
    <div class="stage-hud-bottom">
      <div class="hud-reaction-pill" role="status">
        <span class="hud-pulse-dot" aria-hidden="true" />
        <span>{{ reactionLog }}</span>
      </div>
      <div class="hud-quick-bar">
        <button type="button" class="hud-chip-btn" :class="{ 'is-active': fullscreenSim }" @click="toggleFullscreenSim($event)">
          {{ fullscreenSim ? '退出细线态' : '模拟全屏细线' }}
        </button>
        <button type="button" class="hud-chip-btn" @click="replayFluidAndIsland($event)">
          ▶ 演示液态开合
        </button>
        <button type="button" class="hud-chip-btn" :class="{ 'is-active': peeking3D }" @click="peeking3D = !peeking3D">
          {{ peeking3D ? '显示设置面板' : '纯净观赏流体' }}
        </button>
      </div>
    </div>

    <!-- 轻提示 -->
    <div class="deck-toasts" role="status" aria-live="polite">
      <div v-for="toast in state.toasts" :key="toast.id" class="deck-toast">
        <Icon :name="toast.kind === 'success' ? 'check' : 'info'" :size="14" />
        <span>{{ toast.text }}</span>
      </div>
    </div>
  </div>
  </HubMotionConfig>
</template>

<style scoped>
.metric-row,
.provider-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.embedded-wrap :deep(.panel) {
  padding: 0;
}

.embedded-wrap :deep(.btn) {
  min-height: 30px;
  padding: 4px 12px;
  border-radius: 999px;
  border: 1px solid var(--deck-line-strong);
  background: var(--deck-key-bg);
  color: var(--deck-ink);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.embedded-wrap :deep(.control) {
  min-height: 32px;
  padding: 4px 11px;
  border-radius: 10px;
  border: 1px solid var(--deck-line-strong);
  background: var(--deck-well);
  color: var(--deck-ink);
  font: inherit;
  font-size: 12px;
}

.deck-toasts {
  position: fixed;
  top: 68px;
  right: 26px;
  z-index: 60;
  display: flex;
  flex-direction: column;
  gap: 6px;
  pointer-events: none;
}

.deck-toast {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 9px 15px;
  border-radius: 999px;
  background: var(--deck-glass);
  border: 1px solid var(--deck-line-strong);
  color: var(--deck-ink);
  box-shadow: inset 0 1px 0 var(--deck-specular), 0 12px 28px rgba(0, 0, 0, 0.22);
  backdrop-filter: blur(20px);
  font-size: 12px;
  font-weight: 600;
}
</style>
