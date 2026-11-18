<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import GlassLight from '@/components/GlassLight.vue'
import LiquidSkin, { type IslandVariant } from './motion/LiquidSkin.vue'
import IslandParticles from './motion/IslandParticles.vue'
import ApprovalPrompt from './ApprovalPrompt.vue'
import MailNotices from '@/features/mail/MailNotices.vue'
import { providerName, useMailList, type MailNotice } from '@/features/mail/mail'
import { useApprovals } from './useApprovals'
import { useIslandEffects, type IslandCue } from './motion/useIslandEffects'
import {
  buildTelemetryReadout,
  countTelemetryMetrics,
  normalizeTelemetryPrefs,
  stepSmartTelemetry,
  type TelemetrySnapshot
} from './telemetryReadout'
import ClipboardPopover from '@/features/clipboard/ClipboardPopover.vue'
import TranslationPanel from '@/features/translation/TranslationPanel.vue'
import FileShelf from '@/features/files/FileShelf.vue'
import AudioMixer from '@/features/audio/AudioMixer.vue'
import { addShelfFiles, shelf, subscribeShelf } from '@/features/files/shelf'
import type { IconName } from '@/components/icon-data'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { useWindowDrag } from '@/lib/useWindowDrag'
import { invokeHost, isDesktop, listenHost, resizeBar, type BarRegion } from '@/lib/desktop/desktop'
import { isDarkTheme, pushToast, iconFor, launchApp, openSettings, setCategory, state, visibleApps } from '@/store'

const props = defineProps<{
  previewFeedback?: { kind: string; sequence: number }
  previewKeepOpen?: boolean
  previewCue?: IslandCue
  previewTelemetry?: TelemetrySnapshot | null
}>()
const element = ref<HTMLElement | null>(null)
const viewport = ref<HTMLElement | null>(null)
const windowDrag = useWindowDrag(viewport, {
  native: () => isDesktop,
  onPress: () => {
    pressed.value = true
    cancelHover()
    clearTimeout(leaveTimer)
    cancelHoverClose()
  },
  onEnd: () => {
    pressed.value = false
    if (!hovering.value) leave()
  },
  onError: error => pushToast(`无法拖动窗口：${String(error)}`, 'error')
})
const panel = ref<HTMLElement | null>(null)
function resolveVariant(mode = state.settings.theme): IslandVariant {
  if (mode === 'midnight') return 'telemetry'
  if (mode === 'mecha') return 'mecha'
  if (mode === 'scroll') return 'scroll'
  if (mode === 'pixel') return 'pixel'
  return 'standard'
}
const systemDark = window.matchMedia('(prefers-color-scheme: dark)')
const dark = ref(isDarkTheme())
const variant = ref<IslandVariant>(resolveVariant())
const telemetry = computed(() => variant.value === 'telemetry')
const glassPalette = computed<'cool' | 'amber' | 'vermilion' | 'lime'>(() => {
  if (variant.value === 'mecha') return 'amber'
  if (variant.value === 'scroll') return 'vermilion'
  if (variant.value === 'pixel') return 'lime'
  return 'cool'
})
const scanSeq = ref(0)
const hostExpanded = computed(() => state.ui.islandExpanded)
const mailNotices = useMailList<MailNotice>('bar', 'mail_notices', 'mail-notices-changed', message => pushToast(message, 'error'))
const mailSelected = ref(false)
const viewingMail = computed(() => mailSelected.value && mailNotices.value.length > 0)
const seenMail = ref<string[]>([])
const newMail = computed(() => mailNotices.value.filter(item => !seenMail.value.includes(item.id)))
const mailSummary = computed(() => {
  const [first] = newMail.value
  if (!first) return ''
  if (newMail.value.length > 1) return `${newMail.value.length} 封新邮件`
  return `${providerName[first.provider]} · ${first.code ? '验证码' : '新邮件'}`
})
const { current: approval, count: approvalCount, busy: answering, answer: answerApproval } =
  useApprovals(message => pushToast(message, 'error'))
const suppressed = computed(() => state.ui.fullscreenSuppressed)
const hoverClosing = ref(false)
const expanded = computed(() => !!approval.value || (hostExpanded.value && !hoverClosing.value))
watch([mailNotices, viewingMail, expanded, approval], ([items, selected, open, prompt]) => {
  const ids = items.map(item => item.id)
  seenMail.value = selected && open && !prompt ? ids : seenMail.value.filter(id => ids.includes(id))
})
// 避让只决定收起形态；细线也能悬停展开。
const tucked = computed(() => suppressed.value && !expanded.value)
const expandMs = 340
const collapseMs = 195
const geometryMoving = ref(false)
const systemMotion = window.matchMedia('(prefers-reduced-motion: reduce)')
const reducedMotion = ref(systemMotion.matches)
const motionReduced = computed(() => state.ui.islandReducedMotion || reducedMotion.value)
watch(expanded, (value, previous) => {
  if (value && !previous && (telemetry.value || variant.value === 'mecha') && !motionReduced.value) scanSeq.value++
})
const materialRequested = computed(() => state.ui.islandAppearance.webgl !== false)
const { active: effectsActive, cue, signal, quality, plainMaterial } = useIslandEffects(panel, tucked, motionReduced, materialRequested)
const shape = computed(() => tucked.value ? 'line' : expanded.value ? 'expanded' : 'compact')
const glassAvailable = ref(true)
const clockText = ref(formatTime())
const feedbackText = ref('')
const telemetrySample = ref<TelemetrySnapshot | null>(null)
const telemetryHold = ref(0)
const telemetryActive = ref(false)
const telemetryPrefs = computed(() => normalizeTelemetryPrefs(state.ui.islandAppearance))
function applyTelemetrySample(sample: TelemetrySnapshot | null): void {
  telemetrySample.value = sample
  const next = stepSmartTelemetry(sample, telemetryPrefs.value, telemetryHold.value)
  telemetryHold.value = next.nextHold
  telemetryActive.value = next.active
}
watch(telemetryPrefs, prefs => {
  if (prefs.telemetryMode === 'off' || countTelemetryMetrics(prefs) === 0) {
    telemetrySample.value = null
    telemetryHold.value = 0
    telemetryActive.value = false
    return
  }
  const next = stepSmartTelemetry(telemetrySample.value, prefs, telemetryHold.value)
  telemetryHold.value = next.nextHold
  telemetryActive.value = next.active
})
watch(tucked, value => {
  if (value) {
    telemetryHold.value = 0
    telemetryActive.value = false
  }
})
const telemetryReadout = computed(() =>
  telemetryActive.value ? buildTelemetryReadout(telemetrySample.value, telemetryPrefs.value) : null
)
const capsuleStatus = computed<{ text: string; ariaText?: string; tooltip?: string; icon: IconName | null; kind: string }>(() => {
  if (approval.value) return { text: approval.value.kind === 'approval' ? '待审批' : '有提问', ariaText: approval.value.title, icon: 'info', kind: 'approval' }
  if (receivingFiles.value) return { text: '松开暂存', icon: 'folder', kind: 'working' }
  if (shelf.busy) return { text: shelf.activity || '文件处理中', icon: 'folder', kind: 'working' }
  const pending = Math.max(state.ui.translationPending, state.translation.status === 'loading' ? 1 : 0)
  if (pending) return { text: pending > 1 ? `翻译中 · ${pending}` : '翻译中', icon: 'translate', kind: 'working' }
  if (feedbackText.value) return { text: feedbackText.value, icon: feedbackText.value === '翻译失败' ? 'info' : 'check', kind: 'feedback' }
  if (mailSummary.value) return { text: mailSummary.value, icon: 'mail', kind: 'mail' }
  if (state.ui.paused) return { text: '记录已暂停', icon: 'pause', kind: 'paused' }
  if (telemetryReadout.value) {
    return {
      text: telemetryReadout.value.text,
      ariaText: telemetryReadout.value.ariaText,
      tooltip: telemetryReadout.value.tooltip,
      icon: null,
      kind: 'telemetry'
    }
  }
  if (shelf.entries.length) return { text: `暂存 ${shelf.entries.length > 999 ? '999+' : shelf.entries.length} 项`, icon: 'folder', kind: 'files' }
  return { text: clockText.value, icon: null, kind: 'idle' }
})
const isNoticeStatus = computed(() => capsuleStatus.value.kind !== 'idle' && capsuleStatus.value.kind !== 'telemetry')
const taskSignal = computed(() => {
  if (capsuleStatus.value.kind === 'idle') return 'idle'
  if (capsuleStatus.value.kind === 'telemetry') return 'telemetry'
  return `${capsuleStatus.value.kind}:${capsuleStatus.value.text}`
})
const hovering = ref(false)
const composing = ref(false)
const pressed = ref(false)
const receivingFiles = ref(false)
let stopFileDrop: (() => void) | undefined
let stopShelf: (() => void) | undefined
const content = ref<HTMLElement | null>(null)
const chrome = ref<HTMLElement | null>(null)
const modeNav = ref<HTMLElement | null>(null)
const contentHeight = ref(330)
const chromeHeight = ref(56)
const availableWidth = ref(window.screen.availWidth)
const revealed = computed(() => !tucked.value)
const page = computed(() => state.ui.islandPage)
const readingExplanation = ref(false)
let readingSync = Promise.resolve()
function setReadingExplanation(active: boolean) {
  if (readingExplanation.value === active) return
  readingExplanation.value = active
  if (isDesktop) readingSync = readingSync.then(() => invokeHost<void>('set_island_reading_explanation', { active }))
    .catch(() => pushToast('无法同步灵动岛的展开状态', 'error'))
}
const keepTaskOpen = computed(() => !viewingMail.value &&
  (page.value === 'translation' || (page.value === 'clipboard' && readingExplanation.value)))
const appearance = computed(() => state.ui.islandAppearance)
const materialAllowed = computed(() => materialRequested.value && !motionReduced.value && !plainMaterial.value)
const materialReady = ref(false)
watch([effectsActive, materialAllowed], ([active, allowed]) => {
  if (!allowed) { materialReady.value = false; glassAvailable.value = true }
  else if (active) materialReady.value = true
}, { immediate: true })
const scale = computed(() => appearance.value.scale / 100)
// 所有页面共用外壳宽度；新增分类也不能按页面单独改变尺寸。
const expandedWidth = computed(() => Math.max(280, Math.min(504 * scale.value, availableWidth.value - 24)))
const tuckedWidth = computed(() => variant.value !== 'standard' ? Math.min(134, 118 * scale.value) : 80 * scale.value)
const visibleWidth = computed(() => {
  if (expanded.value) return expandedWidth.value - 24
  if (suppressed.value) return tuckedWidth.value
  if (capsuleStatus.value.kind === 'telemetry' && telemetryReadout.value) {
    const count = telemetryReadout.value.metricCount
    const table: Record<IslandVariant, [number, number][]> = {
      standard: [[140, 118], [188, 168], [240, 218]],
      telemetry: [[158, 136], [204, 184], [256, 234]],
      scroll: [[178, 156], [226, 206], [278, 256]],
      mecha: [[190, 168], [240, 220], [292, 270]],
      pixel: [[204, 182], [258, 238], [312, 290]]
    }
    const [base, floor] = table[variant.value][Math.min(2, Math.max(0, count - 1))]
    return Math.min(availableWidth.value - 24, Math.max(floor, Math.round(base * scale.value)))
  }
  const idle = capsuleStatus.value.kind === 'idle'
  if (variant.value === 'telemetry') return (idle ? 148 : 186) * scale.value
  if (variant.value === 'mecha' || variant.value === 'pixel') return (idle ? 176 : 208) * scale.value
  if (variant.value === 'scroll') return (idle ? 164 : 196) * scale.value
  return (idle ? 128 : 168) * scale.value
})
// 内容高度已经来自实际布局；再次乘缩放比例会让小尺寸裁切、大尺寸多出留白。
const lineHeights: Record<IslandVariant, number> = { standard: 5, telemetry: 7, mecha: 8, scroll: 9, pixel: 9 }
const compactHeights: Record<IslandVariant, number> = { standard: 32, telemetry: 32, mecha: 34, scroll: 34, pixel: 36 }
const visibleHeight = computed(() => expanded.value
  ? Math.min(44 + (approval.value ? 0 : chromeHeight.value) + (!approval.value && !viewingMail.value && page.value === 'clipboard' && !state.ui.detailId ? 420 : contentHeight.value), window.screen.availHeight - 60)
  : (suppressed.value ? lineHeights[variant.value] : compactHeights[variant.value]) * scale.value)
const cornerRadius = computed(() => {
  if (variant.value === 'mecha' || variant.value === 'scroll' || variant.value === 'pixel') return 4
  const base = expanded.value ? (telemetry.value ? 12 : 18) : tucked.value ? (telemetry.value ? 4 : 5) : (telemetry.value ? 10 : 24)
  return Math.min(base, visibleHeight.value / 2, visibleWidth.value / 2)
})
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
  '--collapse-time': `${collapseMs}ms`,
  '--expand-time': `${expandMs}ms`,
  '--content-width': `${expandedWidth.value - 24}px`,
  '--capsule-height': `${compactHeights[variant.value] * scale.value}px`,
  '--page-index': Math.max(0, sections.findIndex(section => section.value === page.value)),
  '--section-count': sections.length
}))
const sections = [
  { value: 'applications', label: '应用', icon: 'grid' },
  { value: 'clipboard', label: '剪贴板', icon: 'clipboard' },
  { value: 'translation', label: '翻译', icon: 'translate' },
  { value: 'files', label: '文件中转', icon: 'folder' },
  { value: 'audio', label: '调音台', icon: 'volume' }
] as const
function revealSelectedPage(): void {
  const nav = modeNav.value
  const selected = nav?.querySelector<HTMLElement>('button.selected')
  if (!nav || !selected) return
  nav.style.setProperty('--sel-x', `${selected.offsetLeft}px`)
  nav.style.setProperty('--sel-w', `${selected.offsetWidth}px`)
  if (selected.offsetLeft < nav.scrollLeft) nav.scrollLeft = selected.offsetLeft
  else if (selected.offsetLeft + selected.offsetWidth > nav.scrollLeft + nav.clientWidth) nav.scrollLeft = selected.offsetLeft + selected.offsetWidth - nav.clientWidth
}
watch([page, expanded, variant], revealSelectedPage, { flush: 'post' })
watch(scale, revealSelectedPage, { flush: 'post' })
onMounted(revealSelectedPage)
let hoverTimer: ReturnType<typeof setTimeout> | undefined
let leaveTimer: ReturnType<typeof setTimeout> | undefined
let closeTimer: ReturnType<typeof setTimeout> | undefined
let closeCommitted = false
let reenterAfterClose = false
let clockTimer: ReturnType<typeof setTimeout> | undefined
let feedbackTimer: ReturnType<typeof setTimeout> | undefined
let stopFeedback: (() => void) | undefined
let stopTelemetry: (() => void) | undefined
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
  if (tucked.value || !['copied', 'translated', 'translation-error'].includes(kind)) return
  clearTimeout(feedbackTimer)
  feedbackText.value = kind === 'copied' ? '已复制' : kind === 'translated' ? '翻译完成' : '翻译失败'
  signal(kind === 'translation-error' ? 'notice' : 'complete')
  feedbackTimer = setTimeout(() => { feedbackText.value = '' }, 2600)
}
watch(() => props.previewFeedback, value => {
  if (isDesktop || !value) return
  clearTimeout(feedbackTimer)
  feedbackText.value = ''
  if (value.kind) showFeedback(value.kind)
})
watch(() => props.previewCue, value => {
  if (!isDesktop && value) signal(value.kind)
}, { flush: 'post' })
watch(() => props.previewTelemetry, value => {
  if (!isDesktop) applyTelemetrySample(value ?? null)
}, { immediate: true })
watch(() => capsuleStatus.value.kind === 'working', (working, before) => {
  if (working && !before) signal('loading')
})
watch(() => state.ui.wechatEvent, (event, previous) => {
  if (event !== previous && state.ui.wechatEnabled && state.ui.wechatAttention) signal('notice')
})
function updateTheme(): void {
  dark.value = isDarkTheme()
  variant.value = resolveVariant()
}

watch(() => state.settings.theme, updateTheme)
watch(geometryMoving, queueGeometry, { flush: 'post' })

function reserveGeometry(): void {
  clearTimeout(geometryTimer)
  const target = bounds.value
  if ((isDesktop && tucked.value) || state.ui.islandReducedMotion || reducedMotion.value) {
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

watch(keepTaskOpen, keep => {
  if (keep) {
    clearTimeout(leaveTimer)
    if (!closeCommitted) cancelHoverClose()
  } else if (hostExpanded.value && !hovering.value) leave()
}, { flush: 'sync' })

watch(() => approval.value?.id, id => {
  if (!id) return
  cancelHover()
  clearTimeout(leaveTimer)
  cancelHoverClose()
})

function expand(value: boolean, focus = false): void {
  if (value) {
    if (!hostExpanded.value) mailSelected.value = newMail.value.length > 0
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
    if (disposed) return
    if (!isDesktop) { state.ui.islandExpanded = value; return }
    await invokeHost('set_island_expanded', { expanded: value, focus })
  }).catch(error => {
    closeCommitted = false
    reenterAfterClose = false
    cancelHoverClose()
    pushToast(String(error), 'error')
  })
}

function activateCapsule(event: MouseEvent): void {
  if (windowDrag.consumeClick(event)) return
  if (approval.value) return
  expand(!expanded.value, true)
}

function startHoverClose(): void {
  if (!hostExpanded.value || hoverClosing.value || editing()) return
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
  if (tucked.value) { expand(true); return }
  hoverTimer = setTimeout(() => {
    if (hovering.value && !pressed.value) expand(true)
  }, state.ui.islandHoverMs)
}

function enter(): void {
  hovering.value = true
  availableWidth.value = window.screen.availWidth
  cancelHover()
  clearTimeout(leaveTimer)
  if (approval.value) return
  if (closeCommitted) {
    // 提交关闭后已经无法撤回，请等宿主确认再按正常悬停处理。
    reenterAfterClose = true
    return
  }
  if (hoverClosing.value) cancelHoverClose()
  if (!hostExpanded.value) scheduleHoverOpen()
}

function editing(): boolean {
  if (approval.value || keepTaskOpen.value) return true
  if (!isDesktop && props.previewKeepOpen) return true
  const active = document.activeElement
  return composing.value || pressed.value || receivingFiles.value || shelf.busy || !!shelf.previewItem ||
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
  if (!value) {
    mailSelected.value = false
    cancelHover()
    clearTimeout(leaveTimer)
    composing.value = false
    pressed.value = false
    if (shouldReopen) scheduleHoverOpen()
  }
})

watch(page, () => {
  mailSelected.value = false
  if (!closeCommitted) cancelHoverClose()
})
watch(() => props.previewKeepOpen, value => { if (!value && !hovering.value) leave() })
watch(() => state.ui.islandAnchor, () => { if (!isDesktop) windowDrag.reset() })
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
  mailSelected.value = false
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
  signal('launch')
  if (await launchApp(application)) expand(false)
}

function keydown(event: KeyboardEvent): void {
  if (approval.value) return
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
    } else if (!tucked.value) {
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
  void subscribeShelf().then(off => { if (disposed) off(); else stopShelf = off })
    .catch(() => { shelf.error = '暂时无法同步中转文件状态' })
  if (isDesktop) {
    void getCurrentWebview().onDragDropEvent(async ({ payload }) => {
      if (shelf.busy || approval.value) return
      if (payload.type === 'enter' || payload.type === 'over') {
        if (receivingFiles.value) return
        receivingFiles.value = true
        mailSelected.value = false
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
  void listenHost<TelemetrySnapshot>('island-telemetry', applyTelemetrySample).then(off => {
    if (disposed) off()
    else stopTelemetry = off
  }).catch(() => {})
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
  stopTelemetry?.()
  stopFileDrop?.()
  stopShelf?.()
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
    :class="[{ expanded, moving: geometryMoving, desktop: isDesktop, dark, telemetry, compact: expandedWidth < 620, narrow: expandedWidth < 440, tucked: !revealed, reduced: state.ui.islandReducedMotion || reducedMotion, suppressed: tucked, avoiding: suppressed }, `variant-${variant}`]"
    :style="rootStyle" aria-label="顶部灵动岛"
    @mouseenter="enter" @mouseleave="leave" @focusout="focusout" @keydown="keydown" @wheel="scrollWheel"
    @pointerdown="pressed = true" @compositionstart="composing = true" @compositionend="composing = false">
    <button v-if="!revealed" class="tuck-trigger" type="button" aria-label="展开灵动岛" @click="expand(true, true)" />
    <div ref="panel" class="island solid-surface" :class="[{ expanded, tucked: !revealed, telemetry }, `variant-${variant}`]"
      :style="{ width: `${visibleWidth}px`, height: `${visibleHeight}px` }">
      <LiquidSkin :width="visibleWidth" :height="visibleHeight" :shape="shape" :variant="variant" />
      <GlassLight v-if="materialReady && glassAvailable"
        :light-surface="!dark" :signal="taskSignal" :expanded="expanded" :quality="quality" :active="effectsActive" :corner-radius="cornerRadius" :palette="glassPalette" @unavailable="glassAvailable = false" />
      <IslandParticles v-if="materialReady"
        :cue="cue" :active="effectsActive" :quality="quality" :dark="dark" />
      <span v-if="(telemetry || variant === 'mecha') && scanSeq && expanded && !motionReduced" :key="scanSeq" class="telemetry-scan" aria-hidden="true" />
      <span v-if="state.ui.wechatEnabled && state.ui.wechatAttention && !tucked" class="attention-dot" role="img" title="微信有待关注提醒" aria-label="微信有待关注提醒" />
      <button class="capsule" :class="{ 'has-feedback': isNoticeStatus }" type="button" :aria-expanded="expanded"
        :title="!expanded && capsuleStatus.tooltip ? capsuleStatus.tooltip : undefined"
        :aria-label="approval ? approval.title : `${capsuleStatus.ariaText || capsuleStatus.text}，${expanded ? '收起面板' : '展开面板'}`"
        :inert="!revealed" aria-description="按住拖动位置，单击展开或收起"
        @pointerdown.stop="windowDrag.begin" @click="activateCapsule">
        <span class="capsule-text" :class="{ feedback: isNoticeStatus }" aria-hidden="true">
          <span v-if="telemetry && !expanded" class="telemetry-led" :class="capsuleStatus.kind" />
          <span v-else-if="variant === 'mecha' && !expanded" class="mecha-badge">U·06</span>
          <span v-else-if="variant === 'scroll' && !expanded" class="scroll-seal">{{ capsuleStatus.kind === 'working' ? '忙' : capsuleStatus.kind === 'paused' ? '止' : capsuleStatus.kind === 'telemetry' ? '巡' : '安' }}</span>
          <span v-else-if="variant === 'pixel' && !expanded" class="pixel-hp">♥HP</span>
          <span class="capsule-label" :class="{ 'is-telemetry': capsuleStatus.kind === 'telemetry' }">
            <template v-if="capsuleStatus.kind === 'telemetry' && telemetryReadout">
              <template v-for="(seg, idx) in telemetryReadout.segments" :key="idx">
                <i v-if="idx > 0" class="tel-sep">·</i>
                <span class="tel-seg"><em class="tel-pre" :class="{ accent: seg.accent }">{{ seg.prefix }}</em>{{ seg.value }}</span>
              </template>
            </template>
            <template v-else>
              <Icon v-if="capsuleStatus.icon" :name="capsuleStatus.icon" :size="13" />{{ capsuleStatus.text }}
            </template>
          </span>
          <span v-if="variant === 'mecha' && !expanded" class="mecha-ribs"><i /><i /><i /></span>
          <span v-else-if="variant === 'scroll' && !expanded" class="scroll-knot" />
          <span v-else-if="variant === 'pixel' && !expanded" class="pixel-lv">1UP</span>
        </span>
        <span class="grip" aria-hidden="true" />
      </button>
      <span class="capsule-announcement" role="status" aria-atomic="true">{{ !tucked && isNoticeStatus ? capsuleStatus.text : '' }}</span>
      <div class="expanded-content" :class="{ 'fixed-search': !approval && !viewingMail && page === 'clipboard' && !state.ui.detailId }" :inert="!expanded">
        <div ref="chrome" v-show="!approval" class="island-chrome">
          <header class="toolbar">
            <nav ref="modeNav" class="mode-nav" aria-label="灵动岛功能">
              <span v-show="!viewingMail" class="mode-selection" aria-hidden="true" />
              <button v-for="section in sections" :key="section.value" type="button"
                :title="section.label" :aria-label="section.label" :aria-pressed="!viewingMail && page === section.value"
                :class="{ selected: !viewingMail && page === section.value }" @click="selectPage(section.value)">
                <Icon :name="section.icon" :size="22" :stroke="1.65" />
              </button>
            </nav>
            <div class="tools">
              <button v-if="mailNotices.length" class="round-action" type="button" :title="mailSummary || '最近邮件'" :aria-label="mailSummary || '最近邮件'" :aria-pressed="viewingMail" @click="mailSelected = !viewingMail"><Icon name="mail" :size="19" /></button>
              <button v-if="!viewingMail && page === 'translation'" class="round-action" type="button" title="独立翻译窗口" aria-label="独立翻译窗口" @click="openSeparateTranslation"><Icon name="external" :size="19" /></button>
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
          <ApprovalPrompt v-if="approval" :key="approval.id" :card="approval" :count="approvalCount" :busy="answering" @answer="answerApproval" />
          <MailNotices v-if="!approval && viewingMail && expanded" :notices="mailNotices" @error="pushToast($event, 'error')" />
          <div v-show="!approval && !viewingMail" class="ordinary-pane">
          <Transition name="island-pane" mode="out-in" @enter="measureContent" @after-enter="measureContent">
            <div v-if="page === 'applications'" :key="'applications'" class="applications">
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
            <div v-else :key="page" class="feature-pane scroll-y">
              <ClipboardPopover v-if="page === 'clipboard'" @reading-explanation="setReadingExplanation" />
              <TranslationPanel v-else-if="page === 'translation'" />
              <FileShelf v-else-if="page === 'files'" :drag-over="receivingFiles" />
              <AudioMixer v-else-if="page === 'audio'" :active="expanded" />
            </div>
          </Transition>
          </div>
        </div>
      </div>
    </div>
  </section>
  </div>
</template>

<style scoped>
/* 材质作用于主浮层；顶部图标导航与内容共用外壳。 */
.island-viewport { position: relative; z-index: 2; flex: none; background: transparent; }
.island-viewport > .island-hit-area { position: absolute; top: 0; left: 50%; transform: translateX(-50%); }
.island-viewport.anchor-left > .island-hit-area { left: 0; transform: none; }
.island-viewport.anchor-right > .island-hit-area { left: auto; right: 0; transform: none; }
.island-hit-area {
  --geometry-time: var(--expand-time);
  --geometry-ease: cubic-bezier(.2, .9, .25, 1);
  --island-shadow: rgb(0 0 0 / .2);
  --island-outline: rgb(0 0 0 / .16);
  --surface-inset: 2px;
  position: relative;
  flex: none;
  transition: width var(--geometry-time) var(--geometry-ease), height var(--geometry-time) var(--geometry-ease), opacity 110ms;
}
.island-hit-area.dark { --island-shadow: rgb(0 0 0 / .5); --island-outline: rgb(255 255 255 / .14); }
.island-hit-area.expanded { --surface-inset: 12px; }
/* 从细线展开时顶边不下移，避免窗口裁切热区后把原位置的鼠标判为移出。 */
.island-hit-area.desktop.avoiding { --surface-inset: 0px; }
.island-hit-area:not(.expanded) { --geometry-time: var(--collapse-time); --geometry-ease: cubic-bezier(.32, .72, .28, 1); }
.island-hit-area.reduced { --geometry-time: 0ms; transition: none; }
.island-hit-area.suppressed { --surface-inset: 0px; }
.island-hit-area.desktop.suppressed { --geometry-time: 0ms; }
/* Windows 立即释放外围鼠标热区，剩余可见轮廓仍能在 140×14 窗口内收成细线。 */
.island-hit-area.desktop.suppressed:not(.reduced) > .island { --geometry-time: var(--collapse-time); }
/* 预览的三种形态保持同一顶部位置，避免变形时整体上下漂移。 */
.island-hit-area:not(.desktop) { --surface-inset: 2px; }
.tuck-trigger { position: absolute; inset: 0; z-index: 3; width: 100%; height: 100%; }
.solid-surface {
  background: transparent;
  isolation: isolate;
}
.island {
  position: absolute; top: var(--surface-inset); left: 50%; transform: translateX(-50%);
  overflow: hidden; border-radius: 24px; color: var(--text); contain: layout;
  box-shadow: inset 0 1px 0 var(--island-top-sheen);
  transition: width var(--geometry-time) var(--geometry-ease), height var(--geometry-time) var(--geometry-ease), top var(--geometry-time) var(--geometry-ease), border-radius var(--geometry-time) var(--geometry-ease);
}
.island.expanded { border-radius: 18px; }
.island.tucked { border-radius: 5px; box-shadow: none; }
.island.tucked .capsule { visibility: hidden; }
.island.tucked :deep(.glass-light) { visibility: hidden; }
.capsule {
  position: relative; z-index: 3; display: flex; align-items: center; justify-content: center;
  height: 24px; width: 156px; margin: 0 auto; white-space: nowrap; font-size: 12px;
  cursor: grab; touch-action: none; user-select: none;
}
.capsule:active { transform: none; }
.window-dragging .capsule { cursor: grabbing; }
.island:not(.expanded) .capsule { height: var(--capsule-height); width: 100%; }
.island-hit-area:not(.desktop) .island .capsule { transition: opacity 110ms ease-out 40ms; }
.island-hit-area:not(.desktop) .island.tucked .capsule {
  visibility: visible;
  opacity: 0;
  transition: opacity 75ms ease-out;
}
.capsule-text {
  display: inline-flex; align-items: center; gap: 6px; font-size: 13px; line-height: 18px;
  font-weight: 600; letter-spacing: .22px; font-variant-numeric: tabular-nums; color: var(--text);
  transition: opacity 110ms ease-out 55ms, transform 150ms var(--geometry-ease);
}
.expanded .capsule-text:not(.feedback) { opacity: 0; transform: translateY(-3px) scale(.96); transition: opacity 65ms ease-out, transform 90ms ease-out; }
.capsule-text.feedback { font-size: 12px; letter-spacing: .2px; }
.capsule-announcement { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
.grip {
  position: absolute; width: 32px; height: 3px; border-radius: 8px;
  background: currentColor; opacity: 0; transform: scaleX(.72);
  transition: opacity 85ms ease-out, transform 130ms var(--geometry-ease);
}
.expanded .grip { opacity: .28; transform: scaleX(1); transition: opacity 130ms ease-out 75ms, transform 180ms var(--geometry-ease) 55ms; }
.expanded .capsule:hover .grip { opacity: .62; transform: scaleX(1.12); }
.capsule.has-feedback .grip { opacity: 0; }
.expanded-content {
  position: relative; z-index: 3; width: var(--content-width); max-height: calc(100% - 24px);
  overflow: hidden; padding: 0 18px 18px; opacity: 0;
  transform: translateY(-6px); clip-path: inset(0 0 100% round 18px);
  transition: opacity 95ms ease-out, transform var(--collapse-time) var(--geometry-ease), clip-path var(--collapse-time) var(--geometry-ease);
}
.expanded .expanded-content {
  overflow-y: auto; opacity: 1; transform: none; clip-path: inset(0 round 18px);
  transition: opacity 170ms ease-out 35ms, transform var(--expand-time) var(--geometry-ease), clip-path var(--expand-time) var(--geometry-ease);
}
.moving .expanded-content { overflow-y: hidden; }
.island-chrome { display: flow-root; }
.island-content { width: 100%; }
.ordinary-pane { display: contents; }
.island-pane-enter-active { transition: opacity 150ms cubic-bezier(.2, .9, .25, 1), transform 160ms cubic-bezier(.2, .9, .25, 1); }
.island-pane-leave-active { transition: opacity 85ms ease-out, transform 85ms ease-out; }
.island-pane-enter-from { opacity: 0; transform: translateY(5px); }
.island-pane-leave-to { opacity: 0; transform: translateY(-3px); }
.toolbar {
  display: flex; flex-wrap: nowrap; align-items: center; justify-content: space-between;
  gap: 8px; margin-bottom: 10px; padding: 0 0 8px; border-bottom: 1px solid var(--line);
}
.tools { display: flex; align-items: center; gap: 4px; flex: none; }
.round-action {
  display: grid; place-items: center; flex: none; width: 34px; height: 34px;
  border-radius: 10px; border: 1px solid transparent; background: transparent; color: var(--text-2);
}
.collapse-action svg { transform: rotate(180deg); }
.round-action:hover { background: var(--surface-3); border-color: var(--line); color: var(--text); }
.categories { display: flex; align-items: center; flex-wrap: wrap; gap: 5px; max-height: 80px; overflow-y: auto; margin: 0 0 10px; padding: 2px; }
.categories button { flex: none; padding: 5px 12px; border-radius: 20px; border: 1px solid transparent; min-height: 32px; font-size: 13px; color: var(--text-2); }
.categories .selected { color: var(--text); background: var(--surface-3); font-weight: 600; box-shadow: inset 0 0 0 1px var(--line-strong); }
.categories button:hover:not(.selected) { color: var(--text); background: var(--hover); border-color: var(--line); }
.app-grid {
  --app-icon-size: 28px; --app-cell-size: 46px; --app-gap: 8px;
  display: grid; grid-template-columns: repeat(auto-fill, var(--app-cell-size));
  grid-auto-rows: var(--app-cell-size); justify-content: start; align-content: start;
  max-height: calc(var(--app-cell-size) * 2 + var(--app-gap)); overflow-y: auto; gap: var(--app-gap); padding: 2px;
  scrollbar-width: thin; scrollbar-color: transparent transparent;
}
.app-tile {
  display: flex; align-items: center; justify-content: center;
  width: var(--app-cell-size); height: var(--app-cell-size); padding: 8px;
  border-radius: 12px; border: 1px solid transparent;
}
.app-tile:hover { background: var(--hover); border-color: var(--line); }
.app-grid:hover, .app-grid:focus-within { scrollbar-color: var(--line-strong) transparent; }
.app-tile img, .fallback { flex: none; width: var(--app-icon-size); height: var(--app-icon-size); object-fit: contain; }
.app-tile img { filter: drop-shadow(0 3px 2px rgba(0,0,0,.08)); }
.fallback { display: grid; place-items: center; border-radius: 8px; color: #fff; font-size: 12px; font-weight: 600; box-shadow: inset 0 1px 1px rgba(255,255,255,.4), 0 3px 5px -3px var(--island-shadow); }
.empty { display: flex; align-items: center; min-height: 64px; margin: 0; color: var(--text-2); font-size: 13px; line-height: 1.8; padding: 16px 4px; }
.mode-nav {
  position: relative; display: flex; flex: 1; align-items: center; gap: 4px;
  min-width: 0; width: auto; height: 42px; padding: 2px;
  overflow-x: auto; scrollbar-width: thin; color: var(--text);
}
.mode-selection {
  position: absolute; top: 3px; bottom: 3px; left: 0;
  width: var(--sel-w, 40px); border-radius: 11px;
  background: var(--accent-weak); box-shadow: inset 0 0 0 1px var(--line-strong);
  transform: translateX(var(--sel-x, 2px));
  transition: transform 200ms cubic-bezier(.2, .9, .25, 1), width 200ms cubic-bezier(.2, .9, .25, 1);
  pointer-events: none;
}
.mode-nav button {
  position: relative; z-index: 1; display: flex; flex: 0 0 40px; align-items: center; justify-content: center;
  height: 36px; min-width: 0; border-radius: 11px; color: var(--text-2); font-size: 14px;
}
.mode-nav .selected { color: var(--text); font-weight: 600; }
.mode-nav .selected svg { color: var(--accent); }
.mode-nav button:hover { color: var(--text); }
button { transition: background 130ms ease, color 130ms ease, border-color 130ms ease; }
:is(.categories button, .attention-strip button, .feature-pane button):active { transform: scale(.97); }
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
.feature-pane :deep(button:disabled) { opacity: 1; color: var(--text-3); background: var(--surface-2); cursor: not-allowed; }
.feature-pane :deep(.pop > .mode-bar), .feature-pane :deep(.pop > .head), .feature-pane :deep(.pop > .detail), .feature-pane :deep(.pop > .foot), .feature-pane :deep(.panel > .content) { padding-left: 0; padding-right: 0; }
.feature-pane :deep(.pop > .mode-bar) { padding-top: 2px; }
.feature-pane { --r-chip: 8px; --r-tile: 10px; }
.feature-pane :deep(.search) { height: 36px; border-radius: 11px; background: var(--surface-2); border: 1px solid var(--line); transition: border-color 120ms ease, box-shadow 120ms ease; }
.feature-pane :deep(.search:hover) { border-color: var(--line-strong); }
.feature-pane :deep(.search:focus-within) { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-weak); }
.feature-pane :deep(.pause) { height: 36px; border-radius: 11px; border: 1px solid var(--line); background: var(--surface-2); }
.feature-pane :deep(.filter-bar) { padding: 0 0 8px; }
.feature-pane :deep(.chips) { gap: 4px; padding: 2px; }
.feature-pane :deep(.chip) { height: 30px; padding: 0 12px; }
.feature-pane :deep(.filter-bar .chip) { height: 28px; padding: 0 8px; }
.feature-pane :deep(.var-bar .chip) { height: 24px; padding: 0 8px; }
.feature-pane :deep(.chips .chip:not(.is-on)) { border-color: transparent; background: var(--surface-2); }
.feature-pane :deep(.chips .chip:not(.is-on):hover) { background: var(--hover); border-color: var(--line); }
.feature-pane :deep(.chips .chip.is-on) { box-shadow: inset 0 0 0 1px var(--accent-weak); }
.feature-pane :deep(.list) { padding-left: 0; padding-right: 0; }
.feature-pane :deep(.row) { padding: 10px; border-radius: 9px; border-bottom: 1px solid var(--line); }
.feature-pane :deep(.row.is-active) { background: var(--accent-weak); box-shadow: inset 2px 0 0 var(--accent); }
.feature-pane :deep(.seg) { padding: 4px; border-radius: 22px; background: var(--surface-2); border: 1px solid var(--line); }
.feature-pane :deep(.seg-btn) { height: 30px; border-radius: 18px; }
.feature-pane :deep(.seg-btn.is-active) { box-shadow: inset 0 0 0 1px var(--line-strong), 0 1px 2px rgb(0 0 0 / .06); }
.feature-pane :deep(.textarea) { border-radius: 14px; background: var(--surface-2); border: 1px solid var(--line); transition: border-color 120ms ease, box-shadow 120ms ease; }
.feature-pane :deep(.textarea:focus-visible) { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-weak); }
.compact .expanded-content { padding-inline: 18px; }
.narrow .expanded-content { padding-inline: 14px; }
.narrow .categories { gap: 2px; }
.narrow .categories button { padding: 5px 10px; }

/* 星舰仪表主题：三态专属轮廓圆角、遥测状态灯、扫描光束与方正秩序控件 */
@keyframes telemetryPulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: .42; transform: scale(.8); }
}
@keyframes telemetrySweep {
  0% { transform: translateX(-80px); opacity: 0; }
  20% { opacity: 1; }
  80% { opacity: 1; }
  100% { transform: translateX(520px); opacity: 0; }
}
.island.telemetry { border-radius: 10px; }
.island.telemetry.expanded { border-radius: 12px; }
.island.telemetry.tucked { border-radius: 4px; }
.telemetry .capsule-text { font-family: var(--font-mono); font-size: 12px; letter-spacing: .06em; }
.telemetry-led {
  flex: none; width: 6px; height: 6px; border-radius: 50%;
  background: var(--accent); box-shadow: 0 0 8px var(--accent);
  animation: telemetryPulse 2.2s ease-in-out infinite;
}
.telemetry-led.working { background: #7de2ff; box-shadow: 0 0 10px #7de2ff; animation-duration: .95s; }
.telemetry-led.telemetry { background: var(--accent); box-shadow: 0 0 9px var(--accent); animation-duration: 1.5s; }
.telemetry-led.feedback { background: var(--success); box-shadow: 0 0 8px var(--success); animation: none; }
.telemetry-led.paused { background: var(--text-3); box-shadow: none; animation: none; opacity: .75; }
.telemetry-scan {
  position: absolute; inset: 0 auto 0 0; width: 84px; z-index: 2; pointer-events: none;
  background: linear-gradient(90deg, transparent, rgba(109, 182, 255, .24), transparent);
  animation: telemetrySweep 460ms cubic-bezier(.22, .61, .36, 1) forwards;
}
.telemetry .grip { height: 2px; width: 34px; border-radius: 1px; background: var(--accent); }
.telemetry .mode-selection {
  border-radius: 6px;
  background: rgba(109, 182, 255, .14);
  box-shadow: inset 0 -2px 0 var(--accent), inset 0 0 0 1px rgba(109, 182, 255, .36);
}
.telemetry .mode-nav button { border-radius: 6px; }
.telemetry .round-action { border-radius: 6px; border: 1px solid rgba(145, 180, 235, .14); }
.telemetry .categories button { border-radius: 6px; font-size: 12px; }
.telemetry .categories .selected { background: rgba(109, 182, 255, .16); box-shadow: inset 0 0 0 1px rgba(109, 182, 255, .42); }
.telemetry .app-tile {
  border-radius: 7px;
  background: rgba(109, 182, 255, .05);
  border-color: rgba(109, 182, 255, .16);
}
.telemetry .app-tile:hover {
  background: rgba(109, 182, 255, .14);
  border-color: rgba(109, 182, 255, .46);
  box-shadow: inset 0 0 10px rgba(109, 182, 255, .1);
}
.telemetry .attention-strip {
  border-radius: 6px;
  border: 1px solid rgba(109, 182, 255, .24);
  box-shadow: inset 2px 0 0 var(--accent);
}
.telemetry .attention-strip button { border-radius: 5px; border: 1px solid rgba(109, 182, 255, .2); }
.telemetry .app-grid:hover, .telemetry .app-grid:focus-within { scrollbar-color: rgba(109, 182, 255, .5) rgba(109, 182, 255, .06); }
.telemetry .feature-pane { --r-chip: 6px; --r-tile: 7px; }
.telemetry .feature-pane :deep(.search),
.telemetry .feature-pane :deep(.pause) {
  border-radius: 6px;
  border: 1px solid rgba(109, 182, 255, .2);
}
.telemetry .feature-pane :deep(.search:focus-within) {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px rgba(109, 182, 255, .36), 0 0 12px rgba(109, 182, 255, .14);
}
.telemetry .feature-pane :deep(.chip) { border-radius: 5px; }
.telemetry .feature-pane :deep(.chip.is-on) {
  border-color: rgba(109, 182, 255, .46);
  box-shadow: inset 0 0 0 1px rgba(109, 182, 255, .18);
}
.telemetry .feature-pane :deep(.row) {
  border-radius: 6px;
  border-bottom-color: rgba(109, 182, 255, .1);
}
.telemetry .feature-pane :deep(.row:hover) {
  box-shadow: inset 2px 0 0 rgba(109, 182, 255, .48);
}
.telemetry .feature-pane :deep(.row.is-active) {
  box-shadow: inset 2px 0 0 var(--accent), inset 0 0 0 1px rgba(109, 182, 255, .26);
}
.telemetry .feature-pane :deep(.seg) {
  padding: 3px;
  border-radius: 7px;
  border: 1px solid rgba(109, 182, 255, .18);
}
.telemetry .feature-pane :deep(.seg-btn) { height: 28px; border-radius: 5px; }
.telemetry .feature-pane :deep(.textarea) {
  border-radius: 8px;
  border-color: rgba(109, 182, 255, .2);
}
.telemetry .feature-pane :deep(.textarea:focus-visible) {
  border-color: var(--accent);
  box-shadow: 0 0 12px rgba(109, 182, 255, .14);
}
.capsule-label { display: inline-flex; align-items: center; gap: 5px; }
.capsule-label.is-telemetry { gap: 4px; font-size: 12px; letter-spacing: 0; font-variant-numeric: tabular-nums; }
.tel-sep { font-style: normal; opacity: .4; margin: 0 1px; }
.tel-seg { display: inline-flex; align-items: baseline; gap: 2.5px; }
.tel-pre { font-style: normal; font-size: 11px; font-weight: 600; color: var(--text-2); }
.tel-pre.accent { color: var(--accent); font-weight: 700; }
.variant-scroll:not(.expanded) .capsule-label.is-telemetry { font-family: var(--font-sans); letter-spacing: 0; }
.variant-pixel:not(.expanded) .capsule-label.is-telemetry { padding: 1px 6px; font-size: 11.5px; }
.variant-pixel:not(.expanded) .tel-pre { color: #86efac; }
.variant-pixel:not(.expanded) .tel-pre.accent { color: #facc15; }

/* ==================== 琥珀重工（variant-mecha：45° 倒角装甲与液压导轨） ==================== */
.island.variant-mecha {
  border-radius: 2px;
  box-shadow: none;
  clip-path: polygon(8px 0, calc(100% - 8px) 0, 100% 8px, 100% calc(100% - 8px), calc(100% - 8px) 100%, 8px 100%, 0 calc(100% - 8px), 0 8px);
}
.island.variant-mecha.expanded {
  clip-path: polygon(13px 0, calc(100% - 13px) 0, 100% 13px, 100% calc(100% - 13px), calc(100% - 13px) 100%, 13px 100%, 0 calc(100% - 13px), 0 13px);
}
.island.variant-mecha.tucked { clip-path: none; }
.variant-mecha .telemetry-scan { background: linear-gradient(90deg, transparent, rgba(245, 158, 11, .3), transparent); }
.variant-mecha:not(.expanded) .capsule-text { width: 100%; justify-content: space-between; padding: 0 12px; font-family: var(--font-mono); font-size: 12px; }
.mecha-badge {
  padding: 1px 5px; font-size: 10px; font-weight: 800; letter-spacing: .04em;
  background: var(--accent); color: #111318;
  clip-path: polygon(4px 0, 100% 0, 100% 100%, 0 100%, 0 4px);
}
.mecha-ribs { display: inline-flex; gap: 2px; opacity: .75; }
.mecha-ribs i { display: block; width: 2px; height: 10px; background: var(--accent); }
.variant-mecha .grip {
  width: 38px; height: 3px; border-radius: 0;
  background: repeating-linear-gradient(90deg, var(--accent) 0 8px, transparent 8px 12px);
}
.variant-mecha .mode-selection {
  border-radius: 0; background: var(--accent); box-shadow: none;
  clip-path: polygon(6px 0, 100% 0, 100% calc(100% - 6px), calc(100% - 6px) 100%, 0 100%, 0 6px);
}
.variant-mecha .mode-nav .selected,
.variant-mecha .mode-nav .selected svg { color: #111318; }
.variant-mecha .round-action {
  border-radius: 3px; border-color: rgba(245, 158, 11, .24);
  clip-path: polygon(4px 0, 100% 0, 100% calc(100% - 4px), calc(100% - 4px) 100%, 0 100%, 0 4px);
}
.variant-mecha .categories button {
  border-radius: 2px; font-family: var(--font-mono); font-size: 12px;
  clip-path: polygon(5px 0, 100% 0, 100% calc(100% - 5px), calc(100% - 5px) 100%, 0 100%, 0 5px);
}
.variant-mecha .categories .selected { background: var(--accent); color: #111318; box-shadow: none; }
.variant-mecha .app-tile {
  position: relative; border-radius: 0; background: var(--surface-2); border-color: rgba(245, 158, 11, .22);
  clip-path: polygon(6px 0, 100% 0, 100% calc(100% - 6px), calc(100% - 6px) 100%, 0 100%, 0 6px);
}
.variant-mecha .app-tile::after {
  content: ''; position: absolute; bottom: 3px; left: 50%; width: 14px; height: 2px;
  transform: translateX(-50%); background: rgba(245, 158, 11, .45); pointer-events: none;
}
.variant-mecha .app-tile:hover { border-color: var(--accent); background: var(--surface-3); }
.variant-mecha .app-tile:hover::after { background: var(--accent); }
.variant-mecha .feature-pane { --r-chip: 3px; --r-tile: 4px; }
.variant-mecha .feature-pane :deep(.search),
.variant-mecha .feature-pane :deep(.pause),
.variant-mecha .feature-pane :deep(.seg),
.variant-mecha .feature-pane :deep(.seg-btn),
.variant-mecha .feature-pane :deep(.textarea),
.variant-mecha .feature-pane :deep(.row) { border-radius: 3px; }
.variant-mecha .feature-pane :deep(.row:hover) { box-shadow: inset 3px 0 0 rgba(245, 158, 11, .55); }

/* ==================== 海棠书卷（variant-scroll：四角内凹花窗、燕尾红签与折角方笺） ==================== */
.island.variant-scroll {
  border-radius: 2px; box-shadow: none;
  -webkit-mask:
    radial-gradient(circle 7px at 0 0, transparent 96%, #000 100%) top left,
    radial-gradient(circle 7px at 100% 0, transparent 96%, #000 100%) top right,
    radial-gradient(circle 7px at 0 100%, transparent 96%, #000 100%) bottom left,
    radial-gradient(circle 7px at 100% 100%, transparent 96%, #000 100%) bottom right;
  -webkit-mask-size: 51% 51%;
  -webkit-mask-repeat: no-repeat;
}
.island.variant-scroll.expanded {
  -webkit-mask:
    radial-gradient(circle 11px at 0 0, transparent 96%, #000 100%) top left,
    radial-gradient(circle 11px at 100% 0, transparent 96%, #000 100%) top right,
    radial-gradient(circle 11px at 0 100%, transparent 96%, #000 100%) bottom left,
    radial-gradient(circle 11px at 100% 100%, transparent 96%, #000 100%) bottom right;
  -webkit-mask-size: 51% 51%;
  -webkit-mask-repeat: no-repeat;
}
.island.variant-scroll.tucked { -webkit-mask: none; }
.variant-scroll:not(.expanded) .capsule-text { width: 100%; justify-content: space-between; padding: 0 14px; font-family: Georgia, 'Songti SC', 'SimSun', serif; letter-spacing: .08em; }
.scroll-seal {
  padding: 1px 5px; border-radius: 1px; background: var(--accent); color: #fff;
  font-size: 10px; font-weight: 700; box-shadow: inset 0 0 0 1px rgba(255, 255, 255, .38);
}
.scroll-knot { width: 6px; height: 6px; transform: rotate(45deg); border: 1.4px solid var(--accent); }
.variant-scroll .grip { width: 30px; height: 2px; border-radius: 0; background: rgba(31, 36, 33, .45); }
.variant-scroll .toolbar { border-bottom: 1px solid rgba(31, 36, 33, .22); padding-top: 0; }
.variant-scroll .mode-selection {
  top: 0; bottom: 0; border-radius: 0; background: var(--accent); box-shadow: none;
  clip-path: polygon(0 0, 100% 0, 100% 100%, 50% 80%, 0 100%);
}
.variant-scroll .mode-nav .selected,
.variant-scroll .mode-nav .selected svg { color: #fff; transform: translateY(-2px); }
.variant-scroll .round-action { border-radius: 3px; border-color: rgba(31, 36, 33, .2); }
.variant-scroll .categories button { border-radius: 2px; border-color: rgba(31, 36, 33, .14); font-family: 'Songti SC', 'SimSun', var(--font-sans); }
.variant-scroll .categories .selected { background: var(--accent); color: #fff; border-color: var(--accent); box-shadow: inset 0 0 0 1px rgba(255, 255, 255, .35); }
.variant-scroll .app-tile {
  position: relative; border-radius: 0; background: var(--surface-2); border: 1px solid rgba(31, 36, 33, .2);
  clip-path: polygon(0 0, calc(100% - 8px) 0, 100% 8px, 100% 100%, 0 100%);
}
.variant-scroll .app-tile::after {
  content: ''; position: absolute; top: 0; right: 0; width: 8px; height: 8px;
  background: #cfc6b2; border-left: 1px solid rgba(31, 36, 33, .25); border-bottom: 1px solid rgba(31, 36, 33, .25);
  pointer-events: none;
}
.variant-scroll .app-tile:hover { border-color: var(--accent); background: #f2ece0; }
.variant-scroll .feature-pane { --r-chip: 3px; --r-tile: 3px; }
.variant-scroll .feature-pane :deep(.search),
.variant-scroll .feature-pane :deep(.pause),
.variant-scroll .feature-pane :deep(.seg),
.variant-scroll .feature-pane :deep(.seg-btn),
.variant-scroll .feature-pane :deep(.textarea) { border-radius: 3px; border-color: rgba(31, 36, 33, .24); }
.variant-scroll .feature-pane :deep(.row) { border-radius: 2px; border-bottom: 1px dashed rgba(31, 36, 33, .22); }

/* ==================== 像素掌机（variant-pixel：锯齿外壳、双层液晶内屏与 3D 厚底按键） ==================== */
.island.variant-pixel {
  border-radius: 2px; box-shadow: none;
  clip-path: polygon(
    6px 0, calc(100% - 6px) 0, calc(100% - 6px) 3px, calc(100% - 3px) 3px, calc(100% - 3px) 6px, 100% 6px,
    100% calc(100% - 6px), calc(100% - 3px) calc(100% - 6px), calc(100% - 3px) calc(100% - 3px), calc(100% - 6px) calc(100% - 3px), calc(100% - 6px) 100%,
    6px 100%, 6px calc(100% - 3px), 3px calc(100% - 3px), 3px calc(100% - 6px), 0 calc(100% - 6px),
    0 6px, 3px 6px, 3px 3px, 6px 3px
  );
}
.island.variant-pixel.tucked { clip-path: none; }
.variant-pixel:not(.expanded) .capsule-text { width: 100%; justify-content: space-between; padding: 0 11px; font-family: var(--font-mono); font-size: 12px; }
.variant-pixel:not(.expanded) .capsule-label {
  padding: 1px 7px; background: #1c221e; border: 2px solid #0f0d1a; color: #bef264; font-weight: 700;
}
.pixel-hp { color: #a3e635; font-size: 11px; font-weight: 700; }
.pixel-lv { color: #facc15; font-size: 10px; font-weight: 700; }
.variant-pixel .grip {
  width: 34px; height: 4px; border-radius: 0;
  background: repeating-linear-gradient(90deg, #0f0d1a 0 6px, transparent 6px 10px); opacity: .75;
}
.variant-pixel .toolbar { border-bottom: 0; margin-bottom: 8px; padding-bottom: 2px; }
.variant-pixel .mode-selection { display: none; }
.variant-pixel .mode-nav { gap: 6px; padding: 2px 2px 5px; }
.variant-pixel .mode-nav button,
.variant-pixel .round-action {
  border-radius: 0; background: #4a436b; color: #e2e8f0;
  border: 2px solid #0f0d1a; box-shadow: 0 3px 0 #0f0d1a;
}
.variant-pixel .mode-nav button:hover,
.variant-pixel .round-action:hover { background: #5a5280; color: #fff; }
.variant-pixel .mode-nav .selected {
  background: var(--accent); color: #0f0d1a;
  transform: translateY(1px); box-shadow: 0 2px 0 #0f0d1a;
}
.variant-pixel .mode-nav .selected svg { color: #0f0d1a; }
.variant-pixel .mode-nav button:active,
.variant-pixel .round-action:active { transform: translateY(3px); box-shadow: none; }
.variant-pixel .island-content {
  background: #1c221e; border: 2px solid #0f0d1a; padding: 10px;
  box-shadow: inset 0 3px 0 rgba(0, 0, 0, .45);
}
.variant-pixel .categories button {
  border-radius: 0; font-family: var(--font-mono); font-size: 12px;
  background: #2b352e; border: 2px solid #0f0d1a; box-shadow: 0 2px 0 #0f0d1a; color: #cbd5e1;
}
.variant-pixel .categories .selected { background: #facc15; color: #0f0d1a; border-color: #0f0d1a; box-shadow: 0 1px 0 #0f0d1a; transform: translateY(1px); }
.variant-pixel .app-tile {
  border-radius: 0; background: #2b352e; border: 2px solid #0f0d1a; box-shadow: 0 3px 0 #0f0d1a;
}
.variant-pixel .app-tile:hover { background: #36443a; border-color: #0f0d1a; }
.variant-pixel .app-tile:active { transform: translateY(3px); box-shadow: none; }
.variant-pixel .feature-pane { --r-chip: 0px; --r-tile: 0px; font-family: var(--font-mono); }
.variant-pixel .feature-pane :deep(.search),
.variant-pixel .feature-pane :deep(.pause),
.variant-pixel .feature-pane :deep(.seg),
.variant-pixel .feature-pane :deep(.seg-btn),
.variant-pixel .feature-pane :deep(.snippet-input),
.variant-pixel .feature-pane :deep(.snippet-select),
.variant-pixel .feature-pane :deep(.textarea) {
  border-radius: 0; border: 2px solid #0f0d1a; background: #252e27; box-shadow: inset 0 2px 0 rgba(0, 0, 0, .35);
}
.variant-pixel .feature-pane :deep(.seg-btn.is-active) {
  background: var(--accent); color: #0f0d1a; box-shadow: none;
}
.variant-pixel .feature-pane :deep(.seg-btn.is-active .seg-count) {
  background: #0f0d1a; color: #facc15; border-radius: 0;
}
.variant-pixel .feature-pane :deep(.row) {
  border-radius: 0; border: 2px solid #0f0d1a; background: #252e27; margin-bottom: 4px;
}
.variant-pixel .feature-pane :deep(.row.is-active) {
  background: var(--accent); color: #0f0d1a; box-shadow: none;
}
.variant-pixel .feature-pane :deep(.row.is-active *) { color: #0f0d1a; }

.reduced :deep(*), .reduced :deep(*::before), .reduced :deep(*::after) { transition: none !important; animation: none !important; }
@media (prefers-reduced-motion: reduce) { *, *::before, *::after { transition: none !important; animation:none !important; } }
@media (prefers-contrast: more) {
  .mode-selection, .categories .selected { box-shadow: inset 0 0 0 2px var(--accent); }
}
</style>

<style scoped>
.expanded-content.fixed-search { display: flex; flex-direction: column; overflow: hidden; }
.fixed-search .island-chrome { flex: none; }
.fixed-search .island-content { min-height: 0; display: flex; flex-direction: column; }
.fixed-search .feature-pane { min-height: 0; overflow: hidden; display: flex; flex-direction: column; }
.fixed-search .feature-pane :deep(.pop) { height: 420px; min-height: 0; max-height: 420px; }
</style>
