<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import GlassLight from '@/components/GlassLight.vue'
import LiquidSkin, { type IslandVariant } from './motion/LiquidSkin.vue'
import IslandParticles from './motion/IslandParticles.vue'
import ApprovalPrompt from './ApprovalPrompt.vue'
import OcrResultPanel from '@/features/ocr/OcrResultPanel.vue'
import MailNotices from '@/features/mail/MailNotices.vue'
import { providerName, useMailList, type MailNotice } from '@/features/mail/mail'
import { useApprovals } from './useApprovals'
import { useIslandEffects, type IslandCue } from './motion/useIslandEffects'
import { advanceSpring } from './motion/physics'
import {
  computeDismissPull,
  shouldAutoCollapseOnWindowBlur,
  shouldCommitDismiss,
  shouldKeepTranslationOpen
} from './dismissGesture'
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
import PackConsole from '@/features/pack/PackConsole.vue'
import { capsuleTaskText, useTaskRunner } from '@/features/pack/useTaskRunner'
import type { IconName } from '@/components/icon-data'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { useWindowDrag } from '@/lib/useWindowDrag'
import { invokeHost, isDesktop, listenHost, resizeBar, type BarRegion } from '@/lib/desktop/desktop'
import {
  applyOcrError,
  applyOcrResult,
  closeOcrPanel,
  type OcrResultEvent,
  runClipboardOcr,
  startScreenOcr,
  isDarkTheme,
  pushToast,
  iconFor,
  launchApp,
  openSettings,
  setCategory,
  setPaused,
  state,
  visibleApps
} from '@/store'
import { MOTION_TIMINGS, normalizeMotionMode, type MotionMode } from '@/lib/motions'

const props = defineProps<{
  previewFeedback?: { kind: string; sequence: number }
  previewKeepOpen?: boolean
  previewCue?: IslandCue
  previewTelemetry?: TelemetrySnapshot | null
  previewMail?: MailNotice[]
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
  if (mode === 'liquid') return 'liquid'
  if (mode === 'midnight') return 'telemetry'
  if (mode === 'mecha') return 'mecha'
  if (mode === 'scroll') return 'scroll'
  if (mode === 'pixel') return 'pixel'
  if (mode === 'blueprint') return 'blueprint'
  if (mode === 'neon') return 'neon'
  if (mode === 'clay') return 'clay'
  return 'standard'
}
const systemDark = window.matchMedia('(prefers-color-scheme: dark)')
const dark = ref(isDarkTheme())
const variant = ref<IslandVariant>(resolveVariant())
const telemetry = computed(() => variant.value === 'telemetry')
const glassPalette = computed<'cool' | 'liquid' | 'amber' | 'vermilion' | 'lime' | 'chalk' | 'neon' | 'candy'>(() => {
  if (variant.value === 'liquid') return 'liquid'
  if (variant.value === 'mecha') return 'amber'
  if (variant.value === 'scroll') return 'vermilion'
  if (variant.value === 'pixel') return 'lime'
  if (variant.value === 'blueprint') return 'chalk'
  if (variant.value === 'neon') return 'neon'
  if (variant.value === 'clay') return 'candy'
  return 'cool'
})
/** 霓虹招牌与软糖表情随胶囊状态切换，只做装饰，读屏仍以状态文字为准。 */
const neonSign = computed(() => ({ working: '加班中', paused: '打烊', feedback: '好嘞', mail: '来信', approval: '请示', files: '寄存', 'quick-ocr': '识字' } as Record<string, string>)[capsuleStatus.value.kind] ?? '营业中')
const clayFace = computed(() => ({ working: '•̀ᴗ•́', paused: '-ᴗ-', feedback: '^ᴗ^', mail: '•o•', approval: '•﹏•', files: '•ᴗ•', 'quick-ocr': '•̀ω•́' } as Record<string, string>)[capsuleStatus.value.kind] ?? 'ᵔᴗᵔ')
const hostExpanded = computed(() => state.ui.islandExpanded)
const hostMailNotices = useMailList<MailNotice>('bar', 'mail_notices', 'mail-notices-changed', message => pushToast(message, 'error'))
const mailNotices = computed(() => (isDesktop ? hostMailNotices.value : (props.previewMail?.length ? props.previewMail : hostMailNotices.value)))
const mailSelected = ref(false)
const viewingMail = computed(() => mailSelected.value && mailNotices.value.length > 0)
const seenMail = ref<string[]>([])
const newMail = computed(() => mailNotices.value.filter(item => !seenMail.value.includes(item.id)))
const quickMailCode = computed(() => newMail.value.find(item => !!item.code?.trim()) ?? null)
const copyingQuickCode = ref(false)
interface QuickImageOcrPayload {
  clipId: string
  width?: number | null
  height?: number | null
}
const quickOcrClip = ref<QuickImageOcrPayload | null>(null)
const hoveringQuickAction = ref(false)
const pinned = ref(false)
const quickMenuOpen = ref(false)
const previewVolume = ref(72)
let volumeBusy = false
const appSearch = ref('')
const appSearchEl = ref<HTMLInputElement | null>(null)
const showAppNames = computed(() => state.ui.islandAppearance.showAppNames !== false)
const filteredApps = computed(() => {
  const q = appSearch.value.trim().toLowerCase()
  if (!q) return visibleApps.value
  return visibleApps.value.filter(app =>
    app.name.toLowerCase().includes(q) || app.target.toLowerCase().includes(q)
  )
})
const mailSummary = computed(() => {
  const [first] = newMail.value
  if (!first) return ''
  if (newMail.value.length > 1) return `${newMail.value.length} 封新邮件`
  return `${providerName[first.provider]} · ${first.code ? '验证码' : '新邮件'}`
})
const { current: approval, count: approvalCount, busy: answering, answer: answerApproval } =
  useApprovals(message => pushToast(message, 'error'))
// 打包 / 联调台的任务状态：只在运行中占用胶囊中间的位置。
const { task: runTask, elapsedMs: runElapsed } = useTaskRunner(message => pushToast(message, 'error'))
const suppressed = computed(() => state.ui.fullscreenSuppressed)
const hoverClosing = ref(false)
const expanded = computed(() => !!approval.value || (hostExpanded.value && !hoverClosing.value))
watch([mailNotices, viewingMail, expanded, approval], ([items, selected, open, prompt]) => {
  if (!items.length) mailSelected.value = false
  const ids = items.map(item => item.id)
  seenMail.value = selected && open && !prompt ? ids : seenMail.value.filter(id => ids.includes(id))
})
// 打包/联调任务结束才给胶囊反馈；before === 'idle' 表示只是加载到上次结果，不补播过期提示。
watch(() => runTask.value.stage, (stage, before) => {
  if (before === 'idle') return
  if (stage === 'done') showFeedback('task-done', runTask.value.kind === 'pack' ? '打包完成' : '执行完成')
  else if (stage === 'cancelled') showFeedback('task-cancelled', '已取消')
  else if (stage === 'error') showFeedback('task-error', '执行失败')
})
// 避让只决定收起形态；细线也能悬停展开。
const tucked = computed(() => suppressed.value && !expanded.value)
const expandMs = computed(() => MOTION_TIMINGS[motionMode.value].expandMs)
const collapseMs = computed(() => MOTION_TIMINGS[motionMode.value].collapseMs)
const geometryMoving = ref(false)
const systemMotion = window.matchMedia('(prefers-reduced-motion: reduce)')
const reducedMotion = ref(systemMotion.matches)
const motionReduced = computed(() => state.ui.islandReducedMotion || reducedMotion.value)
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
const showQuickOcr = computed(() =>
  !expanded.value && !tucked.value && !approval.value && !quickMenuOpen.value && !!quickOcrClip.value
)
const capsuleStatus = computed<{ text: string; ariaText?: string; tooltip?: string; icon: IconName | null; kind: string }>(() => {
  if (approval.value) return { text: approval.value.kind === 'approval' ? '待审批' : '有提问', ariaText: approval.value.title, icon: 'info', kind: 'approval' }
  // 打包/联调任务运行中：胶囊中间实时显示「任务名 · 约剩 mm:ss」
  if (runTask.value.stage === 'running') return { text: capsuleTaskText(runTask.value, runElapsed.value), ariaText: `${runTask.value.name}：${runTask.value.message}`, tooltip: '打包 / 联调任务运行中', icon: 'terminal', kind: 'working' }
  if (receivingFiles.value) return { text: '松开暂存', icon: 'folder', kind: 'working' }
  if (shelf.busy) return { text: shelf.activity || '文件处理中', icon: 'folder', kind: 'working' }
  const pending = Math.max(state.ui.translationPending, state.translation.status === 'loading' ? 1 : 0)
  if (pending) return { text: pending > 1 ? `翻译中 · ${pending}` : '翻译中', icon: 'translate', kind: 'working' }
  if (feedbackText.value) return { text: feedbackText.value, icon: feedbackText.value === '翻译失败' ? 'info' : 'check', kind: 'feedback' }
  if (showQuickOcr.value) {
    return {
      text: '已截屏',
      ariaText: '检测到新截图，可点击右侧识字按钮识别文字',
      tooltip: '刚捕获新截图 · 点击右侧「识字」直接提取文字',
      icon: 'image',
      kind: 'quick-ocr'
    }
  }
  if (telemetryReadout.value) {
    return {
      text: telemetryReadout.value.text,
      ariaText: telemetryReadout.value.ariaText,
      tooltip: telemetryReadout.value.tooltip,
      icon: null,
      kind: 'telemetry'
    }
  }
  return { text: clockText.value, icon: null, kind: 'idle' }
})
/** 收起态右侧副舱徽标：与时钟/微读数并存，不再互相挤掉。 */
const capsuleBadges = computed<{ id: string; icon: IconName; text: string; title: string; tone?: 'accent' | 'paused' }[]>(() => {
  if (expanded.value || tucked.value || approval.value || receivingFiles.value || shelf.busy || feedbackText.value || showQuickOcr.value) return []
  const list: { id: string; icon: IconName; text: string; title: string; tone?: 'accent' | 'paused' }[] = []
  if (newMail.value.length && !quickMailCode.value) {
    list.push({ id: 'mail', icon: 'mail', text: String(newMail.value.length), title: mailSummary.value || '未读邮件', tone: 'accent' })
  }
  if (shelf.entries.length) {
    list.push({ id: 'files', icon: 'folder', text: shelf.entries.length > 99 ? '99+' : String(shelf.entries.length), title: `暂存 ${shelf.entries.length} 个文件` })
  }
  if (state.ui.paused) {
    list.push({ id: 'paused', icon: 'pause', text: '暂停', title: '剪贴板已暂停记录', tone: 'paused' })
  }
  return list
})
const showQuickCode = computed(() => !expanded.value && !tucked.value && !approval.value && !feedbackText.value && !showQuickOcr.value && !!quickMailCode.value)
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
const modeSelection = ref<HTMLElement | null>(null)
const paneDir = ref<1 | -1>(1)
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
const ocrPanelOpen = computed(() => state.ocr.open)
const keepTaskOpen = computed(() => ocrPanelOpen.value || (!viewingMail.value &&
  ((page.value === 'translation' && shouldKeepTranslationOpen(state.translation)) ||
   (page.value === 'clipboard' && readingExplanation.value))))
const appearance = computed(() => state.ui.islandAppearance)
const motionMode = computed<MotionMode>(() => normalizeMotionMode(appearance.value.motion))
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
  const badgeExtra = showQuickOcr.value
    ? 116 * scale.value
    : showQuickCode.value
      ? 112 * scale.value
      : capsuleBadges.value.reduce((acc, item) => acc + (item.text.length > 2 ? 48 : 34) * scale.value, 0)
  // 快捷操作条 5 个按钮时的宽度下限。
  const menuFloor = quickMenuOpen.value ? 412 * scale.value : 0
  if (capsuleStatus.value.kind === 'telemetry' && telemetryReadout.value) {
    const count = telemetryReadout.value.metricCount
    const table: Record<IslandVariant, [number, number][]> = {
      standard: [[140, 118], [188, 168], [240, 218]],
      liquid: [[156, 134], [202, 182], [254, 232]],
      telemetry: [[158, 136], [204, 184], [256, 234]],
      scroll: [[178, 156], [226, 206], [278, 256]],
      mecha: [[190, 168], [240, 220], [292, 270]],
      pixel: [[204, 182], [258, 238], [312, 290]],
      blueprint: [[196, 174], [248, 228], [300, 280]],
      neon: [[200, 178], [252, 232], [304, 284]],
      clay: [[176, 156], [226, 206], [278, 258]]
    }
    const [base, floor] = table[variant.value][Math.min(2, Math.max(0, count - 1))]
    return Math.min(availableWidth.value - 24, Math.max(floor, menuFloor, Math.round(base * scale.value + badgeExtra)))
  }
  const idle = capsuleStatus.value.kind === 'idle'
  let baseWidth = (idle ? 128 : 168) * scale.value
  if (variant.value === 'liquid') baseWidth = (idle ? 142 : 178) * scale.value
  else if (variant.value === 'telemetry') baseWidth = (idle ? 148 : 186) * scale.value
  else if (variant.value === 'mecha' || variant.value === 'pixel') baseWidth = (idle ? 176 : 208) * scale.value
  else if (variant.value === 'scroll') baseWidth = (idle ? 164 : 196) * scale.value
  else if (variant.value === 'blueprint' || variant.value === 'neon') baseWidth = (idle ? 184 : 216) * scale.value
  else if (variant.value === 'clay') baseWidth = (idle ? 168 : 200) * scale.value
  if (runTask.value.stage === 'running') {
    baseWidth = Math.max(baseWidth, 236 * scale.value)
  }
  return Math.min(availableWidth.value - 24, Math.max(menuFloor, Math.round(baseWidth + badgeExtra)))
})
// 内容高度已经来自实际布局；再次乘缩放比例会让小尺寸裁切、大尺寸多出留白。
const lineHeights: Record<IslandVariant, number> = { standard: 5, liquid: 6, telemetry: 7, mecha: 8, scroll: 9, pixel: 9, blueprint: 8, neon: 6, clay: 8 }
const compactHeights: Record<IslandVariant, number> = { standard: 32, liquid: 34, telemetry: 32, mecha: 34, scroll: 34, pixel: 36, blueprint: 34, neon: 34, clay: 36 }
const maxExpandedHeight = computed(() => Math.max(220, window.screen.availHeight - 60))
const rawExpandedHeight = computed(() =>
  48 + (approval.value ? 0 : chromeHeight.value) + (!approval.value && !viewingMail.value && !ocrPanelOpen.value && page.value === 'clipboard' && !state.ui.detailId ? 420 : contentHeight.value)
)
const needsOuterScroll = computed(() =>
  expanded.value && !geometryMoving.value && !pageSwitching.value && rawExpandedHeight.value > maxExpandedHeight.value
)
const visibleHeight = computed(() => {
  if (expanded.value) {
    return Math.min(rawExpandedHeight.value, maxExpandedHeight.value)
  }
  if (suppressed.value) return lineHeights[variant.value] * scale.value
  return compactHeights[variant.value] * scale.value + (quickMenuOpen.value ? 42 : 0)
})
const cornerRadius = computed(() => {
  if (variant.value === 'mecha' || variant.value === 'scroll' || variant.value === 'pixel') return 4
  if (variant.value === 'blueprint') return 2
  const base = expanded.value
    ? (telemetry.value ? 12 : variant.value === 'clay' ? 28 : variant.value === 'liquid' ? 22 : variant.value === 'neon' ? 20 : 18)
    : tucked.value ? (telemetry.value ? 4 : 5) : quickMenuOpen.value ? 16 : (telemetry.value ? 10 : variant.value === 'liquid' ? 26 : 24)
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
  '--collapse-time': `${collapseMs.value}ms`,
  '--expand-time': `${expandMs.value}ms`,
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
  { value: 'audio', label: '调音台', icon: 'volume' },
  { value: 'pack', label: '打包', icon: 'terminal' }
] as const
let navMeasureRaf = 0
let lastNavLeft = -1
let indicatorStretchAnim: Animation | null = null
const pageSwitching = ref(false)
let pageSwitchTimer: ReturnType<typeof setTimeout> | undefined
const pageSwitchDuration = () => (
  motionMode.value === 'flicker'
    ? 100
    : motionMode.value === 'step'
      ? 135
      : motionMode.value === 'sweep'
        ? 170
        : motionMode.value === 'bounce'
          ? 185
          : 150
)
function revealSelectedPage(): void {
  navMeasureRaf = 0
  const nav = modeNav.value
  const selected = nav?.querySelector<HTMLElement>('button.selected')
  if (!nav || !selected) return
  const indicator = modeSelection.value
  if (indicator) {
    const nextLeft = selected.offsetLeft
    const nextWidth = selected.offsetWidth
    const delta = lastNavLeft >= 0 ? nextLeft - lastNavLeft : 0
    indicator.style.transform = `translate3d(${nextLeft}px, 0, 0)`
    indicator.style.width = `${nextWidth}px`
    // 粘液蠕虫模式：导航指示块横向切换时短暂沿移动方向拉伸再回弹（可被随时打断）
    if (motionMode.value === 'sweep' && !motionReduced.value && Math.abs(delta) > 4 && typeof indicator.animate === 'function') {
      indicatorStretchAnim?.cancel()
      const sx = Math.min(1.32, 1 + Math.abs(delta) / 140)
      indicatorStretchAnim = indicator.animate(
        [
          { scale: '1 1', offset: 0 },
          { scale: `${sx} 0.92`, offset: 0.42 },
          { scale: '0.96 1.03', offset: 0.78 },
          { scale: '1 1', offset: 1 }
        ],
        { duration: 220, easing: 'cubic-bezier(0.23, 1, 0.32, 1)' }
      )
    }
    lastNavLeft = nextLeft
  }
  if (selected.offsetLeft < nav.scrollLeft) nav.scrollLeft = selected.offsetLeft
  else if (selected.offsetLeft + selected.offsetWidth > nav.scrollLeft + nav.clientWidth) nav.scrollLeft = selected.offsetLeft + selected.offsetWidth - nav.clientWidth
}
function syncSelectedIndicator(): void {
  revealSelectedPage()
  cancelAnimationFrame(navMeasureRaf)
  navMeasureRaf = requestAnimationFrame(revealSelectedPage)
}
watch([page, expanded, variant, viewingMail], syncSelectedIndicator, { flush: 'post' })
watch(scale, syncSelectedIndicator, { flush: 'post' })
onMounted(syncSelectedIndicator)
let hoverTimer: ReturnType<typeof setTimeout> | undefined
let leaveTimer: ReturnType<typeof setTimeout> | undefined
let closeTimer: ReturnType<typeof setTimeout> | undefined
let manualCollapseUntil = 0
let closeCommitted = false
let reenterAfterClose = false
let clockTimer: ReturnType<typeof setTimeout> | undefined
let feedbackTimer: ReturnType<typeof setTimeout> | undefined
let quickOcrTimer: ReturnType<typeof setTimeout> | undefined
let stopFeedback: (() => void) | undefined
let stopTelemetry: (() => void) | undefined
let stopWindowBlur: (() => void) | undefined
let stopOcrResult: (() => void) | undefined
let stopOcrError: (() => void) | undefined
let stopQuickImageOcr: (() => void) | undefined
const dismissHandle = ref<HTMLButtonElement | null>(null)
let dismissSpring = { value: 0, velocity: 0, target: 0 }
let dismissSpringFrame = 0
let dismissSpringPrev = 0
let suppressDismissClick = false
let dismissSession: {
  pointerId: number
  handle: HTMLButtonElement
  startX: number
  startY: number
  baseOffsetY: number
  moved: boolean
  samples: { y: number; ts: number }[]
  lastOffsetY: number
} | undefined
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
function showFeedback(kind: string, customLabel?: string): void {
  if (tucked.value) return
  if (!customLabel && !['copied', 'translated', 'translation-error'].includes(kind)) return
  clearTimeout(feedbackTimer)
  feedbackText.value = customLabel ?? (kind === 'copied' ? '已复制' : kind === 'translated' ? '翻译完成' : '翻译失败')
  signal(kind === 'translation-error' ? 'notice' : 'complete')
  feedbackTimer = setTimeout(() => { feedbackText.value = '' }, 2400)
}

/** 收起态一键直拷最新邮件验证码，不打断当前窗口工作流。 */
async function copyQuickMailCode(): Promise<void> {
  const item = quickMailCode.value
  if (!item?.code || copyingQuickCode.value) return
  copyingQuickCode.value = true
  try {
    if (isDesktop) {
      await invokeHost('mail_action', { id: item.id, action: 'copy-code' })
    } else if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(item.code).catch(() => {})
    }
    if (!seenMail.value.includes(item.id)) seenMail.value = [...seenMail.value, item.id]
    showFeedback('copied', `验证码 ${item.code} 已复制`)
  } catch (error) {
    pushToast(String(error), 'error')
  } finally {
    copyingQuickCode.value = false
  }
}

function clearQuickOcr(): void {
  clearTimeout(quickOcrTimer)
  quickOcrTimer = undefined
  quickOcrClip.value = null
  hoveringQuickAction.value = false
}

function scheduleQuickOcrExpire(delayMs = 15000): void {
  clearTimeout(quickOcrTimer)
  quickOcrTimer = setTimeout(() => {
    if (hoveringQuickAction.value || hovering.value) {
      scheduleQuickOcrExpire(4000)
      return
    }
    clearQuickOcr()
  }, delayMs)
}

function showQuickImageOcr(payload: QuickImageOcrPayload): void {
  if (!payload?.clipId || tucked.value || approval.value) return
  clearTimeout(feedbackTimer)
  feedbackText.value = ''
  quickOcrClip.value = payload
  signal('complete')
  scheduleQuickOcrExpire(15000)
}

async function triggerQuickImageOcr(): Promise<void> {
  const target = quickOcrClip.value
  clearQuickOcr()
  if (!target?.clipId) return
  mailSelected.value = false
  quickMenuOpen.value = false
  manualCollapseUntil = 0
  expand(true, true)
  await runClipboardOcr(target.clipId)
}

function toggleQuickMenu(): void {
  if (expanded.value || tucked.value || approval.value) return
  cancelHover()
  quickMenuOpen.value = !quickMenuOpen.value
}

async function runQuickAction(action: 'pause' | 'pin-open' | 'audio' | 'settings' | 'ocr'): Promise<void> {
  quickMenuOpen.value = false
  if (action === 'pause') {
    await setPaused(!state.ui.paused)
    showFeedback('copied', state.ui.paused ? '已暂停剪贴板' : '已恢复剪贴板')
  } else if (action === 'pin-open') {
    pinned.value = true
    expand(true, true)
  } else if (action === 'audio') {
    await selectPage('audio')
    expand(true, true)
  } else if (action === 'ocr') {
    await startScreenOcr()
  } else {
    openSettings()
  }
}

/** 收起态胶囊滚轮快调音量（桌面版调用真实活动音频会话，浏览器态模拟演示）。 */
async function adjustCapsuleVolume(deltaY: number): Promise<void> {
  const step = deltaY < 0 ? 5 : -5
  if (!isDesktop) {
    previewVolume.value = Math.max(0, Math.min(100, previewVolume.value + step))
    showFeedback('copied', `模拟音量 ${previewVolume.value}%`)
    return
  }
  if (volumeBusy) return
  volumeBusy = true
  try {
    const snapshot = await invokeHost<{ sessions: { id: string; name: string; volume: number; muted: boolean }[] }>('list_audio_sessions')
    const target = snapshot.sessions[0]
    if (!target) {
      showFeedback('translation-error', '暂无活动音频')
      return
    }
    const nextVol = Math.max(0, Math.min(100, Math.round(target.volume + step)))
    const actual = await invokeHost<{ volume: number }>('update_audio_session', { id: target.id, volume: nextVol, muted: nextVol === 0 ? true : false })
    showFeedback('copied', `${target.name} ${Math.round(actual.volume)}%`)
  } catch {
    showFeedback('translation-error', '音量读取失败')
  } finally {
    volumeBusy = false
  }
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
  const durationMs = Math.max(
    expanded.value ? expandMs.value : collapseMs.value,
    pageSwitching.value ? pageSwitchDuration() : 0
  )
  geometryTimer = setTimeout(() => {
    viewportSize.value = { ...bounds.value }
    geometryMoving.value = false
    queueGeometry()
  }, durationMs + 24)
}

watch([bounds, expanded, () => state.ui.islandReducedMotion, reducedMotion], reserveGeometry, { flush: 'sync' })
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

function applyDismissVisual(offsetY: number, scaleY: number, dragging: boolean, ready: boolean): void {
  const surface = panel.value
  if (surface) {
    if (offsetY === 0 && scaleY === 1) {
      surface.style.translate = ''
      surface.style.scale = ''
    } else {
      surface.style.translate = `0 ${offsetY.toFixed(2)}px`
      surface.style.scale = `1 ${scaleY.toFixed(4)}`
    }
    surface.classList.toggle('dismiss-dragging', dragging)
  }
  const handle = dismissHandle.value
  if (handle) {
    handle.classList.toggle('dragging', dragging)
    handle.classList.toggle('ready', ready)
  }
}

function stopDismissSpring(): void {
  if (dismissSpringFrame) {
    cancelAnimationFrame(dismissSpringFrame)
    dismissSpringFrame = 0
  }
  dismissSpringPrev = 0
}

function clearDismissVisual(): void {
  stopDismissSpring()
  dismissSpring = { value: 0, velocity: 0, target: 0 }
  dismissSession = undefined
  applyDismissVisual(0, 1, false, false)
}

function startDismissSnapBack(initialVelocityPxPerSec: number): void {
  stopDismissSpring()
  if (motionReduced.value || Math.abs(dismissSpring.value) < 0.05) {
    clearDismissVisual()
    return
  }
  dismissSpring.target = 0
  dismissSpring.velocity = initialVelocityPxPerSec
  dismissSpringPrev = performance.now()
  const step = (now: number): void => {
    dismissSpringFrame = 0
    if (disposed || !expanded.value || dismissSession) return
    const dt = Math.min(0.064, Math.max(0.001, (now - dismissSpringPrev) / 1000))
    dismissSpringPrev = now
    const moving = advanceSpring(dismissSpring, dt, 22, 16)
    const offsetY = dismissSpring.value
    const scaleY = offsetY < 0 ? Math.max(0.94, 1 + offsetY * 0.0011) : 1
    applyDismissVisual(offsetY, scaleY, false, false)
    if (moving) dismissSpringFrame = requestAnimationFrame(step)
    else clearDismissVisual()
  }
  dismissSpringFrame = requestAnimationFrame(step)
}

function collapseNow(clearPin = true): void {
  clearDismissVisual()
  if (clearPin) pinned.value = false
  const active = document.activeElement
  if (active instanceof HTMLElement && element.value?.contains(active)) active.blur()
  manualCollapseUntil = performance.now() + 420
  expand(false)
}

function handleWindowBlur(): void {
  if (quickMenuOpen.value) quickMenuOpen.value = false
  const allowed = shouldAutoCollapseOnWindowBlur({
    expanded: expanded.value,
    pinned: pinned.value,
    hasApproval: !!approval.value,
    shelfBusy: shelf.busy,
    receivingFiles: receivingFiles.value,
    composing: composing.value,
    translationLoading: state.translation.status === 'loading' || (ocrPanelOpen.value && state.ocr.status === 'loading'),
    previewKeepOpen: !isDesktop && !!props.previewKeepOpen
  })
  if (!allowed) return
  collapseNow(false)
}

function handlePreviewOutsidePointerDown(event: PointerEvent): void {
  if (isDesktop || !expanded.value) return
  const target = event.target instanceof Element ? event.target : null
  if (!target) return
  if (target.closest('.island-hit-area, .preview-tools, .preview-lab, dialog')) return
  handleWindowBlur()
}

function computeDismissVelocity(samples: { y: number; ts: number }[], now: number): number {
  const recent = samples.filter(item => now - item.ts <= 110)
  const latest = recent.at(-1)
  if (!latest || now - latest.ts > 80 || recent.length < 2) return 0
  const first = recent[0]
  const dt = Math.max(8, latest.ts - first.ts)
  const velocity = ((latest.y - first.y) / dt) * 1000
  return Math.max(-2000, Math.min(2000, velocity))
}

function beginDismissDrag(event: PointerEvent): void {
  if (event.button !== 0 || !event.isPrimary || !expanded.value || approval.value) return
  const handle = event.currentTarget as HTMLButtonElement | null
  if (!handle) return
  const liveOffset = dismissSpring.value
  stopDismissSpring()
  pressed.value = true
  cancelHover()
  clearTimeout(leaveTimer)
  cancelHoverClose()
  suppressDismissClick = false
  const ts = event.timeStamp || performance.now()
  dismissSession = {
    pointerId: event.pointerId,
    handle,
    startX: event.clientX,
    startY: event.clientY,
    baseOffsetY: liveOffset,
    moved: Math.abs(liveOffset) > 1,
    samples: [{ y: event.clientY, ts }],
    lastOffsetY: liveOffset
  }
  handle.setPointerCapture(event.pointerId)
  handle.classList.add('dragging')
}

function moveDismissDrag(event: PointerEvent): void {
  const session = dismissSession
  if (!session || event.pointerId !== session.pointerId) return
  if (!(event.buttons & 1)) {
    endDismissDrag(event, false)
    return
  }
  const dx = event.clientX - session.startX
  const dy = event.clientY - session.startY
  if (!session.moved) {
    if (Math.abs(dy) < 6 && Math.hypot(dx, dy) < 6) return
    if (Math.abs(dy) < 6 && Math.abs(dx) >= 6) return
    session.moved = true
  }
  const ts = event.timeStamp || performance.now()
  session.samples.push({ y: event.clientY, ts })
  if (session.samples.length > 10) session.samples.shift()
  const pull = computeDismissPull(session.baseOffsetY + dy)
  dismissSpring.value = pull.offsetY
  session.lastOffsetY = pull.offsetY
  const visualY = motionReduced.value ? 0 : pull.offsetY
  const visualScale = motionReduced.value ? 1 : pull.scaleY
  applyDismissVisual(visualY, visualScale, true, pull.ready)
}

function endDismissDrag(event: PointerEvent, cancelled = false): void {
  const session = dismissSession
  if (!session || event.pointerId !== session.pointerId) return
  dismissSession = undefined
  if (session.handle.hasPointerCapture(session.pointerId)) {
    session.handle.releasePointerCapture(session.pointerId)
  }
  pressed.value = false
  const now = event.timeStamp || performance.now()
  const velocityPxPerSec = cancelled ? 0 : computeDismissVelocity(session.samples, now)

  if (!cancelled && !session.moved && Math.abs(session.baseOffsetY) < 2) {
    suppressDismissClick = true
    collapseNow(true)
    return
  }

  if (!cancelled && shouldCommitDismiss(session.lastOffsetY, velocityPxPerSec, session.moved, motionReduced.value)) {
    suppressDismissClick = true
    collapseNow(true)
    return
  }

  suppressDismissClick = session.moved
  startDismissSnapBack(velocityPxPerSec)
  if (!hovering.value) leave()
}

function onDismissClick(): void {
  if (suppressDismissClick) {
    suppressDismissClick = false
    return
  }
  collapseNow(true)
}

function activateCapsule(event: MouseEvent): void {
  if (windowDrag.consumeClick(event)) return
  if (approval.value) return
  quickMenuOpen.value = false
  if (expanded.value) {
    collapseNow(true)
  } else {
    clearQuickOcr()
    manualCollapseUntil = 0
    expand(true, true)
  }
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
  }, collapseMs.value + 34)
}

/** 聪明防误触（Hover Intent）：高速甩过顶边时重置计时，停住满延时后稳定展开。 */
let lastPointerSample: { x: number; y: number; ts: number } | null = null
function trackPointerSpeed(event: PointerEvent): void {
  const now = performance.now()
  if (lastPointerSample) {
    const dt = Math.max(8, now - lastPointerSample.ts)
    const dist = Math.hypot(event.clientX - lastPointerSample.x, event.clientY - lastPointerSample.y)
    const velocity = dist / dt
    const threshold = tucked.value ? 0.45 : 0.9
    // 仅在移动期间顺延倒计时；细线模式下光标路过时不立即触发，停稳后必定按时展开
    if (velocity > threshold && hovering.value && !hostExpanded.value && !quickMenuOpen.value && !hoveringQuickAction.value) {
      cancelHover()
      scheduleHoverOpen()
    }
  }
  lastPointerSample = { x: event.clientX, y: event.clientY, ts: now }
}

function scheduleHoverOpen(): void {
  if (quickMenuOpen.value || hoveringQuickAction.value || performance.now() < manualCollapseUntil) return
  const delayMs = tucked.value
    ? Math.max(state.ui.islandHoverMs, 420)
    : showQuickOcr.value
      ? Math.max(state.ui.islandHoverMs, 900)
      : state.ui.islandHoverMs
  hoverTimer = setTimeout(() => {
    hoverTimer = undefined
    if (hovering.value && !pressed.value && !quickMenuOpen.value && !hoveringQuickAction.value && performance.now() >= manualCollapseUntil) expand(true)
  }, delayMs)
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
  if (!hostExpanded.value && !quickMenuOpen.value && !hoveringQuickAction.value) scheduleHoverOpen()
}

function editing(): boolean {
  if (approval.value || keepTaskOpen.value || pinned.value) return true
  if (!isDesktop && props.previewKeepOpen) return true
  const active = document.activeElement
  return composing.value || pressed.value || receivingFiles.value || shelf.busy || !!shelf.previewItem ||
    (document.hasFocus() && active instanceof HTMLElement && !!element.value?.contains(active) &&
      (active instanceof HTMLTextAreaElement || active instanceof HTMLSelectElement ||
       active.isContentEditable ||
       (active instanceof HTMLInputElement && !['button', 'checkbox', 'radio', 'range', 'submit'].includes(active.type))))
}

function leave(): void {
  hovering.value = false
  hoveringQuickAction.value = false
  reenterAfterClose = false
  manualCollapseUntil = 0
  lastPointerSample = null
  cancelHover()
  clearTimeout(leaveTimer)
  leaveTimer = setTimeout(() => {
    if (!hovering.value && quickMenuOpen.value) quickMenuOpen.value = false
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
    clearDismissVisual()
    mailSelected.value = false
    cancelHover()
    clearTimeout(leaveTimer)
    composing.value = false
    pressed.value = false
    if (shouldReopen && !quickMenuOpen.value && !hoveringQuickAction.value) expand(true)
  }
})

watch(page, (nextPage, prevPage) => {
  mailSelected.value = false
  if (prevPage && nextPage !== prevPage) {
    const prevIdx = sections.findIndex(item => item.value === prevPage)
    const nextIdx = sections.findIndex(item => item.value === nextPage)
    if (prevIdx >= 0 && nextIdx >= 0) paneDir.value = nextIdx >= prevIdx ? 1 : -1
  }
  if (!closeCommitted) cancelHoverClose()
  if (expanded.value && prevPage && nextPage !== prevPage && !motionReduced.value) {
    pageSwitching.value = true
    clearTimeout(pageSwitchTimer)
    pageSwitchTimer = setTimeout(() => { pageSwitching.value = false }, pageSwitchDuration())
  }
}, { flush: 'sync' })
watch(() => props.previewKeepOpen, value => { if (!value && !hovering.value) leave() })
watch(() => state.ui.islandAnchor, () => { if (!isDesktop) windowDrag.reset() })
watch(() => shelf.busy, busy => {
  if (!busy) {
    pressed.value = false
    if (!hovering.value) leave()
  }
})

watch(suppressed, () => {
  clearDismissVisual()
  clearQuickOcr()
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
  closeOcrPanel()
  const previous = state.ui.islandPage
  const previousPanel = state.ui.panel
  state.ui.islandPage = value
  state.ui.panel = value === 'applications' ? null : value
  if (!isDesktop) return
  try { await invokeHost('select_island_page', { page: value, focus: !document.hasFocus() }) }
  catch (error) {
    state.ui.islandPage = previous
    state.ui.panel = previousPanel
    pushToast(String(error), 'error')
  }
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
  if (event.key === 'Escape') {
    event.stopPropagation()
    if (quickMenuOpen.value) { quickMenuOpen.value = false; return }
    if (page.value === 'applications' && appSearch.value) { appSearch.value = ''; return }
    collapseNow(true)
    return
  }
  if (!expanded.value) return
  const target = event.target instanceof HTMLElement ? event.target : null
  const inTextField = !!target?.closest('input, textarea, select, [contenteditable="true"]')

  // Alt+1~N 或非输入态 1~N 快切功能页（N 随标签数量变化）
  const digit = Number(event.key)
  if ((event.altKey || !inTextField) && !event.ctrlKey && !event.metaKey && Number.isInteger(digit) && digit >= 1 && digit <= sections.length) {
    const section = sections[digit - 1]
    if (section) {
      event.preventDefault()
      void selectPage(section.value)
    }
    return
  }
  // Alt+左右方向键循环切页
  if (event.altKey && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
    event.preventDefault()
    const currentIdx = Math.max(0, sections.findIndex(item => item.value === page.value))
    const delta = event.key === 'ArrowRight' ? 1 : -1
    const next = sections[(currentIdx + delta + sections.length) % sections.length]
    if (next) void selectPage(next.value)
    return
  }
  // Alt+P 一键切换钉住展开
  if (event.altKey && event.key.toLowerCase() === 'p') {
    event.preventDefault()
    pinned.value = !pinned.value
    showFeedback('copied', pinned.value ? '已钉住展开' : '已取消钉住')
    return
  }
  // 在应用页直接敲字母/数字自动聚焦搜索框筛选应用
  if (page.value === 'applications' && !viewingMail.value && !inTextField && !event.ctrlKey && !event.metaKey && !event.altKey && event.key.length === 1 && /\S/.test(event.key)) {
    appSearchEl.value?.focus()
  }
}

function launchFirstFilteredApp(): void {
  const first = filteredApps.value[0]
  if (first) void launch(first.id)
}

function scrollWheel(event: WheelEvent): void {
  const boundary = element.value
  if (!boundary || event.defaultPrevented || !event.cancelable || event.ctrlKey || event.metaKey) return
  // 收起态胶囊滚轮快速调音
  if (!expanded.value) {
    if (!tucked.value && !approval.value && Math.abs(event.deltaY) > 0) {
      event.preventDefault()
      void adjustCapsuleVolume(event.deltaY)
    }
    return
  }
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

function onPaneBeforeLeave(el: Element): void {
  if (el instanceof HTMLElement) el.inert = true
}

watch([ocrPanelOpen, () => state.ocr.status, () => state.ocr.text], () => {
  void nextTick(measureContent)
})

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
  void listenHost<OcrResultEvent>('island-ocr-result', payload => {
    mailSelected.value = false
    applyOcrResult(payload)
  }).then(off => {
    if (disposed) off()
    else stopOcrResult = off
  }).catch(() => {})
  void listenHost<string>('island-ocr-error', message => {
    mailSelected.value = false
    applyOcrError(message, 'screen')
  }).then(off => {
    if (disposed) off()
    else stopOcrError = off
  }).catch(() => {})
  void listenHost<QuickImageOcrPayload>('island-quick-image-ocr', showQuickImageOcr).then(off => {
    if (disposed) off()
    else stopQuickImageOcr = off
  }).catch(() => {})
  if (isDesktop) {
    void listenHost<void>('island-window-blur', handleWindowBlur).then(off => {
      if (disposed) off()
      else stopWindowBlur = off
    }).catch(() => {})
  } else {
    document.addEventListener('pointerdown', handlePreviewOutsidePointerDown)
  }
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
  clearTimeout(quickOcrTimer)
  stopFeedback?.()
  stopTelemetry?.()
  stopOcrResult?.()
  stopOcrError?.()
  stopQuickImageOcr?.()
  stopWindowBlur?.()
  stopFileDrop?.()
  stopShelf?.()
  clearDismissVisual()
  if (!isDesktop) document.removeEventListener('pointerdown', handlePreviewOutsidePointerDown)
  document.removeEventListener('visibilitychange', updateClock)
  clearTimeout(closeTimer)
  clearTimeout(geometryTimer)
  cancelAnimationFrame(navMeasureRaf)
  clearTimeout(pageSwitchTimer)
  window.removeEventListener('pointerup', handlePointerUp)
  window.removeEventListener('pointercancel', handlePointerUp)
  systemDark.removeEventListener('change', updateTheme)
  systemMotion.removeEventListener('change', updateMotion)
})
</script>

<template>
  <div ref="viewport" class="island-viewport" :class="`anchor-${state.ui.islandAnchor}`" :style="viewportStyle">
  <section ref="element" class="island-hit-area"
    :class="[{ expanded, 'page-switching': pageSwitching, moving: geometryMoving, desktop: isDesktop, dark, telemetry, compact: expandedWidth < 620, narrow: expandedWidth < 440, tucked: !revealed, reduced: state.ui.islandReducedMotion || reducedMotion, suppressed: tucked, avoiding: suppressed, 'has-quick-menu': quickMenuOpen && !expanded }, `variant-${variant}`, `motion-${motionMode}`]"
    :style="rootStyle" aria-label="顶部灵动岛"
    @mouseenter="enter" @mouseleave="leave" @pointermove="trackPointerSpeed" @focusout="focusout" @keydown="keydown" @wheel="scrollWheel"
    @pointerdown="pressed = true" @compositionstart="composing = true" @compositionend="composing = false">
    <button v-if="!revealed" class="tuck-trigger" type="button" aria-label="展开灵动岛" @click="expand(true, true)" />
    <div ref="panel" class="island solid-surface" :data-motion="motionMode" :class="[{ expanded, 'page-switching': pageSwitching, tucked: !revealed, telemetry, 'has-quick-menu': quickMenuOpen && !expanded, 'no-physics': !(effectsActive && quality === 'full') && !motionReduced }, `variant-${variant}`, `motion-${motionMode}`]"
      :style="{ width: `${visibleWidth}px`, height: `${visibleHeight}px` }">
      <LiquidSkin :width="visibleWidth" :height="visibleHeight" :shape="shape" :variant="variant" :moving="geometryMoving || pageSwitching" />
      <GlassLight v-if="materialReady && glassAvailable"
        :light-surface="!dark" :signal="taskSignal" :quality="quality" :active="effectsActive" :paused="geometryMoving || pageSwitching" :corner-radius="cornerRadius" :palette="glassPalette" @unavailable="glassAvailable = false" />
      <IslandParticles v-if="materialReady"
        :cue="cue" :active="effectsActive" :quality="quality" :dark="dark" :motion="motionMode" />
      <span v-if="state.ui.wechatEnabled && state.ui.wechatAttention && !tucked" class="attention-dot" role="img" title="微信有待关注提醒" aria-label="微信有待关注提醒" />
      <div class="capsule-bar" :class="{ 'has-quick-code': showQuickOcr || showQuickCode }">
        <button class="capsule" :class="{ 'has-feedback': isNoticeStatus }" type="button" :aria-expanded="expanded"
          :title="expanded ? '单击收起 · 按住拖动位置' : (capsuleStatus.tooltip || '单击展开 · 右键快操 · 滚轮调音')"
          :aria-label="approval ? approval.title : `${capsuleStatus.ariaText || capsuleStatus.text}，${expanded ? '收起面板' : '展开面板'}`"
          :inert="!revealed" aria-description="按住拖动位置，单击展开或收起，右键打开快捷操作"
          @pointerdown.stop="windowDrag.begin" @click="activateCapsule" @contextmenu.prevent="toggleQuickMenu">
          <span class="capsule-text" :class="{ feedback: isNoticeStatus }" aria-hidden="true">
            <span v-if="telemetry && !expanded" class="telemetry-led" :class="capsuleStatus.kind" />
            <span v-else-if="variant === 'liquid' && !expanded" class="liquid-orb-dot" />
            <span v-else-if="variant === 'mecha' && !expanded" class="mecha-badge">U·06</span>
            <span v-else-if="variant === 'scroll' && !expanded" class="scroll-seal">{{ capsuleStatus.kind === 'working' ? '忙' : capsuleStatus.kind === 'paused' ? '止' : capsuleStatus.kind === 'telemetry' ? '巡' : '安' }}</span>
            <span v-else-if="variant === 'pixel' && !expanded" class="pixel-hp">♥HP</span>
            <span v-else-if="variant === 'blueprint' && !expanded" class="bp-tag">图 01</span>
            <span v-else-if="variant === 'neon' && !expanded" class="neon-sign">{{ neonSign }}</span>
            <span v-else-if="variant === 'clay' && !expanded" class="clay-face">{{ clayFace }}</span>
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
            <!-- 右侧副舱微徽标：与时钟/微读数和平并存 -->
            <span v-if="capsuleBadges.length && !expanded" class="capsule-badges">
              <span v-for="badge in capsuleBadges" :key="badge.id" class="capsule-badge" :class="badge.tone ? `tone-${badge.tone}` : ''" :title="badge.title">
                <Icon :name="badge.icon" :size="11" />{{ badge.text }}
              </span>
            </span>
            <span v-if="variant === 'mecha' && !expanded" class="mecha-ribs"><i /><i /><i /></span>
            <span v-else-if="variant === 'scroll' && !expanded" class="scroll-knot" />
            <span v-else-if="variant === 'pixel' && !expanded" class="pixel-lv">1UP</span>
            <span v-else-if="variant === 'blueprint' && !expanded" class="bp-scale">1:1</span>
            <span v-else-if="variant === 'neon' && !expanded" class="neon-hours">24H</span>
            <span v-else-if="variant === 'clay' && !expanded" class="clay-dots"><i /><i /><i /></span>
          </span>
          <span class="grip" aria-hidden="true" />
        </button>
        <!-- 检测到 Win+Shift+S 等新截图进入剪贴板时：收起态浮现一键识字按钮 -->
        <div
          v-if="showQuickOcr && quickOcrClip"
          class="capsule-ocr-actions"
          @pointerenter="hoveringQuickAction = true; cancelHover(); scheduleQuickOcrExpire(15000)"
          @pointerleave="hoveringQuickAction = false"
        >
          <button
            class="capsule-code-btn capsule-ocr-btn"
            type="button"
            title="识别刚刚截取的图片中的文字"
            @click.stop="triggerQuickImageOcr"
          >
            <Icon name="scan-text" :size="11" />
            <span>识字</span>
          </button>
          <button
            class="capsule-ocr-dismiss"
            type="button"
            title="关闭提示"
            aria-label="关闭截图识字提示"
            @click.stop="clearQuickOcr"
          >
            <Icon name="close" :size="10" />
          </button>
        </div>
        <!-- 检测到新邮件验证码时：收起态直接露出一键直拷按钮 -->
        <button
          v-if="showQuickCode && quickMailCode"
          class="capsule-code-btn"
          type="button"
          :disabled="copyingQuickCode"
          :title="`一键复制来自 ${quickMailCode.sender || '邮件'} 的验证码`"
          @click.stop="copyQuickMailCode"
        >
          <Icon name="copy" :size="11" />
          <span>{{ quickMailCode.code }}</span>
        </button>
      </div>
      <!-- 右键胶囊免展开快捷操作条 -->
      <div v-if="quickMenuOpen && !expanded && !tucked" class="capsule-quick-menu" role="toolbar" aria-label="胶囊快捷操作">
        <button type="button" @click="runQuickAction('pause')">
          <Icon :name="state.ui.paused ? 'play' : 'pause'" :size="12" />{{ state.ui.paused ? '恢复剪贴板' : '暂停剪贴板' }}
        </button>
        <button type="button" @click="runQuickAction('ocr')"><Icon name="scan-text" :size="12" />截图识字</button>
        <button type="button" @click="runQuickAction('pin-open')"><Icon name="pin" :size="12" />钉住展开</button>
        <button type="button" @click="runQuickAction('audio')"><Icon name="volume" :size="12" />调音台</button>
        <button type="button" @click="runQuickAction('settings')"><Icon name="settings" :size="12" />设置</button>
      </div>
      <span class="capsule-announcement" role="status" aria-atomic="true">{{ !tucked && isNoticeStatus ? capsuleStatus.text : '' }}</span>
      <div class="expanded-content" :class="{ 'can-scroll': needsOuterScroll, 'fixed-search': !approval && !ocrPanelOpen && !viewingMail && page === 'clipboard' && !state.ui.detailId }" :inert="!expanded">
        <div ref="chrome" v-show="!approval" class="island-chrome">
          <header class="toolbar">
            <nav ref="modeNav" class="mode-nav" aria-label="灵动岛功能">
              <span v-show="!viewingMail && !ocrPanelOpen" ref="modeSelection" class="mode-selection" aria-hidden="true" />
              <button v-for="(section, idx) in sections" :key="section.value" type="button"
                :title="`${section.label}（按 ${idx + 1} 或 Alt+${idx + 1}）`" :aria-label="section.label" :aria-pressed="!viewingMail && !ocrPanelOpen && page === section.value"
                :class="{ selected: !viewingMail && !ocrPanelOpen && page === section.value }" @click="selectPage(section.value)">
                <Icon :name="section.icon" :size="19" :stroke="1.7" />
                <span class="mode-label">{{ section.label }}</span>
              </button>
            </nav>
            <div class="tools">
              <button v-if="mailNotices.length" class="round-action" type="button" :title="viewingMail ? '返回当前功能' : mailSummary || '查看最近邮件'" aria-label="最近邮件" :aria-pressed="viewingMail" @click="closeOcrPanel(); mailSelected = !viewingMail"><Icon name="mail" :size="16" /></button>
              <button class="round-action" type="button" :title="`截图识字（${state.shortcuts.ocr || 'Ctrl+Shift+O'}）`" aria-label="截图识字" :aria-pressed="ocrPanelOpen" @click="startScreenOcr"><Icon name="scan-text" :size="16" /></button>
              <button v-if="!viewingMail && !ocrPanelOpen && page === 'translation'" class="round-action" type="button" title="独立翻译窗口" aria-label="独立翻译窗口" @click="openSeparateTranslation"><Icon name="external" :size="16" /></button>
              <button class="round-action pin-action" type="button" :title="pinned ? '已钉住展开（点击或按 Esc 取消）' : '钉住展开，移开鼠标不自动收起（Alt+P）'" aria-label="钉住灵动岛" :aria-pressed="pinned" @click="pinned = !pinned"><Icon name="pin" :size="15" /></button>
              <button class="round-action" type="button" title="设置" aria-label="设置" @click="openSettings()"><Icon name="settings" :size="16" /></button>
              <button class="round-action collapse-action" type="button" title="收起（Esc）" aria-label="收起灵动岛" @click="collapseNow(true)"><Icon name="chevron-down" :size="16" /></button>
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
          <OcrResultPanel v-else-if="ocrPanelOpen && expanded" />
          <MailNotices v-else-if="viewingMail && expanded" :notices="mailNotices" @error="pushToast($event, 'error')" />
          <div v-show="!approval && !ocrPanelOpen && !viewingMail" class="ordinary-pane" :style="{ '--pane-dir': paneDir }">
          <Transition name="island-pane" @before-leave="onPaneBeforeLeave" @enter="measureContent" @after-enter="measureContent" @after-leave="measureContent">
            <div v-if="page === 'applications'" :key="'applications'" class="applications">
              <div class="app-toolbar">
                <div class="app-search-box">
                  <Icon name="search" :size="13" />
                  <input
                    ref="appSearchEl"
                    v-model="appSearch"
                    type="text"
                    spellcheck="false"
                    autocomplete="off"
                    placeholder="打字筛选应用，回车启动首项…"
                    aria-label="筛选应用"
                    @keydown.enter.prevent="launchFirstFilteredApp"
                  />
                  <button v-if="appSearch" class="app-search-clear" type="button" aria-label="清空应用搜索" @click="appSearch = ''">
                    <Icon name="close" :size="12" />
                  </button>
                </div>
              </div>
              <nav class="categories" aria-label="应用分类">
                <button v-for="category in state.categories" :key="category.id" type="button"
                  :class="{ selected: state.ui.activeCategoryId === category.id }" :aria-pressed="state.ui.activeCategoryId === category.id"
                  @click="setCategory(category.id)">{{ category.name }}</button>
              </nav>
              <p v-if="!filteredApps.length" class="empty">{{ appSearch ? '没有匹配的应用' : '这个分类还没有应用' }}</p>
              <div v-else class="app-grid scroll-y" :class="{ 'with-names': showAppNames }">
                <button v-for="app in filteredApps" :key="app.id" class="app-tile" type="button" :title="app.name" :aria-label="app.name" @click="launch(app.id)">
                  <img v-if="iconFor(app.target)" :src="iconFor(app.target)" alt="" />
                  <span v-else class="fallback" :style="{ background: app.color }">{{ app.initials }}</span>
                  <span v-if="showAppNames" class="app-name">{{ app.name }}</span>
                </button>
              </div>
            </div>
            <div v-else :key="page" class="feature-pane scroll-y">
              <ClipboardPopover v-if="page === 'clipboard'" @reading-explanation="setReadingExplanation" />
              <TranslationPanel v-else-if="page === 'translation'" />
              <FileShelf v-else-if="page === 'files'" :drag-over="receivingFiles" />
              <AudioMixer v-else-if="page === 'audio'" :active="expanded" />
              <PackConsole v-else-if="page === 'pack'" />
            </div>
          </Transition>
          </div>
        </div>
      </div>
      <button
        v-if="expanded && !approval"
        ref="dismissHandle"
        class="dismiss-handle"
        type="button"
        title="单击或向上轻推收起"
        aria-label="收起灵动岛（底部拉条）"
        @pointerdown.stop="beginDismissDrag"
        @pointermove.stop="moveDismissDrag"
        @pointerup.stop="endDismissDrag($event, false)"
        @pointercancel.stop="endDismissDrag($event, true)"
        @lostpointercapture.stop="endDismissDrag($event, true)"
        @click.stop="onDismissClick"
      >
        <span class="dismiss-pill" aria-hidden="true" />
      </button>
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
.island-hit-area, .island {
  --size-transition: width var(--geometry-time) var(--width-ease, var(--geometry-ease)), height var(--geometry-time) var(--height-ease, var(--geometry-ease));
}
.island-hit-area {
  --geometry-time: var(--expand-time);
  --geometry-ease: var(--ease-out);
  --island-shadow: rgb(0 0 0 / .2);
  --island-outline: rgb(0 0 0 / .16);
  --surface-inset: 2px;
  position: relative;
  flex: none;
  transition: opacity 110ms ease;
}
.island-hit-area.dark { --island-shadow: rgb(0 0 0 / .5); --island-outline: rgb(255 255 255 / .14); }
.island-hit-area.expanded { --surface-inset: 12px; }
/* 从细线展开时顶边不下移，避免窗口裁切热区后把原位置的鼠标判为移出。 */
.island-hit-area.desktop.avoiding { --surface-inset: 0px; }
.island-hit-area:not(.expanded) { --geometry-time: var(--collapse-time); --geometry-ease: var(--ease-drawer); }
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
.island.variant-standard,
.island.variant-telemetry {
  background: var(--surface);
  box-shadow: inset 0 0 0 1px var(--island-rim, var(--island-outline)), inset 0 1px 0 var(--island-top-sheen);
}
.island {
  position: absolute; top: var(--surface-inset); left: 50%; transform: translateX(-50%); transform-origin: 50% 0;
  overflow: hidden; border-radius: 24px; color: var(--text); contain: layout;
  box-shadow: inset 0 1px 0 var(--island-top-sheen);
  transition: var(--size-transition), top var(--geometry-time) var(--geometry-ease), border-radius var(--geometry-time) var(--geometry-ease);
}
.island.dismiss-dragging { transition: none !important; }
.island.expanded { border-radius: 18px; }
.island.tucked { border-radius: 5px; box-shadow: none; }
.island.tucked .capsule-bar,
.island.tucked .capsule { visibility: hidden; }
.island.tucked :deep(.glass-light) { visibility: hidden; }
.capsule-bar {
  position: relative; z-index: 3; display: flex; align-items: center; justify-content: center; width: 100%;
}
.capsule-bar.has-quick-code { justify-content: space-between; padding: 0 5px 0 8px; gap: 6px; }
.capsule {
  position: relative; z-index: 3; display: flex; align-items: center; justify-content: center;
  height: 24px; width: min(260px, calc(100% - 32px)); margin: 0 auto; white-space: nowrap; font-size: 12px;
  cursor: grab; touch-action: none; user-select: none;
  transition: height var(--geometry-time) var(--height-ease, var(--geometry-ease)), opacity 110ms ease-out 40ms;
}
.capsule-bar.has-quick-code .capsule { flex: 1; width: auto; margin: 0; justify-content: flex-start; min-width: 0; }
.capsule:active { transform: none; }
.window-dragging .capsule { cursor: grabbing; }
.island:not(.expanded) .capsule { height: var(--capsule-height); width: 100%; }
.island:not(.expanded) .capsule-bar.has-quick-code .capsule { width: auto; }
.liquid-orb-dot {
  flex: none; width: 8px; height: 8px; border-radius: 50%;
  background: radial-gradient(circle at 30% 30%, #ffffff, #7dd3fc 58%, #c084fc);
  box-shadow: 0 0 10px rgba(125, 211, 252, .65);
}
.capsule-badges {
  display: inline-flex; align-items: center; gap: 4px; margin-left: 4px;
}
.capsule-badge {
  display: inline-flex; align-items: center; gap: 3px; height: 18px; padding: 0 6px;
  border-radius: 999px; font-size: 10.5px; font-weight: 650; line-height: 1;
  background: var(--surface-3); color: var(--text-2); border: 1px solid var(--line);
}
.capsule-badge.tone-accent { background: var(--accent-weak); color: var(--accent); border-color: var(--accent); }
.capsule-badge.tone-paused { color: var(--danger); border-color: var(--danger-weak); background: var(--danger-weak); }
.capsule-code-btn {
  flex: none; display: inline-flex; align-items: center; gap: 4px; height: 22px; padding: 0 8px;
  border-radius: 999px; border: 1px solid var(--accent); background: var(--accent-weak); color: var(--text);
  font-family: var(--font-mono); font-size: 11px; font-weight: 700; letter-spacing: .04em; cursor: pointer;
  transform-origin: right center;
  touch-action: manipulation;
  transition: transform 150ms var(--ease-out), opacity 150ms var(--ease-out), background-color 120ms ease, color 120ms ease;
}
.capsule-code-btn:hover { background: var(--accent); color: var(--accent-text); }
.capsule-ocr-actions {
  position: relative; z-index: 3; flex: none; display: inline-flex; align-items: center; gap: 4px;
  transform-origin: right center;
  transition: opacity 150ms var(--ease-out), transform 150ms var(--ease-out);
}
@starting-style {
  .capsule-ocr-actions,
  .capsule-code-btn {
    opacity: 0;
    transform: translate3d(-4px, 0, 0) scale(.94);
  }
}
.capsule-ocr-btn { font-family: var(--font-sans); letter-spacing: 0; }
.capsule-ocr-dismiss {
  flex: none; display: grid; place-items: center; width: 20px; height: 20px; padding: 0;
  border-radius: 999px; border: 1px solid var(--line); background: var(--surface-2); color: var(--text-2);
  cursor: pointer;
}
.capsule-ocr-dismiss:hover { background: var(--hover); border-color: var(--line-strong); color: var(--text); }
.capsule-quick-menu {
  position: relative; z-index: 3; display: flex; align-items: center; justify-content: center; gap: 4px;
  padding: 4px 8px 7px; border-top: 1px solid var(--line);
  transform-origin: top center;
  transition: opacity 170ms var(--ease-out), transform 170ms var(--ease-out);
}
@starting-style {
  .capsule-quick-menu {
    opacity: 0;
    transform: translate3d(0, -4px, 0) scale(.96);
  }
}
.capsule-quick-menu button {
  display: inline-flex; align-items: center; gap: 4px; height: 26px; padding: 0 8px;
  border-radius: 8px; border: 1px solid transparent; background: var(--surface-2); color: var(--text-2);
  font-size: 11px; font-weight: 550; white-space: nowrap; cursor: pointer;
}
.capsule-quick-menu button:hover { background: var(--hover); color: var(--text); border-color: var(--line); }
.island-hit-area:not(.desktop) .island.tucked .capsule {
  visibility: visible;
  opacity: 0;
  transition: opacity 75ms ease-out;
}
.capsule-text {
  display: inline-flex; align-items: center; gap: 6px; font-size: 13px; line-height: 18px;
  font-weight: 600; letter-spacing: .22px; font-variant-numeric: tabular-nums; color: var(--text);
  transition: opacity calc(var(--geometry-time) * .36) var(--ease-out) calc(var(--geometry-time) * .28), transform calc(var(--geometry-time) * .68) var(--geometry-ease);
}
.expanded .capsule-text:not(.feedback) { opacity: 0; transform: translateY(-3px) scale(.96); transition: opacity calc(var(--geometry-time) * .2) var(--ease-out), transform calc(var(--geometry-time) * .28) var(--ease-out); }
.capsule-text.feedback { font-size: 12px; letter-spacing: .2px; }
.capsule-announcement { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
.grip {
  position: absolute; width: 32px; height: 3px; border-radius: 8px;
  background: currentColor; opacity: 0; transform: scaleX(.72);
  transition: opacity calc(var(--geometry-time) * .36) ease-out, transform calc(var(--geometry-time) * .52) var(--geometry-ease);
}
.expanded .grip { opacity: .28; transform: scaleX(1); transition-delay: calc(var(--geometry-time) * .18); }
.expanded .capsule:hover .grip { opacity: .62; transform: scaleX(1.12); }
.capsule.has-feedback .grip { opacity: 0; }
.expanded-content {
  --content-gutter: 18px;
  position: relative; z-index: 3; width: 100%; max-height: calc(100% - 24px);
  overflow: clip; padding: 0 var(--content-gutter) 18px;
}
.expanded .expanded-content.can-scroll { overflow-y: auto; overflow-x: hidden; scrollbar-gutter: stable; }
.expanded .expanded-content.can-scroll .island-content { max-width: 100%; }
.moving :deep(::-webkit-scrollbar-thumb),
.page-switching :deep(::-webkit-scrollbar-thumb) { background: transparent !important; }
.moving .feature-pane,
.moving .app-grid,
.page-switching .feature-pane { scrollbar-color: transparent transparent; }
.moving .island-chrome,
.moving .island-content { will-change: transform, opacity; }
.island-chrome { display: flow-root; }
/* 工具栏随外壳分配可用宽度，正文维持最终列宽，开合途中不会反复换行。 */
.island-content {
  position: relative; left: 50%; translate: -50% 0;
  width: calc(var(--content-width) - 2 * var(--content-gutter));
}
.ordinary-pane { position: relative; display: block; overflow: clip; }
.island-pane-enter-active {
  will-change: transform, opacity;
  transition: opacity 115ms var(--ease-out), transform 130ms var(--ease-out);
}
.island-pane-leave-active {
  position: absolute; top: 0; left: 0; right: 0;
  pointer-events: none;
  will-change: transform, opacity;
  transition: opacity 75ms var(--ease-out), transform 85ms var(--ease-out);
}
.island-pane-enter-from { opacity: 0; transform: translate3d(calc(var(--pane-dir, 1) * 6px), 0, 0); }
.island-pane-leave-to { opacity: 0; transform: translate3d(calc(var(--pane-dir, 1) * -4px), 0, 0); }
.toolbar {
  display: flex; flex-wrap: nowrap; align-items: center; justify-content: space-between;
  gap: 6px; margin-bottom: 8px; padding: 0 0 7px; border-bottom: 1px solid var(--line);
}
.tools { display: flex; align-items: center; gap: 2px; flex: none; }
.round-action {
  display: grid; place-items: center; flex: none; width: 28px; height: 28px;
  border-radius: 8px; border: 1px solid transparent; background: transparent; color: var(--text-3);
}
.collapse-action svg { transform: rotate(180deg); }
.round-action:hover { background: var(--surface-3); border-color: var(--line); color: var(--text); }
.tools .round-action[aria-pressed='true'] { color: var(--accent); border-color: var(--accent); background: var(--accent-weak); }
.categories { display: flex; align-items: center; flex-wrap: wrap; gap: 4px; max-height: 74px; overflow-y: auto; overflow-x: hidden; scrollbar-gutter: stable; margin: 0 0 8px; padding: 1px; }
.categories button { flex: none; padding: 3px 10px; border-radius: 20px; border: 1px solid transparent; min-height: 26px; font-size: 12px; color: var(--text-2); }
.categories .selected { color: var(--text); background: var(--surface-3); font-weight: 600; box-shadow: inset 0 0 0 1px var(--line-strong); }
.categories button:hover:not(.selected) { color: var(--text); background: var(--hover); border-color: var(--line); }
.app-toolbar {
  display: flex; align-items: center; gap: 6px; margin-bottom: 6px;
}
.app-search-box {
  flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px;
  height: 30px; padding: 0 9px; border-radius: 9px;
  background: var(--surface-2); border: 1px solid var(--line); color: var(--text-2);
  transition: border-color 120ms ease, box-shadow 120ms ease;
}
.app-search-box:focus-within { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-weak); color: var(--text); }
.app-search-box input {
  flex: 1; min-width: 0; border: 0; background: transparent; color: var(--text); font: inherit; font-size: 12.5px; outline: none;
}
.app-search-box input::placeholder { color: var(--text-3); }
.app-search-clear {
  display: grid; place-items: center; width: 20px; height: 20px; border-radius: 50%; color: var(--text-2);
}
.app-search-clear:hover { background: var(--hover); color: var(--text); }
.app-grid {
  --app-icon-size: 28px; --app-cell-size: 46px; --app-gap: 8px;
  display: grid; grid-template-columns: repeat(auto-fill, var(--app-cell-size));
  grid-auto-rows: var(--app-cell-size); justify-content: start; align-content: start;
  max-height: calc(var(--app-cell-size) * 3 + var(--app-gap) * 2); overflow-y: auto; overflow-x: hidden; scrollbar-gutter: stable; gap: var(--app-gap); padding: 2px;
  scrollbar-width: thin; scrollbar-color: transparent transparent;
}
.app-grid.with-names {
  --app-cell-size: 62px;
  grid-template-columns: repeat(auto-fill, minmax(62px, 1fr));
  grid-auto-rows: 62px;
  max-height: 220px;
}
.app-tile {
  display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 5px;
  width: 100%; height: 100%; padding: 6px;
  border-radius: 12px; border: 1px solid transparent; min-width: 0;
}
.app-name {
  width: 100%; font-size: 11px; line-height: 1.25; color: var(--text-2); text-align: center;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.app-tile:hover .app-name { color: var(--text); }
.app-tile:hover { background: var(--hover); border-color: var(--line); }
.app-grid:hover, .app-grid:focus-within { scrollbar-color: var(--line-strong) transparent; }
.app-tile img, .fallback { flex: none; width: var(--app-icon-size); height: var(--app-icon-size); object-fit: contain; }
.app-tile img { filter: drop-shadow(0 3px 2px rgba(0,0,0,.08)); }
.fallback { display: grid; place-items: center; border-radius: 8px; color: #fff; font-size: 12px; font-weight: 600; box-shadow: inset 0 1px 1px rgba(255,255,255,.4), 0 3px 5px -3px var(--island-shadow); }
.no-physics .app-tile img, .no-physics .app-tile .fallback {
  transition: transform 160ms var(--ease-out);
}
@media (hover: hover) and (pointer: fine) {
  .no-physics .app-tile:hover img, .no-physics .app-tile:hover .fallback {
    transform: scale(1.03);
  }
}
.empty { display: flex; align-items: center; min-height: 64px; margin: 0; color: var(--text-2); font-size: 13px; line-height: 1.8; padding: 16px 4px; }
.mode-nav {
  position: relative; display: flex; flex: 1; align-items: center; gap: 2px;
  min-width: 0; width: auto; height: 38px; padding: 2px;
  overflow-x: auto; overflow-y: hidden; scrollbar-width: none; color: var(--text);
}
.island .mode-nav::-webkit-scrollbar { display: none; width: 0; height: 0; }
.mode-selection {
  position: absolute; top: 2px; bottom: 2px; left: 0;
  width: 36px; border-radius: 10px;
  background: var(--accent-weak); box-shadow: inset 0 0 0 1px var(--line-strong);
  contain: layout style;
  will-change: transform;
  transform: translate3d(2px, 0, 0);
  transition: transform 150ms var(--ease-out), width 150ms var(--ease-out);
  pointer-events: none;
}
.mode-nav button {
  position: relative; z-index: 1; display: inline-flex; flex: none; align-items: center; justify-content: center; gap: 0;
  height: 34px; min-width: 34px; padding: 0 7px; border-radius: 10px; color: var(--text-2); font-size: 12.5px;
  transition: transform 150ms var(--ease-out), color 130ms ease;
}
.mode-nav .mode-label {
  max-width: 0; opacity: 0; overflow: hidden; white-space: nowrap; font-size: 12px; font-weight: 650;
  transition: opacity 140ms ease;
}
.mode-nav button.selected { gap: 5px; padding: 0 10px; color: var(--text); font-weight: 600; }
.mode-nav button.selected .mode-label { max-width: 64px; opacity: 1; }
.narrow .mode-nav button.selected { gap: 0; padding: 0 7px; }
.narrow .mode-nav button.selected .mode-label { max-width: 0; opacity: 0; }
.mode-nav .selected svg { color: var(--accent); }
.mode-nav button:hover { color: var(--text); }
button {
  touch-action: manipulation;
  transition: transform 90ms var(--ease-out), background-color 110ms ease, color 110ms ease, border-color 110ms ease;
}
:where(.round-action, .mode-nav button, .categories button, .attention-strip button, .feature-pane button, .app-tile, .capsule-code-btn, .capsule-ocr-dismiss, .capsule-quick-menu button):active:not(:disabled) {
  transform: scale(.97);
  transition-duration: 70ms;
}
/* 启用 physics.ts 时由内层图标弹簧承担主按压微缩，外框仅轻微沉降避免双重过度缩水。 */
.island:not(.no-physics) :where(.app-tile, .mode-nav button, .round-action):active:not(:disabled) {
  transform: scale(.985);
}
button:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.attention-dot { position: absolute; right: 8px; top: 0; width: 5px; height: 5px; z-index: 2; background: var(--success); border-radius: 50%; }
.island:not(.tucked) > .attention-dot { right: 16px; top: 50%; transform: translateY(-50%); background: #42d580; }
.island.expanded > .attention-dot { right: 20px; top: 12px; transform: none; }
.attention-strip { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 8px 10px; margin-bottom: 12px; border-radius: 12px; background: var(--surface-2); font-size: 12px; }
.attention-strip > span { flex: 1; }
.attention-strip button { padding: 5px 8px; border-radius: 8px; background: var(--hover); }
.feature-pane { max-height: 420px; overflow-y: auto; overflow-x: hidden; scrollbar-gutter: stable; }
.feature-pane :deep(.pop), .feature-pane :deep(.panel) { width: 100%; max-height: none; border: 0; border-radius: 0; box-shadow: none; background: transparent; }
.feature-pane :deep(.panel > .head) { display: none; }
.feature-pane :deep(button:disabled) { opacity: 1; color: var(--text-3); background: var(--surface-2); cursor: not-allowed; }
.feature-pane :deep(.pop > .mode-bar), .feature-pane :deep(.pop > .head), .feature-pane :deep(.pop > .detail), .feature-pane :deep(.pop > .foot), .feature-pane :deep(.panel > .content) { padding-left: 0; padding-right: 0; }
.feature-pane :deep(.panel > .content) { overflow: visible; scrollbar-gutter: auto; }
.feature-pane :deep(.pop > .mode-bar) { padding-top: 0; }
.feature-pane { --r-chip: 8px; --r-tile: 10px; }
.feature-pane :deep(.search) { height: 32px; border-radius: 9px; background: var(--surface-2); border: 1px solid var(--line); transition: border-color 120ms ease, box-shadow 120ms ease; }
.feature-pane :deep(.search:hover) { border-color: var(--line-strong); }
.feature-pane :deep(.search:focus-within) { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-weak); }
.feature-pane :deep(.pause) { height: 32px; border-radius: 9px; border: 1px solid var(--line); background: var(--surface-2); }
.feature-pane :deep(.filter-bar) { padding: 0 0 6px; }
.feature-pane :deep(.chips) { gap: 4px; padding: 1px; }
.feature-pane :deep(.chip) { height: 26px; padding: 0 10px; }
.feature-pane :deep(.filter-bar .chip) { height: 24px; padding: 0 8px; }
.feature-pane :deep(.var-bar .chip) { height: 22px; padding: 0 7px; }
.feature-pane :deep(.chips .chip:not(.is-on)) { border-color: transparent; background: var(--surface-2); }
.feature-pane :deep(.chips .chip:not(.is-on):hover) { background: var(--hover); border-color: var(--line); }
.feature-pane :deep(.chips .chip.is-on) { box-shadow: inset 0 0 0 1px var(--accent-weak); }
.feature-pane :deep(.list) { padding-left: 0; padding-right: 0; }
.feature-pane :deep(.row) { padding: 7px 8px; border-radius: 9px; border-bottom: 1px solid var(--line); }
.feature-pane :deep(.row.is-active) { background: var(--accent-weak); box-shadow: inset 2px 0 0 var(--accent); }
.feature-pane :deep(.seg) { padding: 3px; border-radius: 14px; background: var(--surface-2); border: 1px solid var(--line); }
.feature-pane :deep(.seg-btn) { height: 26px; border-radius: 11px; }
.feature-pane :deep(.seg-btn.is-active) { box-shadow: inset 0 0 0 1px var(--line-strong), 0 1px 2px rgb(0 0 0 / .06); }
.feature-pane :deep(.textarea) { border-radius: 14px; background: var(--surface-2); border: 1px solid var(--line); transition: border-color 120ms ease, box-shadow 120ms ease; }
.feature-pane :deep(.textarea:focus-visible) { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-weak); }
.narrow .expanded-content { --content-gutter: 14px; }
.narrow .categories { gap: 2px; }
.narrow .categories button { padding: 5px 10px; }

/* 星舰仪表主题：三态专属轮廓圆角、遥测状态灯、扫描光束与方正秩序控件 */
@keyframes telemetryPulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: .42; transform: scale(.8); }
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
.variant-pixel .mode-nav { gap: 4px; padding: 2px 2px 5px; }
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

.dismiss-handle {
  position: absolute; left: 0; right: 0; bottom: 0; z-index: 4;
  display: flex; align-items: center; justify-content: center;
  height: 18px; width: 100%; padding: 0; border: 0; background: transparent;
  color: var(--text); cursor: grab; touch-action: none; user-select: none;
}
.dismiss-handle::before {
  content: ''; position: absolute; left: 0; right: 0; top: -4px; bottom: 0;
}
.dismiss-handle:active, .dismiss-handle.dragging { cursor: grabbing; }
.dismiss-pill {
  position: relative; z-index: 1;
  width: 40px; height: 3.5px; border-radius: 999px;
  background: currentColor; opacity: .22;
  transition: width 150ms cubic-bezier(.22, 1, .36, 1), opacity 140ms ease, background-color 140ms ease, transform 150ms cubic-bezier(.22, 1, .36, 1);
}
@media (hover: hover) and (pointer: fine) {
  .dismiss-handle:hover .dismiss-pill { width: 52px; opacity: .52; }
}
.dismiss-handle.dragging .dismiss-pill { width: 56px; opacity: .68; transform: translateY(-1px); }
.dismiss-handle.ready .dismiss-pill { background: var(--accent); opacity: .92; width: 62px; }
.variant-mecha .dismiss-pill,
.variant-pixel .dismiss-pill,
.variant-scroll .dismiss-pill,
.variant-blueprint .dismiss-pill { border-radius: 0; }

.reduced :deep(*), .reduced :deep(*::before), .reduced :deep(*::after) { transition: none !important; animation: none !important; filter: none !important; }
@media (prefers-reduced-motion: reduce) { *, *::before, *::after { transition: none !important; animation:none !important; filter: none !important; } }
@media (prefers-contrast: more) {
  .mode-selection, .categories .selected { box-shadow: inset 0 0 0 2px var(--accent); }
}
</style>

<style scoped>
.expanded-content.fixed-search { display: flex; flex-direction: column; overflow: hidden; }
.fixed-search .island-chrome { flex: none; }
.fixed-search .island-content { min-height: 0; display: flex; flex-direction: column; }
.fixed-search .feature-pane { min-height: 0; overflow: hidden; scrollbar-gutter: auto; display: flex; flex-direction: column; }
.fixed-search .feature-pane :deep(.pop) { height: 420px; min-height: 0; max-height: 420px; }
</style>

<!-- 蓝晒图纸、霓虹夜行与软糖黏土三套大胆主题的灵动岛外形 -->
<style scoped src="./bold-variants.css"></style>

<!-- 6 套独立于外观主题的灵动岛动态效果 -->
<style scoped src="./motion-variants.css"></style>
