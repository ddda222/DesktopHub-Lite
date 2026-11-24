<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import IslandEntry from '@/features/island/IslandEntry.vue'
import type { TelemetrySnapshot } from '@/features/island/telemetryReadout'
import { THEME_LABELS } from '@/lib/themes'
import { MOTION_LABELS, MOTION_TIMINGS, isMotionMode, type MotionMode } from '@/lib/motions'
import { isDarkTheme, state, setPaused, pushToast } from '@/store'
import { loadBrowserShelfSamples, shelf } from '@/features/files/shelf'
import { makeShelfSamples } from '@/mock/shelf-samples'

/** 幕布环境配置：支持在设置台内实时测试桌面状态、壁纸明暗与情境触发。 */
const props = defineProps<{
  anchor: 'left' | 'center' | 'right'
  scale: number
  hideFromCapture: boolean
  avoidFullscreen: boolean
  telemetryActive: boolean
  telemetrySnapshot: TelemetrySnapshot | null
  feedbackEvent: { kind: string; sequence: number }
}>()

const emit = defineEmits<{
  'update:anchor': [value: 'left' | 'center' | 'right']
  'toggle-telemetry': []
  'trigger-feedback': [kind: string]
  'replay-motion': []
}>()

type DeskEnv = 'normal' | 'maximized' | 'fullscreen'
type DeskBackdrop = 'paper' | 'night' | 'grid'

const deskEnv = ref<DeskEnv>('normal')
const deskBackdrop = ref<DeskBackdrop>('grid')
const keepOpen = ref(false)
const captureMirrorMode = ref(false)
const translatingDemo = ref(false)

let translateDemoTimer: number | null = null

const currentMotion = computed<MotionMode>(() => {
  const raw = state.ui.islandAppearance.motion
  return isMotionMode(raw) ? raw : 'glide'
})

const anchorOptions: { id: 'left' | 'center' | 'right'; label: string; code: string }[] = [
  { id: 'left', label: '左上', code: 'L' },
  { id: 'center', label: '顶部居中', code: 'C' },
  { id: 'right', label: '右上', code: 'R' }
]

const stageJustify = computed(() => {
  if (props.anchor === 'left') return 'flex-start'
  if (props.anchor === 'right') return 'flex-end'
  return 'center'
})

const anchorRailIndex = computed(() => (props.anchor === 'left' ? 0 : props.anchor === 'center' ? 1 : 2))

/** 同步桌面状态到灵动岛全屏避让逻辑，方便立即验收「收为细线」。 */
watch([deskEnv, () => props.avoidFullscreen], ([env, avoid]) => {
  state.ui.fullscreenSuppressed = avoid && env !== 'normal'
  if (state.ui.fullscreenSuppressed) {
    state.ui.islandExpanded = false
    keepOpen.value = false
  }
}, { immediate: true })

/** 隐藏捕获关闭时，同步退出“共享视角模拟”。 */
watch(() => props.hideFromCapture, hidden => {
  if (!hidden) captureMirrorMode.value = false
})

function triggerTranslateDemo(): void {
  if (translateDemoTimer !== null) {
    window.clearTimeout(translateDemoTimer)
    translateDemoTimer = null
  }
  deskEnv.value = 'normal'
  state.ui.islandExpanded = false
  translatingDemo.value = true
  state.ui.translationPending = 1
  translateDemoTimer = window.setTimeout(() => {
    state.ui.translationPending = 0
    translatingDemo.value = false
    emit('trigger-feedback', 'translated')
    translateDemoTimer = null
  }, 1400)
}

function triggerFilesDemo(): void {
  deskEnv.value = 'normal'
  if (!shelf.entries.length) {
    loadBrowserShelfSamples(makeShelfSamples())
  }
  keepOpen.value = true
  state.ui.islandPage = 'files'
  state.ui.panel = 'files'
  state.ui.islandExpanded = true
  pushToast('已载入示例文件到活体灵动岛，可直接点开预览或移出', 'info')
}

function togglePauseDemo(): void {
  deskEnv.value = 'normal'
  void setPaused(!state.ui.paused)
}

function toggleExpanded(): void {
  deskEnv.value = 'normal'
  keepOpen.value = !state.ui.islandExpanded
  state.ui.islandExpanded = !state.ui.islandExpanded
}

onBeforeUnmount(() => {
  if (translateDemoTimer !== null) window.clearTimeout(translateDemoTimer)
  state.ui.translationPending = 0
  state.ui.fullscreenSuppressed = false
})
</script>

<template>
  <div class="stage-shell">
    <!-- 顶部栏：环境模拟与停靠标尺 -->
    <div class="stage-header">
      <div class="stage-title-row">
        <span class="stage-live-dot" aria-hidden="true" />
        <span class="stage-heading">活体桌面幕布</span>
        <span class="stage-hint-badge">真实组件 · 可悬停可点击</span>
      </div>
      <div class="stage-env-bar">
        <div class="mini-seg" role="group" aria-label="模拟窗口状态">
          <button type="button" :class="{ 'is-on': deskEnv === 'normal' }" @click="deskEnv = 'normal'">桌面</button>
          <button type="button" :class="{ 'is-on': deskEnv === 'maximized' }" @click="deskEnv = 'maximized'">最大化</button>
          <button type="button" :class="{ 'is-on': deskEnv === 'fullscreen' }" @click="deskEnv = 'fullscreen'">全屏</button>
        </div>
        <div class="mini-seg" role="group" aria-label="幕布底色">
          <button type="button" :class="{ 'is-on': deskBackdrop === 'paper' }" @click="deskBackdrop = 'paper'">昼</button>
          <button type="button" :class="{ 'is-on': deskBackdrop === 'night' }" @click="deskBackdrop = 'night'">夜</button>
          <button type="button" :class="{ 'is-on': deskBackdrop === 'grid' }" @click="deskBackdrop = 'grid'">网</button>
        </div>
      </div>
    </div>

    <!-- 物理停靠标尺轨道 -->
    <div class="anchor-rail" role="radiogroup" aria-label="灵动岛停靠位置">
      <span class="rail-label">停靠轨</span>
      <div class="rail-track">
        <div class="rail-thumb" :style="{ transform: `translateX(${anchorRailIndex * 100}%)` }" aria-hidden="true" />
        <button
          v-for="item in anchorOptions"
          :key="item.id"
          type="button"
          class="rail-stop"
          :class="{ 'is-active': props.anchor === item.id }"
          :aria-checked="props.anchor === item.id"
          role="radio"
          @click="emit('update:anchor', item.id)"
        >
          <span class="rail-code">{{ item.code }}</span>
          <span>{{ item.label }}</span>
        </button>
      </div>
    </div>

    <!-- 模拟桌面屏幕框 -->
    <div
      class="desk-viewport"
      :class="[`bg-${deskBackdrop}`, { 'is-dark-theme': isDarkTheme(), 'is-fullscreen': deskEnv !== 'normal' }]"
    >
      <!-- 模拟全屏或最大化窗口边框提示 -->
      <div v-if="deskEnv !== 'normal'" class="mock-window-banner" aria-hidden="true">
        <span>{{ deskEnv === 'fullscreen' ? '全屏应用演示中 · 鼠标移到顶部细线即可唤出' : '窗口已最大化 · 鼠标移入细线可展开' }}</span>
      </div>

      <!-- 捕获隐藏模式下的共享视角切换条 -->
      <div v-if="props.hideFromCapture" class="capture-banner">
        <span>截图与共享隐藏已启用</span>
        <button type="button" @click="captureMirrorMode = !captureMirrorMode">
          {{ captureMirrorMode ? '切回本机视角' : '预览共享方视角' }}
        </button>
      </div>

      <!-- 真实灵动岛停靠容器 -->
      <div class="island-dock-lane" :style="{ justifyContent: stageJustify }">
        <div class="island-live-wrap" :class="{ 'is-cloaked': props.hideFromCapture && captureMirrorMode }">
          <IslandEntry
            :preview-feedback="props.feedbackEvent"
            :preview-keep-open="keepOpen"
            :preview-telemetry="props.telemetryActive ? props.telemetrySnapshot : null"
          />
        </div>
        <div v-if="props.hideFromCapture && captureMirrorMode" class="cloak-note" role="status">
          共享屏幕 / 截图视角下灵动岛不可见，本机仍可正常使用
        </div>
      </div>

      <!-- 桌面底部装饰：微缩状态提示 -->
      <div class="desk-floor">
        <label class="pin-open">
          <input v-model="keepOpen" type="checkbox" @change="state.ui.islandExpanded = keepOpen" />
          <span>锁定展开</span>
        </label>
        <span class="desk-tip">点击胶囊或悬停即可操作真实面板</span>
      </div>
    </div>

    <!-- 情境触发微动键（Cue Pads） -->
    <div class="cue-deck">
      <div class="cue-head">
        <span>即时情境测试垫</span>
        <button type="button" class="cue-toggle-btn" @click="toggleExpanded">
          {{ state.ui.islandExpanded ? '收起胶囊' : '展开面板' }}
        </button>
      </div>
      <div class="cue-grid">
        <button type="button" class="cue-pad" @click="emit('replay-motion')">
          <span class="cue-led is-accent" />
          <span class="cue-name">演示开合</span>
          <span class="cue-sub">{{ MOTION_LABELS[currentMotion] }}</span>
        </button>
        <button
          type="button"
          class="cue-pad"
          :class="{ 'is-armed': props.telemetryActive }"
          @click="emit('toggle-telemetry')"
        >
          <span class="cue-led" :class="{ 'is-on': props.telemetryActive }" />
          <span class="cue-name">微读数脉动</span>
          <span class="cue-sub">{{ props.telemetryActive ? '实时波动中' : '注入模拟流' }}</span>
        </button>
        <button
          type="button"
          class="cue-pad"
          :class="{ 'is-armed': translatingDemo }"
          @click="triggerTranslateDemo"
        >
          <span class="cue-led" :class="{ 'is-on': translatingDemo }" />
          <span class="cue-name">翻译任务流</span>
          <span class="cue-sub">{{ translatingDemo ? '译文中…' : '工作→完成' }}</span>
        </button>
        <button type="button" class="cue-pad" @click="emit('trigger-feedback', 'copied')">
          <span class="cue-led" />
          <span class="cue-name">复制反馈</span>
          <span class="cue-sub">触发结果态</span>
        </button>
        <button
          type="button"
          class="cue-pad"
          :class="{ 'is-armed': state.ui.paused }"
          @click="togglePauseDemo"
        >
          <span class="cue-led" :class="{ 'is-warn': state.ui.paused }" />
          <span class="cue-name">{{ state.ui.paused ? '已暂停记录' : '暂停剪贴板' }}</span>
          <span class="cue-sub">双向同步开关</span>
        </button>
        <button type="button" class="cue-pad" @click="triggerFilesDemo">
          <span class="cue-led" />
          <span class="cue-name">中转文件</span>
          <span class="cue-sub">载入并展开</span>
        </button>
      </div>
    </div>

    <!-- 实时参数遥测条 -->
    <div class="telemetry-strip" aria-label="当前参数摘要">
      <div class="t-cell">
        <span class="t-k">主题</span>
        <span class="t-v">{{ THEME_LABELS[state.settings.theme] }}</span>
      </div>
      <div class="t-cell">
        <span class="t-k">动效</span>
        <span class="t-v">{{ MOTION_LABELS[currentMotion] }} · {{ MOTION_TIMINGS[currentMotion].expandMs }}ms</span>
      </div>
      <div class="t-cell">
        <span class="t-k">缩放</span>
        <span class="t-v">{{ props.scale }}%</span>
      </div>
      <div class="t-cell">
        <span class="t-k">光轨</span>
        <span class="t-v">{{ state.ui.islandAppearance.webgl && !state.ui.islandReducedMotion ? '开启' : '关闭' }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.stage-shell {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
}

.stage-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}

.stage-title-row {
  display: flex;
  align-items: center;
  gap: 7px;
}

.stage-live-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--deck-led-ok);
  box-shadow: 0 0 8px var(--deck-led-ok);
}

.stage-heading {
  font-size: 13px;
  font-weight: 700;
}

.stage-hint-badge {
  font-size: 10.5px;
  color: var(--deck-ink-2);
  padding: 1px 7px;
  border-radius: 99px;
  background: var(--deck-well);
  border: 1px solid var(--deck-line);
}

.stage-env-bar {
  display: flex;
  align-items: center;
  gap: 6px;
}

.mini-seg {
  display: inline-flex;
  padding: 2px;
  border-radius: 8px;
  background: var(--deck-well);
  border: 1px solid var(--deck-line);
  gap: 2px;
}

.mini-seg button {
  height: 24px;
  padding: 0 8px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--deck-ink-2);
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}

.mini-seg button.is-on {
  background: var(--deck-key-bg);
  color: var(--deck-ink);
  font-weight: 650;
  box-shadow: var(--deck-key-shadow);
}

/* 停靠标尺轨 */
.anchor-rail {
  display: flex;
  align-items: center;
  gap: 10px;
}

.rail-label {
  font-family: var(--deck-mono);
  font-size: 11px;
  color: var(--deck-ink-2);
  flex: none;
}

.rail-track {
  position: relative;
  flex: 1;
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  padding: 3px;
  border-radius: 10px;
  background: var(--deck-well);
  border: 1px solid var(--deck-line-strong);
  box-shadow: var(--deck-inset-shadow);
}

.rail-thumb {
  position: absolute;
  top: 3px;
  left: 3px;
  width: calc((100% - 6px) / 3);
  height: calc(100% - 6px);
  border-radius: 7px;
  background: var(--deck-key-bg);
  box-shadow: var(--deck-key-shadow);
  transition: transform 200ms cubic-bezier(0.22, 1, 0.36, 1);
  pointer-events: none;
}

.rail-stop {
  position: relative;
  z-index: 1;
  height: 28px;
  border: 0;
  background: transparent;
  color: var(--deck-ink-2);
  font: inherit;
  font-size: 11.5px;
  font-weight: 550;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  cursor: pointer;
}

.rail-stop.is-active {
  color: var(--deck-ink);
  font-weight: 700;
}

.rail-code {
  font-family: var(--deck-mono);
  font-size: 10px;
  color: var(--deck-accent);
}

/* 模拟桌面屏幕 */
.desk-viewport {
  position: relative;
  flex: 1;
  min-height: 290px;
  border-radius: 16px;
  border: 1px solid var(--deck-line-strong);
  box-shadow: var(--deck-inset-shadow);
  overflow: auto;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  padding: 14px 16px 12px;
  transition: background 220ms ease;
}

.desk-viewport.bg-paper {
  background:
    radial-gradient(circle at 80% 20%, rgba(242, 78, 30, 0.12), transparent 45%),
    radial-gradient(circle at 18% 85%, rgba(56, 132, 255, 0.14), transparent 50%),
    linear-gradient(155deg, #f3f1ea 0%, #dcdad0 100%);
  color: #24262c;
}

.desk-viewport.bg-night {
  background:
    radial-gradient(circle at 22% 16%, rgba(88, 130, 255, 0.22), transparent 48%),
    radial-gradient(circle at 82% 84%, rgba(242, 78, 30, 0.16), transparent 52%),
    linear-gradient(160deg, #111622 0%, #1a2130 100%);
  color: #dfe6f3;
}

.desk-viewport.bg-grid {
  background:
    radial-gradient(rgba(128, 134, 148, 0.32) 1px, transparent 1px) 0 0 / 18px 18px,
    linear-gradient(180deg, var(--deck-well) 0%, var(--deck-chassis) 100%);
  color: var(--deck-ink-2);
}

.mock-window-banner {
  position: absolute;
  inset: 10px;
  border-radius: 10px;
  border: 1px dashed currentColor;
  opacity: 0.35;
  pointer-events: none;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11.5px;
  padding-top: 32px;
}

.capture-banner {
  position: absolute;
  top: 10px;
  right: 12px;
  z-index: 3;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 4px 8px;
  border-radius: 8px;
  background: rgba(18, 20, 24, 0.8);
  color: #f8fafc;
  font-size: 11px;
  backdrop-filter: blur(6px);
}

.capture-banner button {
  border: 1px solid rgba(255, 255, 255, 0.26);
  background: rgba(255, 255, 255, 0.14);
  color: #ffffff;
  border-radius: 6px;
  padding: 2px 7px;
  font: inherit;
  font-size: 10.5px;
  cursor: pointer;
}

.island-dock-lane {
  position: relative;
  z-index: 2;
  display: flex;
  align-items: flex-start;
  width: 100%;
  min-height: 220px;
  padding-top: 2px;
  transition: justify-content 160ms ease;
}

.island-live-wrap {
  max-width: 100%;
  transition: opacity 200ms ease, filter 200ms ease;
}

.island-live-wrap.is-cloaked {
  opacity: 0.1;
  filter: grayscale(1) blur(1px);
  pointer-events: none;
}

.cloak-note {
  position: absolute;
  top: 56px;
  left: 50%;
  transform: translateX(-50%);
  padding: 6px 12px;
  border-radius: 9px;
  border: 1px dashed var(--deck-accent);
  background: var(--deck-panel);
  color: var(--deck-ink);
  font-size: 11.5px;
  white-space: nowrap;
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.14);
}

.desk-floor {
  position: relative;
  z-index: 1;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding-top: 8px;
  font-size: 11px;
  opacity: 0.85;
}

.pin-open {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  cursor: pointer;
  font-weight: 550;
}

.pin-open input {
  accent-color: var(--deck-accent);
}

.desk-tip {
  opacity: 0.78;
}

/* 情境触发垫 */
.cue-deck {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.cue-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11.5px;
  font-weight: 650;
  color: var(--deck-ink-2);
}

.cue-toggle-btn {
  border: 1px solid var(--deck-line-strong);
  background: var(--deck-key-bg);
  color: var(--deck-ink);
  border-radius: 7px;
  padding: 3px 9px;
  font: inherit;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  box-shadow: var(--deck-key-shadow);
}

.cue-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 7px;
}

.cue-pad {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
  padding: 8px 10px;
  border-radius: 10px;
  border: 1px solid var(--deck-line-strong);
  background: var(--deck-key-bg);
  color: var(--deck-ink);
  text-align: left;
  cursor: pointer;
  box-shadow: var(--deck-key-shadow);
  transition: transform 80ms ease, border-color 140ms ease;
}

.cue-pad:hover {
  border-color: var(--deck-accent);
}

.cue-pad:active {
  transform: translateY(1px);
  box-shadow: var(--deck-key-active-shadow);
}

.cue-pad.is-armed {
  border-color: var(--deck-accent);
  background: var(--deck-panel);
}

.cue-led {
  position: absolute;
  top: 7px;
  right: 7px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--deck-line-strong);
}

.cue-led.is-accent {
  background: var(--deck-accent);
}

.cue-led.is-on {
  background: var(--deck-led-ok);
  box-shadow: 0 0 7px var(--deck-led-ok);
}

.cue-led.is-warn {
  background: var(--deck-led-warn);
  box-shadow: 0 0 7px var(--deck-led-warn);
}

.cue-name {
  font-size: 12px;
  font-weight: 650;
}

.cue-sub {
  font-size: 10.5px;
  color: var(--deck-ink-2);
}

/* 遥测参数条 */
.telemetry-strip {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 6px;
  padding: 8px 10px;
  border-radius: 10px;
  background: var(--deck-well);
  border: 1px solid var(--deck-line);
}

.t-cell {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.t-k {
  font-family: var(--deck-mono);
  font-size: 9.5px;
  color: var(--deck-ink-3);
}

.t-v {
  font-family: var(--deck-mono);
  font-size: 11px;
  font-weight: 650;
  color: var(--deck-ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

@media (max-width: 560px) {
  .cue-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .telemetry-strip {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
</style>
