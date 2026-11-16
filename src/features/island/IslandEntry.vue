<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import ClipboardPopover from '@/features/clipboard/ClipboardPopover.vue'
import TranslationPanel from '@/features/translation/TranslationPanel.vue'
import { invokeHost, resizeBar } from '@/lib/desktop/desktop'
import { pushToast, copyClip, iconFor, launchApp, openSettings, setCategory, state, visibleApps } from '@/store'

const element = ref<HTMLElement | null>(null)
const hostExpanded = computed(() => state.ui.islandExpanded)
const hoverClosing = ref(false)
const expanded = computed(() => hostExpanded.value && !hoverClosing.value)
const collapseMs = 235
const attentionPreview = ref(false)
const hovering = ref(false)
const composing = ref(false)
const pressed = ref(false)
const content = ref<HTMLElement | null>(null)
const chrome = ref<HTMLElement | null>(null)
const contentHeight = ref(330)
const chromeHeight = ref(56)
const availableWidth = ref(window.screen.availWidth)
const revealed = computed(() => expanded.value || attentionPreview.value)
const page = computed(() => state.ui.islandPage)
const recentClips = computed(() => state.clips.slice(0, 5))
const appearance = computed(() => state.ui.islandAppearance)
const scale = computed(() => appearance.value.scale / 100)
const expandedWidth = computed(() => Math.max(260, Math.min(656 * scale.value, availableWidth.value - 24)))
const tuckedWidth = computed(() => 80 * scale.value)
const visibleWidth = computed(() => {
  if (expanded.value) return expandedWidth.value
  if (attentionPreview.value) return 156 * scale.value
  return tuckedWidth.value
})
const visibleHeight = computed(() => expanded.value
  ? Math.min(46 + chromeHeight.value + contentHeight.value * scale.value, window.screen.availHeight - 36)
  : (attentionPreview.value ? 36 : 5) * scale.value)
const bounds = computed(() => ({
  width: Math.max(140, visibleWidth.value),
  height: Math.max(14, visibleHeight.value)
}))
const rootStyle = computed(() => ({
  width: `${bounds.value.width}px`,
  height: `${bounds.value.height}px`,
  '--island-tint': appearance.value.tint || 'var(--accent)',
  '--island-opacity': `${appearance.value.opacity}%`,
  '--collapse-time': `${collapseMs}ms`,
  '--content-width': `${expandedWidth.value}px`,
  '--toolbar-target': `${Math.max(30, 36 * scale.value)}px`
}))
const sections = [
  { value: 'applications', label: '应用', icon: 'grid' },
  { value: 'clipboard', label: '剪贴板', icon: 'clipboard' },
  { value: 'translation', label: '翻译', icon: 'translate' }
] as const
const anchors = [
  { value: 'left', label: '顶部左侧', icon: 'align-left' },
  { value: 'center', label: '顶部居中', icon: 'align-center' },
  { value: 'right', label: '顶部右侧', icon: 'align-right' }
] as const
let hoverTimer: ReturnType<typeof setTimeout> | undefined
let leaveTimer: ReturnType<typeof setTimeout> | undefined
let closeTimer: ReturnType<typeof setTimeout> | undefined
let closeCommitted = false
let reenterAfterClose = false
let attentionTimer: ReturnType<typeof setTimeout> | undefined
let observer: ResizeObserver | undefined
let contentObserver: ResizeObserver | undefined
let frame = 0
let sending = false
let disposed = false
let queued: [number, number] | undefined
let sent: [number, number] | undefined
let visibilityIntent = Promise.resolve()

function cancelHover(): void { clearTimeout(hoverTimer) }

function cancelHoverClose(): void {
  clearTimeout(closeTimer)
  hoverClosing.value = false
}

function expand(value: boolean, focus = false): void {
  if (value) {
    closeCommitted = false
    reenterAfterClose = false
    cancelHoverClose()
  } else {
    if (closeCommitted) return
    clearTimeout(closeTimer)
    hoverClosing.value = true
    closeCommitted = true
  }
  cancelHover()
  clearTimeout(leaveTimer)
  visibilityIntent = visibilityIntent.catch(() => {}).then(async () => {
    if (!disposed) await invokeHost('set_island_expanded', { expanded: value, focus })
  }).catch(error => {
    closeCommitted = false
    reenterAfterClose = false
    cancelHoverClose()
    pushToast(String(error), 'error')
  })
}

function activateCapsule(): void {
  if (attentionPreview.value && !hostExpanded.value) {
    void openWechat()
    return
  }
  expand(!expanded.value, true)
}

function startHoverClose(): void {
  if (!hostExpanded.value || hoverClosing.value) return
  if (state.ui.islandReducedMotion || window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    expand(false)
    return
  }

  // 先收起视觉外壳。鼠标有机会在实际关闭（会取消翻译）之前反向展开。
  hoverClosing.value = true
  closeTimer = setTimeout(() => {
    if (!hoverClosing.value) return
    if (hovering.value || editing()) cancelHoverClose()
    else expand(false)
  }, collapseMs + 34)
}

function scheduleHoverOpen(): void {
  hoverTimer = setTimeout(() => {
    if (hovering.value && !state.ui.fullscreenSuppressed) expand(true)
  }, state.ui.islandHoverMs)
}

function enter(): void {
  hovering.value = true
  availableWidth.value = window.screen.availWidth
  cancelHover()
  clearTimeout(leaveTimer)
  if (closeCommitted) {
    // 提交关闭后已经无法撤回，请等宿主确认再按正常悬停处理。
    reenterAfterClose = true
    return
  }
  if (hoverClosing.value) cancelHoverClose()
  if (!hostExpanded.value) scheduleHoverOpen()
}

function editing(): boolean {
  const active = document.activeElement
  return composing.value || pressed.value ||
    (active instanceof HTMLElement && !!element.value?.contains(active) &&
      (active instanceof HTMLTextAreaElement || active instanceof HTMLSelectElement ||
       active.isContentEditable ||
       (active instanceof HTMLInputElement && !['button', 'checkbox', 'radio', 'range', 'submit'].includes(active.type))))
}

function leave(): void {
  hovering.value = false
  reenterAfterClose = false
  cancelHover()
  clearTimeout(leaveTimer)
  leaveTimer = setTimeout(() => {
    if (hovering.value || editing()) return
    startHoverClose()
  }, state.ui.islandLeaveMs)
}

function handlePointerUp(): void {
  pressed.value = false
  if (!hovering.value) leave()
}

function focusout(): void {
  queueMicrotask(() => {
    if (!document.hasFocus()) composing.value = false
    if (!hovering.value) leave()
  })
}

watch(hostExpanded, value => {
  const shouldReopen = !value && hovering.value && reenterAfterClose
  closeCommitted = false
  reenterAfterClose = false
  cancelHoverClose()
  if (value) {
    attentionPreview.value = false
    clearTimeout(attentionTimer)
  } else {
    cancelHover()
    clearTimeout(leaveTimer)
    composing.value = false
    pressed.value = false
    if (shouldReopen) scheduleHoverOpen()
  }
})

watch(page, () => {
  if (!closeCommitted) cancelHoverClose()
})

watch(() => state.ui.wechatEvent, (event, previous) => {
  if (event === previous || !event || hostExpanded.value || state.ui.fullscreenSuppressed) return
  attentionPreview.value = true
  clearTimeout(attentionTimer)
  attentionTimer = setTimeout(() => { attentionPreview.value = false }, 4500)
})
watch(() => state.ui.wechatAttention, active => {
  if (!active) attentionPreview.value = false
})

async function selectPage(value: typeof page.value): Promise<void> {
  try { await invokeHost('select_island_page', { page: value, focus: true }) }
  catch (error) { pushToast(String(error), 'error') }
}

async function openWechat(): Promise<void> {
  try { await invokeHost('open_existing_wechat') }
  catch (error) { pushToast(String(error), 'error') }
}

async function openSeparateTranslation(): Promise<void> {
  try { await invokeHost('open_translation_window') }
  catch (error) { pushToast(String(error), 'error') }
}

async function launch(id: string): Promise<void> {
  const application = state.apps.find(item => item.id === id)
  if (!application) return
  await launchApp(application)
  expand(false)
}

function keydown(event: KeyboardEvent): void {
  if (event.isComposing || event.keyCode === 229) return
  if (event.key === 'Escape') { event.stopPropagation(); expand(false) }
}

function queueGeometry(): void {
  if (frame || disposed) return
  frame = requestAnimationFrame(() => {
    frame = 0
    const rect = element.value?.getBoundingClientRect()
    if (!rect) return
    queued = [Math.ceil(rect.width), Math.ceil(rect.height)]
    void sendGeometry()
  })
}

async function sendGeometry(): Promise<void> {
  if (sending || !queued || disposed) return
  const next = queued
  queued = undefined
  if (sent && next[0] === sent[0] && next[1] === sent[1]) return
  sending = true
  try { await resizeBar(...next); sent = next }
  catch { sent = undefined }
  finally {
    sending = false
    if (queued) void sendGeometry()
  }
}

function measureContent(): void {
  const height = content.value?.getBoundingClientRect().height
  if (height) contentHeight.value = Math.min(478, Math.ceil(height))
  const header = chrome.value?.getBoundingClientRect().height
  if (header) chromeHeight.value = Math.ceil(header)
}

onMounted(() => {
  observer = new ResizeObserver(queueGeometry)
  contentObserver = new ResizeObserver(measureContent)
  if (element.value) observer.observe(element.value)
  if (content.value) contentObserver.observe(content.value)
  if (chrome.value) contentObserver.observe(chrome.value)
  window.addEventListener('pointerup', handlePointerUp)
  window.addEventListener('pointercancel', handlePointerUp)
  queueGeometry()
})

onBeforeUnmount(() => {
  disposed = true
  observer?.disconnect()
  contentObserver?.disconnect()
  cancelAnimationFrame(frame)
  cancelHover()
  clearTimeout(leaveTimer)
  clearTimeout(attentionTimer)
  clearTimeout(closeTimer)
  window.removeEventListener('pointerup', handlePointerUp)
  window.removeEventListener('pointercancel', handlePointerUp)
})
</script>

<template>
  <section ref="element" class="island-hit-area"
    :class="{ expanded, compact: expandedWidth < 560, tucked: !revealed, reduced: state.ui.islandReducedMotion, suppressed: state.ui.fullscreenSuppressed }"
    :style="rootStyle" aria-label="顶部灵动岛" :inert="state.ui.fullscreenSuppressed"
    @mouseenter="enter" @mouseleave="leave" @focusout="focusout" @keydown="keydown"
    @pointerdown="pressed = true" @compositionstart="composing = true" @compositionend="composing = false">
    <button v-if="!revealed" class="tuck-trigger" type="button" aria-label="展开灵动岛" @click="expand(true, true)" />
    <div class="island" :class="{ expanded, tucked: !revealed, glass: appearance.preset === 'glass' }"
      :style="{ width: `${visibleWidth}px`, height: `${visibleHeight}px` }">
    <span v-if="state.ui.wechatAttention" class="attention-dot" aria-label="微信有待关注提醒" />
    <button class="capsule" type="button" :aria-expanded="expanded" :aria-label="attentionPreview && !expanded ? '微信有新提醒，打开微信' : '展开或收起应用面板'"
      :inert="!revealed" @pointerdown="cancelHover" @click="activateCapsule">
      <span v-if="attentionPreview && !expanded">微信有新提醒</span>
      <span v-else class="grip" aria-hidden="true" />
    </button>

    <div class="expanded-content" :inert="!expanded">
      <div ref="chrome" class="island-chrome">
      <header class="toolbar">
        <nav aria-label="灵动岛功能">
          <button v-for="section in sections" :key="section.value" type="button" :title="section.label"
            :aria-label="section.label" :aria-pressed="page === section.value" :class="{ selected: page === section.value }"
            @click="selectPage(section.value)">
            <Icon :name="section.icon" :size="16" /><span>{{ section.label }}</span>
          </button>
        </nav>
        <div class="tools">
          <div class="anchor-actions" role="group" aria-label="顶部停靠位置">
            <button v-for="anchor in anchors" :key="anchor.value" class="icon-action" type="button"
              :title="anchor.label" :aria-label="anchor.label" :aria-pressed="state.ui.islandAnchor === anchor.value"
              :class="{ selected: state.ui.islandAnchor === anchor.value }"
              @click="invokeHost('set_island_anchor', { anchor: anchor.value })"><Icon :name="anchor.icon" :size="14" /></button>
          </div>
          <button class="icon-action" type="button" title="独立翻译窗口" aria-label="独立翻译窗口" @click="openSeparateTranslation"><Icon name="external" :size="15" /></button>
          <button class="icon-action" type="button" title="设置" aria-label="设置" @click="openSettings()"><Icon name="settings" :size="16" /></button>
          <button class="icon-action" type="button" title="收起" aria-label="收起灵动岛" @click="expand(false)"><Icon name="chevron-down" :size="15" /></button>
        </div>
      </header>

      <div v-if="state.ui.wechatAttention" class="attention-strip" role="status">
        <span>微信有新提醒</span>
        <button type="button" @click="openWechat">打开微信</button>
        <button type="button" @click="invokeHost('clear_wechat_attention')">清除待关注</button>
      </div>
      </div>
      <div ref="content" class="island-content">
      <div v-if="page === 'applications'" class="applications">
        <div class="columns">
          <section class="launch-shelf" aria-labelledby="launch-heading">
            <h2 id="launch-heading">启动台</h2>
            <nav class="categories" aria-label="应用分类">
              <button v-for="category in state.categories" :key="category.id" type="button"
                :class="{ selected: state.ui.activeCategoryId === category.id }" @click="setCategory(category.id)">{{ category.name }}</button>
            </nav>
            <div class="app-grid scroll-y">
              <button v-for="app in visibleApps" :key="app.id" class="app-tile" type="button" :title="app.name" :aria-label="app.name" @click="launch(app.id)">
                <img v-if="iconFor(app.target)" :src="iconFor(app.target)" alt="" />
                <span v-else class="fallback" :style="{ background: app.color }">{{ app.initials }}</span>
                <span class="app-name">{{ app.name }}</span>
              </button>
              <p v-if="!visibleApps.length" class="empty">这个分类还没有应用</p>
            </div>
          </section>
          <aside class="recent">
            <div class="recent-heading"><h2>最近复制</h2><button type="button" title="全部剪贴板记录" aria-label="全部剪贴板记录" @click="selectPage('clipboard')"><Icon name="chevron-right" :size="16" /></button></div>
            <button v-for="clip in recentClips" :key="clip.id" type="button" class="clip" @click="copyClip(clip)">
              <Icon :name="clip.kind === 'image' ? 'image' : 'clipboard'" :size="14" />
              <span>{{ clip.kind === 'image' ? `图片 ${clip.width} × ${clip.height}` : clip.text }}</span>
            </button>
            <p v-if="!recentClips.length" class="empty">新的复制内容会出现在这里</p>
            <span v-if="state.ui.paused" class="paused-note">已暂停记录</span>
          </aside>
        </div>
      </div>
      <div v-else class="feature-pane scroll-y">
        <ClipboardPopover v-if="page === 'clipboard'" />
        <TranslationPanel v-else />
      </div>
      </div>
    </div>
    </div>
  </section>
</template>

<style scoped>
.island-hit-area {
  position: relative; display: flex; justify-content: center; align-items: flex-start;
  --geometry-time: 310ms;
  --geometry-ease: cubic-bezier(.23, 1, .32, 1);
  transition: width var(--geometry-time) var(--geometry-ease), height var(--geometry-time) var(--geometry-ease), opacity 125ms;
}
.island-hit-area:not(.expanded) { --geometry-time: var(--collapse-time); --geometry-ease: cubic-bezier(.36, .15, .38, 1); }
.island-hit-area.reduced { --geometry-time: 0ms; transition: none; }
.island-hit-area.suppressed { opacity: 0; pointer-events: none; }
.attention-dot { position: absolute; right: 8px; top: 0; width: 5px; height: 5px; z-index: 1; background: #46c88a; border-radius: 50%; }
.expanded > .attention-dot, .island:not(.tucked) > .attention-dot { top: 9px; }
.attention-strip { display: flex; align-items: center; gap: 8px; min-height: 30px; color: var(--text); font-size: 12px; }
.attention-strip button { padding: 2px 5px; border-radius: 5px; background: var(--hover); border: 1px solid var(--line); }
.island-chrome { display: flow-root; }
.island-content { width: 100%; }

.tuck-trigger { position: absolute; inset: 0; z-index: 1; width: 100%; height: 100%; padding: 0; background: transparent; border: 0; }
/* 玻璃感来自单层纵深与边缘反光，不依赖 Windows 桌面模糊。 */
.island {
  position: relative;
  overflow: hidden;
  border-radius: 0 0 21px 21px;
  flex: none;
  --glass-base: color-mix(in srgb, var(--surface) 77%, #101d2a);
  --glass-rim: color-mix(in srgb, var(--line-strong) 67%, #637f94);
  --glass-well: rgba(6, 15, 24, .24);
  --glass-sheen: rgba(178, 220, 255, .12);
  background: color-mix(in srgb, var(--surface) var(--island-opacity), transparent);
  color: var(--text);
  box-shadow: inset 0 0 0 1px var(--glass-rim), inset 0 -2px 4px rgba(0, 0, 0, .22);
  transition: width var(--geometry-time) var(--geometry-ease), height var(--geometry-time) var(--geometry-ease), border-radius var(--geometry-time) var(--geometry-ease);
}
:global(:root[data-theme='light'] .island) {
  --glass-base: #e9f3fb;
  --glass-rim: rgba(57, 83, 109, .34);
  --glass-well: rgba(44, 98, 130, .08);
  --glass-sheen: rgba(255, 255, 255, .55);
}
.island.glass {
  background:
    radial-gradient(100% 88% at 50% -38%, var(--glass-sheen), transparent 93%),
    linear-gradient(160deg, color-mix(in srgb, color-mix(in srgb, var(--glass-base) 90%, var(--island-tint)) var(--island-opacity), transparent), color-mix(in srgb, var(--glass-base) var(--island-opacity), transparent));
}
.island.tucked {
  border-radius: 5px;
}
.island::before {
  position: absolute;
  inset: 0 0 auto;
  height: 100%;
  content: '';
  pointer-events: none;
  opacity: 0;
  background: linear-gradient(90deg, transparent, color-mix(in srgb, var(--island-tint) 35%, #d1ebff) 50%, transparent);
  transition: opacity 85ms ease-out;
}
.island.tucked::before { opacity: 1; transition: opacity 95ms ease-in 100ms; }
.island.tucked .capsule { visibility: hidden; }
.island.expanded { border-radius: 0 0 29px 29px; }
button { color: inherit; cursor: pointer; }
.capsule { display: flex; align-items: center; justify-content: center; gap: 9px; height: 30px; width: 156px; margin: 0 auto; border: 0; background: transparent; white-space: nowrap; font-size: 12px; }
.grip { width: 44px; height: 3px; border-radius: 6px; background: var(--text-3); opacity: 0; transition: opacity 85ms ease, background 175ms ease; }
.expanded .grip { opacity: .83; transition-delay: 70ms, 0ms; }
.capsule:hover .grip { background: var(--text); }
.expanded-content { width: var(--content-width); max-height: calc(100% - 30px); overflow: hidden; padding: 0 20px 16px; opacity: 0; transform: translateY(-8px); transition: opacity 85ms ease-in, transform 100ms ease-in; }
.expanded .expanded-content { overflow-y: auto; opacity: 1; transform: translateY(0); transition: opacity 165ms ease-out 68ms, transform 200ms var(--geometry-ease) 62ms; }
.toolbar {
  display: flex; min-height: 44px; flex-wrap: wrap; align-items: center;
  justify-content: space-between; gap: 6px 9px; padding-bottom: 10px; border-bottom: 1px solid var(--line);
  margin-bottom: 14px;
}
.toolbar nav { display: flex; flex-wrap: wrap; gap: 3px; padding: 3px; border-radius: 11px; background: var(--glass-well); box-shadow: inset 0 0 0 1px var(--line); }
.toolbar nav button { display: flex; align-items: center; justify-content: center; gap: 7px; min-height: max(29px, calc(var(--toolbar-target) - 6px)); padding: 4px 11px; white-space: nowrap; font-size: 12px; font-weight: 600; }
.toolbar button, .categories button { border: 0; background: transparent; border-radius: 8px; color: var(--text-2); }
.toolbar button, .categories button, .app-tile, .clip { transition: background 130ms ease, color 130ms ease, transform 170ms var(--geometry-ease); }
.toolbar button:hover, .categories button:hover { background: var(--hover); color: var(--text); }
.toolbar button:active, .categories button:active, .app-tile:active { transform: scale(.96); }
.toolbar nav .selected { color: var(--text); background: color-mix(in srgb, var(--accent-weak) 50%, var(--hover)); box-shadow: inset 0 0 0 1px var(--line-strong); }
.toolbar nav .selected svg { color: var(--accent-hover); }
.tools, .anchor-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 2px; }
.anchor-actions { padding-right: 7px; margin-right: 5px; border-right: 1px solid var(--line-strong); }
.anchor-actions .selected { color: var(--accent-hover); }
.toolbar .icon-action { display: grid; place-items: center; width: 29px; height: 29px; padding: 0; }
.columns { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 36%); gap: 12px; align-items: stretch; }
.launch-shelf { min-width: 0; padding: 3px 4px 0 0; }
h2 { margin: 0; font-size: 14px; line-height: 1.35; font-weight: 650; letter-spacing: .03em; }
.categories { display: flex; flex-wrap: wrap; gap: 4px; max-height: 64px; overflow-y: auto; margin: 9px 0; }
.categories button { padding: 4px 10px; font-size: 12px; }
.categories .selected { background: var(--accent-weak); color: var(--accent-hover); }
.app-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); align-content: start; max-height: 195px; overflow-y: auto; gap: 4px; }
.app-tile { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 5px; min-width: 0; min-height: 75px; border: 0; background: transparent; border-radius: 13px; padding: 7px 4px; }
.app-tile:hover { background: var(--hover); }
.app-tile img, .fallback { width: 33px; height: 33px; object-fit: contain; }
.fallback { display: grid; place-items: center; border-radius: 10px; color: white; font-weight: 700; box-shadow: inset 0 1px 0 rgba(255, 255, 255, .25), inset 0 -12px 16px rgba(11, 22, 55, .23); }
.app-name { max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; color: var(--text-2); }
.recent { min-width: 0; padding: 9px 10px 8px; border-radius: 15px; background: var(--glass-well); box-shadow: inset 0 0 0 1px var(--line); }
.recent-heading { display: flex; align-items: center; justify-content: space-between; gap: 4px; margin: 0 0 5px; }
.recent-heading h2 { font-size: 12px; color: var(--text-2); }
.recent-heading button { display: grid; place-items: center; width: 25px; height: 25px; border-radius: 8px; color: var(--text-2); }
.recent-heading button:hover { background: var(--hover); color: var(--text); }
.clip { display: flex; width: 100%; align-items: center; gap: 7px; padding: 6px; border-radius: 7px; text-align: left; }
.clip + .clip { margin-top: 1px; }
.clip:hover { background: var(--hover); }
.clip svg { flex: none; color: var(--accent-hover); }
.clip span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; }
.empty { grid-column: 1/-1; color: var(--text-2); font-size: 12px; line-height: 1.7; padding: 9px 4px; }
.paused-note { color: var(--text-2); font-size: 11px; }
.feature-pane { max-height: 420px; overflow: auto; }
.feature-pane :deep(.pop), .feature-pane :deep(.panel) { width: 100%; max-height: none; border: 0; border-radius: 0; box-shadow: none; background: transparent; }
button:focus-visible, select:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.compact .columns { grid-template-columns: minmax(0, 1fr); gap: 6px; }
.compact .app-grid { grid-template-columns: repeat(4, minmax(0, 1fr)); }
.compact .expanded-content { padding-right: 12px; padding-left: 12px; }
.island-hit-area.reduced .island::before, .island-hit-area.reduced .expanded-content, .island-hit-area.reduced .grip, .island-hit-area.reduced button { transition: none; }
@media (prefers-reduced-motion: reduce) { .island-hit-area, .island, .island::before, .expanded-content, .grip, button { transition: none; } }
</style>
