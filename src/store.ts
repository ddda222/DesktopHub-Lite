/**
 * 界面状态与动作。
 *
 * 多窗口注意：桌面版里 bar / pop / settings / translation 是独立 WebView，
 * 各自有一份 store。所以「哪个弹层打开、当前分类、主题、暂停」这些跨窗口状态
 * 以 Rust 为准，这里只保留一份镜像并订阅 `ui-state` 广播。
 * 业务数据（应用配置、剪贴板、翻译设置）也来自 Rust 命令，不再用合成数据。
 */
import { computed, reactive, watch } from 'vue'
import { host, newRequestId } from '@/lib/desktop/host'
import { isDarkThemeMode, normalizeThemeMode } from '@/lib/themes'
import { normalizeIslandShape, type IslandShape } from '@/lib/islandShapes'
import { isMotionMode, normalizeMotionMode, type MotionMode } from '@/lib/motions'
import {
  defaultIslandAppearance,
  dismissPanel,
  dismissSettings,
  describeTarget,
  importDesktopShortcuts,
  type ImportShortcutsResult,
  invokeHost,
  isDesktop,
  windowLabel,
  onApplicationsChanged,
  listenHost,
  onUiState,
  pickApplicationTarget,
  readAppIcons,
  requestCategory,
  requestIslandShape,
  requestMotion,
  requestPanel,
  requestSettings,
  requestTheme,
  type SettingsSection,
  type UiState
} from '@/lib/desktop/desktop'
import type {
  AppConfig,
  Category,
  ClipItem,
  ClipboardRecord,
  HubApp,
  HubSettings,
  SnippetCopyField,
  SnippetDraft,
  SnippetItem,
  SnippetKind,
  SnippetPayload,
  Toast,
  ToastKind,
  TranslateDirection,
  TranslationResult,
  TranslationSettings
} from '@/lib/desktop/types'

export type PanelName = 'clipboard' | 'translation' | 'files' | 'audio' | 'pack' | 'category' | 'overflow' | null
export type { SettingsSection } from '@/lib/desktop/desktop'

export interface DiscoveredAppItem {
  name: string
  target: string
  suggestedCategoryId: string
  discoveredAtMs: number
}

export interface DismissedAppItem {
  name: string
  target: string
  dismissedAtMs: number
}

export interface DiscoverySnapshot {
  initialized: boolean
  lastScanMs: number
  pending: DiscoveredAppItem[]
  dismissed: DismissedAppItem[]
}

export const state = reactive({
  /** 数据是否已经从宿主加载过；false 时界面显示占位而不是假装有数据 */
  ready: false,
  shortcuts: { bar: 'Ctrl+Alt+Space', clipboard: 'Ctrl+Shift+V', translation: 'Ctrl+Shift+F', ocr: 'Ctrl+Shift+O', errors: [] as string[] },
  ocr: {
    open: false,
    status: 'idle' as 'idle' | 'loading' | 'done' | 'error',
    text: '',
    lineCount: 0,
    charCount: 0,
    language: '',
    elapsedMs: 0,
    source: 'screen' as 'screen' | 'clipboard',
    error: ''
  },
  loadError: '',
  apps: [] as HubApp[],
  categories: [] as Category[],
  clips: [] as ClipItem[],
  groups: [] as string[],
  snippets: [] as SnippetItem[],
  snippetGroups: ['未分组', '账密'] as string[],
  clipboardPreferences: { captureImages: true, historyLimit: null as number | null, defaultHistoryLimit: 500 },
  settings: {
    theme: 'dark',
    recordClipboard: true,
    launchAtLogin: false,
    clipboardHotkey: ''
  } as HubSettings,
  translationSettings: {
    providerId: '',
    model: '',
    baseUrl: '',
    direction: 'zh-en',
    hasSecret: false
  } as TranslationSettings,
  translation: {
    source: '',
    direction: 'zh-en' as TranslateDirection,
    status: 'idle' as 'idle' | 'loading' | 'error' | 'done',
    result: null as TranslationResult | null,
    error: '',
    requestSeq: 0,
    filledFromClip: false
  },
  toasts: [] as Toast[],
  discovery: {
    initialized: false,
    lastScanMs: 0,
    pending: [] as DiscoveredAppItem[],
    dismissed: [] as DismissedAppItem[],
    scanning: false
  },
  /** 目标路径 → 图标 data URI。读不到就不放进来，界面回退到首字母色块。 */
  icons: {} as Record<string, string>,
  ui: {
    islandExpanded: false,
    islandAnchor: 'center',
    islandShape: 'floating' as IslandShape,
    islandPage: 'applications' as UiState['islandPage'],
    islandHoverMs: 320,
    islandLeaveMs: 450,
    islandReducedMotion: false,
    islandAppearance: defaultIslandAppearance(),
    mixerVisibility: { devices: {}, apps: {} } as UiState['mixerVisibility'],
    islandCaptureError: null as string | null,
    translationPending: 0,
    fullscreenSuppressed: false,
    wechatEnabled: false,
    wechatStatus: 'off',
    wechatAttention: false,
    wechatEvent: 0,
    activeCategoryId: 'all',
    panel: null as PanelName,
    settingsOpen: false,
    settingsSection: 'general' as SettingsSection,
    clipboardView: 'history' as 'history' | 'snippets',
    query: '',
    favoritesOnly: false,
    group: '全部',
    detailId: null as string | null,
    activeIndex: 0,
    snippetQuery: '',
    snippetGroup: '全部',
    snippetPinnedOnly: false,
    snippetActiveIndex: 0,
    paused: false,
    dockOffset: { x: 0, y: 0 }
  }
})

/* ---------- 数据加载 ---------- */

function toClipItem(record: ClipboardRecord): ClipItem {
  return {
    id: record.id,
    kind: record.kind,
    text: record.text ?? undefined,
    favorite: record.favorite ?? false,
    note: record.note ?? '',
    group: record.group,
    copiedAt: record.copiedAt,
    width: record.width ?? undefined,
    height: record.height ?? undefined
  }
}

let clipboardRefresh = 0
async function refreshClipboard(): Promise<void> {
  const seq = ++clipboardRefresh
  const [entries, status] = await Promise.all([
    invokeHost<ClipboardRecord[]>('list_clipboard_entries'),
    invokeHost<{ groups: string[]; paused: boolean; captureImages: boolean; historyLimit: number | null; defaultHistoryLimit: number }>('clipboard_status')
  ])
  if (seq !== clipboardRefresh) return
  const previous = new Map(state.clips.map((clip) => [clip.id, clip]))
  state.clips = entries.map((record) => ({ ...toClipItem(record), imageUrl: previous.get(record.id)?.imageUrl, imageMissing: previous.get(record.id)?.imageMissing }))
  state.groups = status.groups
  state.clipboardPreferences = {
    captureImages: status.captureImages,
    historyLimit: status.historyLimit,
    defaultHistoryLimit: status.defaultHistoryLimit
  }
  state.ui.paused = status.paused
  state.settings.recordClipboard = !status.paused
  if (!groupFilters.value.includes(state.ui.group)) state.ui.group = '全部'
  if (state.ui.panel === 'clipboard') void ensureClipThumbnails(state.clips.slice(0, 20))
}

const loadingThumbIds = new Set<string>()

/** 仅按需读取指定剪贴板条目（当前页）的图片缩略图，避免全量加载造成卡顿。 */
export async function ensureClipThumbnails(items: ClipItem[]): Promise<void> {
  if (!isDesktop) return
  for (const clip of items) {
    if (clip.kind !== 'image' || clip.imageUrl || clip.imageMissing || loadingThumbIds.has(clip.id)) {
      continue
    }
    loadingThumbIds.add(clip.id)
    try {
      clip.imageUrl = await invokeHost<string>('read_clipboard_thumbnail', { id: clip.id })
    } catch {
      clip.imageMissing = true
    } finally {
      loadingThumbIds.delete(clip.id)
    }
  }
}

async function loadClipImages(): Promise<void> {
  await ensureClipThumbnails(state.clips.slice(0, 20))
}

let translationSettingsRefresh = 0
async function refreshTranslationSettings(): Promise<void> {
  const ticket = ++translationSettingsRefresh
  const settings = await invokeHost<TranslationSettings>('get_translation_settings')
  if (ticket === translationSettingsRefresh) {
    if (settings.direction && settings.direction !== state.translationSettings?.direction) {
      setTranslationDirection(settings.direction)
    }
    state.translationSettings = settings
  }
}

function applySnippets(payload: SnippetPayload): void {
  state.snippetGroups = payload.groups.length ? [...payload.groups] : ['未分组', '账密']
  state.snippets = payload.entries.map((entry) => ({ ...entry }))
  if (state.ui.snippetGroup !== '全部' && !state.snippetGroups.includes(state.ui.snippetGroup)) {
    state.ui.snippetGroup = '全部'
  }
}

let snippetsRefresh = 0
async function refreshSnippets(): Promise<void> {
  const ticket = ++snippetsRefresh
  const payload = await invokeHost<SnippetPayload>('list_snippets')
  if (ticket === snippetsRefresh) applySnippets(payload)
}

function applyApps(config: AppConfig): void {
  state.categories = config.categories.map((cat) => ({ ...cat }))
  state.apps = config.apps.map((app) => ({ ...app }))
  void loadMissingIcons()
}

/**
 * 读取还没缓存的目标图标。
 *
 * 图标由 Rust 从 Shell 抽取并落盘缓存，这里只补缺失的那些，
 * 避免每次列表变化都把全部目标重读一遍。读失败的目标不写入，
 * 界面据此回退首字母色块——不伪造图标。
 */
async function loadMissingIcons(extraTargets: string[] = []): Promise<void> {
  if (!isDesktop) return
  const targets = [
    ...new Set([
      ...state.apps.filter((app) => !app.hidden && !app.broken).map((app) => app.target),
      ...extraTargets
    ])
  ].filter((target) => target && !state.icons[target])
  if (targets.length === 0) return
  const payloads = await readAppIcons(targets)
  const next = { ...state.icons }
  for (const payload of payloads) {
    if (payload.dataUri) next[payload.target] = payload.dataUri
  }
  state.icons = next
}

/** 目标对应的图标；没有就返回空字符串，由界面决定回退样式。 */
export function iconFor(target: string): string {
  return state.icons[target] ?? ''
}

/** 清掉某个目标的图标缓存，下次重新读取（目标被编辑后调用）。 */
function dropIcon(target: string): void {
  if (!(target in state.icons)) return
  const next = { ...state.icons }
  delete next[target]
  state.icons = next
}

/**
 * 从宿主加载真实数据。失败时保留 loadError，界面必须显示失败原因，
 * 不能悄悄退回合成数据让用户以为一切正常。
 */
const mockPasswordVault = new Map<string, string>()
let bootstrapPromise: Promise<void> | null = null
let bootstrapForceQueued = false

/** 幂等且防重入的窗口冷启动初始化：已就绪时直接返回，在途期间复用同一 Promise；失败后可重试，force=true 在途时也会在当前轮结束后补拉一轮。 */
export async function bootstrap(force = false): Promise<void> {
  if (!force && state.ready) return
  if (bootstrapPromise) {
    if (!force) return bootstrapPromise
    bootstrapForceQueued = true
    await bootstrapPromise
    if (bootstrapPromise) return bootstrapPromise
    if (!bootstrapForceQueued) return
    bootstrapForceQueued = false
  }

  bootstrapPromise = (async () => {
    if (!isDesktop) {
      // 浏览器样稿：按需加载合成数据，避免正式桌面包携带演示代码；并明确标注不是真实数据。
      const mock = await import('@/mock/data')
      state.apps = mock.apps.map((app) => ({ ...app }))
      state.categories = mock.categories.map((cat) => ({ ...cat }))
      state.clips = mock.clips.map((clip) => ({ ...clip }))
      state.groups = [...mock.groups]
      state.snippets = mock.snippets.map((item) => ({ ...item }))
      state.snippetGroups = [...mock.snippetGroups]
      mockPasswordVault.clear()
      for (const [id, pw] of Object.entries(mock.mockSnippetPasswords)) {
        mockPasswordVault.set(id, pw)
      }
      state.translationSettings = { ...mock.defaultTranslationSettings }
      state.ready = true
      return
    }

    try {
      const config = await invokeHost<AppConfig>('list_applications')
      applyApps(config)
      await refreshClipboard()
      await refreshSnippets()
      const loadedShortcuts = await invokeHost<typeof state.shortcuts>('get_shortcuts')
      state.shortcuts = { ...loadedShortcuts, ocr: loadedShortcuts.ocr ?? 'Ctrl+Shift+O' }
      state.settings.clipboardHotkey = state.shortcuts.clipboard
      await refreshTranslationSettings()
      const ui = await invokeHost<UiState>('ui_state')
      applyUiState(ui)
      state.ready = true
      state.loadError = ''
    } catch (error) {
      state.loadError = error instanceof Error ? error.message : String(error)
    }
  })().finally(() => {
    bootstrapPromise = null
  })

  return bootstrapPromise
}

let lastSettingsRequest = 0
let pendingPageOps = 0
let latestPageReq = 0
let pendingThemeOps = 0
let latestThemeReq = 0
let pendingMotionOps = 0
let latestMotionReq = 0
let pendingShapeOps = 0
let latestShapeReq = 0
let pendingCategoryOps = 0
let latestCategoryReq = 0

function applyUiState(ui: UiState): void {
  const pageBusy = typeof pendingPageOps !== 'undefined' && pendingPageOps > 0
  const motionBusy = typeof pendingMotionOps !== 'undefined' && pendingMotionOps > 0
  const shapeBusy = typeof pendingShapeOps !== 'undefined' && pendingShapeOps > 0
  const categoryBusy = typeof pendingCategoryOps !== 'undefined' && pendingCategoryOps > 0
  const themeBusy = typeof pendingThemeOps !== 'undefined' && pendingThemeOps > 0

  state.ui.islandExpanded = ui.islandExpanded
  state.ui.islandAnchor = ui.islandAnchor
  if (!shapeBusy) {
    const rawShape = ui.islandShape
    state.ui.islandShape = (
      rawShape === 'notch' || rawShape === 'split' || rawShape === 'floating'
        ? rawShape
        : 'floating'
    ) as IslandShape
  }
  if (!pageBusy) {
    state.ui.islandPage = ['applications', 'clipboard', 'translation', 'files', 'audio', 'pack'].includes(ui.islandPage) ? ui.islandPage : 'applications'
  }
  state.ui.islandHoverMs = ui.islandHoverMs
  state.ui.islandLeaveMs = ui.islandLeaveMs
  state.ui.islandReducedMotion = ui.islandReducedMotion
  const keepMotion = state.ui.islandAppearance?.motion
  state.ui.islandAppearance = motionBusy && ui.islandAppearance && keepMotion
    ? { ...ui.islandAppearance, motion: keepMotion }
    : ui.islandAppearance
  if (state.ui.islandAppearance && !isMotionMode(state.ui.islandAppearance.motion)) {
    state.ui.islandAppearance.motion = 'glide'
  }
  state.ui.mixerVisibility = ui.mixerVisibility
  state.ui.islandCaptureError = ui.islandCaptureError ?? null
  state.ui.translationPending = ui.translationPending ?? 0
  state.ui.fullscreenSuppressed = ui.fullscreenSuppressed
  state.ui.wechatEnabled = ui.wechatEnabled
  state.ui.wechatStatus = ui.wechatStatus
  state.ui.wechatAttention = ui.wechatAttention
  state.ui.wechatEvent = ui.wechatEvent
  if (!pageBusy) {
    state.ui.panel = ['clipboard', 'translation', 'files', 'audio', 'pack', 'category', 'overflow'].includes(ui.panel ?? '') ? ui.panel : null
  }
  state.ui.settingsOpen = ui.settingsOpen
  if (ui.settingsOpen && ui.settingsRequest && ui.settingsRequest[0] > lastSettingsRequest) {
    lastSettingsRequest = ui.settingsRequest[0]
    state.ui.settingsSection = ui.settingsRequest[1]
  }
  state.ui.paused = ui.paused
  if (!categoryBusy) {
    state.ui.activeCategoryId = ui.activeCategoryId
  }
  if (!themeBusy) {
    const rawTheme = ui.theme
    state.settings.theme = (
      rawTheme === 'scroll' || rawTheme === 'clay'
        ? 'light'
        : ['system', 'light', 'dark', 'midnight', 'mecha', 'pixel', 'blueprint', 'neon'].includes(rawTheme)
          ? rawTheme
          : 'dark'
    ) as HubSettings['theme']
  }
}

export async function selectIslandPage(page: UiState['islandPage'], focus: boolean): Promise<void> {
  const previousPage = state.ui.islandPage
  const previousPanel = state.ui.panel
  state.ui.islandPage = page
  state.ui.panel = page === 'applications' ? null : page
  if (!isDesktop) return
  const reqId = ++latestPageReq
  pendingPageOps++
  try {
    await invokeHost('select_island_page', { page, focus })
  } catch (error) {
    if (reqId === latestPageReq) {
      state.ui.islandPage = previousPage
      state.ui.panel = previousPanel
    }
    pushToast(String(error), 'error')
  } finally {
    pendingPageOps--
  }
}

/** 订阅 Rust 广播的跨窗口状态；返回取消订阅函数。 */
export async function subscribeUiState(): Promise<() => void> {
  return onUiState(applyUiState)
}

/**
 * 订阅应用配置变化。任意窗口改了应用列表（添加/编辑/删除/排序/启动时发现失效），
 * Rust 都会广播，其它窗口据此重新拉取，避免各窗口显示不一致。
 */
export async function subscribeApplications(): Promise<() => void> {
  return onApplicationsChanged(applyApps)
}

/**
 * 一次性订阅所有跨窗口事件。
 *
 * 四个窗口各自有一份 store，任何一处漏订阅就会出现「设置里改了、图标条没变」，
 * 所以统一从这里订阅，窗口只负责在卸载时调用返回的取消函数。
 */
export async function subscribeHostEvents(): Promise<() => void> {
  // 后台全局错误/剪贴板告警仅由 bar 主窗响应，避免设置窗或独立翻译窗同时打开时重复弹出多条提示。
  const handleGlobalNotice = !isDesktop || windowLabel === 'bar'
  const offs = await Promise.all([
    subscribeUiState(), subscribeApplications(),
    listenHost('clipboard-changed', () => {
      void refreshClipboard().catch(() => {
        if (handleGlobalNotice) pushToast('读取剪贴板历史失败', 'error')
      })
    }),
    listenHost<SnippetPayload>('snippets-changed', (payload) => applySnippets(payload)),
    listenHost<DiscoverySnapshot>('app-discovery-changed', (payload) => applyDiscovery(payload)),
    listenHost<string>('clipboard-notice', (message) => {
      if (handleGlobalNotice) pushToast(message, 'error')
    }),
    listenHost('translation-settings-changed', () => {
      void refreshTranslationSettings().catch(() => {
        if (handleGlobalNotice) pushToast('读取模型设置失败', 'error')
      })
    })
  ])
  return () => offs.forEach((off) => off())
}

/* ---------- 窗内与岛内轻提示 ---------- */

let toastSeq = 0
const toastTimers = new Map<number, number>()

export function pushToast(text: string, kind: ToastKind = 'info'): void {
  const clean = text.trim()
  if (!clean) return
  // 同文案防抖去重，避免短时间连点把提示条堆叠得过高。
  const dupIdx = state.toasts.findIndex((item) => item.text === clean && item.kind === kind)
  if (dupIdx >= 0) {
    const existing = state.toasts[dupIdx]!
    const prevTimer = toastTimers.get(existing.id)
    if (prevTimer !== undefined) {
      window.clearTimeout(prevTimer)
      toastTimers.delete(existing.id)
    }
    state.toasts.splice(dupIdx, 1)
  }
  const toast: Toast = { id: ++toastSeq, text: clean, kind }
  state.toasts.push(toast)
  while (state.toasts.length > 2) {
    const dropped = state.toasts.shift()
    if (dropped) {
      const oldTimer = toastTimers.get(dropped.id)
      if (oldTimer !== undefined) {
        window.clearTimeout(oldTimer)
        toastTimers.delete(dropped.id)
      }
    }
  }
  const duration = kind === 'error' ? 4000 : 2600
  const timer = window.setTimeout(() => {
    toastTimers.delete(toast.id)
    const index = state.toasts.findIndex((item) => item.id === toast.id)
    if (index >= 0) state.toasts.splice(index, 1)
  }, duration)
  toastTimers.set(toast.id, timer)
}

/* ---------- 计算属性 ---------- */

export const visibleApps = computed(() =>
  state.apps.filter(
    (app) =>
      !app.hidden &&
      (state.ui.activeCategoryId === 'all' || app.categoryId === state.ui.activeCategoryId)
  )
)

export const categoryName = computed(
  () => state.categories.find((cat) => cat.id === state.ui.activeCategoryId)?.name ?? '全部'
)

export const groupFilters = computed(() => ['全部', ...state.groups])

export const filteredClips = computed(() => {
  const query = state.ui.query.trim().toLowerCase()
  return state.clips.filter((clip) => {
    if (state.ui.group !== '全部' && clip.group !== state.ui.group) return false
    if (state.ui.favoritesOnly && !clip.favorite) return false
    if (!query || (clip.note ?? '').toLowerCase().includes(query)) return true
    if (clip.kind === 'image') return '图片'.includes(query) || query.includes('图片')
    return (clip.text ?? '').toLowerCase().includes(query)
  })
})

export const activeClip = computed(
  () => state.clips.find((clip) => clip.id === state.ui.detailId) ?? null
)

export const snippetGroupFilters = computed(() => ['全部', ...state.snippetGroups])

export const filteredSnippets = computed(() => {
  const query = state.ui.snippetQuery.trim().toLowerCase()
  return state.snippets.filter((item) => {
    if (state.ui.snippetGroup !== '全部' && item.group !== state.ui.snippetGroup) return false
    if (state.ui.snippetPinnedOnly && !item.pinned) return false
    if (!query) return true
    return (
      item.title.toLowerCase().includes(query) ||
      item.content.toLowerCase().includes(query) ||
      (item.account ?? '').toLowerCase().includes(query) ||
      item.group.toLowerCase().includes(query)
    )
  })
})

/* ---------- 弹层与导航 ---------- */

export function togglePanel(panel: Exclude<PanelName, null>): void {
  state.ui.detailId = null
  state.ui.query = ''
  state.ui.snippetQuery = ''
  state.ui.activeIndex = 0
  state.ui.snippetActiveIndex = 0
  if (isDesktop) {
    void requestPanel(panel)
    return
  }
  state.ui.panel = state.ui.panel === panel ? null : panel
}

export function closePanel(): void {
  if (state.ui.detailId) {
    state.ui.detailId = null
    return
  }
  state.ui.query = ''
  state.ui.snippetQuery = ''
  if (isDesktop) {
    void dismissPanel()
    return
  }
  state.ui.panel = null
}

export function openSettings(section: SettingsSection = 'general'): void {
  state.ui.settingsSection = section
  if (isDesktop) {
    void requestSettings(section)
    return
  }
  state.ui.settingsOpen = true
  state.ui.panel = null
}

export function closeSettings(): void {
  if (isDesktop) {
    void dismissSettings()
    return
  }
  state.ui.settingsOpen = false
}

export function openDetail(id: string): void {
  state.ui.detailId = id
  const clip = state.clips.find((item) => item.id === id)
  if (isDesktop && clip?.kind === 'image') void invokeHost<string>('read_clipboard_thumbnail', { id, full: true }).then((url) => { clip.imageUrl = url }).catch(() => { clip.imageMissing = true })
}

export function closeDetail(): void {
  state.ui.detailId = null
}

export function setQuery(value: string): void {
  state.ui.query = value
  state.ui.activeIndex = 0
}

export function setGroup(group: string): void {
  state.ui.group = group
  state.ui.activeIndex = 0
}

export function setCategory(id: string): void {
  const previous = state.ui.activeCategoryId
  state.ui.activeCategoryId = id
  if (!isDesktop) return
  const reqId = ++latestCategoryReq
  pendingCategoryOps++
  void requestCategory(id)
    .catch(error => {
      if (reqId === latestCategoryReq) state.ui.activeCategoryId = previous
      pushToast(`切换分类失败：${String(error)}`, 'error')
    })
    .finally(() => {
      pendingCategoryOps--
    })
}

export function moveActive(delta: number): void {
  const total = filteredClips.value.length
  if (!total) return
  state.ui.activeIndex = (state.ui.activeIndex + delta + total) % total
}

/* ---------- 应用 ---------- */

const launchingApps = new Set<string>()
export async function launchApp(app: HubApp): Promise<boolean> {
  if (!isDesktop) {
    return true
  }
  if (launchingApps.has(app.id)) return false
  launchingApps.add(app.id)
  try {
    const result = await invokeHost<{ launched: boolean; reason: string }>('open_application', {
      id: app.id
    })
    // 桌面端由宿主直接通过 island-feedback 在灵动岛胶囊内展示打开结果，不再另弹独立 toast 窗口。
    return result.launched
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '启动失败', 'error')
  } finally {
    launchingApps.delete(app.id)
  }
  return false
}

export function reorderApps(fromId: string, toId: string): void {
  void reorderVisibleApps(state.apps.map(app => app.id), fromId, toId)
}

/** 上移 / 下移一位，并立刻落盘。 */
export function moveApp(index: number, delta: number): void {
  const target = index + delta
  if (index < 0 || index >= state.apps.length) return
  if (target < 0 || target >= state.apps.length) return
  reorderApps(state.apps[index].id, state.apps[target].id)
}

export async function toggleAppHidden(id: string): Promise<boolean> {
  const app = state.apps.find((item) => item.id === id)
  if (!app) return false
  return saveApp({ ...app, hidden: !app.hidden })
}

export async function setAppCategory(id: string, categoryId: string): Promise<boolean> {
  const app = state.apps.find((item) => item.id === id)
  if (!app) return false
  return saveApp({ ...app, categoryId })
}

async function saveApp(app: HubApp): Promise<boolean> {
  if (!isDesktop) {
    const current = state.apps.find(item => item.id === app.id)
    if (current) Object.assign(current, app)
    return !!current
  }
  try {
    const config = await invokeHost<AppConfig>('save_application', {
      application: { ...app }
    })
    applyApps(config)
    return true
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '保存应用设置失败', 'error')
    return false
  }
}

/** 仅重排指定可见项占据的位置；搜索外、其他分类的条目保持原位。 */
export async function reorderVisibleApps(visibleIds: string[], fromId: string, toId: string): Promise<boolean> {
  const visible = new Set(visibleIds)
  const ordered = state.apps.filter(app => visible.has(app.id))
  const from = ordered.findIndex(app => app.id === fromId)
  const to = ordered.findIndex(app => app.id === toId)
  if (from < 0 || to < 0 || from === to) return false
  const [moved] = ordered.splice(from, 1)
  ordered.splice(to, 0, moved)
  let cursor = 0
  const next = state.apps.map(app => visible.has(app.id) ? ordered[cursor++] : app)
  if (!isDesktop) { state.apps = next; return true }
  try {
    applyApps(await invokeHost<AppConfig>('reorder_applications', { ids: next.map(app => app.id) }))
    return true
  } catch (error) { pushToast(`保存应用顺序失败：${String(error)}`, 'error'); return false }
}

/** 添加结果，供界面给出明确反馈（而不是笼统的「成功」）。 */
export interface AddAppOutcome {
  ok: boolean
  /** 取消选择时为 true，此时不提示错误。 */
  cancelled?: boolean
  name?: string
  reason?: string
}

/**
 * 让用户选一个 EXE / LNK / 文件，再添加成应用。
 *
 * 不做全盘扫描：目标来自用户在系统选择框里的明确选择。
 */
export async function addAppFromPicker(
  categoryId?: string,
  kind: 'file' | 'folder' = 'file'
): Promise<AddAppOutcome> {
  if (!isDesktop) {
    pushToast('添加应用只在桌面版可用', 'info')
    return { ok: false, reason: 'browser' }
  }
  let target: string | null = null
  try {
    target = await pickApplicationTarget(kind)
  } catch (error) {
    const reason = error instanceof Error ? error.message : '打开选择框失败'
    pushToast(reason, 'error')
    return { ok: false, reason }
  }
  if (!target) return { ok: false, cancelled: true }
  return addAppByTarget(target, categoryId)
}

/** 用已知目标添加应用（选择框与拖入文件都走这里）。 */
export async function addAppByTarget(
  target: string,
  categoryId?: string,
  name?: string
): Promise<AddAppOutcome> {
  const trimmed = target.trim()
  if (!trimmed) return { ok: false, reason: '目标为空' }

  const duplicated = state.apps.find(
    (app) => app.target.trim().toLowerCase() === trimmed.toLowerCase()
  )
  if (duplicated) {
    pushToast(`已经添加过：${duplicated.name}`, 'info')
    return { ok: false, reason: 'duplicate', name: duplicated.name }
  }

  const description = await describeTarget(trimmed)
  const finalName = (name ?? description?.suggestedName ?? '').trim() || trimmed
  const category = categoryId ?? state.categories.find((cat) => cat.id !== 'all')?.id ?? 'all'
  const app: HubApp = {
    id: `a-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`,
    name: finalName,
    target: trimmed,
    categoryId: category,
    color: '#57606A',
    initials: finalName.slice(0, 1).toUpperCase(),
    hidden: false,
    broken: description ? !description.exists : undefined
  }

  if (!isDesktop) {
    state.apps.push(app)
    pushToast(`已添加 ${app.name}（浏览器样稿，未落盘）`, 'success')
    return { ok: true, name: app.name }
  }

  try {
    const config = await invokeHost<AppConfig>('save_application', { application: app })
    applyApps(config)
    if (description && !description.exists) {
      pushToast(`已添加 ${app.name}，但目标不存在`, 'error')
    } else {
      pushToast(`已添加 ${app.name}`, 'success')
    }
    return { ok: true, name: app.name }
  } catch (error) {
    const reason = error instanceof Error ? error.message : '保存失败'
    pushToast(reason, 'error')
    return { ok: false, reason }
  }
}

/** 编辑名称 / 目标 / 分类；目标变了要重新读图标。 */
export async function updateApp(
  id: string,
  patch: { name?: string; target?: string; categoryId?: string }
): Promise<AddAppOutcome> {
  const app = state.apps.find((item) => item.id === id)
  if (!app) return { ok: false, reason: '找不到这个应用' }

  const nextTarget = (patch.target ?? app.target).trim()
  if (!nextTarget) return { ok: false, reason: '目标不能为空' }
  const nextName = (patch.name ?? app.name).trim() || app.name

  if (nextTarget.toLowerCase() !== app.target.trim().toLowerCase()) {
    const duplicated = state.apps.find(
      (item) => item.id !== id && item.target.trim().toLowerCase() === nextTarget.toLowerCase()
    )
    if (duplicated) {
      pushToast(`这个目标已被 ${duplicated.name} 使用`, 'error')
      return { ok: false, reason: 'duplicate' }
    }
  }

  const targetChanged = nextTarget !== app.target
  const updated: HubApp = {
    ...app,
    name: nextName,
    target: nextTarget,
    categoryId: patch.categoryId ?? app.categoryId,
    initials: nextName.slice(0, 1).toUpperCase()
  }

  if (targetChanged) {
    const description = await describeTarget(nextTarget)
    updated.broken = description ? !description.exists : undefined
  }

  if (!isDesktop) {
    Object.assign(app, updated)
    return { ok: true, name: updated.name }
  }

  try {
    const config = await invokeHost<AppConfig>('save_application', { application: updated })
    applyApps(config)
    if (targetChanged) dropIcon(app.target)
    pushToast(`已保存 ${updated.name}`, 'success')
    return { ok: true, name: updated.name }
  } catch (error) {
    const reason = error instanceof Error ? error.message : '保存失败'
    pushToast(reason, 'error')
    return { ok: false, reason }
  }
}

/** 从图标条与设置里移除；只删配置，不动目标文件。 */
export async function deleteApp(id: string): Promise<boolean> {
  const app = state.apps.find((item) => item.id === id)
  if (!app) return false
  if (!isDesktop) {
    const index = state.apps.findIndex((item) => item.id === id)
    if (index >= 0) state.apps.splice(index, 1)
    return true
  }
  try {
    const config = await invokeHost<AppConfig>('remove_application', { id })
    applyApps(config)
    pushToast(`已移除 ${app.name}`, 'success')
    return true
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '移除失败', 'error')
    return false
  }
}

export async function importSelectedSystemApps(categoryId: string, targets: string[]) {
  const result = await invokeHost<ImportShortcutsResult>('import_system_apps', { categoryId, targets })
  applyApps(result.config)
  return result
}

export async function importShortcuts(categoryId?: string): Promise<void> {
  if (!isDesktop) {
    pushToast('导入只在桌面版可用', 'info')
    return
  }
  try {
    const result = await importDesktopShortcuts(categoryId)
    applyApps(result.config)
    if (result.added > 0) {
      pushToast(`导入 ${result.added} 个快捷方式（跳过 ${result.skipped} 个已存在）`, 'success')
    } else if (result.total === 0) {
      pushToast('桌面上没有找到快捷方式', 'info')
    } else {
      pushToast(`桌面上的 ${result.total} 个快捷方式都已经在列表里`, 'info')
    }
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '导入失败', 'error')
  }
}

/* ---------- 剪贴板 ---------- */

export async function copyClip(clip: ClipItem, formattedText?: string): Promise<void> {
  if (clip.imageMissing) {
    pushToast('图片文件缺失，无法复制', 'error')
    return
  }
  if (!isDesktop) {
    pushToast(`样稿：已模拟复制${formattedText !== undefined ? '格式化结果' : clip.kind === 'image' ? '图片' : '原文'}，未写入系统剪贴板`, 'info')
    return
  }
  try {
    await invokeHost('copy_clipboard_entry', { id: clip.id, formattedText })
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '复制失败，可重试', 'error')
  }
}

async function updateClipboard(command: string, args: Record<string, unknown>): Promise<boolean> {
  try {
    await invokeHost(command, args)
  } catch (error) {
    pushToast(error instanceof Error ? error.message : String(error), 'error')
    return false
  }
  try { await refreshClipboard() }
  catch { pushToast('已保存，但读取最新剪贴板状态失败', 'error') }
  return true
}

export async function editClip(clip: ClipItem, favorite: boolean, note: string): Promise<boolean> {
  if (!isDesktop) { Object.assign(clip, { favorite, note: note.trim() }); return true }
  return updateClipboard('edit_clipboard_entry', { id: clip.id, favorite, note })
}

export async function deleteSelectedClips(ids: string[], expectedFavorites: number): Promise<boolean> {
  if (!isDesktop) {
    state.clips = state.clips.filter((clip) => !ids.includes(clip.id))
    return true
  }
  return updateClipboard('delete_clipboard_entries', { ids, expectedFavorites })
}

export async function setCaptureImages(enabled: boolean): Promise<boolean> {
  if (!isDesktop) { state.clipboardPreferences.captureImages = enabled; return true }
  return updateClipboard('set_clipboard_capture_images', { enabled })
}

export async function saveClipHistoryLimit(limit: number, expectedRemoved: number): Promise<boolean> {
  if (!isDesktop) {
    let ordinary = 0
    state.clips = state.clips.filter((clip) => clip.favorite || ++ordinary <= limit)
    state.clipboardPreferences.historyLimit = limit
    return true
  }
  return updateClipboard('set_clipboard_history_limit', { limit, expectedRemoved })
}

export async function deleteClip(clip: ClipItem): Promise<void> {
  if (!isDesktop) {
    const index = state.clips.findIndex((item) => item.id === clip.id)
    if (index >= 0) state.clips.splice(index, 1)
    if (state.ui.detailId === clip.id) state.ui.detailId = null
    return
  }
  try {
    await invokeHost('delete_clipboard_entry', { id: clip.id })
    const index = state.clips.findIndex((item) => item.id === clip.id)
    if (index >= 0) state.clips.splice(index, 1)
    if (state.ui.detailId === clip.id) state.ui.detailId = null
    pushToast('已删除这条记录', 'success')
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '删除失败', 'error')
  }
}

export async function clearClips(): Promise<void> {
  if (!isDesktop) {
    state.clips.splice(0, state.clips.length)
    state.ui.detailId = null
    return
  }
  try {
    await invokeHost('clear_clipboard_entries')
    await refreshClipboard()
    state.ui.detailId = null
    pushToast('已清空全部记录', 'success')
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '清空失败', 'error')
  }
}

export async function setPaused(paused: boolean): Promise<void> {
  if (!isDesktop) {
    state.ui.paused = paused
    state.settings.recordClipboard = !paused
    return
  }
  try {
    const next = await invokeHost<UiState>('set_clipboard_paused', { paused })
    applyUiState(next)
    pushToast(paused ? '已暂停记录剪贴板' : '已恢复记录剪贴板', 'info')
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '切换失败', 'error')
  }
}

/* ---------- 翻译 ---------- */

export function openTranslation(source?: string, fromClipboard = false): void {
  if (typeof source === 'string') {
    state.translation.source = source
    state.translation.filledFromClip = fromClipboard
  }
  if (state.ui.panel !== 'translation') {
    if (isDesktop) {
      void requestPanel('translation')
    } else {
      state.ui.panel = 'translation'
      state.ui.settingsOpen = false
    }
  }
}

export function setTranslationSource(value: string): void {
  cancelTranslation()
  state.translation.source = value.slice(0, 5000)
  state.translation.filledFromClip = false
}

export function setTranslationDirection(direction: TranslateDirection): void {
  if (state.translation.direction === direction) return
  if (state.translation.status === 'loading') {
    cancelTranslation()
  }
  state.translation.direction = direction
  // 切换翻译方向时保留已有译文上下文，仅清理旧错误提示。
  if (state.translation.status === 'error') {
    state.translation.status = state.translation.result ? 'done' : 'idle'
    state.translation.error = ''
  }
}

export async function submitTranslation(): Promise<void> {
  const text = state.translation.source.trim()
  if (!text) {
    pushToast('请先输入要翻译的原文', 'error')
    return
  }
  if (!isDesktop) {
    pushToast('翻译服务在桌面版接通（浏览器样稿不发送请求）', 'info')
    return
  }
  const seq = ++state.translation.requestSeq
  state.translation.status = 'loading'
  state.translation.error = ''

  try {
    const result = await invokeHost<TranslationResult>('translate_text', {
      text,
      direction: state.translation.direction
    })
    // 旧请求不能覆盖新输入的结果
    if (seq !== state.translation.requestSeq) return
    state.translation.result = result
    state.translation.status = 'done'
  } catch (error) {
    if (seq !== state.translation.requestSeq) return
    state.translation.result = null
    state.translation.status = 'error'
    state.translation.error = error instanceof Error ? error.message : String(error)
  }
}

export async function copyTranslation(): Promise<void> {
  const result = state.translation.result
  if (!result) return
  if (!isDesktop) {
    pushToast('译文已复制（浏览器样稿）', 'success')
    return
  }
  try {
    await invokeHost('copy_translation_result', { text: result.text })
    // 独立翻译窗由自身底部轻提示条即时反馈，不依赖灵动岛跨窗广播。
    if (windowLabel === 'translation') {
      pushToast('译文已复制', 'success')
    }
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '复制失败', 'error')
  }
}

/* ---------- 设置 ---------- */

export function setTheme(theme: HubSettings['theme']): void {
  const previous = state.settings.theme
  state.settings.theme = theme
  if (!isDesktop) return
  const reqId = ++latestThemeReq
  pendingThemeOps++
  void requestTheme(theme)
    .catch(error => {
      if (reqId === latestThemeReq) state.settings.theme = previous
      pushToast(`保存主题失败：${String(error)}`, 'error')
    })
    .finally(() => {
      pendingThemeOps--
    })
}

export function setMotion(motion: MotionMode): void {
  const previous = state.ui.islandAppearance.motion
  state.ui.islandAppearance.motion = motion
  if (!isDesktop) return
  const reqId = ++latestMotionReq
  pendingMotionOps++
  void requestMotion(motion)
    .catch(error => {
      if (reqId === latestMotionReq) state.ui.islandAppearance.motion = previous
      pushToast(`保存动态效果失败：${String(error)}`, 'error')
    })
    .finally(() => {
      pendingMotionOps--
    })
}

/** 切换灵动岛全局形态（悬浮胶囊 / 贴顶刘海 / 贴顶分层），乐观更新并在失败时回滚。 */
export function setIslandShape(shape: IslandShape): void {
  const normalized = normalizeIslandShape(shape)
  const previous = state.ui.islandShape
  state.ui.islandShape = normalized
  if (!isDesktop) return
  const reqId = ++latestShapeReq
  pendingShapeOps++
  void requestIslandShape(normalized)
    .catch(error => {
      if (reqId === latestShapeReq) state.ui.islandShape = previous
      pushToast(`保存灵动岛形态失败：${String(error)}`, 'error')
    })
    .finally(() => {
      pendingShapeOps--
    })
}

/* ---------- 主题落地 ---------- */

const prefersDark = window.matchMedia('(prefers-color-scheme: dark)')

export function isDarkTheme(theme: HubSettings['theme'] = state.settings.theme): boolean {
  if (theme === 'system') return prefersDark.matches
  return isDarkThemeMode(theme)
}

function applyTheme(): void {
  const mode = normalizeThemeMode(state.settings.theme)
  const resolved = mode === 'system' ? (prefersDark.matches ? 'dark' : 'light') : mode
  document.documentElement.dataset.theme = resolved
}

function applyMotion(): void {
  document.documentElement.dataset.motion = normalizeMotionMode(state.ui.islandAppearance.motion)
}

function applyIslandShape(): void {
  document.documentElement.dataset.islandShape = normalizeIslandShape(state.ui.islandShape)
}

function applyReducedMotion(): void {
  document.documentElement.dataset.reducedMotion = state.ui.islandReducedMotion ? '1' : '0'
}

watch(() => state.settings.theme, applyTheme, { immediate: true })
watch(() => state.ui.islandAppearance.motion, applyMotion, { immediate: true })
watch(() => state.ui.islandShape, applyIslandShape, { immediate: true })
watch(() => state.ui.islandReducedMotion, applyReducedMotion, { immediate: true })
prefersDark.addEventListener('change', () => {
  if (state.settings.theme === 'system') applyTheme()
})

/** 供调试与未使用的宿主接口占位，避免打包器警告。 */
export const hostBridge = { host, newRequestId }

export function cancelTranslation(): void {
  const wasLoading = state.translation.status === 'loading'
  state.translation.requestSeq++
  if (state.translation.status === 'loading') state.translation.status = 'idle'
  if (isDesktop && wasLoading) void invokeHost('cancel_translation').catch(() => {})
}

export async function saveTranslationSettings(settings: TranslationSettings = state.translationSettings): Promise<boolean> {
  if (!isDesktop) return false
  try {
    state.translationSettings = await invokeHost<TranslationSettings>('save_translation_settings', { settings: { ...settings } })
    setTranslationDirection(state.translationSettings.direction)
    pushToast('模型设置已保存', 'success')
    return true
  } catch (error) { pushToast(String(error), 'error'); return false }
}

export async function saveTranslationSecret(secret: string, settings: TranslationSettings = state.translationSettings): Promise<boolean> {
  if (!await saveTranslationSettings(settings)) return false
  try { await invokeHost('save_translation_secret', { secret }); await refreshTranslationSettings(); pushToast('密钥已保存', 'success'); return true }
  catch (error) { pushToast(String(error), 'error'); return false }
}

export async function deleteTranslationSecret(): Promise<void> {
  try { await invokeHost('delete_translation_secret'); await refreshTranslationSettings(); pushToast('密钥已删除', 'success') }
  catch (error) { pushToast(String(error), 'error') }
}

export async function createClipGroup(group: string): Promise<void> {
  try { await invokeHost('create_clipboard_group', { group }); await refreshClipboard() }
  catch (error) { pushToast(String(error), 'error') }
}

export async function removeClipGroup(group: string): Promise<void> {
  try { await invokeHost('remove_clipboard_group', { group }); await refreshClipboard() }
  catch (error) { pushToast(String(error), 'error') }
}

export async function assignClipGroup(id: string, group: string): Promise<void> {
  try { await invokeHost('assign_clipboard_group', { id, group }); await refreshClipboard() }
  catch (error) { pushToast(String(error), 'error') }
}

/* ---------- 常用短语 ---------- */

const KNOWN_SNIPPET_VARS = new Set(['date', 'time', 'datetime', 'weekday', 'clipboard'])

export function hasSnippetVariables(template: string): boolean {
  let rest = template
  while (true) {
    const start = rest.indexOf('{{')
    if (start < 0) return false
    let afterOpen = rest.slice(start + 2)
    while (afterOpen.startsWith('{')) afterOpen = afterOpen.slice(1)
    const end = afterOpen.indexOf('}}')
    if (end < 0) return false
    if (afterOpen.slice(0, end).includes('{{')) {
      rest = afterOpen
      continue
    }
    if (KNOWN_SNIPPET_VARS.has(afterOpen.slice(0, end).trim().toLowerCase())) return true
    rest = afterOpen.slice(end + 2)
  }
}

export function renderSnippetPreview(
  template: string,
  clipboardSample = '（当前剪贴板文本）',
  now = new Date()
): string {
  if (!template.includes('{{')) return template
  const pad = (n: number) => String(n).padStart(2, '0')
  const dateStr = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
  const timeStr = `${pad(now.getHours())}:${pad(now.getMinutes())}:${pad(now.getSeconds())}`
  const datetimeStr = `${dateStr} ${timeStr}`
  const weekdays = ['周日', '周一', '周二', '周三', '周四', '周五', '周六']
  const weekdayStr = weekdays[now.getDay()] ?? '周一'

  let out = ''
  let rest = template
  while (true) {
    const start = rest.indexOf('{{')
    if (start < 0) break
    out += rest.slice(0, start)
    let openOffset = start
    let afterOpen = rest.slice(start + 2)
    while (afterOpen.startsWith('{')) {
      out += '{'
      openOffset += 1
      afterOpen = afterOpen.slice(1)
    }
    const end = afterOpen.indexOf('}}')
    if (end < 0) {
      out += rest.slice(openOffset)
      return out
    }
    if (afterOpen.slice(0, end).includes('{{')) {
      out += '{{'
      rest = afterOpen
      continue
    }
    const token = afterOpen.slice(0, end).trim().toLowerCase()
    if (token === 'date') out += dateStr
    else if (token === 'time') out += timeStr
    else if (token === 'datetime') out += datetimeStr
    else if (token === 'weekday') out += weekdayStr
    else if (token === 'clipboard') out += clipboardSample
    else out += rest.slice(openOffset, openOffset + 2 + end + 2)
    rest = afterOpen.slice(end + 2)
  }
  return out + rest
}

function normalizeMockSnippetOrder(entries: SnippetItem[]): SnippetItem[] {
  return [...entries.filter((item) => item.pinned), ...entries.filter((item) => !item.pinned)]
}

export function setClipboardView(view: 'history' | 'snippets'): void {
  state.ui.clipboardView = view
  if (view === 'snippets') {
    state.ui.detailId = null
    state.ui.snippetActiveIndex = 0
  } else {
    state.ui.activeIndex = 0
  }
}

export function setSnippetQuery(value: string): void {
  state.ui.snippetQuery = value
  state.ui.snippetActiveIndex = 0
}

export function setSnippetGroup(group: string): void {
  state.ui.snippetGroup = group
  state.ui.snippetActiveIndex = 0
}

export function moveSnippetActive(delta: number): void {
  const total = filteredSnippets.value.length
  if (!total) return
  state.ui.snippetActiveIndex = (state.ui.snippetActiveIndex + delta + total) % total
}

export function isCredentialSnippet(item: Pick<SnippetItem, 'kind' | 'group'> | null | undefined): boolean {
  if (!item) return false
  return item.kind === 'credential'
}

export async function revealSnippetPassword(id: string): Promise<string> {
  if (!isDesktop) {
    return mockPasswordVault.get(id) ?? ''
  }
  return invokeHost<string>('reveal_snippet_password', { id })
}

export async function copySnippet(
  snippet: SnippetItem,
  raw = false,
  field?: SnippetCopyField
): Promise<boolean> {
  const credential = snippet.kind === 'credential'
  const targetField: SnippetCopyField | undefined = credential ? (field ?? 'account') : undefined
  if (!isDesktop) {
    if (credential) {
      const value =
        targetField === 'password'
          ? (mockPasswordVault.get(snippet.id) ?? '')
          : (snippet.account ?? '')
      if (!value) {
        pushToast(targetField === 'password' ? '这条账密尚未设置密码' : '这条账密尚未填写账号', 'error')
        return false
      }
      pushToast(
        `样稿：已模拟复制${targetField === 'password' ? '密码（不入历史）' : '账号'}「${snippet.title}」，未写入系统剪贴板`,
        'success'
      )
      return true
    }
    const dynamic = !raw && hasSnippetVariables(snippet.content)
    pushToast(
      `样稿：已模拟复制短语「${snippet.title}」${dynamic ? '（已展开动态变量）' : ''}，未写入系统剪贴板`,
      'info'
    )
    return true
  }
  try {
    await invokeHost('copy_snippet', { id: snippet.id, raw, field: targetField })
    if (credential) {
      pushToast(targetField === 'password' ? '已复制密码（不记入剪贴板历史）' : '已复制账号', 'success')
    }
    return true
  } catch (error) {
    pushToast(error instanceof Error ? error.message : String(error), 'error')
    return false
  }
}

export async function saveSnippet(draft: SnippetDraft): Promise<boolean> {
  const title = draft.title.trim()
  const rawGroup = draft.group.trim() || '未分组'
  const isCredential = draft.kind === 'credential' || rawGroup === '账密'
  const kind: SnippetKind = isCredential ? 'credential' : 'text'
  const group = isCredential ? '账密' : rawGroup

  if (!title) {
    pushToast('请填写短语标题', 'error')
    return false
  }
  if ([...title].length > 80) {
    pushToast('短语标题不能超过 80 个字符', 'error')
    return false
  }

  let content = draft.content
  let account: string | undefined
  let password: string | undefined

  if (isCredential) {
    const trimmedAccount = (draft.account ?? '').trim()
    if (!trimmedAccount) {
      pushToast('请填写账号', 'error')
      return false
    }
    if ([...trimmedAccount].length > 300) {
      pushToast('账号不能超过 300 个字符', 'error')
      return false
    }
    const trimmedNote = draft.content.trim()
    if ([...trimmedNote].length > 500) {
      pushToast('备注不能超过 500 个字符', 'error')
      return false
    }
    const existingItem = draft.id ? state.snippets.find((item) => item.id === draft.id) : undefined
    if (draft.password !== undefined) {
      if (!draft.password) {
        pushToast('请填写密码', 'error')
        return false
      }
      if ([...draft.password].length > 1000) {
        pushToast('密码不能超过 1000 个字符', 'error')
        return false
      }
      password = draft.password
    } else if (!existingItem?.hasPassword) {
      pushToast('请填写密码', 'error')
      return false
    }
    account = trimmedAccount
    content = trimmedNote
  } else {
    if (!content.trim()) {
      pushToast('短语内容不能为空', 'error')
      return false
    }
    if (group === '全部' || [...group].length > 40) {
      pushToast('分组名需为 1–40 个字符，且不能为“全部”', 'error')
      return false
    }
  }

  if (!isDesktop) {
    if (!state.snippetGroups.includes(group)) state.snippetGroups.push(group)
    const now = Date.now()
    const targetId =
      draft.id || `snip-${now.toString(36)}-${Math.random().toString(36).slice(2, 6)}`
    if (isCredential) {
      if (password !== undefined) mockPasswordVault.set(targetId, password)
    } else {
      mockPasswordVault.delete(targetId)
    }
    const nextItem: SnippetItem = {
      id: targetId,
      kind,
      title,
      content,
      account,
      hasPassword: isCredential ? Boolean(password ?? mockPasswordVault.has(targetId)) : undefined,
      group,
      pinned: draft.pinned,
      updatedAt: now
    }
    if (draft.id) {
      const target = state.snippets.find((item) => item.id === draft.id)
      if (target) {
        Object.assign(target, nextItem)
        if (!isCredential) {
          delete target.account
          delete target.hasPassword
        }
      }
    } else {
      state.snippets.unshift(nextItem)
    }
    state.snippets = normalizeMockSnippetOrder(state.snippets)
    pushToast(`${isCredential ? '账密' : '常用短语'}已保存（浏览器样稿，未落盘）`, 'success')
    return true
  }

  try {
    const payload = await invokeHost<SnippetPayload>('save_snippet', {
      snippet: {
        id: draft.id,
        kind,
        title,
        content,
        account,
        password,
        group,
        pinned: draft.pinned
      }
    })
    applySnippets(payload)
    pushToast(isCredential ? '账密已保存' : '常用短语已保存', 'success')
    return true
  } catch (error) {
    pushToast(error instanceof Error ? error.message : String(error), 'error')
    return false
  }
}

export async function deleteSnippet(id: string): Promise<boolean> {
  if (!isDesktop) {
    const before = state.snippets.length
    state.snippets = state.snippets.filter((item) => item.id !== id)
    mockPasswordVault.delete(id)
    if (state.snippets.length < before) pushToast('已删除常用短语', 'success')
    return state.snippets.length < before
  }
  try {
    const payload = await invokeHost<SnippetPayload>('delete_snippet', { id })
    applySnippets(payload)
    pushToast('已删除常用短语', 'success')
    return true
  } catch (error) {
    pushToast(error instanceof Error ? error.message : String(error), 'error')
    return false
  }
}

export async function saveClipAsSnippet(clip: ClipItem): Promise<boolean> {
  if (clip.kind !== 'text' || !clip.text?.trim()) {
    pushToast('只有非空文本记录可以存为常用短语', 'error')
    return false
  }
  if (!isDesktop) {
    const rawLine = (clip.note?.trim() || clip.text.split(/\r?\n/).find((line) => line.trim()) || '常用短语').trim()
    const chars = [...rawLine]
    const title = chars.length > 28 ? `${chars.slice(0, 28).join('')}…` : rawLine
    const group =
      clip.group && clip.group !== '全部' && clip.group !== '账密' ? clip.group : '未分组'
    if (!state.snippetGroups.includes(group)) state.snippetGroups.push(group)
    state.snippets = normalizeMockSnippetOrder([
      {
        id: `snip-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`,
        kind: 'text',
        title,
        content: clip.text,
        group,
        pinned: false,
        updatedAt: Date.now()
      },
      ...state.snippets
    ])
    pushToast(`已存入常用短语：${title}（浏览器样稿）`, 'success')
    return true
  }
  try {
    const payload = await invokeHost<SnippetPayload>('save_clip_as_snippet', { clipId: clip.id })
    applySnippets(payload)
    pushToast('已存入常用短语', 'success')
    return true
  } catch (error) {
    pushToast(error instanceof Error ? error.message : String(error), 'error')
    return false
  }
}

export async function reorderVisibleSnippets(
  visibleIds: string[],
  fromId: string,
  toId: string
): Promise<boolean> {
  const fromSnippet = state.snippets.find((item) => item.id === fromId)
  const toSnippet = state.snippets.find((item) => item.id === toId)
  if (!fromSnippet || !toSnippet || fromSnippet.pinned !== toSnippet.pinned) return false
  const visible = new Set(visibleIds)
  const ordered = state.snippets.filter((item) => visible.has(item.id) && item.pinned === fromSnippet.pinned)
  const from = ordered.findIndex((item) => item.id === fromId)
  const to = ordered.findIndex((item) => item.id === toId)
  if (from < 0 || to < 0 || from === to) return false
  const [moved] = ordered.splice(from, 1)
  ordered.splice(to, 0, moved)
  let cursor = 0
  const next = state.snippets.map((item) =>
    visible.has(item.id) && item.pinned === fromSnippet.pinned ? ordered[cursor++] : item
  )
  if (!isDesktop) {
    state.snippets = normalizeMockSnippetOrder(next)
    return true
  }
  try {
    const payload = await invokeHost<SnippetPayload>('reorder_snippets', {
      ids: next.map((item) => item.id)
    })
    applySnippets(payload)
    return true
  } catch (error) {
    pushToast(`保存短语顺序失败：${String(error)}`, 'error')
    return false
  }
}

export async function createSnippetGroup(group: string): Promise<boolean> {
  const trimmed = group.trim()
  if (!trimmed || trimmed === '全部' || [...trimmed].length > 40) {
    pushToast('分组名需为 1–40 个字符，且不能为“全部”', 'error')
    return false
  }
  if (!isDesktop) {
    if (!state.snippetGroups.includes(trimmed)) state.snippetGroups.push(trimmed)
    return true
  }
  try {
    const payload = await invokeHost<SnippetPayload>('create_snippet_group', { group: trimmed })
    applySnippets(payload)
    return true
  } catch (error) {
    pushToast(String(error), 'error')
    return false
  }
}

export async function removeSnippetGroup(group: string): Promise<boolean> {
  if (group === '未分组' || group === '账密') return false
  if (!isDesktop) {
    state.snippetGroups = state.snippetGroups.filter((name) => name !== group)
    state.snippets.forEach((item) => {
      if (item.group === group) item.group = '未分组'
    })
    if (state.ui.snippetGroup === group) state.ui.snippetGroup = '全部'
    return true
  }
  try {
    const payload = await invokeHost<SnippetPayload>('remove_snippet_group', { group })
    applySnippets(payload)
    return true
  } catch (error) {
    pushToast(String(error), 'error')
    return false
  }
}

export async function saveShortcuts(settings: typeof state.shortcuts = state.shortcuts): Promise<boolean> {
  if (!isDesktop) return false
  try {
    state.shortcuts = await invokeHost<typeof state.shortcuts>('save_shortcuts', { settings: { ...settings } })
    state.settings.clipboardHotkey = state.shortcuts.clipboard
    pushToast(state.shortcuts.errors.length ? '部分快捷键注册失败，请查看提示' : '快捷键已保存', state.shortcuts.errors.length ? 'error' : 'success')
    return !state.shortcuts.errors.length
  } catch (error) { pushToast(String(error), 'error'); return false }
}

export async function saveAppCategory(id: string, name: string | null): Promise<boolean> {
  if (id === 'all' || !id.trim() || (name !== null && (!name.trim() || [...name.trim()].length > 40))) {
    pushToast('分类名称需为 1–40 个字符', 'error'); return false
  }
  try {
    if (isDesktop) applyApps(await invokeHost<AppConfig>('save_application_category', { id, name }))
    else if (name === null) {
      state.categories = state.categories.filter(category => category.id !== id)
      state.apps.forEach(app => { if (app.categoryId === id) app.categoryId = 'all' })
    } else {
      const category = state.categories.find(category => category.id === id)
      if (category) category.name = name.trim()
      else state.categories.push({ id, name: name.trim() })
    }
    if (!state.categories.some((category) => category.id === state.ui.activeCategoryId)) setCategory('all')
    return true
  } catch (error) { pushToast(String(error), 'error'); return false }
}

watch(() => state.ui.panel, (panel) => {
  if (!isDesktop) return
  if (panel === 'clipboard') {
    void refreshClipboard().catch(() => pushToast('读取剪贴板历史失败', 'error'))
    void refreshSnippets().catch(() => pushToast('读取常用短语失败', 'error'))
  }
})

/* ---------- 新装应用增量发现与失效应用健康巡检 ---------- */

function applyDiscovery(snapshot: DiscoverySnapshot): void {
  state.discovery.initialized = snapshot.initialized
  state.discovery.lastScanMs = snapshot.lastScanMs
  state.discovery.pending = snapshot.pending
  state.discovery.dismissed = snapshot.dismissed
  const targets = snapshot.pending.map((item) => item.target)
  if (targets.length) void loadMissingIcons(targets)
}

export async function refreshDiscoveredApps(): Promise<void> {
  if (!isDesktop) return
  try {
    const snap = await invokeHost<DiscoverySnapshot>('list_discovered_apps')
    applyDiscovery(snap)
  } catch {
    // 读取失败不影响设置主流程
  }
}

export async function scanDiscoveredApps(force = false): Promise<DiscoverySnapshot | null> {
  if (!isDesktop) {
    if (force) pushToast('扫描新安装应用请在桌面版中使用', 'info')
    return null
  }
  if (state.discovery.scanning) return null
  state.discovery.scanning = true
  try {
    const snap = await invokeHost<DiscoverySnapshot>('scan_discovered_apps', { force })
    applyDiscovery(snap)
    if (force) {
      pushToast(
        snap.pending.length ? `已检查完成，当前有 ${snap.pending.length} 个新装应用待分类` : '已检查完成，暂未发现新安装的应用',
        'info'
      )
    }
    return snap
  } catch (error) {
    if (force) pushToast(String(error), 'error')
    return null
  } finally {
    state.discovery.scanning = false
  }
}

export async function acceptDiscoveredApp(
  target: string,
  name: string,
  categoryId: string
): Promise<boolean> {
  if (!isDesktop) {
    state.discovery.pending = state.discovery.pending.filter((item) => item.target !== target)
    return true
  }
  try {
    const res = await invokeHost<{ config: AppConfig; discovery: DiscoverySnapshot }>('accept_discovered_app', {
      target,
      name,
      categoryId
    })
    applyApps(res.config)
    applyDiscovery(res.discovery)
    const catLabel = state.categories.find((c) => c.id === categoryId)?.name ?? '未分类'
    pushToast(`已将「${name}」加入「${catLabel}」`, 'success')
    return true
  } catch (error) {
    pushToast(String(error), 'error')
    return false
  }
}

export async function dismissDiscoveredApp(target: string): Promise<boolean> {
  if (!isDesktop) {
    state.discovery.pending = state.discovery.pending.filter((item) => item.target !== target)
    return true
  }
  try {
    const snap = await invokeHost<DiscoverySnapshot>('dismiss_discovered_app', { target })
    applyDiscovery(snap)
    return true
  } catch (error) {
    pushToast(String(error), 'error')
    return false
  }
}

export async function dismissAllDiscoveredApps(): Promise<boolean> {
  if (!isDesktop) {
    state.discovery.pending = []
    return true
  }
  try {
    const snap = await invokeHost<DiscoverySnapshot>('dismiss_all_discovered_apps')
    applyDiscovery(snap)
    pushToast('已忽略当前全部待分类新应用', 'info')
    return true
  } catch (error) {
    pushToast(String(error), 'error')
    return false
  }
}

export async function restoreDismissedApp(target: string): Promise<boolean> {
  if (!isDesktop) return false
  try {
    const snap = await invokeHost<DiscoverySnapshot>('restore_dismissed_app', { target })
    applyDiscovery(snap)
    pushToast('已恢复到待分类列表', 'success')
    return true
  } catch (error) {
    pushToast(String(error), 'error')
    return false
  }
}

export async function auditApplications(): Promise<void> {
  if (!isDesktop) return
  try {
    const next = await invokeHost<AppConfig>('audit_applications')
    applyApps(next)
  } catch {
    // 静默健康巡检失败不打扰用户
  }
}

export async function removeBrokenApplications(): Promise<number> {
  if (!isDesktop) {
    const before = state.apps.length
    state.apps = state.apps.filter((app) => !app.broken)
    return before - state.apps.length
  }
  try {
    const res = await invokeHost<{ removed: number; config: AppConfig }>('remove_broken_applications')
    applyApps(res.config)
    if (res.removed > 0) {
      pushToast(`已清理 ${res.removed} 个失效应用`, 'success')
    } else {
      pushToast('当前没有失效应用', 'info')
    }
    return res.removed
  } catch (error) {
    pushToast(String(error), 'error')
    return 0
  }
}

export interface OcrResultEvent {
  text: string
  lineCount: number
  charCount: number
  language: string
  elapsedMs: number
  source: 'screen' | 'clipboard'
}

export function applyOcrResult(payload: OcrResultEvent): void {
  state.ocr.open = true
  state.ocr.status = 'done'
  state.ocr.text = payload.text
  state.ocr.lineCount = payload.lineCount
  state.ocr.charCount = payload.charCount
  state.ocr.language = payload.language
  state.ocr.elapsedMs = payload.elapsedMs
  state.ocr.source = payload.source
  state.ocr.error = ''
  state.ui.islandExpanded = true
}

export function applyOcrError(message: string, source: 'screen' | 'clipboard' = 'screen'): void {
  state.ocr.open = true
  state.ocr.status = 'error'
  state.ocr.text = ''
  state.ocr.lineCount = 0
  state.ocr.charCount = 0
  state.ocr.language = ''
  state.ocr.elapsedMs = 0
  state.ocr.source = source
  state.ocr.error = message
  state.ui.islandExpanded = true
}

export async function startScreenOcr(): Promise<void> {
  if (!isDesktop) {
    applyOcrResult({
      text: '截图识字预览示例 DesktopHub Lite 0.1.3\n支持直接复制全文或选段修改',
      lineCount: 2,
      charCount: 38,
      language: 'zh-Hans-CN',
      elapsedMs: 42,
      source: 'screen'
    })
    return
  }
  try {
    await invokeHost('start_screen_ocr')
  } catch (error) {
    pushToast(String(error), 'error')
  }
}

export async function runClipboardOcr(id: string): Promise<void> {
  state.ocr.open = true
  state.ocr.status = 'loading'
  state.ocr.source = 'clipboard'
  state.ocr.error = ''
  state.ui.islandExpanded = true
  if (!isDesktop) {
    applyOcrResult({
      text: '剪贴板图片示例文字 DesktopHub Lite 2026',
      lineCount: 1,
      charCount: 27,
      language: 'zh-Hans-CN',
      elapsedMs: 36,
      source: 'clipboard'
    })
    return
  }
  try {
    const payload = await invokeHost<OcrResultEvent>('ocr_clipboard_image', { id })
    applyOcrResult(payload)
  } catch (error) {
    applyOcrError(String(error), 'clipboard')
  }
}

export async function copyOcrText(text: string): Promise<boolean> {
  if (!text) return false
  if (!isDesktop) {
    if (navigator.clipboard?.writeText) await navigator.clipboard.writeText(text).catch(() => {})
    pushToast('已复制识别文字（浏览器预览）', 'success')
    return true
  }
  try {
    await invokeHost('copy_ocr_text', { text })
    return true
  } catch (error) {
    pushToast(String(error), 'error')
    return false
  }
}

export function openOcrPanel(): void {
  if (state.ocr.status === 'idle' && !state.ocr.text.trim()) return
  state.ocr.open = true
  state.ui.islandExpanded = true
}

export function closeOcrPanel(clear = false): void {
  state.ocr.open = false
  if (clear) {
    state.ocr.status = 'idle'
    state.ocr.text = ''
    state.ocr.lineCount = 0
    state.ocr.charCount = 0
    state.ocr.language = ''
    state.ocr.elapsedMs = 0
    state.ocr.error = ''
  }
}

export function clearOcrResult(): void {
  closeOcrPanel(true)
}

export function sendOcrToTranslation(text: string): void {
  closeOcrPanel(false)
  state.ui.islandPage = 'translation'
  state.ui.islandExpanded = true
  openTranslation(text, false)
}
