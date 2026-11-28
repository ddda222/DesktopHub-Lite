<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import GsapPresence from '@/components/GsapPresence.vue'
import { closeAnyOpenThemedSelect, hasOpenThemedSelect } from '@/components/ThemedSelect.vue'
import CapsuleMorphGlyph from './CapsuleMorphGlyph.vue'
import CapsuleOrbitTrack from './motion/CapsuleOrbitTrack.vue'
import { animateFlipFromState, animateScrambleText, captureFlipSnapshot, Flip, gsap, playFlipSnapshot, vGsapHover, vGsapPress } from '@/lib/gsapMotion'
import Icon from '@/components/Icon.vue'
import GlassLight from '@/components/GlassLight.vue'
import LiquidSkin, { type IslandVariant } from './motion/LiquidSkin.vue'
import ApprovalPrompt from './ApprovalPrompt.vue'
import OcrResultPanel from '@/features/ocr/OcrResultPanel.vue'
import MailNotices from '@/features/mail/MailNotices.vue'
import { providerName, useMailList, type MailNotice } from '@/features/mail/mail'
import { useApprovals } from './useApprovals'
import { useIslandEffects, type IslandCue } from './motion/useIslandEffects'
import { getMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
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
  normalizeTelemetryStyle,
  stepSmartTelemetry,
  type TelemetrySnapshot
} from './telemetryReadout'
import TelemetryCapsuleReadout from './TelemetryCapsuleReadout.vue'
import ClipboardPopover from '@/features/clipboard/ClipboardPopover.vue'
import TranslationPanel from '@/features/translation/TranslationPanel.vue'
import FileShelf from '@/features/files/FileShelf.vue'
import AudioMixer from '@/features/audio/AudioMixer.vue'
import { addShelfFiles, shelf, subscribeShelf } from '@/features/files/shelf'
import PackConsole from '@/features/pack/PackConsole.vue'
import {
  calcTaskProgress,
  capsuleTaskText,
  clock,
  extractKeyLogSummary,
  formatMinuteDuration,
  resolvePackPhase,
  useTaskRunner
} from '@/features/pack/useTaskRunner'
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
  selectIslandPage,
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
    if (!hovering.value && leftAfterExpand) leave()
  },
  onError: error => pushToast(`无法拖动窗口：${String(error)}`, 'error')
})
const panel = ref<HTMLElement | null>(null)
const capsuleTextEl = ref<HTMLElement | null>(null)
const capsuleStatusLabelEl = ref<HTMLElement | null>(null)
const quickCodeTextEl = ref<HTMLElement | null>(null)
const appGridEl = ref<HTMLElement | null>(null)
function resolveVariant(mode = state.settings.theme): IslandVariant {
  if (mode === 'midnight') return 'telemetry'
  if (mode === 'mecha') return 'mecha'
  if (mode === 'pixel') return 'pixel'
  if (mode === 'blueprint') return 'blueprint'
  if (mode === 'neon') return 'neon'
  return 'standard'
}
const systemDark = window.matchMedia('(prefers-color-scheme: dark)')
const dark = ref(isDarkTheme())
const variant = ref<IslandVariant>(resolveVariant())
const telemetry = computed(() => variant.value === 'telemetry')
const glassPalette = computed<'cool' | 'amber' | 'lime' | 'chalk' | 'neon'>(() => {
  if (variant.value === 'mecha') return 'amber'
  if (variant.value === 'pixel') return 'lime'
  if (variant.value === 'blueprint') return 'chalk'
  if (variant.value === 'neon') return 'neon'
  return 'cool'
})
/** 霓虹招牌随胶囊状态切换，只做装饰，读屏仍以状态文字为准。 */
const neonSign = computed(() => ({ working: '加班中', paused: '打烊', feedback: '好嘞', mail: '来信', approval: '请示', files: '寄存', 'quick-ocr': '识字' } as Record<string, string>)[capsuleStatus.value.kind] ?? '营业中')
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
// 打包 / 联调台的任务状态与实时日志流：与灵动岛胶囊、副条及 07 轨道联动。
const { task: runTask, log: runLog, elapsedMs: runElapsed } = useTaskRunner(
  message => pushToast(message, 'error'),
  { withLog: true }
)
const suppressed = computed(() => state.ui.fullscreenSuppressed)
const hoverClosing = ref(false)
const expanded = computed(() => !!approval.value || (hostExpanded.value && !hoverClosing.value))
const feedbackIsError = ref(false)
const packStripDismissedKey = ref('')
const packHighlightTone = ref<'success' | 'error' | null>(null)
let packHighlightTimer: ReturnType<typeof setTimeout> | undefined
const packInfoTextEl = ref<HTMLElement | null>(null)
const runKey = computed(() => `${runTask.value.startedAt}:${runTask.value.stage}`)
const runProgress = computed(() => calcTaskProgress(runTask.value, runElapsed.value))
const runPercentText = computed(() => runProgress.value === null ? '' : `${Math.round(runProgress.value * 100)}%`)
const runOvertime = computed(() =>
  runTask.value.stage === 'running' && !!runTask.value.estimateSec && runElapsed.value >= runTask.value.estimateSec * 1000
)
const runDurationMinuteText = computed(() => formatMinuteDuration(runTask.value.elapsedMs || runElapsed.value))
const packPhase = computed(() => resolvePackPhase(runTask.value))
const runLatestLog = computed(() => extractKeyLogSummary(runTask.value, runLog.value))
const packLiveBadge = computed<'INFO' | 'LOG' | 'WARN' | 'ERR' | 'DONE'>(() => {
  if (runTask.value.stage === 'error') return 'ERR'
  if (runTask.value.stage === 'done') return 'DONE'
  const raw = runLog.value[runLog.value.length - 1] ?? ''
  if (/^\[(ERR|ERROR)\]/i.test(raw)) return 'ERR'
  if (/^\[WARN\]/i.test(raw)) return 'WARN'
  if (/^\[INFO\]/i.test(raw)) return 'INFO'
  return 'LOG'
})
const packLiveSummary = computed(() => {
  if (runTask.value.stage === 'error') {
    return runLatestLog.value || '执行出错，点击查看详情'
  }
  if (runTask.value.stage === 'done') {
    const verb = runTask.value.kind === 'pack' ? '打包完成' : '执行完成'
    return `${verb} · 用时 ${runDurationMinuteText.value}${runLatestLog.value ? ` · ${runLatestLog.value}` : ''}`
  }
  return runLatestLog.value || packPhase.value.label || '准备中'
})
watch([mailNotices, viewingMail, expanded, approval], ([items, selected, open, prompt]) => {
  if (!items.length) mailSelected.value = false
  const ids = items.map(item => item.id)
  seenMail.value = selected && open && !prompt ? ids : seenMail.value.filter(id => ids.includes(id))
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
const { active: effectsActive, quality, plainMaterial } = useIslandEffects(panel, tucked, motionReduced, materialRequested)
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
const telemetryStyle = computed(() => normalizeTelemetryStyle(appearance.value.telemetryStyle))
const showQuickOcr = computed(() =>
  !expanded.value && !tucked.value && !approval.value && !quickMenuOpen.value && !!quickOcrClip.value
)
const capsuleStatus = computed<{ text: string; ariaText?: string; tooltip?: string; icon: IconName | null; kind: string }>(() => {
  if (approval.value) return { text: approval.value.kind === 'approval' ? '待审批' : '有提问', ariaText: approval.value.title, icon: 'info', kind: 'approval' }
  // 打包/联调任务运行中：胶囊中间实时显示「任务名 · 约剩 mm:ss」
  if (runTask.value.stage === 'running') return { text: capsuleTaskText(runTask.value, runElapsed.value), ariaText: `${runTask.value.name}：${runTask.value.message}`, tooltip: `${runTask.value.name} · ${packLiveSummary.value}`, icon: 'terminal', kind: 'working' }
  if (receivingFiles.value) return { text: '松开暂存', icon: 'folder', kind: 'working' }
  if (shelf.busy) return { text: shelf.activity || '文件处理中', icon: 'folder', kind: 'working' }
  const pending = Math.max(state.ui.translationPending, state.translation.status === 'loading' ? 1 : 0)
  if (pending) return { text: pending > 1 ? `翻译中 · ${pending}` : '翻译中', icon: 'translate', kind: 'working' }
  if (feedbackText.value) return { text: feedbackText.value, icon: feedbackIsError.value ? 'info' : 'check', kind: 'feedback' }
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
const showPackStrip = computed(() =>
  !expanded.value &&
  !tucked.value &&
  !approval.value &&
  !quickMenuOpen.value &&
  (runTask.value.stage === 'running' || (packHighlightTone.value !== null && packStripDismissedKey.value !== runKey.value))
)
watch(() => runTask.value.stage, (stage, before) => {
  clearTimeout(packHighlightTimer)
  if (stage === 'running') {
    packStripDismissedKey.value = ''
    packHighlightTone.value = null
    return
  }
  if (before === 'idle' || stage === 'idle') {
    packHighlightTone.value = null
    return
  }
  if (stage === 'done') {
    packHighlightTone.value = 'success'
    const verb = runTask.value.kind === 'pack' ? '打包完成' : '执行完成'
    showFeedback('task-done', `${verb} · ${runDurationMinuteText.value}`)
    packHighlightTimer = setTimeout(() => { packHighlightTone.value = null }, 5500)
  } else if (stage === 'cancelled') {
    packHighlightTone.value = null
    showFeedback('task-cancelled', '已取消')
  } else if (stage === 'error') {
    packHighlightTone.value = 'error'
    showFeedback('task-error', runTask.value.kind === 'pack' ? '打包出错' : '执行失败')
    packHighlightTimer = setTimeout(() => { packHighlightTone.value = null }, 7000)
  }
})
watch([packLiveSummary, showPackStrip], ([text, visible]) => {
  if (!visible || !text) return
  void nextTick(() => {
    animateScrambleText(packInfoTextEl.value, text, motionReduced.value, { chars: '01·INFO_', duration: 0.24, speed: 0.85 })
  })
}, { flush: 'post' })
const isNoticeStatus = computed(() => capsuleStatus.value.kind !== 'idle' && capsuleStatus.value.kind !== 'telemetry')
const orbitActive = computed(() =>
  !motionReduced.value &&
  !tucked.value &&
  (capsuleStatus.value.kind === 'working' ||
    capsuleStatus.value.kind === 'quick-ocr' ||
    capsuleStatus.value.kind === 'approval' ||
    packHighlightTone.value !== null)
)
const orbitTone = computed<'accent' | 'success' | 'error' | 'warning'>(() => {
  if (packHighlightTone.value) return packHighlightTone.value
  if (runTask.value.stage === 'running' && runOvertime.value) return 'warning'
  return 'accent'
})
const orbitSpeed = computed(() => {
  if (packHighlightTone.value === 'error') return 1.5
  if (packHighlightTone.value === 'success') return 1.25
  if (runTask.value.stage === 'running') {
    if (runOvertime.value) return 1.65
    return runTask.value.env === 'remote' ? 1.35 : 1.05
  }
  return 1
})
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
const motionPreset = computed(() => getMotionPreset(motionMode.value))
const shellTransition = computed(() => {
  if ((isDesktop && tucked.value) || motionReduced.value) return INSTANT_TRANSITION
  if (pageSwitching.value && expanded.value) return motionPreset.value.shellResize
  return expanded.value ? motionPreset.value.shellExpand : motionPreset.value.shellCollapse
})
const pillTransition = computed(() => (motionReduced.value ? INSTANT_TRANSITION : motionPreset.value.pill))
const paneTransition = computed(() => (motionReduced.value ? INSTANT_TRANSITION : motionPreset.value.paneTransition))
const contentRevealTransition = computed(() => {
  if (motionReduced.value) return INSTANT_TRANSITION
  if (expanded.value) return motionPreset.value.contentRevealTransition
  if (motionMode.value === 'flicker') return { duration: 0.03, delay: 0, ease: 'none' }
  // 收起时正文与工具栏快速干净退场（强缓出前段迅速降透明），避免收缩外壳底边残留淡淡灰字与叠层。
  return {
    duration: Math.min(0.11, Math.max(0.07, (collapseMs.value / 1000) * 0.36)),
    delay: 0,
    ease: 'power2.out'
  }
})
const contentHiddenPose = computed(() => {
  const preset = motionPreset.value
  return {
    opacity: 0,
    y: preset.contentOffsetY,
    scaleX: preset.contentScaleX,
    scaleY: preset.contentScaleY,
    filter: preset.contentBlurPx ? `blur(${preset.contentBlurPx}px)` : 'blur(0px)'
  }
})
const paneInitial = computed(() => {
  if (motionReduced.value) return false
  const preset = motionPreset.value
  return {
    opacity: motionMode.value === 'flicker' ? 0.18 : 0,
    x: paneDir.value * preset.paneOffsetX,
    y: preset.paneOffsetY,
    scaleX: preset.paneScaleX,
    scaleY: preset.paneScaleY
  }
})
const capsuleTextTransition = computed(() => {
  if (motionReduced.value) return INSTANT_TRANSITION
  if (motionMode.value === 'flicker') return { duration: 0.035, delay: 0, ease: 'none' }
  // 收起时等顶部工具栏基本淡出后再浮现胶囊读数，避免两者同帧上下重叠。
  return expanded.value
    ? { duration: 0.08, ease: 'power3.out' }
    : {
        duration: 0.13,
        delay: Math.min(0.12, Math.max(0.08, (collapseMs.value / 1000) * 0.42)),
        ease: 'power3.out'
      }
})
const actionHoverPose = computed(() => (motionReduced.value ? undefined : motionPreset.value.tileHover))
const actionTapPose = computed(() => (motionReduced.value ? undefined : motionPreset.value.buttonTap))
const tileTapPose = computed(() => (motionReduced.value ? undefined : motionPreset.value.tileTap))
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
    ? 128 * scale.value
    : showQuickCode.value
      ? 132 * scale.value
      : capsuleBadges.value.reduce((acc, item) => acc + (item.text.length > 2 ? 48 : 34) * scale.value, 0)
  // 快捷操作条 5 个按钮时的宽度下限。
  const menuFloor = quickMenuOpen.value ? 412 * scale.value : 0
  if (capsuleStatus.value.kind === 'telemetry' && telemetryReadout.value) {
    const count = telemetryReadout.value.metricCount
    const table: Record<IslandVariant, [number, number][]> = {
      standard: [[140, 118], [188, 168], [240, 218]],
      telemetry: [[158, 136], [204, 184], [256, 234]],
      mecha: [[190, 168], [240, 220], [292, 270]],
      pixel: [[204, 182], [258, 238], [312, 290]],
      blueprint: [[196, 174], [248, 228], [300, 280]],
      neon: [[200, 178], [252, 232], [304, 284]]
    }
    const [base, floor] = table[variant.value][Math.min(2, Math.max(0, count - 1))]
    const visualCount = Number(telemetryPrefs.value.telemetryCpu) + Number(telemetryPrefs.value.telemetryMemory)
    const stylePerMetric = telemetryStyle.value === 'bar' ? 34 : telemetryStyle.value === 'ring' ? 18 : telemetryStyle.value === 'pulse' ? 20 : 0
    const styleExtra = visualCount * stylePerMetric * scale.value
    return Math.min(availableWidth.value - 24, Math.max(floor, menuFloor, Math.round(base * scale.value + badgeExtra + styleExtra)))
  }
  const idle = capsuleStatus.value.kind === 'idle'
  let baseWidth = (idle ? 128 : 168) * scale.value
  if (variant.value === 'telemetry') baseWidth = (idle ? 148 : 186) * scale.value
  else if (variant.value === 'mecha' || variant.value === 'pixel') baseWidth = (idle ? 176 : 208) * scale.value
  else if (variant.value === 'blueprint' || variant.value === 'neon') baseWidth = (idle ? 184 : 216) * scale.value
  if (runTask.value.stage === 'running') {
    const runningFloor = (variant.value === 'standard' ? 276 : variant.value === 'telemetry' ? 294 : 332) * scale.value
    baseWidth = Math.max(baseWidth, runningFloor)
  }
  if (showPackStrip.value) {
    baseWidth = Math.max(baseWidth, 392 * scale.value)
  }
  return Math.min(availableWidth.value - 24, Math.max(menuFloor, Math.round(baseWidth + badgeExtra)))
})
// 内容高度已经来自实际布局；再次乘缩放比例会让小尺寸裁切、大尺寸多出留白。
const lineHeights: Record<IslandVariant, number> = { standard: 5, telemetry: 7, mecha: 8, pixel: 9, blueprint: 8, neon: 6 }
const compactHeights: Record<IslandVariant, number> = { standard: 32, telemetry: 32, mecha: 34, pixel: 36, blueprint: 34, neon: 34 }
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
  return compactHeights[variant.value] * scale.value + (quickMenuOpen.value ? 42 : 0) + (showPackStrip.value ? Math.round(32 * scale.value) : 0)
})
const cornerRadius = computed(() => {
  if (variant.value === 'mecha' || variant.value === 'pixel') return 4
  if (variant.value === 'blueprint') return 2
  const base = expanded.value
    ? (telemetry.value ? 12 : variant.value === 'neon' ? 20 : 18)
    : tucked.value ? (telemetry.value ? 4 : 5) : quickMenuOpen.value ? 16 : (telemetry.value ? 10 : 24)
  return Math.min(base, visibleHeight.value / 2, visibleWidth.value / 2)
})
const bounds = computed(() => ({
  width: Math.max(140, expanded.value ? expandedWidth.value : visibleWidth.value + (suppressed.value ? 0 : 4)),
  // 展开态与收起态共用 2px 顶部边距，底部保留 14px 余量以容纳弹性微过冲，开合零上下跳动。
  height: Math.max(14, visibleHeight.value + (expanded.value ? 16 : suppressed.value ? 0 : 4))
}))
const DESKTOP_MIN_STAGE_HEIGHT = 560
/** 计算桌面端稳态定帧窗口尺寸：固定为展开最大宽与高水位高度，日常开合与切页不改原生窗口尺寸，仅切换 HRGN。 */
function computeDesktopStageSize(
  target: { width: number; height: number },
  currentHeight = DESKTOP_MIN_STAGE_HEIGHT
): { width: number; height: number } {
  const width = Math.max(140, Math.ceil(expandedWidth.value), Math.ceil(target.width))
  const maxStageH = Math.min(1180, Math.max(DESKTOP_MIN_STAGE_HEIGHT, Math.ceil(maxExpandedHeight.value + 16)))
  const desiredH = Math.max(DESKTOP_MIN_STAGE_HEIGHT, Math.ceil(target.height))
  const height = Math.min(maxStageH, Math.max(currentHeight, desiredH))
  return { width, height }
}
// 桌面端使用定帧稳定画布避免跨进程 resize/横移闪烁；浏览器样稿保持贴合内容尺寸。
const viewportSize = ref(isDesktop ? computeDesktopStageSize(bounds.value) : { ...bounds.value })
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
const pageSwitching = ref(false)
let pageSwitchTimer: ReturnType<typeof setTimeout> | undefined
const pageSwitchDuration = () => (
  motionMode.value === 'flicker'
    ? 60
    : motionMode.value === 'focus'
      ? 165
      : motionMode.value === 'sweep'
        ? 185
        : motionMode.value === 'bounce'
          ? 210
          : 170
)
function revealSelectedPage(): void {
  const nav = modeNav.value
  const selected = nav?.querySelector<HTMLElement>('button.selected')
  if (!nav || !selected) return
  if (selected.offsetLeft < nav.scrollLeft) nav.scrollLeft = selected.offsetLeft
  else if (selected.offsetLeft + selected.offsetWidth > nav.scrollLeft + nav.clientWidth) nav.scrollLeft = selected.offsetLeft + selected.offsetWidth - nav.clientWidth
}
function syncSelectedIndicator(): void {
  void nextTick(revealSelectedPage)
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
/** 标记本次展开后指针是否确实从灵动岛区域移出过，避免非悬停展开在岛内操作时被误自动收起。 */
let leftAfterExpand = false
/** 标记悬停触发的展开正处于 IPC 在途或首帧静默准备期，指针快速移开时直接撤销避免闪展黑影。 */
let hoverExpanding = false
/** 桌面端两阶段无闪黑扩容标志：窗口已扩容但 HRGN 暂锁在旧面板轮廓内，待 WebView2 提交透明新帧后再放开。 */
let expandStaging = false
let expandStageGen = 0
let expandStageFrameA = 0
let expandStageFrameB = 0
/** 标记当前几何动画处于收缩方向：桌面定帧窗口下直接复用展开态 HRGN，收缩启动零 Win32 调用。 */
let geometryShrinking = false
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
const ordinaryPaneEl = ref<HTMLElement | null>(null)
const dismissProxy = { y: 0, scaleY: 1 }
let dismissTween: gsap.core.Tween | null = null
let shellTween: gsap.core.Tween | gsap.core.Timeline | null = null
let capsuleTween: gsap.core.Tween | null = null
let chromeTween: gsap.core.Tween | null = null
let contentTween: gsap.core.Tween | null = null
let paneTween: gsap.core.Tween | null = null
let navFlipTween: gsap.core.Timeline | null = null
let dismissCurrentY = 0
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
let measureFrame = 0
let sendingPromise: Promise<void> | null = null
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
  feedbackIsError.value = kind === 'translation-error' || kind === 'task-error' || kind === 'task-cancelled'
  feedbackText.value = customLabel ?? (kind === 'copied' ? '已复制' : kind === 'translated' ? '翻译完成' : '翻译失败')
  feedbackTimer = setTimeout(() => {
    feedbackText.value = ''
    feedbackIsError.value = false
  }, kind.startsWith('task-') ? 3400 : 2400)
}

async function openPackConsoleFromCapsule(): Promise<void> {
  closeOcrPanel()
  mailSelected.value = false
  quickMenuOpen.value = false
  await selectPage('pack')
  manualCollapseUntil = 0
  if (!expanded.value) expand(true, true)
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
watch(() => props.previewTelemetry, value => {
  if (!isDesktop) applyTelemetrySample(value ?? null)
}, { immediate: true })
function updateTheme(): void {
  dark.value = isDarkTheme()
  variant.value = resolveVariant()
}

watch(() => state.settings.theme, updateTheme)
watch(geometryMoving, () => queueGeometry(), { flush: 'post' })

function cancelExpandStaging(): void {
  expandStageGen++
  expandStaging = false
  if (expandStageFrameA) {
    cancelAnimationFrame(expandStageFrameA)
    expandStageFrameA = 0
  }
  if (expandStageFrameB) {
    cancelAnimationFrame(expandStageFrameB)
    expandStageFrameB = 0
  }
}

function settleGeometry(): void {
  clearTimeout(geometryTimer)
  geometryTimer = undefined
  cancelExpandStaging()
  geometryShrinking = false
  viewportSize.value = isDesktop
    ? computeDesktopStageSize(bounds.value, viewportSize.value.height)
    : { ...bounds.value }
  geometryMoving.value = false
  queueGeometry()
}

function reserveGeometry(prevWidth?: number, prevHeight?: number): void {
  clearTimeout(geometryTimer)
  cancelExpandStaging()
  const target = bounds.value
  if ((isDesktop && tucked.value) || state.ui.islandReducedMotion || reducedMotion.value) {
    geometryMoving.value = false
    geometryShrinking = false
    viewportSize.value = isDesktop
      ? computeDesktopStageSize(target, viewportSize.value.height)
      : { ...target }
    queueGeometry()
    return
  }
  const headroom = expanded.value && !pageSwitching.value ? motionPreset.value.bounceHeadroom : 0
  if (isDesktop) {
    const stage = computeDesktopStageSize(
      { width: target.width + headroom, height: target.height + headroom },
      viewportSize.value.height
    )
    const stageGrowing = stage.width > viewportSize.value.width || stage.height > viewportSize.value.height
    const isShrinking = !stageGrowing &&
      prevWidth !== undefined &&
      prevHeight !== undefined &&
      target.width <= prevWidth &&
      target.height <= prevHeight
    geometryMoving.value = true
    geometryShrinking = isShrinking
    viewportSize.value = stage
    if (stageGrowing) {
      // 仅在极少数超高面板突破定帧水位时执行两阶段无黑影扩容。
      stageDesktopExpandGeometry(stage.width, stage.height)
    } else if (!isShrinking) {
      // 定帧窗口尺寸不变：展开/增高时直接同步放开目标包络 HRGN，零延迟且零 SetWindowPos。
      sendDesktopExpandRegionNow(stage.width, stage.height, target.width + headroom, target.height + headroom)
    }
    // 收缩方向（isShrinking === true）：当前展开态 HRGN 已完全包含收缩轨迹，不触发任何 Win32 调用。
  } else {
    const nextWidth = Math.max(viewportSize.value.width, target.width + headroom)
    const nextHeight = Math.max(viewportSize.value.height, target.height + headroom)
    geometryMoving.value = true
    geometryShrinking = false
    viewportSize.value = { width: nextWidth, height: nextHeight }
  }
  const durationMs = Math.max(
    expanded.value ? expandMs.value : collapseMs.value,
    pageSwitching.value ? pageSwitchDuration() : 0
  )
  geometryTimer = setTimeout(settleGeometry, durationMs + 180)
}

function onShellAnimationComplete(): void {
  if (geometryMoving.value && !dismissSession) settleGeometry()
}

// 仅监听数值标量而非 bounds 对象引用，防止展开稳态下无关重算重复触发窗口扩容或闪跳。
watch(
  [() => bounds.value.width, () => bounds.value.height, expanded, () => state.ui.islandReducedMotion, reducedMotion],
  ([w, h, exp, redA, redB], [prevW, prevH, prevExp, prevRedA, prevRedB]) => {
    if (
      w === prevW &&
      h === prevH &&
      exp === prevExp &&
      redA === prevRedA &&
      redB === prevRedB
    ) {
      return
    }
    reserveGeometry(prevW, prevH)
  },
  { flush: 'sync' }
)
// 缩放比例或屏幕宽度变化时同步更新桌面定帧窗口宽度。
watch(expandedWidth, () => {
  if (!isDesktop) return
  viewportSize.value = computeDesktopStageSize(bounds.value, viewportSize.value.height)
  queueGeometry()
}, { flush: 'post' })
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
  } else if (hostExpanded.value && !hovering.value && leftAfterExpand) leave()
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
    leftAfterExpand = false
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

function syncDismissTransform(): void {
  const surface = panel.value
  if (surface) gsap.set(surface, { y: dismissProxy.y, scaleY: dismissProxy.scaleY })
}

function applyDismissVisual(offsetY: number, scaleY: number, dragging: boolean, ready: boolean): void {
  dismissProxy.y = offsetY
  dismissProxy.scaleY = scaleY
  syncDismissTransform()
  const surface = panel.value
  if (surface) surface.classList.toggle('dismiss-dragging', dragging)
  const handle = dismissHandle.value
  if (handle) {
    handle.classList.toggle('dragging', dragging)
    handle.classList.toggle('ready', ready)
  }
}

function stopDismissSpring(): void {
  dismissTween?.kill()
  dismissTween = null
}

function clearDismissVisual(): void {
  stopDismissSpring()
  dismissCurrentY = 0
  dismissSession = undefined
  applyDismissVisual(0, 1, false, false)
}

function startDismissSnapBack(_initialVelocityPxPerSec: number): void {
  stopDismissSpring()
  if (motionReduced.value || Math.abs(dismissCurrentY) < 0.05) {
    clearDismissVisual()
    return
  }
  const snapTransition = motionPreset.value.dismissSnap
  dismissTween = gsap.to(dismissProxy, {
    y: 0,
    scaleY: 1,
    duration: Math.max(0.001, snapTransition.duration),
    ease: snapTransition.ease,
    onUpdate: syncDismissTransform,
    onComplete: () => {
      dismissTween = null
      if (!disposed && !dismissSession) {
        dismissCurrentY = 0
        applyDismissVisual(0, 1, false, false)
      }
    }
  })
}

function collapseNow(clearPin = true): void {
  closeAnyOpenThemedSelect(element.value)
  clearDismissVisual()
  if (clearPin) pinned.value = false
  const active = document.activeElement
  if (active instanceof HTMLElement && element.value?.contains(active)) active.blur()
  manualCollapseUntil = performance.now() + 420
  expand(false)
}

function handleWindowBlur(): void {
  closeAnyOpenThemedSelect(element.value)
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
  if (target.closest('.island-hit-area, .themed-select-menu, .preview-tools, .preview-lab, dialog')) return
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
  const liveOffset = dismissProxy.y
  dismissCurrentY = liveOffset
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
  dismissCurrentY = pull.offsetY
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
  if (!hovering.value && leftAfterExpand) leave()
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
    // 触发前再次校验真实 :hover，避免指针快速滑出顶边偶发 mouseleave 滞后导致误展开。
    const stillHovering = hovering.value && (!element.value || element.value.matches(':hover'))
    if (!stillHovering) {
      hovering.value = false
      return
    }
    if (!pressed.value && !quickMenuOpen.value && !hoveringQuickAction.value && performance.now() >= manualCollapseUntil) {
      hoverExpanding = true
      expand(true)
    }
  }, delayMs)
}

function enter(): void {
  hovering.value = true
  leftAfterExpand = false
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

function onMouseLeave(): void {
  if (hostExpanded.value || hoverExpanding) leftAfterExpand = true
  // 悬停展开尚在 IPC 途中或静默准备阶段，指针已快速移开则立即撤回，杜绝展开黑影一闪而过。
  if (hoverExpanding && (!hostExpanded.value || expandStaging)) {
    hoverExpanding = false
    hovering.value = false
    cancelHover()
    clearTimeout(leaveTimer)
    cancelExpandStaging()
    expand(false)
    return
  }
  leave()
}

function editing(): boolean {
  if (approval.value || keepTaskOpen.value || pinned.value) return true
  if (hasOpenThemedSelect(element.value)) return true
  if (!isDesktop && props.previewKeepOpen) return true
  const active = document.activeElement
  return composing.value || pressed.value || receivingFiles.value || shelf.busy || !!shelf.previewItem ||
    (document.hasFocus() && active instanceof HTMLElement && !!element.value?.contains(active) &&
      (active instanceof HTMLTextAreaElement || active instanceof HTMLSelectElement ||
       active.isContentEditable ||
       (active instanceof HTMLInputElement && !['button', 'checkbox', 'radio', 'range', 'submit'].includes(active.type))))
}

function handleThemedSelectClosed(event: Event): void {
  const detail = (event as CustomEvent<{ trigger?: HTMLElement | null }>).detail
  if (!detail?.trigger || !element.value?.contains(detail.trigger)) return
  void nextTick(() => {
    if (disposed || !expanded.value) return
    if (element.value?.matches(':hover')) {
      hovering.value = true
      clearTimeout(leaveTimer)
    } else if (!editing()) {
      leave()
    }
  })
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
  const wasPressed = pressed.value
  pressed.value = false
  if (wasPressed && !hovering.value && leftAfterExpand) leave()
}

function focusout(event: FocusEvent): void {
  const nextFocused = event.relatedTarget as Node | null
  // 焦点仍在灵动岛内部流转（如点击分类、切换标签或按钮获焦），不视为离开。
  if (nextFocused && element.value?.contains(nextFocused)) return
  queueMicrotask(() => {
    if (!document.hasFocus()) composing.value = false
    const active = document.activeElement
    if (active instanceof Node && element.value?.contains(active)) return
    // 仅当指针此前确实移出过灵动岛（被输入态暂缓收起）时，失焦后补触发收起。
    if (!hovering.value && leftAfterExpand) leave()
  })
}

watch(hostExpanded, value => {
  const shouldReopen = !value && hovering.value && reenterAfterClose
  closeCommitted = false
  reenterAfterClose = false
  cancelHoverClose()
  if (value) {
    // 悬停展开的宿主广播返回时，若指针已提前移开，直接静默撤回而不展示空壳动画。
    if (hoverExpanding && !hovering.value) {
      hoverExpanding = false
      cancelExpandStaging()
      expand(false)
    }
    return
  }
  hoverExpanding = false
  leftAfterExpand = false
  closeAnyOpenThemedSelect(element.value)
  clearDismissVisual()
  mailSelected.value = false
  cancelHover()
  clearTimeout(leaveTimer)
  composing.value = false
  pressed.value = false
  if (shouldReopen && !quickMenuOpen.value && !hoveringQuickAction.value) {
    hoverExpanding = true
    expand(true)
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

watch(page, (nextPage, prevPage) => {
  measureContent()
  paneTween?.kill()
  paneTween = null
  if (!expanded.value || approval.value || ocrPanelOpen.value || viewingMail.value) return
  if (!prevPage || nextPage === prevPage || motionReduced.value || !paneInitial.value) return
  const target = ordinaryPaneEl.value?.firstElementChild as HTMLElement | null
  if (!target) return
  paneTween = gsap.fromTo(
    target,
    paneInitial.value,
    {
      opacity: 1,
      x: 0,
      y: 0,
      scaleX: 1,
      scaleY: 1,
      duration: Math.max(0.001, Math.min(0.18, paneTransition.value.duration)),
      ease: paneTransition.value.ease,
      clearProps: 'transform,opacity',
      onComplete: () => {
        paneTween = null
        measureContent()
      }
    }
  )
}, { flush: 'post' })
watch(() => props.previewKeepOpen, value => { if (!value && !hovering.value) leave() })
watch(() => state.ui.islandAnchor, () => { if (!isDesktop) windowDrag.reset() })
watch(() => shelf.busy, busy => {
  if (!busy) {
    pressed.value = false
    if (!hovering.value && leftAfterExpand) leave()
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
  await selectIslandPage(value, !document.hasFocus())
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

/** 计算当前可见外壳在目标新视口坐标系中的精确贴合区域，用于扩容首帧遮罩未渲染的透明区。 */
function computeLockedPanelRegion(viewportW: number, viewportH: number): BarRegion | null {
  const surface = panel.value
  if (!surface) return null
  const box = surface.getBoundingClientRect()
  const w = Math.min(Math.ceil(box.width), Math.ceil(viewportW))
  const h = Math.min(Math.ceil(box.height), Math.ceil(viewportH))
  if (w <= 0 || h <= 0) return null
  const anchor = state.ui.islandAnchor
  const x = anchor === 'left'
    ? 0
    : anchor === 'right'
      ? Math.max(0, Math.round(viewportW - w))
      : Math.max(0, Math.round((viewportW - w) / 2))
  const y = Math.min(suppressed.value ? 0 : 2, Math.max(0, Math.ceil(viewportH) - h))
  const radius = Math.min(parseFloat(getComputedStyle(surface).borderTopLeftRadius) || 0, w / 2, h / 2)
  return { x, y, width: Math.min(w, Math.ceil(viewportW) - x), height: Math.min(h, Math.ceil(viewportH) - y), radius }
}

/**
 * 桌面端两阶段无黑影扩容：
 * 1. 先以旧面板紧贴区域作为 HRGN 扩大原生窗口，外部未提交的新区域被系统完全裁掉；
 * 2. 等待扩容 IPC 落地并经过两帧 rAF，确保 WebView2 透明合成帧就绪后再放开 HRGN 并启动展开动效。
 */
function stageDesktopExpandGeometry(width: number, height: number): void {
  if (disposed) return
  if (frame) {
    cancelAnimationFrame(frame)
    frame = 0
  }
  const w = Math.ceil(width)
  const h = Math.ceil(height)
  const locked = computeLockedPanelRegion(w, h)
  if (!locked) {
    queued = { width: w, height: h, regions: [{ x: 0, y: 0, width: w, height: h, radius: 0 }] }
    void sendGeometry()
    return
  }
  const gen = ++expandStageGen
  expandStaging = true
  queued = { width: w, height: h, regions: [locked] }
  void sendGeometry().then(() => {
    if (gen !== expandStageGen || disposed) return
    expandStageFrameA = requestAnimationFrame(() => {
      expandStageFrameA = 0
      if (gen !== expandStageGen || disposed) return
      expandStageFrameB = requestAnimationFrame(() => {
        expandStageFrameB = 0
        if (gen !== expandStageGen || disposed) return
        if (hoverExpanding && !hovering.value) {
          hoverExpanding = false
          expandStaging = false
          expand(false)
          settleGeometry()
          return
        }
        hoverExpanding = false
        expandStaging = false
        queued = { width: w, height: h, regions: [{ x: 0, y: 0, width: w, height: h, radius: 0 }] }
        void sendGeometry()
        runShellResize()
      })
    })
  })
}

/** 计算动画扩张期间在定帧窗口内的最小包围盒，只放开动画需要覆盖的区域。 */
function computeMovingEnvelopeRegion(stageW: number, stageH: number, targetW: number, targetH: number): BarRegion {
  const w = Math.min(Math.ceil(stageW), Math.max(1, Math.ceil(targetW)))
  const h = Math.min(Math.ceil(stageH), Math.max(1, Math.ceil(targetH)))
  const anchor = state.ui.islandAnchor
  const x = anchor === 'left'
    ? 0
    : anchor === 'right'
      ? Math.max(0, Math.ceil(stageW) - w)
      : Math.max(0, Math.round((stageW - w) / 2))
  return { x, y: 0, width: Math.min(w, Math.ceil(stageW) - x), height: h, radius: 0 }
}

/** 定帧窗口尺寸不变时，展开瞬间同步放开目标动画包络 HRGN，避免等待 rAF 产生裁切。 */
function sendDesktopExpandRegionNow(stageW: number, stageH: number, targetW: number, targetH: number): void {
  if (disposed) return
  if (frame) {
    cancelAnimationFrame(frame)
    frame = 0
  }
  const w = Math.ceil(stageW)
  const h = Math.ceil(stageH)
  queued = {
    width: w,
    height: h,
    regions: [computeMovingEnvelopeRegion(w, h, targetW, targetH)]
  }
  void sendGeometry()
}

function queueGeometry(): void {
  if (frame || disposed || expandStaging) return
  if (isDesktop && geometryMoving.value && geometryShrinking) return
  frame = requestAnimationFrame(() => {
    frame = 0
    if (expandStaging) return
    if (isDesktop && geometryMoving.value && geometryShrinking) return
    const rect = viewport.value?.getBoundingClientRect()
    if (!rect) return
    const rectW = Math.ceil(rect.width)
    const rectH = Math.ceil(rect.height)
    const regions: BarRegion[] = []
    if (geometryMoving.value) {
      // 仅放开本次动画包络区域，避免不必要地放开整块大矩形。
      regions.push(computeMovingEnvelopeRegion(rectW, rectH, bounds.value.width, bounds.value.height))
    } else if (tucked.value) {
      // 全屏避让细线态：用 HRGN 精准裁切为顶部 140×14 热区，其余透明区域完全穿透给全屏应用。
      const hit = element.value?.getBoundingClientRect()
      if (hit && hit.width > 0 && hit.height > 0) {
        const x = Math.max(0, Math.round(hit.left - rect.left))
        const y = Math.max(0, Math.round(hit.top - rect.top))
        const width = Math.min(Math.ceil(hit.width), rectW - x)
        const height = Math.min(Math.ceil(hit.height), rectH - y)
        if (width > 0 && height > 0) regions.push({ x, y, width, height, radius: 0 })
      }
    } else {
      for (const surface of [panel.value]) {
        if (!surface) continue
        const box = surface.getBoundingClientRect()
        const x = Math.max(0, Math.round(box.left - rect.left))
        const y = Math.max(0, Math.round(box.top - rect.top))
        const width = Math.min(Math.ceil(box.width), rectW - x)
        const height = Math.min(Math.ceil(box.height), rectH - y)
        if (width > 0 && height > 0) regions.push({ x, y, width, height,
          radius: Math.min(parseFloat(getComputedStyle(surface).borderTopLeftRadius) || 0, width / 2, height / 2) })
      }
    }
    queued = { width: rectW, height: rectH, regions }
    void sendGeometry()
  })
}

function sendGeometry(): Promise<void> {
  if (disposed) return Promise.resolve()
  if (sendingPromise) return sendingPromise
  if (!queued) return Promise.resolve()
  sendingPromise = (async () => {
    try {
      while (queued && !disposed) {
        const next = queued
        queued = undefined
        const key = JSON.stringify(next)
        if (sent === key) continue
        try {
          await resizeBar(next.width, next.height, next.regions)
          sent = key
        } catch {
          sent = ''
        }
      }
    } finally {
      sendingPromise = null
    }
  })()
  return sendingPromise
}

function measureContent(): void {
  // offsetHeight 读取布局高度，不受父级动画的亚像素位移影响，避免 ceil 把 188 算成 189。
  const el = content.value as HTMLElement | null
  const height = el?.offsetHeight
  if (height) contentHeight.value = Math.min(478, height)
  const hdr = chrome.value as HTMLElement | null
  const header = hdr?.offsetHeight
  if (header) chromeHeight.value = header
}

watch([ocrPanelOpen, () => state.ocr.status, () => state.ocr.text, page], () => {
  void nextTick(measureContent)
})

function runShellResize(): void {
  const el = panel.value
  if (!el || expandStaging) return
  shellTween?.kill()
  shellTween = null
  const targetW = `${visibleWidth.value}px`
  const targetH = `${visibleHeight.value}px`
  const targetRadius = `${cornerRadius.value}px`
  if ((isDesktop && tucked.value) || motionReduced.value) {
    gsap.set(el, { width: targetW, height: targetH, borderRadius: targetRadius })
    onShellAnimationComplete()
    return
  }
  if (motionMode.value === 'sweep' && !pageSwitching.value) {
    const tl = gsap.timeline({
      onComplete: () => {
        shellTween = null
        onShellAnimationComplete()
      }
    })
    if (expanded.value) {
      // 粘液蠕虫特征：先横向拉开，慢半拍再垂展高度，圆角随同一时间轴过渡
      tl.to(el, { width: targetW, borderRadius: targetRadius, duration: 0.28, ease: 'back.out(1.25)' }, 0)
        .to(el, { height: targetH, duration: 0.36, ease: 'back.out(1.2)' }, 0.085)
    } else {
      // 收起先压扁，再缩窄
      tl.to(el, { height: targetH, duration: 0.16, ease: 'power3.inOut' }, 0)
        .to(el, { width: targetW, borderRadius: targetRadius, duration: 0.22, ease: 'power3.out' }, 0.065)
    }
    shellTween = tl
    return
  }
  const spec = shellTransition.value
  shellTween = gsap.to(el, {
    width: targetW,
    height: targetH,
    borderRadius: targetRadius,
    duration: Math.max(0.001, spec.duration),
    ease: spec.ease,
    onComplete: () => {
      shellTween = null
      onShellAnimationComplete()
    }
  })
}

watch([visibleWidth, visibleHeight], runShellResize, { flush: 'post' })
watch(cornerRadius, radius => {
  if (!shellTween && panel.value) {
    gsap.set(panel.value, { borderRadius: `${radius}px` })
  }
}, { flush: 'post' })

watch([expanded, isNoticeStatus], ([open, notice]) => {
  const el = capsuleTextEl.value
  if (!el) return
  capsuleTween?.kill()
  const hide = open && !notice
  const spec = capsuleTextTransition.value
  if (motionReduced.value) {
    gsap.set(el, hide ? { opacity: 0, y: -3, scale: 0.96 } : { opacity: 1, y: 0, scale: 1 })
    return
  }
  capsuleTween = gsap.to(el, {
    opacity: hide ? 0 : 1,
    y: hide ? -3 : 0,
    scale: hide ? 0.96 : 1,
    duration: Math.max(0.001, spec.duration),
    delay: spec.delay ?? 0,
    ease: spec.ease
  })
}, { flush: 'post' })

watch(expanded, (open) => {
  const ch = chrome.value
  const co = content.value
  chromeTween?.kill()
  contentTween?.kill()
  const spec = contentRevealTransition.value
  const hidden = contentHiddenPose.value
  if (motionReduced.value) {
    if (ch) gsap.set(ch, open ? { opacity: 1, y: 0, scaleX: 1, scaleY: 1 } : { opacity: 0, y: -4, scaleX: 0.98, scaleY: 0.98 })
    if (co) gsap.set(co, open ? { opacity: 1, y: 0, scaleX: 1, scaleY: 1, filter: 'blur(0px)' } : hidden)
    return
  }

  if (open) {
    const tl = gsap.timeline()
    if (ch) {
      tl.to(ch, {
        opacity: 1,
        y: 0,
        scaleX: 1,
        scaleY: 1,
        duration: Math.max(0.001, spec.duration),
        delay: spec.delay ?? 0,
        ease: spec.ease
      }, 0)
    }
    if (co) {
      tl.to(co, {
        opacity: 1,
        y: 0,
        scaleX: 1,
        scaleY: 1,
        filter: 'blur(0px)',
        duration: Math.max(0.001, spec.duration),
        delay: spec.delay ?? 0,
        ease: spec.ease
      }, 0.02)
      const tiles = co.querySelectorAll<HTMLElement>('.app-tile, .categories button')
      if (tiles.length && tiles.length <= 28 && motionMode.value !== 'flicker') {
        tl.fromTo(
          tiles,
          { opacity: 0, y: 6, scale: 0.95 },
          { opacity: 1, y: 0, scale: 1, duration: 0.22, stagger: 0.012, ease: spec.ease, clearProps: 'transform,opacity' },
          (spec.delay ?? 0) + 0.03
        )
      }
    }
    contentTween = tl as unknown as gsap.core.Tween
    return
  }

  if (ch) {
    // 顶部工具栏贴近胶囊区，收起时略快退场以避开胶囊文字浮现。
    chromeTween = gsap.to(ch, {
      opacity: 0,
      y: -4,
      scaleX: 0.98,
      scaleY: 0.98,
      duration: Math.max(0.001, spec.duration * 0.8),
      ease: spec.ease
    })
  }
  if (co) {
    contentTween = gsap.to(co, {
      opacity: 0,
      y: -8,
      scaleX: 0.97,
      scaleY: 0.97,
      filter: hidden.filter ?? 'blur(0px)',
      duration: Math.max(0.001, spec.duration),
      ease: spec.ease
    })
  }
}, { flush: 'post' })

watch(() => [showQuickCode.value, quickMailCode.value?.code], ([visible, code]) => {
  if (!visible || typeof code !== 'string') return
  void nextTick(() => {
    animateScrambleText(quickCodeTextEl.value, code, motionReduced.value, { chars: '0123456789', duration: 0.42 })
  })
}, { flush: 'post' })

watch(() => [capsuleStatus.value.kind, capsuleStatus.value.text], ([kind, text]) => {
  if (kind !== 'feedback' || !text) return
  void nextTick(() => {
    animateScrambleText(capsuleStatusLabelEl.value, text, motionReduced.value, { chars: '✓✦01·', duration: 0.28, speed: 0.8 })
  })
}, { flush: 'post' })

watch(() => filteredApps.value.map(item => item.id).join('|'), () => {
  if (!expanded.value || page.value !== 'applications') return
  const snap = captureFlipSnapshot(appGridEl.value, '.app-tile', motionReduced.value, 36)
  if (!snap) return
  void nextTick(() => playFlipSnapshot(snap, appGridEl.value, '.app-tile', pillTransition.value, { stagger: 0.01, maxItems: 36 }))
}, { flush: 'pre' })

watch([page, expanded, viewingMail, ocrPanelOpen], () => {
  const nav = modeNav.value
  if (!nav || motionReduced.value) return
  if (navFlipTween) {
    navFlipTween.progress(1).kill()
    navFlipTween = null
  }
  const prev = nav.querySelector<HTMLElement>('.mode-selection')
  if (!prev) return
  const flipState = Flip.getState(prev)
  void nextTick(() => {
    const next = nav.querySelector<HTMLElement>('.mode-selection')
    if (next) navFlipTween = animateFlipFromState(flipState, next, pillTransition.value, { absolute: true })
  })
}, { flush: 'pre' })

onMounted(() => {
  if (panel.value) {
    gsap.set(panel.value, {
      width: `${visibleWidth.value}px`,
      height: `${visibleHeight.value}px`,
      borderRadius: `${cornerRadius.value}px`
    })
  }
  if (capsuleTextEl.value) {
    const hide = expanded.value && !isNoticeStatus.value
    gsap.set(capsuleTextEl.value, hide ? { opacity: 0, y: -3, scale: 0.96 } : { opacity: 1, y: 0, scale: 1 })
  }
  if (chrome.value) {
    gsap.set(chrome.value, expanded.value ? { opacity: 1, y: 0, scaleX: 1, scaleY: 1 } : { opacity: 0, y: -4, scaleX: 0.98, scaleY: 0.98 })
  }
  if (content.value) {
    gsap.set(content.value, expanded.value ? { opacity: 1, y: 0, scaleX: 1, scaleY: 1, filter: 'blur(0px)' } : contentHiddenPose.value)
  }
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
  contentObserver = new ResizeObserver(() => {
    if (measureFrame || disposed) return
    measureFrame = requestAnimationFrame(() => {
      measureFrame = 0
      measureContent()
    })
  })
  if (viewport.value) observer.observe(viewport.value)
  if (content.value) contentObserver.observe(content.value)
  if (chrome.value) contentObserver.observe(chrome.value)
  window.addEventListener('hub:themed-select-closed', handleThemedSelectClosed)
  window.addEventListener('pointerup', handlePointerUp)
  window.addEventListener('pointercancel', handlePointerUp)
  systemDark.addEventListener('change', updateTheme)
  systemMotion.addEventListener('change', updateMotion)
  updateTheme()
  queueGeometry()
})

onBeforeUnmount(() => {
  disposed = true
  cancelExpandStaging()
  shellTween?.kill()
  capsuleTween?.kill()
  chromeTween?.kill()
  contentTween?.kill()
  paneTween?.kill()
  navFlipTween?.kill()
  observer?.disconnect()
  contentObserver?.disconnect()
  cancelAnimationFrame(frame)
  cancelAnimationFrame(measureFrame)
  cancelHover()
  clearTimeout(leaveTimer)
  clearTimeout(clockTimer)
  clearTimeout(feedbackTimer)
  clearTimeout(packHighlightTimer)
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
  clearTimeout(pageSwitchTimer)
  window.removeEventListener('hub:themed-select-closed', handleThemedSelectClosed)
  window.removeEventListener('pointerup', handlePointerUp)
  window.removeEventListener('pointercancel', handlePointerUp)
  systemDark.removeEventListener('change', updateTheme)
  systemMotion.removeEventListener('change', updateMotion)
})
</script>

<template>
  <div ref="viewport" class="island-viewport" :class="[`anchor-${state.ui.islandAnchor}`, { 'desktop-shell': isDesktop }]" :style="viewportStyle">
  <section ref="element" class="island-hit-area"
    :class="[{ expanded, 'page-switching': pageSwitching, moving: geometryMoving, desktop: isDesktop, dark, telemetry, compact: expandedWidth < 620, narrow: expandedWidth < 440, tucked: !revealed, reduced: state.ui.islandReducedMotion || reducedMotion, suppressed: tucked, avoiding: suppressed, 'has-quick-menu': quickMenuOpen && !expanded }, `variant-${variant}`, `motion-${motionMode}`]"
    :style="rootStyle" aria-label="顶部灵动岛"
    @mouseenter="enter" @mouseleave="onMouseLeave" @pointermove="trackPointerSpeed" @focusout="focusout" @keydown="keydown" @wheel="scrollWheel"
    @pointerdown="pressed = true" @compositionstart="composing = true" @compositionend="composing = false">
    <button v-if="!revealed" class="tuck-trigger" type="button" aria-label="展开灵动岛" @click="expand(true, true)" />
    <div
      ref="panel"
      class="island solid-surface"
      :data-motion="motionMode"
      :class="[{ expanded, 'page-switching': pageSwitching, tucked: !revealed, telemetry, 'has-quick-menu': quickMenuOpen && !expanded, 'no-physics': !(effectsActive && quality === 'full') && !motionReduced }, `variant-${variant}`, `motion-${motionMode}`]"
    >
      <LiquidSkin :width="visibleWidth" :height="visibleHeight" :shape="shape" :variant="variant" :moving="geometryMoving || pageSwitching" />
      <GlassLight v-if="materialReady && glassAvailable"
        :light-surface="!dark" :signal="taskSignal" :quality="quality" :active="effectsActive" :paused="pageSwitching"
        :corner-radius="variant === 'mecha' || variant === 'pixel' ? 4 : variant === 'blueprint' ? 2 : undefined"
        :palette="glassPalette" @unavailable="glassAvailable = false" />
      <CapsuleOrbitTrack
        :width="visibleWidth"
        :height="visibleHeight"
        :corner-radius="cornerRadius"
        :active="orbitActive"
        :tone="orbitTone"
        :speed="orbitSpeed"
        :reduced="motionReduced"
      />
      <span v-if="state.ui.wechatEnabled && state.ui.wechatAttention && !tucked" class="attention-dot" role="img" title="微信有待关注提醒" aria-label="微信有待关注提醒" />
      <div class="capsule-bar" :class="{ 'has-quick-code': showQuickOcr || showQuickCode }">
        <button class="capsule" :class="{ 'has-feedback': isNoticeStatus }" type="button" :aria-expanded="expanded"
          :title="expanded ? '单击收起 · 按住拖动位置' : (capsuleStatus.tooltip || '单击展开 · 右键快操 · 滚轮调音')"
          :aria-label="approval ? approval.title : `${capsuleStatus.ariaText || capsuleStatus.text}，${expanded ? '收起面板' : '展开面板'}`"
          :inert="!revealed" aria-description="按住拖动位置，单击展开或收起，右键打开快捷操作"
          @pointerdown.stop="windowDrag.begin" @click="activateCapsule" @contextmenu.prevent="toggleQuickMenu">
          <span
            ref="capsuleTextEl"
            class="capsule-text"
            :class="{ feedback: isNoticeStatus }"
            aria-hidden="true"
          >
            <span v-if="telemetry && !expanded" class="telemetry-led" :class="capsuleStatus.kind" />
            <span v-else-if="variant === 'mecha' && !expanded" class="mecha-badge">U·06</span>
            <span v-else-if="variant === 'pixel' && !expanded" class="pixel-hp">♥HP</span>
            <span v-else-if="variant === 'blueprint' && !expanded" class="bp-tag">图 01</span>
            <span v-else-if="variant === 'neon' && !expanded" class="neon-sign">{{ neonSign }}</span>
            <span class="capsule-label" :class="{ 'is-telemetry': capsuleStatus.kind === 'telemetry' }">
              <TelemetryCapsuleReadout
                v-if="capsuleStatus.kind === 'telemetry' && telemetryReadout"
                :readout="telemetryReadout"
                :style-mode="telemetryStyle"
                :reduced="motionReduced"
              />
              <template v-else>
                <CapsuleMorphGlyph
                  v-if="capsuleStatus.kind !== 'idle'"
                  :kind="capsuleStatus.kind"
                  :error="capsuleStatus.icon === 'info'"
                  :progress="runTask.stage === 'running' ? runProgress : null"
                  :reduced="motionReduced"
                />
                <span ref="capsuleStatusLabelEl">{{ capsuleStatus.text }}</span>
                <span v-if="runTask.stage === 'running' && runPercentText" class="capsule-pct">{{ runPercentText }}</span>
              </template>
            </span>
            <!-- 右侧副舱微徽标：与时钟/微读数和平并存 -->
            <span v-if="capsuleBadges.length && !expanded" class="capsule-badges">
              <span v-for="badge in capsuleBadges" :key="badge.id" class="capsule-badge" :class="badge.tone ? `tone-${badge.tone}` : ''" :title="badge.title">
                <Icon :name="badge.icon" :size="11" />{{ badge.text }}
              </span>
            </span>
            <span v-if="variant === 'mecha' && !expanded" class="mecha-ribs"><i /><i /><i /></span>
            <span v-else-if="variant === 'pixel' && !expanded" class="pixel-lv">1UP</span>
            <span v-else-if="variant === 'blueprint' && !expanded" class="bp-scale">1:1</span>
            <span v-else-if="variant === 'neon' && !expanded" class="neon-hours">24H</span>
          </span>
          <span class="grip" aria-hidden="true" />
        </button>
        <!-- 检测到 Win+Shift+S 等新截图进入剪贴板时：收起态浮现一键识字按钮 -->
        <GsapPresence
          :initial="motionReduced ? false : { opacity: 0, x: -4, scale: 0.94 }"
          :animate="{ opacity: 1, x: 0, scale: 1 }"
          :exit="motionReduced ? { opacity: 0 } : { opacity: 0, x: -4, scale: 0.94 }"
          :transition="pillTransition"
        >
          <div
            v-if="showQuickOcr && quickOcrClip"
            key="quick-ocr"
            class="capsule-ocr-actions"
            @pointerenter="hoveringQuickAction = true; cancelHover(); scheduleQuickOcrExpire(15000)"
            @pointerleave="hoveringQuickAction = false"
          >
            <button
              v-gsap-press="actionTapPose"
              class="capsule-code-btn capsule-ocr-btn"
              type="button"
              title="识别刚刚截取的图片中的文字"
              @click.stop="triggerQuickImageOcr"
            >
              <Icon name="scan-text" :size="11" />
              <span>识字</span>
            </button>
            <button
              v-gsap-press="actionTapPose"
              class="capsule-ocr-dismiss"
              type="button"
              title="关闭提示"
              aria-label="关闭截图识字提示"
              @click.stop="clearQuickOcr"
            >
              <Icon name="close" :size="10" />
            </button>
          </div>
        </GsapPresence>
        <!-- 检测到新邮件验证码时：收起态直接露出一键直拷按钮 -->
        <GsapPresence
          :initial="motionReduced ? false : { opacity: 0, x: -4, scale: 0.94 }"
          :animate="{ opacity: 1, x: 0, scale: 1 }"
          :exit="motionReduced ? { opacity: 0 } : { opacity: 0, x: -4, scale: 0.94 }"
          :transition="pillTransition"
        >
          <button
            v-if="showQuickCode && quickMailCode"
            key="quick-code"
            v-gsap-press="actionTapPose"
            class="capsule-code-btn"
            type="button"
            :disabled="copyingQuickCode"
            :title="`一键复制来自 ${quickMailCode.sender || '邮件'} 的验证码`"
            @click.stop="copyQuickMailCode"
          >
            <Icon name="copy" :size="11" />
            <span ref="quickCodeTextEl">{{ quickMailCode.code }}</span>
          </button>
        </GsapPresence>
      </div>
      <!-- 打包/联调实时日志、INFO、出错详情与剩余分钟副条 -->
      <GsapPresence
        :initial="motionReduced ? false : { opacity: 0, y: -4 }"
        :animate="{ opacity: 1, y: 0 }"
        :exit="motionReduced ? { opacity: 0 } : { opacity: 0, y: -4 }"
        :transition="pillTransition"
      >
        <div
          v-if="showPackStrip"
          key="pack-live-strip"
          class="capsule-pack-strip"
          :class="`is-${packLiveBadge.toLowerCase()}`"
          role="status"
          @click="openPackConsoleFromCapsule"
        >
          <span class="pack-badge">{{ packLiveBadge }}</span>
          <span ref="packInfoTextEl" class="pack-msg" :title="packLiveSummary">{{ packLiveSummary }}</span>
          <span v-if="runTask.stage === 'error'" class="pack-jump">查看日志 ↗</span>
          <button
            v-else-if="runTask.stage !== 'running'"
            type="button"
            class="pack-dismiss"
            title="收起提示"
            aria-label="收起打包状态条"
            @click.stop="packStripDismissedKey = runKey; packHighlightTone = null"
          >
            <Icon name="close" :size="10" />
          </button>
          <span v-if="runTask.stage === 'running'" class="pack-bar">
            <i :style="{ width: runProgress !== null ? `${Math.round(runProgress * 100)}%` : '38%' }" :class="{ indeterminate: runProgress === null }" />
          </span>
        </div>
      </GsapPresence>
      <!-- 右键胶囊免展开快捷操作条 -->
      <GsapPresence
        :initial="motionReduced ? false : { opacity: 0, y: -4, scale: 0.96 }"
        :animate="{ opacity: 1, y: 0, scale: 1 }"
        :exit="motionReduced ? { opacity: 0 } : { opacity: 0, y: -4, scale: 0.96 }"
        :transition="pillTransition"
      >
        <div
          v-if="quickMenuOpen && !expanded && !tucked"
          key="quick-menu"
          class="capsule-quick-menu"
          role="toolbar"
          aria-label="胶囊快捷操作"
        >
          <button v-gsap-press="actionTapPose" type="button" @click="runQuickAction('pause')">
            <Icon :name="state.ui.paused ? 'play' : 'pause'" :size="12" />{{ state.ui.paused ? '恢复剪贴板' : '暂停剪贴板' }}
          </button>
          <button v-gsap-press="actionTapPose" type="button" @click="runQuickAction('ocr')"><Icon name="scan-text" :size="12" />截图识字</button>
          <button v-gsap-press="actionTapPose" type="button" @click="runQuickAction('pin-open')"><Icon name="pin" :size="12" />钉住展开</button>
          <button v-gsap-press="actionTapPose" type="button" @click="runQuickAction('audio')"><Icon name="volume" :size="12" />调音台</button>
          <button v-gsap-press="actionTapPose" type="button" @click="runQuickAction('settings')"><Icon name="settings" :size="12" />设置</button>
        </div>
      </GsapPresence>
      <span class="capsule-announcement" role="status" aria-atomic="true">{{ !tucked && isNoticeStatus ? capsuleStatus.text : '' }}</span>
      <div class="expanded-content" :class="{ 'can-scroll': needsOuterScroll, 'fixed-search': !approval && !ocrPanelOpen && !viewingMail && page === 'clipboard' && !state.ui.detailId }" :inert="!expanded">
        <div
          ref="chrome"
          v-show="!approval"
          class="island-chrome"
        >
          <header class="toolbar">
            <nav ref="modeNav" class="mode-nav" aria-label="灵动岛功能">
              <button v-for="(section, idx) in sections" :key="section.value" v-gsap-press="actionTapPose" type="button"
                :title="`${section.label}（按 ${idx + 1} 或 Alt+${idx + 1}）`" :aria-label="section.label" :aria-pressed="!viewingMail && !ocrPanelOpen && page === section.value"
                :class="{ selected: !viewingMail && !ocrPanelOpen && page === section.value }"
                @click="selectPage(section.value)">
                <span
                  v-if="!viewingMail && !ocrPanelOpen && page === section.value"
                  data-flip-id="island-nav-pill"
                  class="mode-selection"
                  aria-hidden="true"
                />
                <Icon :name="section.icon" :size="19" :stroke="1.7" />
                <span class="mode-label">{{ section.label }}</span>
              </button>
            </nav>
            <div class="tools">
              <button
                v-if="(runTask.stage === 'running' || packHighlightTone !== null) && (viewingMail || ocrPanelOpen || page !== 'pack')"
                v-gsap-press="actionTapPose"
                class="toolbar-pack-pill"
                :class="runTask.stage === 'error' ? 'is-err' : runTask.stage === 'done' ? 'is-done' : 'is-run'"
                type="button"
                :title="`${runTask.name || '打包'}：${packLiveSummary}（点击切回打包台）`"
                @click="openPackConsoleFromCapsule"
              >
                <span class="pill-dot" aria-hidden="true" />
                <span class="pill-text">{{ packPhase.label || '打包' }}</span>
                <span v-if="runTask.stage === 'running'" class="pill-time">{{ clock(runElapsed) }}</span>
              </button>
              <button v-if="mailNotices.length" v-gsap-press="actionTapPose" class="round-action" type="button" :title="viewingMail ? '返回当前功能' : mailSummary || '查看最近邮件'" aria-label="最近邮件" :aria-pressed="viewingMail" @click="closeOcrPanel(); mailSelected = !viewingMail"><Icon name="mail" :size="16" /></button>
              <button v-gsap-press="actionTapPose" class="round-action" type="button" :title="`截图识字（${state.shortcuts.ocr || 'Ctrl+Shift+O'}）`" aria-label="截图识字" :aria-pressed="ocrPanelOpen" @click="startScreenOcr"><Icon name="scan-text" :size="16" /></button>
              <button v-if="!viewingMail && !ocrPanelOpen && page === 'translation'" v-gsap-press="actionTapPose" class="round-action" type="button" title="独立翻译窗口" aria-label="独立翻译窗口" @click="openSeparateTranslation"><Icon name="external" :size="16" /></button>
              <button v-gsap-press="actionTapPose" class="round-action pin-action" type="button" :title="pinned ? '已钉住展开（点击或按 Esc 取消）' : '钉住展开，移开鼠标不自动收起（Alt+P）'" aria-label="钉住灵动岛" :aria-pressed="pinned" @click="pinned = !pinned"><Icon name="pin" :size="15" /></button>
              <button v-gsap-press="actionTapPose" class="round-action" type="button" title="设置" aria-label="设置" @click="openSettings()"><Icon name="settings" :size="16" /></button>
              <button v-gsap-press="actionTapPose" class="round-action collapse-action" type="button" title="收起（Esc）" aria-label="收起灵动岛" @click="collapseNow(true)"><Icon name="chevron-down" :size="16" /></button>
            </div>
          </header>
          <div v-if="state.ui.wechatAttention" class="attention-strip" role="status">
            <span>微信有新提醒</span>
            <button type="button" @click="openWechat">打开微信</button>
            <button type="button" @click="invokeHost('clear_wechat_attention')">清除待关注</button>
          </div>
        </div>
        <div
          ref="content"
          class="island-content"
        >
          <ApprovalPrompt v-if="approval" :key="approval.id" :card="approval" :count="approvalCount" :busy="answering" @answer="answerApproval" />
          <OcrResultPanel v-else-if="ocrPanelOpen && expanded" />
          <MailNotices v-else-if="viewingMail && expanded" :notices="mailNotices" @error="pushToast($event, 'error')" />
          <div v-show="!approval && !ocrPanelOpen && !viewingMail" ref="ordinaryPaneEl" class="ordinary-pane" :style="{ '--pane-dir': paneDir }">
            <div
              v-if="page === 'applications'"
              key="applications"
              class="applications"
            >
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
                <button v-for="category in state.categories" :key="category.id" v-gsap-press="actionTapPose" type="button"
                  :class="{ selected: state.ui.activeCategoryId === category.id }" :aria-pressed="state.ui.activeCategoryId === category.id"
                  @click="setCategory(category.id)">{{ category.name }}</button>
              </nav>
              <p v-if="!filteredApps.length" class="empty">{{ appSearch ? '没有匹配的应用' : '这个分类还没有应用' }}</p>
              <div v-else ref="appGridEl" class="app-grid scroll-y" :class="{ 'with-names': showAppNames }">
                <button
                  v-for="app in filteredApps"
                  :key="app.id"
                  v-gsap-hover="actionHoverPose"
                  v-gsap-press="tileTapPose"
                  class="app-tile"
                  type="button"
                  :title="app.name"
                  :aria-label="app.name"
                  @click="launch(app.id)"
                >
                  <img v-if="iconFor(app.target)" :src="iconFor(app.target)" alt="" />
                  <span v-else class="fallback" :style="{ background: app.color }">{{ app.initials }}</span>
                  <span v-if="showAppNames" class="app-name">{{ app.name }}</span>
                </button>
              </div>
            </div>
            <div
              v-else
              :key="page"
              class="feature-pane scroll-y"
            >
              <ClipboardPopover v-if="page === 'clipboard'" @reading-explanation="setReadingExplanation" />
              <TranslationPanel v-else-if="page === 'translation'" />
              <FileShelf v-else-if="page === 'files'" :drag-over="receivingFiles" />
              <AudioMixer v-else-if="page === 'audio'" :active="expanded" />
              <PackConsole v-else-if="page === 'pack'" />
            </div>
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
/* 桌面态居中/右停靠时随真实原生窗口宽度自动对齐，杜绝收起尾帧窗口缩容前的瞬间水平闪跳。 */
.island-viewport.desktop-shell.anchor-center { margin-inline: auto; }
.island-viewport.desktop-shell.anchor-right { margin-left: auto; }
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
/* 桌面原生窗口稳态由 SetWindowRgn 紧贴面板圆角裁切，外投影稳态不可见，
   只会在切页/开合临时放开矩形窗口区域时露出黑影；因此桌面态仅保留内高光与内描边。 */
.island-hit-area.desktop {
  --glass-backdrop: none;
  --island-drop-shadow: inset 0 1px 0 var(--island-top-sheen), inset 0 0 0 1px var(--island-rim, var(--island-outline));
}
/* 展开态与收起态保持统一的 2px 顶部边距，消除开合瞬间 10px 纵向跳动。 */
.island-hit-area.expanded { --surface-inset: 2px; }
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
.island.variant-standard {
  background: var(--surface-glass);
  backdrop-filter: var(--glass-backdrop);
  box-shadow: var(--island-drop-shadow);
}
.island.variant-telemetry {
  background: var(--surface);
  box-shadow: inset 0 0 0 1px var(--island-rim, var(--island-outline)), inset 0 1px 0 var(--island-top-sheen);
}
.island {
  position: absolute; top: var(--surface-inset); left: -320px; right: -320px; margin-inline: auto; transform-origin: 50% 0;
  overflow: hidden; border-radius: 24px; color: var(--text); contain: layout;
  box-shadow: inset 0 1px 0 var(--island-top-sheen);
}
.island-viewport.anchor-left > .island-hit-area > .island { left: 0; right: auto; margin-inline: 0; transform-origin: 0 0; }
.island-viewport.anchor-right > .island-hit-area > .island { left: auto; right: 0; margin-inline: 0; transform-origin: 100% 0; }
.island.dismiss-dragging { transition: none !important; }
.island.expanded { border-radius: 18px; }
.island.tucked { border-radius: 5px; box-shadow: none; }
.island.tucked .capsule-bar,
.island.tucked .capsule { visibility: hidden; }
.island.tucked :deep(.glass-light) { visibility: hidden; }
.capsule-bar {
  position: relative; z-index: 3; display: flex; align-items: center; justify-content: center; width: 100%;
}
.capsule-bar.has-quick-code { justify-content: space-between; padding: 0 7px 0 8px; gap: 6px; }
.capsule {
  position: relative; z-index: 3; display: flex; align-items: center; justify-content: center;
  height: 24px; width: min(260px, calc(100% - 32px)); margin: 0 auto; white-space: nowrap; font-size: 12px;
  cursor: grab; touch-action: none; user-select: none;
  transition: height var(--geometry-time) var(--height-ease, var(--geometry-ease)), opacity 110ms ease-out 40ms;
}
.capsule-bar.has-quick-code .capsule { flex: 1; width: auto; margin: 0; justify-content: flex-start; min-width: 0; overflow: hidden; }
.capsule:active { transform: none; }
.window-dragging .capsule { cursor: grabbing; }
.island:not(.expanded) .capsule { height: var(--capsule-height); width: 100%; }
.island:not(.expanded) .capsule-bar.has-quick-code .capsule { width: auto; }
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
.capsule-pct {
  display: inline-flex; align-items: center; height: 17px; padding: 0 5px; margin-left: 2px;
  border-radius: 999px; font-family: var(--font-mono); font-size: 10px; font-weight: 700;
  font-variant-numeric: tabular-nums;
  background: var(--accent-weak); color: var(--accent);
}
.capsule-pack-strip {
  position: relative; z-index: 3; display: flex; align-items: center; gap: 6px;
  height: 28px; margin: 0 8px 4px; padding: 0 8px;
  border-radius: 10px; border: 1px solid var(--line);
  background: var(--surface-2); color: var(--text-2);
  font-size: 11px; cursor: pointer; overflow: hidden;
  transition: border-color 140ms ease, background-color 140ms ease;
}
.capsule-pack-strip:hover { border-color: var(--line-strong); color: var(--text); }
.capsule-pack-strip.is-err { border-color: color-mix(in srgb, var(--danger) 48%, var(--line)); background: color-mix(in srgb, var(--danger) 10%, var(--surface-2)); }
.capsule-pack-strip.is-done { border-color: color-mix(in srgb, #10b981 48%, var(--line)); background: color-mix(in srgb, #10b981 10%, var(--surface-2)); }
.pack-badge {
  flex: none; padding: 1px 5px; border-radius: 5px;
  font-family: var(--font-mono); font-size: 9.5px; font-weight: 700; letter-spacing: .04em;
  background: var(--accent-weak); color: var(--accent);
}
.capsule-pack-strip.is-warn .pack-badge { background: color-mix(in srgb, #f59e0b 16%, transparent); color: #d97706; }
.capsule-pack-strip.is-err .pack-badge { background: var(--danger-weak); color: var(--danger); }
.capsule-pack-strip.is-done .pack-badge { background: color-mix(in srgb, #10b981 16%, transparent); color: #10b981; }
.pack-msg { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font-size: 11px; }
.pack-jump { flex: none; font-size: 10.5px; font-weight: 650; color: var(--danger); }
.pack-dismiss {
  flex: none; display: grid; place-items: center; width: 18px; height: 18px; border-radius: 99px;
  color: var(--text-3);
}
.pack-dismiss:hover { background: var(--hover); color: var(--text); }
.pack-bar {
  position: absolute; left: 0; right: 0; bottom: 0; height: 2px; background: color-mix(in srgb, var(--line) 50%, transparent);
}
.pack-bar i {
  display: block; height: 100%; border-radius: 99px; background: var(--accent);
  transition: width 360ms var(--ease-out);
}
.pack-bar i.indeterminate { animation: pack-bar-slide 1.2s ease-in-out infinite alternate; }
@keyframes pack-bar-slide { from { transform: translateX(0); } to { transform: translateX(160%); } }
.capsule-code-btn {
  flex: none; display: inline-flex; align-items: center; gap: 4px; height: 22px; padding: 0 8px;
  border-radius: 999px; border: 1px solid var(--accent); background: var(--accent-weak); color: var(--text);
  font-family: var(--font-mono); font-size: 11px; font-weight: 700; letter-spacing: .04em; cursor: pointer;
  transform-origin: right center;
  touch-action: manipulation;
  transition: background-color 120ms ease, color 120ms ease;
}
.capsule-code-btn:hover { background: var(--accent); color: var(--accent-text); }
.capsule-ocr-actions {
  position: relative; z-index: 3; flex: none; display: inline-flex; align-items: center; gap: 4px;
  transform-origin: right center;
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
}
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
.expanded .expanded-content.can-scroll .island-chrome,
.expanded .expanded-content.can-scroll .island-content { max-width: 100%; }
/* 仅在收起动画期间隐去内部滚动条；展开时让滚动条随正文透明度自然浮现，避免尾帧突兀闪现。 */
.island-hit-area:not(.expanded).moving :deep(::-webkit-scrollbar-thumb) { background: transparent !important; }
.island-hit-area:not(.expanded).moving .feature-pane,
.island-hit-area:not(.expanded).moving .app-grid { scrollbar-color: transparent transparent; }
.moving .island-chrome,
.moving .island-content { will-change: transform, opacity; }
/* 工具栏与正文共同锁定展开态最终内容宽并水平居中，收起动画中途不被外壳挤压、不触发导航栏滚动条。 */
.island-chrome {
  position: relative; left: 50%; translate: -50% 0;
  display: flow-root;
  width: calc(var(--content-width) - 2 * var(--content-gutter));
}
.island-content {
  position: relative; left: 50%; translate: -50% 0;
  width: calc(var(--content-width) - 2 * var(--content-gutter));
}
.ordinary-pane { position: relative; display: block; overflow: clip; }
.toolbar {
  display: flex; flex-wrap: nowrap; align-items: center; justify-content: space-between;
  gap: 6px; margin-bottom: 8px; padding: 0 0 7px; border-bottom: 1px solid var(--line);
}
.tools { display: flex; align-items: center; gap: 2px; flex: none; }
.toolbar-pack-pill {
  display: inline-flex; align-items: center; gap: 5px; height: 26px; padding: 0 8px; margin-right: 2px;
  border-radius: 999px; border: 1px solid color-mix(in srgb, var(--accent) 42%, var(--line));
  background: var(--accent-weak); color: var(--accent); font-size: 11px; font-weight: 600;
  white-space: nowrap; cursor: pointer;
}
.toolbar-pack-pill.is-done {
  border-color: color-mix(in srgb, #10b981 48%, var(--line));
  background: color-mix(in srgb, #10b981 12%, var(--surface)); color: #10b981;
}
.toolbar-pack-pill.is-err {
  border-color: color-mix(in srgb, var(--danger) 50%, var(--line));
  background: color-mix(in srgb, var(--danger) 12%, var(--surface)); color: var(--danger);
}
.toolbar-pack-pill .pill-dot {
  width: 6px; height: 6px; border-radius: 50%; background: currentColor; flex: none;
}
.toolbar-pack-pill.is-run .pill-dot {
  box-shadow: 0 0 6px currentColor;
}
.toolbar-pack-pill .pill-time {
  font-family: var(--font-mono); font-size: 10px; font-variant-numeric: tabular-nums; opacity: .88;
}
.round-action {
  display: grid; place-items: center; flex: none; width: 28px; height: 28px;
  border-radius: 8px; border: 1px solid transparent; background: transparent; color: var(--text-3);
  transition: background 140ms ease, border-color 140ms ease, color 140ms ease, box-shadow 140ms ease;
}
.collapse-action svg { transform: rotate(180deg); }
.round-action:hover { background: var(--tile-surface); border-color: var(--line); color: var(--text); box-shadow: var(--tile-shadow); }
.tools .round-action[aria-pressed='true'] { color: var(--accent); border-color: var(--accent); background: var(--accent-weak); box-shadow: var(--tile-shadow); }
.categories { display: flex; align-items: center; flex-wrap: wrap; gap: 5px; max-height: 74px; overflow-y: auto; overflow-x: hidden; scrollbar-gutter: stable; margin: 0 0 8px; padding: 2px; }
.categories button { flex: none; padding: 3px 10px; border-radius: 20px; border: 1px solid transparent; min-height: 26px; font-size: 12px; color: var(--text-2); transition: background 140ms ease, color 140ms ease, box-shadow 140ms ease, border-color 140ms ease; }
.categories .selected { color: var(--text); background: var(--tile-surface); font-weight: 600; border-color: var(--line); box-shadow: var(--tile-shadow); }
.categories button:hover:not(.selected) { color: var(--text); background: var(--hover); border-color: var(--line); }
.app-toolbar {
  display: flex; align-items: center; gap: 6px; margin-bottom: 6px;
}
.app-search-box {
  flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px;
  height: 31px; padding: 0 10px; border-radius: 10px;
  background: var(--well-bg); border: 1px solid var(--line); color: var(--text-2);
  box-shadow: var(--well-shadow);
  transition: border-color 140ms ease, box-shadow 140ms ease, background 140ms ease;
}
.app-search-box:focus-within { border-color: var(--accent); background: var(--surface); box-shadow: 0 0 0 2px var(--accent-weak), var(--well-shadow); color: var(--text); }
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
  transition: background 150ms ease, border-color 150ms ease, box-shadow 150ms ease;
}
.variant-standard .app-tile {
  background: var(--tile-surface);
  border-color: var(--line);
  box-shadow: var(--tile-shadow);
}
.app-name {
  width: 100%; font-size: 11px; line-height: 1.25; color: var(--text-2); text-align: center;
  white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
}
.app-tile:hover .app-name { color: var(--text); }
.app-tile:hover { background: var(--tile-surface); border-color: var(--line-strong); box-shadow: var(--tile-hover-shadow); }
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
  min-width: 0; width: auto; height: 38px; padding: 3px;
  overflow-x: auto; overflow-y: hidden; color: var(--text);
  border-radius: 12px;
}
/* 显式压过各主题通配符的 scrollbar-width: thin，确保顶部导航栏永不露出原生横向滚动条。 */
.island .mode-nav {
  scrollbar-width: none !important;
  scrollbar-color: transparent transparent !important;
}
.variant-standard .mode-nav {
  background: var(--well-bg);
  box-shadow: var(--well-shadow);
  border: 1px solid var(--line);
}
.island .mode-nav::-webkit-scrollbar { display: none !important; width: 0 !important; height: 0 !important; }
.mode-selection {
  position: absolute; inset: 0; z-index: 0;
  border-radius: 9px;
  background: var(--tile-surface);
  border: 1px solid var(--line);
  box-shadow: var(--tile-shadow);
  pointer-events: none;
}
.mode-nav button {
  position: relative; z-index: 1; display: inline-flex; flex: none; align-items: center; justify-content: center; gap: 0;
  height: 34px; min-width: 34px; padding: 0 7px; border-radius: 10px; color: var(--text-2); font-size: 12.5px;
  transition: color 130ms ease;
}
.mode-nav button > svg,
.mode-nav .mode-label { position: relative; z-index: 1; }
.mode-nav .mode-label {
  max-width: 0; opacity: 0; overflow: hidden; white-space: nowrap; font-size: 12px; font-weight: 650;
  transition: opacity 140ms ease;
}
.mode-nav button.selected { gap: 5px; padding: 0 10px; color: var(--text); font-weight: 600; }
.mode-nav button.selected .mode-label { max-width: 64px; opacity: 1; }
.narrow .mode-nav { height: 34px; padding: 2px; gap: 2px; }
.narrow .mode-nav button,
.narrow .mode-nav button.selected { height: 28px; min-width: 28px; padding: 0 5px; gap: 0; }
.narrow .mode-nav button.selected .mode-label { max-width: 0; opacity: 0; }
.narrow .round-action { width: 25px; height: 25px; }
.mode-nav .selected svg { color: var(--accent); }
.mode-nav button:hover { color: var(--text); }
button {
  touch-action: manipulation;
  transition: background-color 110ms ease, color 110ms ease, border-color 110ms ease;
}
.feature-pane button:active:not(:disabled),
.attention-strip button:active:not(:disabled) {
  transform: scale(.97);
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
.narrow .feature-pane :deep(.filter-bar .chip) { padding: 0 6px; font-size: 11px; }

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
.variant-pixel:not(.expanded) .capsule-label.is-telemetry { padding: 1px 6px; font-size: 11.5px; }
.variant-pixel:not(.expanded) :deep(.tel-pre) { color: #86efac; }
.variant-pixel:not(.expanded) :deep(.tel-pre.accent) { color: #facc15; }
.variant-pixel:not(.expanded) :deep(.tel-val) { color: #bef264; }
.variant-pixel:not(.expanded) :deep(.tel-bar-track),
.variant-pixel:not(.expanded) :deep(.tel-pulse-bar) { border-radius: 0; }

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
.variant-mecha .feature-pane :deep(.seg-btn.is-active) { background: transparent; color: #111318; box-shadow: none; }
.variant-mecha .feature-pane :deep(.seg-btn.is-active .mode-seg-pill) {
  border-radius: 2px; background: var(--accent); border-color: var(--accent); box-shadow: none;
  clip-path: polygon(5px 0, 100% 0, 100% calc(100% - 5px), calc(100% - 5px) 100%, 0 100%, 0 5px);
}
.variant-mecha .feature-pane :deep(.seg-btn.is-active .seg-count) { background: rgba(17, 19, 24, .22); color: #111318; }

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
  box-shadow: 0 2px 0 #0f0d1a;
}
.variant-pixel .mode-nav .selected svg { color: #0f0d1a; }
.variant-pixel .mode-nav button:active,
.variant-pixel .round-action:active { box-shadow: none; }
.variant-pixel .island-content {
  background: #1c221e; border: 2px solid #0f0d1a; padding: 10px;
  box-shadow: inset 0 3px 0 rgba(0, 0, 0, .45);
}
.variant-pixel .categories button {
  border-radius: 0; font-family: var(--font-mono); font-size: 12px;
  background: #2b352e; border: 2px solid #0f0d1a; box-shadow: 0 2px 0 #0f0d1a; color: #cbd5e1;
}
.variant-pixel .categories .selected { background: #facc15; color: #0f0d1a; border-color: #0f0d1a; box-shadow: 0 1px 0 #0f0d1a; }
.variant-pixel .app-tile {
  border-radius: 0; background: #2b352e; border: 2px solid #0f0d1a; box-shadow: 0 3px 0 #0f0d1a;
}
.variant-pixel .app-tile:hover { background: #36443a; border-color: #0f0d1a; }
.variant-pixel .app-tile:active { box-shadow: none; }
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
  background: transparent; color: #0f0d1a; border-color: transparent; box-shadow: none;
}
.variant-pixel .feature-pane :deep(.seg-btn.is-active .mode-seg-pill) {
  border-radius: 0; background: var(--accent); border: 2px solid #0f0d1a; box-shadow: none;
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
.variant-pixel .feature-pane :deep(.row.is-active .row-title),
.variant-pixel .feature-pane :deep(.row.is-active .row-meta-text),
.variant-pixel .feature-pane :deep(.row.is-active .snippet-title-text) { color: #0f0d1a; }
.variant-pixel .feature-pane :deep(.row.is-active .thumb) { background: #0f0d1a; color: #a3e635; border-color: #0f0d1a; }
.variant-pixel .feature-pane :deep(.row.is-active .index-kbd),
.variant-pixel .feature-pane :deep(.row.is-active .smart-tag) { background: #0f0d1a; color: #facc15; border-color: #0f0d1a; opacity: 1; }
.variant-pixel .feature-pane :deep(.row.is-active .row-actions .chip) {
  background: #0f0d1a; color: #facc15; border: 1.5px solid #0f0d1a;
}
.variant-pixel .feature-pane :deep(.row.is-active .row-actions .chip:disabled) {
  background: #1c221e; color: #cbd5e1; border-color: #0f0d1a;
}
.variant-pixel .feature-pane :deep(.row.is-active .row-actions .icon-btn) { color: #0f0d1a; }
.variant-pixel .feature-pane :deep(.row.is-active .row-actions .icon-btn:hover) { background: rgba(15, 13, 26, .16); color: #0f0d1a; }

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
