<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import ClipboardPopover from '@/features/clipboard/ClipboardPopover.vue'
import TranslationPanel from '@/features/translation/TranslationPanel.vue'
import FileShelf from '@/features/files/FileShelf.vue'
import { addShelfFiles, shelf } from '@/features/files/shelf'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { invokeHost, isDesktop, listenHost, resizeBar, type BarRegion } from '@/lib/desktop/desktop'
import { pushToast, iconFor, launchApp, openSettings, setCategory, state, visibleApps } from '@/store'

const element = ref<HTMLElement | null>(null)
const viewport = ref<HTMLElement | null>(null)
const panel = ref<HTMLElement | null>(null)
const systemDark = window.matchMedia('(prefers-color-scheme: dark)')
const dark = ref(document.documentElement.dataset.theme === 'dark')
const hostExpanded = computed(() => state.ui.islandExpanded)
const suppressed = computed(() => state.ui.fullscreenSuppressed)
const hoverClosing = ref(false)
const expanded = computed(() => hostExpanded.value && !hoverClosing.value && !suppressed.value)
const expandMs = 260
const collapseMs = 200
const geometryMoving = ref(false)
const systemMotion = window.matchMedia('(prefers-reduced-motion: reduce)')
const reducedMotion = ref(systemMotion.matches)
const clockText = ref(formatTime())
const feedbackText = ref('')
const capsuleText = computed(() => feedbackText.value || (!expanded.value ? clockText.value : ''))
const hovering = ref(false)
const composing = ref(false)
const pressed = ref(false)
const receivingFiles = ref(false)
let stopFileDrop: (() => void) | undefined
const content = ref<HTMLElement | null>(null)
const chrome = ref<HTMLElement | null>(null)
const contentHeight = ref(330)
const chromeHeight = ref(56)
const availableWidth = ref(window.screen.availWidth)
const revealed = computed(() => !suppressed.value)
const page = computed(() => state.ui.islandPage)
const appearance = computed(() => state.ui.islandAppearance)
const scale = computed(() => appearance.value.scale / 100)
// 所有页面共用外壳宽度；新增分类也不能按页面单独改变尺寸。
const expandedWidth = computed(() => Math.max(280, Math.min(504 * scale.value, availableWidth.value - 24)))
const tuckedWidth = computed(() => 80 * scale.value)
const visibleWidth = computed(() => {
  if (expanded.value) return expandedWidth.value - 24
  if (suppressed.value) return tuckedWidth.value
  return 128 * scale.value
})
// 内容高度已经来自实际布局；再次乘缩放比例会让小尺寸裁切、大尺寸多出留白。
const visibleHeight = computed(() => expanded.value
  ? Math.min(44 + chromeHeight.value + (page.value === 'clipboard' && !state.ui.detailId ? 420 : contentHeight.value), window.screen.availHeight - 60)
  : (suppressed.value ? 5 : 32) * scale.value)
const bounds = computed(() => ({
  width: Math.max(140, expanded.value ? expandedWidth.value : visibleWidth.value + (suppressed.value ? 0 : 4)),
  height: Math.max(14, visibleHeight.value + (expanded.value ? 24 : suppressed.value ? 0 : 4))
}))
// 原生窗口只在动画前扩容、结束后收紧；动画由透明窗口内的 CSS 独立完成。
const viewportSize = ref({ ...bounds.value })
const viewportStyle = computed(() => ({
  width: `${viewportSize.value.width}px`, height: `${viewportSize.value.height}px`
}))
const rootStyle = computed(() => ({
  width: `${bounds.value.width}px`,
  height: `${bounds.value.height}px`,
  '--island-tint': appearance.value.tint || 'var(--accent)',
  '--island-opacity': `${appearance.value.opacity}%`,
  '--collapse-time': `${collapseMs}ms`,
  '--expand-time': `${expandMs}ms`,
  '--content-width': `${expandedWidth.value - 24}px`,
  // 桌面端不用会产生黑色底板的整窗模糊，提高本地面板遮罩以保持文字对比。
  '--glass-opacity': `${isDesktop ? 72 + (appearance.value.opacity - 75) * .4 : 18 + appearance.value.opacity - 75}%`,
  '--page-index': Math.max(0, sections.findIndex(section => section.value === page.value)),
  '--section-count': sections.length
}))
const sections = [
  { value: 'applications', label: '应用', icon: 'grid' },
  { value: 'clipboard', label: '剪贴板', icon: 'clipboard' },
  { value: 'translation', label: '翻译', icon: 'translate' },
  { value: 'files', label: '文件中转', icon: 'folder' }
] as const
let hoverTimer: ReturnType<typeof setTimeout> | undefined
let leaveTimer: ReturnType<typeof setTimeout> | undefined
let closeTimer: ReturnType<typeof setTimeout> | undefined
let closeCommitted = false
let reenterAfterClose = false
let clockTimer: ReturnType<typeof setTimeout> | undefined
let feedbackTimer: ReturnType<typeof setTimeout> | undefined
let stopFeedback: (() => void) | undefined
let geometryTimer: ReturnType<typeof setTimeout> | undefined
let observer: ResizeObserver | undefined
let contentObserver: ResizeObserver | undefined
let frame = 0
let sending = false
let disposed = false
let queued: { width: number; height: number; regions: BarRegion[] } | undefined
let sent = ''
let visibilityIntent = Promise.resolve()
function formatTime(): string {
  const now = new Date()
  return `${String(now.getHours()).padStart(2, '0')}:${String(now.getMinutes()).padStart(2, '0')}`
}
function updateClock(): void {
  clearTimeout(clockTimer)
  clockText.value = formatTime()
  clockTimer = setTimeout(updateClock, 60000 - Date.now() % 60000 + 20)
}
function showFeedback(kind: string): void {
  if (suppressed.value || (kind !== 'copied' && kind !== 'translated')) return
  clearTimeout(feedbackTimer)
  feedbackText.value = kind === 'copied' ? '已复制' : '翻译完成'
  feedbackTimer = setTimeout(() => { feedbackText.value = '' }, 2600)
}
function updateTheme(): void {
  dark.value = state.settings.theme === 'system' ? systemDark.matches : state.settings.theme === 'dark'
}

watch(() => state.settings.theme, updateTheme)
watch(geometryMoving, queueGeometry, { flush: 'post' })

function reserveGeometry(): void {
  clearTimeout(geometryTimer)
  const target = bounds.value
  if (suppressed.value || state.ui.islandReducedMotion || reducedMotion.value) {
    geometryMoving.value = false
    viewportSize.value = { ...target }
    queueGeometry()
    return
  }
  geometryMoving.value = true
  viewportSize.value = {
    width: Math.max(viewportSize.value.width, target.width),
    height: Math.max(viewportSize.value.height, target.height)
  }
  geometryTimer = setTimeout(() => {
    viewportSize.value = { ...bounds.value }
    geometryMoving.value = false
    queueGeometry()
  }, (expanded.value ? expandMs : collapseMs) + 50)
}

watch([bounds, () => state.ui.islandReducedMotion, reducedMotion], reserveGeometry)
function updateMotion(): void { reducedMotion.value = systemMotion.matches }

function cancelHover(): void { clearTimeout(hoverTimer) }

function cancelHoverClose(): void {
  clearTimeout(closeTimer)
  hoverClosing.value = false
}

function expand(value: boolean, focus = false): void {
  if (value && suppressed.value) return
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
    if (disposed || (value && suppressed.value)) return
    if (!isDesktop) { state.ui.islandExpanded = value; return }
    await invokeHost('set_island_expanded', { expanded: value, focus })
  }).catch(error => {
    closeCommitted = false
    reenterAfterClose = false
    cancelHoverClose()
    pushToast(String(error), 'error')
  })
}

function activateCapsule(): void {
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
  if (suppressed.value) return
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
  return composing.value || pressed.value || receivingFiles.value || shelf.busy ||
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
  if (suppressed.value) return
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
  if (!value) {
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
watch(() => shelf.busy, busy => {
  if (!busy) {
    pressed.value = false
    if (!hovering.value) leave()
  }
})

watch(suppressed, () => {
  cancelHover()
  clearTimeout(leaveTimer)
  clearTimeout(feedbackTimer)
  cancelHoverClose()
  feedbackText.value = ''
  receivingFiles.value = false
  hovering.value = false
  pressed.value = false
  composing.value = false
  reenterAfterClose = false
})

async function selectPage(value: typeof page.value): Promise<void> {
  if (suppressed.value) return
  if (!isDesktop) {
    state.ui.islandPage = value
    state.ui.panel = value === 'applications' ? null : value
    return
  }
  try { await invokeHost('select_island_page', { page: value, focus: true }) }
  catch (error) { pushToast(String(error), 'error') }
}

async function openWechat(): Promise<void> {
  if (!isDesktop) { pushToast('浏览器样稿不打开微信', 'info'); return }
  try { await invokeHost('open_existing_wechat') }
  catch (error) { pushToast(String(error), 'error') }
}

async function openSeparateTranslation(): Promise<void> {
  if (!isDesktop) { pushToast('独立窗口请在桌面版中使用', 'info'); return }
  try { await invokeHost('open_translation_window') }
  catch (error) { pushToast(String(error), 'error') }
}

async function launch(id: string): Promise<void> {
  const application = state.apps.find(item => item.id === id)
  if (!application) return
  if (await launchApp(application)) expand(false)
}

function keydown(event: KeyboardEvent): void {
  if (event.isComposing || event.keyCode === 229) return
  if (event.key === 'Escape') { event.stopPropagation(); expand(false) }
}

function scrollWheel(event: WheelEvent): void {
  const boundary = element.value
  if (!expanded.value || !boundary || event.defaultPrevented || !event.cancelable || event.ctrlKey || event.metaKey) return
  // 嵌套面板里不可滚动的内层会截住 WebView 的默认滚动链。
  // 从命中元素向外找能沿当前方向滚动的容器；只处理岛内，不改变窗口焦点。
  let target = event.target instanceof Element ? event.target : null
  if (target?.closest('select, input[type="number"], input[type="range"]')) return
  const horizontal = event.deltaX || (event.shiftKey ? event.deltaY : 0)
  const vertical = event.shiftKey ? 0 : event.deltaY
  while (target && boundary.contains(target)) {
    if (target instanceof HTMLElement) {
      const style = getComputedStyle(target)
      const line = parseFloat(style.lineHeight) || 16
      const dx = horizontal * (event.deltaMode === 1 ? line : event.deltaMode === 2 ? target.clientWidth : 1)
      const dy = vertical * (event.deltaMode === 1 ? line : event.deltaMode === 2 ? target.clientHeight : 1)
      const maxX = target.scrollWidth - target.clientWidth
      const maxY = target.scrollHeight - target.clientHeight
      const canX = /^(auto|scroll)$/.test(style.overflowX) && maxX > 1 &&
        (dx > 0 ? target.scrollLeft < maxX - .5 : dx < 0 && target.scrollLeft > .5)
      const canY = /^(auto|scroll)$/.test(style.overflowY) && maxY > 1 &&
        (dy > 0 ? target.scrollTop < maxY - .5 : dy < 0 && target.scrollTop > .5)
      if (canX || canY) {
        event.preventDefault()
        target.scrollBy({ left: canX ? dx : 0, top: canY ? dy : 0, behavior: 'instant' })
        return
      }
    }
    target = target.parentElement
  }
}

function queueGeometry(): void {
  if (frame || disposed) return
  frame = requestAnimationFrame(() => {
    frame = 0
    const rect = viewport.value?.getBoundingClientRect()
    if (!rect) return
    const regions: BarRegion[] = []
    if (geometryMoving.value) {
      // 仅在这块小浮窗内临时预留动画空间；不逐帧调用 SetWindowRgn，避免裁切追帧。
      regions.push({ x: 0, y: 0, width: Math.ceil(rect.width), height: Math.ceil(rect.height), radius: 0 })
    } else if (!suppressed.value) {
      for (const surface of [panel.value]) {
        if (!surface) continue
        const box = surface.getBoundingClientRect()
        const x = Math.max(0, Math.round(box.left - rect.left))
        const y = Math.max(0, Math.round(box.top - rect.top))
        const width = Math.min(Math.ceil(box.width), Math.ceil(rect.width) - x)
        const height = Math.min(Math.ceil(box.height), Math.ceil(rect.height) - y)
        if (width > 0 && height > 0) regions.push({ x, y, width, height,
          radius: Math.min(parseFloat(getComputedStyle(surface).borderTopLeftRadius) || 0, width / 2, height / 2) })
      }
    }
    queued = { width: Math.ceil(rect.width), height: Math.ceil(rect.height), regions }
    void sendGeometry()
  })
}

async function sendGeometry(): Promise<void> {
  if (sending || !queued || disposed) return
  const next = queued
  queued = undefined
  const key = JSON.stringify(next)
  if (sent === key) return
  sending = true
  try { await resizeBar(next.width, next.height, next.regions); sent = key }
  catch { sent = '' }
  finally {
    sending = false
    if (queued) void sendGeometry()
  }
}

function measureContent(): void {
  // offsetHeight 读取布局高度，不受父级动画的亚像素位移影响，避免 ceil 把 188 算成 189。
  const height = content.value?.offsetHeight
  if (height) contentHeight.value = Math.min(478, height)
  const header = chrome.value?.offsetHeight
  if (header) chromeHeight.value = header
}

onMounted(() => {
  if (isDesktop) {
    void getCurrentWebview().onDragDropEvent(async ({ payload }) => {
      if (suppressed.value || shelf.busy) return
      if (payload.type === 'enter' || payload.type === 'over') {
        if (receivingFiles.value) return
        receivingFiles.value = true
        cancelHover(); clearTimeout(leaveTimer); cancelHoverClose()
        try { await invokeHost('select_island_page', { page: 'files', focus: false }) }
        catch (error) { receivingFiles.value = false; pushToast(String(error), 'error') }
      } else if (payload.type === 'drop') {
        receivingFiles.value = false
        await addShelfFiles(payload.paths)
      } else {
        receivingFiles.value = false
        if (!hovering.value) leave()
      }
    }).then(off => { if (disposed) off(); else stopFileDrop = off })
      .catch(() => pushToast('文件拖入暂不可用，请使用添加文件', 'error'))
  }
  updateClock()
  document.addEventListener('visibilitychange', updateClock)
  void listenHost<string>('island-feedback', showFeedback).then(off => {
    if (disposed) off()
    else stopFeedback = off
  }).catch(() => pushToast('胶囊操作提示暂不可用', 'error'))
  observer = new ResizeObserver(queueGeometry)
  contentObserver = new ResizeObserver(measureContent)
  if (viewport.value) observer.observe(viewport.value)
  if (content.value) contentObserver.observe(content.value)
  if (chrome.value) contentObserver.observe(chrome.value)
  window.addEventListener('pointerup', handlePointerUp)
  window.addEventListener('pointercancel', handlePointerUp)
  systemDark.addEventListener('change', updateTheme)
  systemMotion.addEventListener('change', updateMotion)
  updateTheme()
  queueGeometry()
})

onBeforeUnmount(() => {
  disposed = true
  observer?.disconnect()
  contentObserver?.disconnect()
  cancelAnimationFrame(frame)
  cancelHover()
  clearTimeout(leaveTimer)
  clearTimeout(clockTimer)
  clearTimeout(feedbackTimer)
  stopFeedback?.()
  stopFileDrop?.()
  document.removeEventListener('visibilitychange', updateClock)
  clearTimeout(closeTimer)
  clearTimeout(geometryTimer)
  window.removeEventListener('pointerup', handlePointerUp)
  window.removeEventListener('pointercancel', handlePointerUp)
  systemDark.removeEventListener('change', updateTheme)
  systemMotion.removeEventListener('change', updateMotion)
})
</script>

<template>
  <div ref="viewport" class="island-viewport" :class="`anchor-${state.ui.islandAnchor}`" :style="viewportStyle">
  <section ref="element" class="island-hit-area"
    :class="{ expanded, desktop: isDesktop, glass: appearance.preset === 'glass', dark, compact: expandedWidth < 620, narrow: expandedWidth < 440, tucked: !revealed, reduced: state.ui.islandReducedMotion || reducedMotion, suppressed: state.ui.fullscreenSuppressed }"
    :style="rootStyle" aria-label="顶部灵动岛" :inert="state.ui.fullscreenSuppressed"
    @mouseenter="enter" @mouseleave="leave" @focusout="focusout" @keydown="keydown" @wheel="scrollWheel"
    @pointerdown="pressed = true" @compositionstart="composing = true" @compositionend="composing = false">
    <button v-if="!revealed" class="tuck-trigger" type="button" aria-label="展开灵动岛" @click="expand(true, true)" />
    <div ref="panel" class="island glass-surface" :class="{ expanded, tucked: !revealed }"
      :style="{ width: `${visibleWidth}px`, height: `${visibleHeight}px` }">
      <span v-if="state.ui.wechatEnabled && state.ui.wechatAttention && !suppressed" class="attention-dot" role="img" title="微信有待关注提醒" aria-label="微信有待关注提醒" />
      <button class="capsule" type="button" :aria-expanded="expanded"
        :aria-label="expanded ? '收起面板' : `${clockText}，展开面板`"
        :inert="!revealed" @pointerdown="cancelHover" @click="activateCapsule">
        <span v-if="capsuleText" class="capsule-text" :class="{ feedback: feedbackText }">{{ capsuleText }}</span>
        <span v-else class="grip" aria-hidden="true" />
      </button>
      <span class="capsule-announcement" role="status">{{ feedbackText }}</span>
      <div class="expanded-content" :class="{ 'fixed-search': page === 'clipboard' && !state.ui.detailId }" :inert="!expanded">
        <div ref="chrome" class="island-chrome">
          <header class="toolbar">
            <nav class="mode-nav" aria-label="灵动岛功能">
              <span class="mode-selection" aria-hidden="true" />
              <button v-for="section in sections" :key="section.value" type="button"
                :title="section.label" :aria-label="section.label" :aria-pressed="page === section.value"
                :class="{ selected: page === section.value }" @click="selectPage(section.value)">
                <Icon :name="section.icon" :size="22" :stroke="1.65" />
              </button>
            </nav>
            <div class="tools">
              <button v-if="page === 'translation'" class="round-action" type="button" title="独立翻译窗口" aria-label="独立翻译窗口" @click="openSeparateTranslation"><Icon name="external" :size="19" /></button>
              <button class="round-action" type="button" title="设置" aria-label="设置" @click="openSettings()"><Icon name="settings" :size="21" /></button>
              <button class="round-action collapse-action" type="button" title="收起" aria-label="收起灵动岛" @click="expand(false)"><Icon name="chevron-down" :size="20" /></button>
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
            <nav class="categories" aria-label="应用分类">
              <button v-for="category in state.categories" :key="category.id" type="button"
                :class="{ selected: state.ui.activeCategoryId === category.id }" :aria-pressed="state.ui.activeCategoryId === category.id"
                @click="setCategory(category.id)">{{ category.name }}</button>
            </nav>
            <p v-if="!visibleApps.length" class="empty">这个分类还没有应用</p>
            <div v-else class="app-grid scroll-y">
              <button v-for="app in visibleApps" :key="app.id" class="app-tile" type="button" :title="app.name" :aria-label="app.name" @click="launch(app.id)">
                <img v-if="iconFor(app.target)" :src="iconFor(app.target)" alt="" />
                <span v-else class="fallback" :style="{ background: app.color }">{{ app.initials }}</span>
              </button>
            </div>
          </div>
          <div v-else class="feature-pane scroll-y">
            <ClipboardPopover v-if="page === 'clipboard'" />
            <TranslationPanel v-else-if="page === 'translation'" />
            <FileShelf v-else-if="page === 'files'" :drag-over="receivingFiles" />
          </div>
        </div>
      </div>
    </div>
  </section>
  </div>
</template>

<style scoped>
/* 材质作用于主浮层；顶部图标导航与内容共用外壳。 */
.island-viewport { position: relative; flex: none; background: transparent; }
.island-viewport > .island-hit-area { position: absolute; top: 0; left: 50%; transform: translateX(-50%); }
.island-viewport.anchor-left > .island-hit-area { left: 0; transform: none; }
.island-viewport.anchor-right > .island-hit-area { left: auto; right: 0; transform: none; }
.island-hit-area {
  --geometry-time: var(--expand-time);
  --geometry-ease: cubic-bezier(.23, 1, .32, 1);
  --glass-fill: #edf0f8;
  --glass-edge: rgba(255, 255, 255, .82);
  --glass-rim: rgba(255, 255, 255, .26);
  --glass-shade: rgba(74, 79, 109, .14);
  --glass-sheen: rgba(255, 255, 255, .32);
  --glass-control: rgba(255, 255, 255, .24);
  --glass-control-active: rgba(255, 255, 255, .46);
  --surface-inset: 2px;
  position: relative;
  flex: none;
  transition: width var(--geometry-time) var(--geometry-ease), height var(--geometry-time) var(--geometry-ease), opacity 125ms;
}
.island-hit-area.glass {
  --text: #202431; --text-2: #505767; --text-3: #666e80;
  --surface: #f2f4fa; --surface-2: rgba(244, 246, 252, .5); --surface-3: rgba(255, 255, 255, .55);
  --hover: rgba(255, 255, 255, .25); --active: rgba(255, 255, 255, .4);
  --line: rgba(90, 103, 133, .13); --line-strong: rgba(90, 103, 133, .22);
  --accent: #087bff; --accent-hover: #0664da; --accent-weak: rgba(8, 123, 255, .12);
  color: var(--text);
}
.island-hit-area.dark {
  --glass-fill: #222632;
  --glass-edge: rgba(255, 255, 255, .43);
  --glass-rim: rgba(255, 255, 255, .07);
  --glass-shade: rgba(4, 8, 20, .28);
  --glass-sheen: rgba(223, 231, 255, .09);
  --glass-control: rgba(255, 255, 255, .065);
  --glass-control-active: rgba(235, 240, 255, .13);
}
.island-hit-area.glass.dark {
  --text: #f4f5fa; --text-2: #c6ccda; --text-3: #a2acbf;
  --surface: #242834; --surface-2: rgba(29, 34, 46, .58); --surface-3: rgba(246, 248, 255, .12);
  --hover: rgba(255, 255, 255, .08); --active: rgba(255, 255, 255, .14);
  --line: rgba(238, 242, 255, .12); --line-strong: rgba(238, 242, 255, .2);
  --accent: #71adff; --accent-hover: #a0c7ff; --accent-weak: rgba(88, 155, 255, .17);
}
.island-hit-area.expanded { --surface-inset: 12px; }
.island-hit-area:not(.expanded) { --geometry-time: var(--collapse-time); --geometry-ease: cubic-bezier(.22, .7, .3, 1); }
.island-hit-area.reduced { --geometry-time: 0ms; transition: none; }
.island-hit-area.suppressed { --surface-inset: 0px; --geometry-time: 0ms; pointer-events: none; }
.tuck-trigger { position: absolute; inset: 0; z-index: 3; width: 100%; height: 100%; }
.glass-surface { background: color-mix(in srgb, var(--surface) var(--island-opacity), transparent); box-shadow: inset 0 0 0 1px var(--line-strong); }
.glass .glass-surface {
  background: linear-gradient(145deg, var(--glass-sheen), transparent 48%, var(--glass-control)),
    color-mix(in srgb, color-mix(in srgb, var(--glass-fill) 96%, var(--island-tint)) var(--glass-opacity), transparent);
  backdrop-filter: blur(22px) saturate(1.4);
  box-shadow: inset 0 2px 2px var(--glass-edge), inset 0 -2px 3px var(--glass-edge),
    inset 1px 0 2px var(--glass-rim), inset -1px 0 2px var(--glass-rim), 0 12px 28px -8px var(--glass-shade);
}
.glass .glass-surface::before {
  content: ''; position: absolute; inset: 0; border-radius: inherit; padding: 1px; pointer-events: none;
  background: linear-gradient(145deg, var(--glass-edge) 4%, var(--glass-rim) 30%, transparent 48%, var(--glass-edge) 76%, var(--glass-rim));
  mask: linear-gradient(#fff 0 0) content-box, linear-gradient(#fff 0 0);
  mask-composite: exclude;
}
/* WebView 滤镜不能模糊桌面；桌面端使用圆角半透明材质，避免整窗原生模糊的黑底。 */
.desktop.glass .glass-surface { backdrop-filter: none; }
/* 小胶囊用轻描边，避免多层亮边在低分辨率圆弧上叠出白色台阶。 */
.glass .island:not(.expanded):not(.tucked) {
  box-shadow: inset 0 0 0 1px var(--glass-rim), inset 0 1px 1px var(--glass-edge);
}
.glass .island:not(.expanded):not(.tucked)::before { opacity: .45; }
.island {
  position: absolute; top: var(--surface-inset); left: 50%; transform: translateX(-50%);
  overflow: hidden; border-radius: 24px; color: var(--text);
  transition: width var(--geometry-time) var(--geometry-ease), height var(--geometry-time) var(--geometry-ease), top var(--geometry-time) var(--geometry-ease), border-radius var(--geometry-time) var(--geometry-ease);
}
.island.expanded { border-radius: 36px; }
.island.tucked { border-radius: 5px; }
.island.tucked .capsule { visibility: hidden; }
.glass .island.tucked { background: linear-gradient(90deg, var(--glass-fill), var(--glass-edge), var(--glass-fill)); }
.capsule { position: relative; z-index: 1; display: flex; align-items: center; justify-content: center; height: 24px; width: 156px; margin: 0 auto; white-space: nowrap; font-size: 12px; }
.island:not(.expanded) .capsule { height: 100%; width: 100%; }
.capsule-text { font-size: 13px; line-height: 18px; font-weight: 500; letter-spacing: .6px; font-variant-numeric: tabular-nums; }
.capsule-text.feedback { font-size: 12px; letter-spacing: .2px; }
.capsule-announcement { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
.grip { width: 40px; height: 4px; border-radius: 8px; background: currentColor; opacity: .26; transition: opacity 130ms; }
.capsule:hover .grip { opacity: .55; }
.expanded-content { position: relative; width: var(--content-width); max-height: calc(100% - 24px); overflow: hidden; padding: 0 20px 20px; opacity: 0; transform: translateY(-6px); transition: opacity 85ms ease, transform 140ms ease; }
.expanded .expanded-content { overflow-y: auto; opacity: 1; transform: translateY(0); transition: opacity 160ms ease-out 65ms, transform 200ms var(--geometry-ease) 60ms; }
.island-chrome { display: flow-root; }
.island-content { width: 100%; }
.toolbar { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; padding: 2px 0 8px; }
.tools { display: flex; align-items: center; gap: 10px; }
.round-action { display: grid; place-items: center; flex: none; width: 36px; height: 36px; border-radius: 50%; background: var(--surface-2); color: var(--text-2); }
.glass .round-action { background: var(--glass-control); box-shadow: inset 0 1px 1px var(--glass-edge), inset 0 0 0 1px var(--glass-rim), 0 2px 5px -3px var(--glass-shade); }
.collapse-action svg { transform: rotate(180deg); }
.round-action:hover { background: var(--glass-control-active); color: var(--text); }
.categories { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; max-height: 80px; overflow-y: auto; margin: 0 0 12px; padding: 2px; }
.categories button { flex: none; padding: 6px 12px; border-radius: 22px; min-height: 34px; font-size: 14px; color: var(--text-2); }
.categories .selected { color: var(--text); background: var(--surface-3); font-weight: 600; }
.glass .categories .selected { background: var(--glass-control-active); box-shadow: inset 0 1px 1px var(--glass-edge), inset 0 -1px 1px var(--glass-rim), 0 2px 5px -3px var(--glass-shade); }
.categories button:hover { color: var(--text); background: var(--hover); }
.app-grid { --app-icon-size: 28px; --app-cell-size: 44px; --app-gap: 8px; display: grid; grid-template-columns: repeat(auto-fill, var(--app-cell-size)); grid-auto-rows: var(--app-cell-size); justify-content: start; align-content: start; max-height: calc(var(--app-cell-size) * 2 + var(--app-gap)); overflow-y: auto; gap: var(--app-gap); padding: 0 2px; }
.app-tile { display: flex; align-items: center; justify-content: center; width: var(--app-cell-size); height: var(--app-cell-size); padding: 8px; border-radius: 12px; }
.app-tile:hover { background: var(--hover); }
.app-grid { scrollbar-width: thin; scrollbar-color: transparent transparent; }
.app-grid:hover, .app-grid:focus-within { scrollbar-color: var(--line-strong) transparent; }
.app-tile img, .fallback { flex: none; width: var(--app-icon-size); height: var(--app-icon-size); object-fit: contain; }
.app-tile img { filter: drop-shadow(0 3px 2px rgba(0,0,0,.08)); }
.fallback { display: grid; place-items: center; border-radius: 8px; color: #fff; font-size: 12px; font-weight: 600; box-shadow: inset 0 1px 1px rgba(255,255,255,.4), 0 3px 5px -3px var(--glass-shade); }
.empty { display: flex; align-items: center; min-height: 64px; margin: 0; color: var(--text-2); font-size: 13px; line-height: 1.8; padding: 16px 4px; }
.mode-nav { position: relative; flex: none; width: calc(var(--section-count) * 44px); height: 40px; display: grid; grid-template-columns: repeat(var(--section-count), minmax(0, 1fr)); color: var(--text); }
.mode-selection { position: absolute; inset: 0 auto 0 0; width: calc(100% / var(--section-count)); border-radius: 20px; background: var(--surface-3); transform: translateX(calc(var(--page-index) * 100%)); transition: transform 220ms var(--geometry-ease); }
.glass .mode-selection { background: var(--glass-control-active); box-shadow: inset 0 1px 1px var(--glass-edge), inset 0 -1px 1px var(--glass-rim), 0 3px 8px -4px var(--glass-shade); }
.mode-nav button { position: relative; display: flex; align-items: center; justify-content: center; gap: 9px; min-width: 0; border-radius: 24px; color: var(--text-2); font-size: 14px; }
.mode-nav .selected { color: var(--text); font-weight: 600; }
.mode-nav .selected svg { color: var(--island-tint); }
.mode-nav button:hover { color: var(--text); }
button { transition: background 130ms ease, color 130ms ease, transform 150ms var(--geometry-ease); }
button:active { transform: scale(.97); }
button:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.attention-dot { position: absolute; right: 8px; top: 0; width: 5px; height: 5px; z-index: 2; background: var(--success); border-radius: 50%; }
.island:not(.tucked) > .attention-dot { right: 16px; top: 50%; transform: translateY(-50%); background: #42d580; }
.island.expanded > .attention-dot { right: 20px; top: 12px; transform: none; }
.attention-strip { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 8px 10px; margin-bottom: 12px; border-radius: 12px; background: var(--surface-2); font-size: 12px; }
.attention-strip > span { flex: 1; }
.attention-strip button { padding: 5px 8px; border-radius: 8px; background: var(--hover); }
.feature-pane { max-height: 420px; overflow: auto; }
.feature-pane :deep(.pop), .feature-pane :deep(.panel) { width: 100%; max-height: none; border: 0; border-radius: 0; box-shadow: none; background: transparent; }
.feature-pane :deep(.panel > .head) { display: none; }
.feature-pane :deep(.pop > .head), .feature-pane :deep(.panel > .content) { padding-left: 0; padding-right: 0; }
.glass .feature-pane { --r-chip: 16px; --r-tile: 16px; --thumb-bg: var(--glass-control); }
.glass .feature-pane :deep(.search) { height: 38px; border-radius: 20px; background: var(--glass-control); }
.glass .feature-pane :deep(.pause) { height: 38px; border-radius: 20px; }
.glass .feature-pane :deep(.chips) { gap: 6px; padding: 0 0 10px; }
.glass .feature-pane :deep(.chip) { height: 30px; padding: 0 12px; }
.glass .feature-pane :deep(.list) { padding-left: 0; padding-right: 0; }
.glass .feature-pane :deep(.row) { padding: 10px; }
.glass .feature-pane :deep(.row.is-active) { background: var(--glass-control-active); box-shadow: inset 0 1px 0 var(--glass-edge); }
.glass .feature-pane :deep(.seg) { padding: 4px; border-radius: 22px; background: var(--glass-control); }
.glass .feature-pane :deep(.seg-btn) { height: 32px; border-radius: 18px; }
.glass .feature-pane :deep(.textarea) { border-radius: 18px; background: var(--glass-control); }
.compact .expanded-content { padding-left: 20px; padding-right: 20px; }
.narrow .expanded-content { padding-left: 16px; padding-right: 16px; }
.narrow .categories { gap: 2px; }
.narrow .categories button { padding: 6px 10px; }
.reduced *, .reduced *::before { transition: none !important; }
@media (prefers-reduced-motion: reduce) { *, *::before { transition: none !important; } }
@media (prefers-reduced-transparency: reduce) { .glass .glass-surface { background: var(--glass-fill); backdrop-filter: none; } }
</style>

<style scoped>
.expanded-content.fixed-search { display: flex; flex-direction: column; overflow: hidden; }
.fixed-search .island-chrome { flex: none; }
.fixed-search .island-content { min-height: 0; display: flex; flex-direction: column; }
.fixed-search .feature-pane { min-height: 0; overflow: hidden; display: flex; flex-direction: column; }
.fixed-search .feature-pane :deep(.pop) { min-height: 0; max-height: 420px; }
</style>
