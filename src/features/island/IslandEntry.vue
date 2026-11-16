<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import ClipboardPopover from '@/features/clipboard/ClipboardPopover.vue'
import TranslationPanel from '@/features/translation/TranslationPanel.vue'
import { invokeHost, resizeBar } from '@/lib/desktop/desktop'
import { pushToast, copyClip, iconFor, launchApp, openSettings, setCategory, state, visibleApps } from '@/store'

const element = ref<HTMLElement | null>(null)
const expanded = computed(() => state.ui.islandExpanded)
const peeking = ref(false)
const attentionPreview = ref(false)
const hovering = ref(false)
const composing = ref(false)
const pressed = ref(false)
const content = ref<HTMLElement | null>(null)
const contentHeight = ref(330)
const availableWidth = ref(window.screen.availWidth)
const revealed = computed(() => expanded.value || peeking.value || attentionPreview.value)
const page = computed(() => state.ui.islandPage)
const recentClips = computed(() => state.clips.slice(0, 5))
const appearance = computed(() => state.ui.islandAppearance)
const scale = computed(() => appearance.value.scale / 100)
const expandedWidth = computed(() => Math.max(260, Math.min(720 * scale.value, availableWidth.value - 24)))
const visibleWidth = computed(() => expanded.value ? expandedWidth.value : (revealed.value ? 184 : 72) * scale.value)
const visibleHeight = computed(() => expanded.value
  ? Math.min((118 + contentHeight.value) * scale.value, window.screen.availHeight - 36)
  : (revealed.value ? 36 : 5) * scale.value)
const bounds = computed(() => ({
  width: Math.max(140, visibleWidth.value),
  height: Math.max(14, visibleHeight.value)
}))
const rootStyle = computed(() => ({
  width: `${bounds.value.width}px`,
  height: `${bounds.value.height}px`,
  '--island-tint': appearance.value.tint || 'var(--accent)',
  '--island-opacity': `${appearance.value.opacity}%`,
  '--content-width': `${expandedWidth.value}px`,
  '--toolbar-target': `${Math.max(30, 36 * scale.value)}px`
}))
const anchors = [
  { value: 'left', label: '顶部左侧', icon: 'align-left' },
  { value: 'center', label: '顶部居中', icon: 'align-center' },
  { value: 'right', label: '顶部右侧', icon: 'align-right' }
] as const
let hoverTimer: ReturnType<typeof setTimeout> | undefined
let leaveTimer: ReturnType<typeof setTimeout> | undefined
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

function expand(value: boolean, focus = false): void {
  cancelHover()
  clearTimeout(leaveTimer)
  peeking.value = value
  visibilityIntent = visibilityIntent.catch(() => {}).then(async () => {
    if (!disposed) await invokeHost('set_island_expanded', { expanded: value, focus })
  }).catch(() => { peeking.value = false })
}

function activateCapsule(): void {
  if (attentionPreview.value && !expanded.value) {
    void openWechat()
    return
  }
  expand(!expanded.value, true)
}

function enter(): void {
  hovering.value = true
  availableWidth.value = window.screen.availWidth
  cancelHover()
  clearTimeout(leaveTimer)
  if (expanded.value) return
  peeking.value = true
  hoverTimer = setTimeout(() => {
    if (hovering.value) expand(true)
  }, state.ui.islandHoverMs)
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
  cancelHover()
  clearTimeout(leaveTimer)
  leaveTimer = setTimeout(() => {
    if (hovering.value || editing()) return
    if (expanded.value) expand(false)
    else peeking.value = false
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

watch(expanded, value => {
  if (!value) {
    cancelHover()
    clearTimeout(leaveTimer)
    peeking.value = false
    composing.value = false
    pressed.value = false
  }
})

watch(() => state.ui.wechatEvent, (event, previous) => {
  if (event === previous || !event || expanded.value || state.ui.fullscreenSuppressed) return
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
}

onMounted(() => {
  observer = new ResizeObserver(queueGeometry)
  contentObserver = new ResizeObserver(measureContent)
  if (element.value) observer.observe(element.value)
  if (content.value) contentObserver.observe(content.value)
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
  window.removeEventListener('pointerup', handlePointerUp)
  window.removeEventListener('pointercancel', handlePointerUp)
})
</script>

<template>
  <section ref="element" class="island-hit-area"
    :class="{ expanded, tucked: !revealed, reduced: state.ui.islandReducedMotion, suppressed: state.ui.fullscreenSuppressed }"
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
      <Icon v-else name="grid" :size="16" />
      <Icon name="chevron-down" :size="12" class="arrow" />
    </button>

    <div class="expanded-content" :inert="!expanded">
      <header class="toolbar">
        <nav aria-label="灵动岛功能">
          <button type="button" title="应用" aria-label="应用" :aria-pressed="page === 'applications'" :class="{ selected: page === 'applications' }" @click="selectPage('applications')"><Icon name="grid" :size="18" /></button>
          <button type="button" title="剪贴板" aria-label="剪贴板" :aria-pressed="page === 'clipboard'" :class="{ selected: page === 'clipboard' }" @click="selectPage('clipboard')"><Icon name="clipboard" :size="18" /></button>
          <button type="button" title="翻译" aria-label="翻译" :aria-pressed="page === 'translation'" :class="{ selected: page === 'translation' }" @click="selectPage('translation')"><Icon name="translate" :size="18" /></button>
        </nav>
        <div class="anchor-actions" role="group" aria-label="顶部停靠位置">
          <button v-for="anchor in anchors" :key="anchor.value" class="icon-action" type="button"
            :title="anchor.label" :aria-label="anchor.label" :aria-pressed="state.ui.islandAnchor === anchor.value"
            :class="{ selected: state.ui.islandAnchor === anchor.value }"
            @click="invokeHost('set_island_anchor', { anchor: anchor.value })"><Icon :name="anchor.icon" :size="16" /></button>
        </div>
        <button class="icon-action" type="button" title="独立翻译窗口" aria-label="独立翻译窗口" @click="openSeparateTranslation"><Icon name="external" :size="16" /></button>
        <button class="icon-action" type="button" title="设置" aria-label="设置" @click="openSettings()"><Icon name="settings" :size="16" /></button>
        <button class="icon-action" type="button" title="收起" aria-label="收起灵动岛" @click="expand(false)"><Icon name="chevron-down" :size="16" /></button>
      </header>

      <div v-if="state.ui.wechatAttention" class="attention-strip" role="status">
        <span>微信有新提醒</span>
        <button type="button" @click="openWechat">打开微信</button>
        <button type="button" @click="invokeHost('clear_wechat_attention')">清除待关注</button>
      </div>
      <div ref="content" class="island-content">
      <div v-if="page === 'applications'" class="applications">
        <nav class="categories" aria-label="应用分类">
          <button v-for="category in state.categories" :key="category.id" type="button"
            :class="{ selected: state.ui.activeCategoryId === category.id }" @click="setCategory(category.id)">{{ category.name }}</button>
        </nav>
        <div class="columns">
          <div class="app-grid scroll-y">
            <button v-for="app in visibleApps" :key="app.id" class="app-tile" type="button" :title="app.name" :aria-label="app.name" @click="launch(app.id)">
              <img v-if="iconFor(app.target)" :src="iconFor(app.target)" alt="" />
              <span v-else class="fallback" :style="{ background: app.color }">{{ app.initials }}</span>
            </button>
            <p v-if="!visibleApps.length" class="empty">这个分类还没有应用</p>
          </div>
          <aside class="recent">
            <div class="recent-heading"><span>最近复制</span><button type="button" title="全部剪贴板记录" aria-label="全部剪贴板记录" @click="selectPage('clipboard')"><Icon name="chevron-right" :size="16" /></button></div>
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
  --geometry-time: 245ms;
  --geometry-ease: cubic-bezier(.22, .75, .25, 1);
  transition: width var(--geometry-time) var(--geometry-ease), height var(--geometry-time) var(--geometry-ease), opacity 125ms;
}
.island-hit-area:not(.expanded) { --geometry-time: 190ms; }
.island-hit-area.reduced { --geometry-time: 0ms; }
.island-hit-area.suppressed { opacity: 0; pointer-events: none; }
.attention-dot { position: absolute; right: 8px; top: 0; width: 5px; height: 5px; z-index: 1; background: #46c88a; border-radius: 50%; }
.expanded > .attention-dot, .island:not(.tucked) > .attention-dot { top: 9px; }
.attention-strip { display: flex; align-items: center; gap: 8px; min-height: 30px; color: var(--text); font-size: 12px; }
.attention-strip button { padding: 2px 5px; border-radius: 5px; background: var(--hover); border: 1px solid var(--line); }
.island-content { width: 100%; }

.tuck-trigger { position: absolute; inset: 0; z-index: 1; width: 100%; height: 100%; padding: 0; background: transparent; border: 0; }
.island {
  position: relative;
  overflow: hidden;
  border-radius: 0 0 20px 20px;
  flex: none;
  background: color-mix(in srgb, color-mix(in srgb, var(--surface) 92%, var(--island-tint)) var(--island-opacity), transparent);
  color: var(--text);
  box-shadow: inset 0 -1px 0 var(--line-strong);
  transition: width var(--geometry-time) var(--geometry-ease), height var(--geometry-time) var(--geometry-ease), border-radius var(--geometry-time) var(--geometry-ease);
}
.island.glass { background: linear-gradient(155deg, color-mix(in srgb, var(--surface-2) 85%, var(--island-tint)), color-mix(in srgb, var(--surface) var(--island-opacity), transparent)); box-shadow: inset 0 1px 1px var(--line-strong); }
.island.tucked { border-radius: 3px; box-shadow: inset 0 0 0 1px var(--line-strong); }
.island.tucked .capsule { visibility: hidden; }
.island.expanded { border-radius: 0 0 32px 32px; }
button { color: inherit; cursor: pointer; }
.capsule { display: flex; align-items: center; justify-content: center; gap: 9px; height: 36px; width: 184px; margin: 0 auto; border: 0; background: transparent; white-space: nowrap; }
.arrow { color: var(--text-3); transition: transform 240ms; }
.expanded .arrow { transform: rotate(180deg); }
.expanded-content { width: var(--content-width); max-height: calc(100% - 36px); overflow: hidden; padding: 0 20px 12px; opacity: 0; transform: translateY(-8px); transition: opacity 160ms ease, transform 220ms ease; }
.expanded .expanded-content { overflow-y: auto; opacity: 1; transform: translateY(0); transition-delay: 80ms; }
.toolbar { display: flex; height: 44px; align-items: center; gap: 8px; border-bottom: 1px solid var(--line); margin-bottom: 12px; }
.toolbar nav { display: flex; gap: 4px; flex: 1; }
.toolbar nav button { display: grid; place-items: center; width: var(--toolbar-target); height: var(--toolbar-target); padding: 0; }
.toolbar button, .categories button { border: 0; background: transparent; padding: 7px 11px; border-radius: 8px; font-size: 12px; }
.toolbar button, .categories button, .app-tile { transition: background 110ms ease, color 110ms ease; }
.toolbar button:hover, .categories button:hover { background: var(--hover); }
.toolbar button:active, .categories button:active { background: var(--active); }
.toolbar button.selected, .categories button.selected { background: var(--accent-weak); color: var(--accent); }
.anchor-actions { display: flex; gap: 2px; }
.toolbar .icon-action { display: grid; place-items: center; width: 32px; height: 32px; padding: 0; }
.categories { display: flex; flex-wrap: wrap; gap: 4px; max-height: 70px; overflow-y: auto; margin-bottom: 12px; }
.columns { display: grid; grid-template-columns: minmax(0,1fr) 216px; gap: 16px; height: 265px; }
.app-grid { display: grid; grid-template-columns: repeat(4,minmax(0,1fr)); align-content: start; overflow-y: auto; gap: 6px; }
.app-tile { display: flex; align-items: center; justify-content: center; min-width: 0; min-height: 64px; border: 0; background: transparent; border-radius: 12px; padding: 12px 5px; }
.app-tile:hover { background: var(--hover); }
.app-tile:active { background: var(--active); }
.app-tile img, .fallback { width: 34px; height: 34px; object-fit: contain; }
.fallback { display: grid; place-items: center; border-radius: 9px; color: white; }
.recent { border-left: 1px solid var(--line); padding-left: 16px; min-width: 0; }
.recent-heading { display: flex; align-items: center; justify-content: space-between; margin: 0 0 10px; font-size: 12px; color: var(--text-2); }
.recent-heading button { border: 0; background: transparent; font-size: 11px; color: var(--accent); }
.clip { display: flex; width: 100%; align-items: center; gap: 8px; margin-bottom: 5px; padding: 10px; background: var(--surface-2); border: 1px solid var(--line); border-radius: 9px; text-align: left; }
.clip:hover { background: var(--surface-3); }
.clip span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
.empty { grid-column: 1/-1; color: var(--text-3); font-size: 12px; line-height: 1.8; padding: 12px 4px; }
.paused-note { color: var(--text-3); font-size: 11px; }
.feature-pane { max-height: 440px; overflow: auto; }
.feature-pane :deep(.pop), .feature-pane :deep(.panel) { width: 100%; max-height: none; border: 0; border-radius: 0; box-shadow: none; }
button:focus-visible, select:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
@media (max-width: 560px) { .columns { grid-template-columns: minmax(0,1fr) 160px; } .app-grid { grid-template-columns: repeat(3,minmax(0,1fr)); } .expanded-content { padding-right: 12px; padding-left: 12px; } }
.island-hit-area.reduced .expanded-content, .island-hit-area.reduced .arrow, .island-hit-area.reduced button { transition: none; }
@media (prefers-reduced-motion: reduce) { .island-hit-area, .island, .expanded-content, .arrow { transition: none; } }
</style>
