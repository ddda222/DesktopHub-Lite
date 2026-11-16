/**
 * 界面状态与动作。
 *
 * 多窗口注意：桌面版里 bar / pop / settings / toast 是四个独立 WebView，
 * 各自有一份 store。所以「哪个弹层打开、当前分类、主题、暂停」这些跨窗口状态
 * 以 Rust 为准，这里只保留一份镜像并订阅 `ui-state` 广播。
 * 业务数据（应用配置、剪贴板、翻译设置）也来自 Rust 命令，不再用合成数据。
 */
import { computed, reactive, watch } from 'vue'
import { host, newRequestId } from '@/lib/desktop/host'
import {
  defaultIslandAppearance,
  dismissPanel,
  dismissSettings,
  describeTarget,
  importDesktopShortcuts,
  invokeHost,
  isDesktop,
  windowLabel,
  onApplicationsChanged,
  listenHost,
  onUiState,
  pickApplicationTarget,
  readAppIcons,
  requestCategory,
  requestPanel,
  requestSettings,
  requestTheme,
  requestToast,
  type UiState
} from '@/lib/desktop/desktop'
import {
  apps as mockApps,
  categories as mockCategories,
  clips as mockClips,
  defaultTranslationSettings as mockTranslationSettings,
  groups as mockGroups
} from '@/mock/data'
import type {
  AppConfig,
  Category,
  ClipItem,
  ClipboardRecord,
  HubApp,
  HubSettings,
  Toast,
  ToastKind,
  TranslateDirection,
  TranslationResult,
  TranslationSettings
} from '@/lib/desktop/types'

export type PanelName = 'clipboard' | 'translation' | 'category' | 'overflow' | null
export type SettingsSection = 'apps' | 'clipboard' | 'translation' | 'general'

export const state = reactive({
  /** 数据是否已经从宿主加载过；false 时界面显示占位而不是假装有数据 */
  ready: false,
  shortcuts: { bar: 'Ctrl+Alt+Space', clipboard: 'Ctrl+Shift+V', translation: 'Ctrl+Shift+F', errors: [] as string[] },
  loadError: '',
  apps: [] as HubApp[],
  categories: [] as Category[],
  clips: [] as ClipItem[],
  groups: [] as string[],
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
    direction: 'auto',
    hasSecret: false
  } as TranslationSettings,
  translation: {
    source: '',
    direction: 'auto' as TranslateDirection,
    status: 'idle' as 'idle' | 'loading' | 'error' | 'done',
    result: null as TranslationResult | null,
    error: '',
    requestSeq: 0,
    filledFromClip: false
  },
  toasts: [] as Toast[],
  /** 目标路径 → 图标 data URI。读不到就不放进来，界面回退到首字母色块。 */
  icons: {} as Record<string, string>,
  ui: {
    islandExpanded: false,
    islandAnchor: 'center',
    islandPage: 'applications' as UiState['islandPage'],
    islandHoverMs: 320,
    islandLeaveMs: 450,
    islandReducedMotion: false,
    islandAppearance: defaultIslandAppearance(),
    fullscreenSuppressed: false,
    wechatEnabled: false,
    wechatStatus: 'off',
    wechatAttention: false,
    wechatEvent: 0,
    activeCategoryId: 'all',
    panel: null as PanelName,
    settingsOpen: false,
    settingsSection: 'apps' as SettingsSection,
    query: '',
    favoritesOnly: false,
    group: '全部',
    detailId: null as string | null,
    activeIndex: 0,
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
  if (state.ui.panel === 'clipboard') void loadClipImages()
}

async function loadClipImages(): Promise<void> {
  for (const clip of state.clips.filter((item) => item.kind === 'image' && !item.imageUrl && !item.imageMissing)) {
    try { clip.imageUrl = await invokeHost<string>('read_clipboard_thumbnail', { id: clip.id }) }
    catch { clip.imageMissing = true }
  }
}

async function refreshTranslationSettings(): Promise<void> {
  state.translationSettings = await invokeHost<TranslationSettings>('get_translation_settings')
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
async function loadMissingIcons(): Promise<void> {
  if (!isDesktop) return
  const targets = [
    ...new Set(state.apps.filter((app) => !app.hidden).map((app) => app.target))
  ].filter((target) => !state.icons[target])
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
export async function bootstrap(): Promise<void> {
  if (!isDesktop) {
    // 浏览器样稿：用合成数据，并明确标注不是真实数据。
    state.apps = mockApps.map((app) => ({ ...app }))
    state.categories = mockCategories.map((cat) => ({ ...cat }))
    state.clips = mockClips.map((clip) => ({ ...clip }))
    state.groups = [...mockGroups]
    state.translationSettings = { ...mockTranslationSettings }
    state.ready = true
    return
  }

  try {
    const config = await invokeHost<AppConfig>('list_applications')
    applyApps(config)
    await refreshClipboard()
    state.shortcuts = await invokeHost<typeof state.shortcuts>('get_shortcuts')
    state.settings.clipboardHotkey = state.shortcuts.clipboard
    const settings = await invokeHost<TranslationSettings>('get_translation_settings')
    state.translationSettings = { ...settings }
    const ui = await invokeHost<UiState>('ui_state')
    applyUiState(ui)
    state.ready = true
    state.loadError = ''
  } catch (error) {
    state.loadError = error instanceof Error ? error.message : String(error)
  }
}

function applyUiState(ui: UiState): void {
  state.ui.islandExpanded = ui.islandExpanded
  state.ui.islandAnchor = ui.islandAnchor
  state.ui.islandPage = ui.islandPage
  state.ui.islandHoverMs = ui.islandHoverMs
  state.ui.islandLeaveMs = ui.islandLeaveMs
  state.ui.islandReducedMotion = ui.islandReducedMotion
  state.ui.islandAppearance = ui.islandAppearance
  state.ui.fullscreenSuppressed = ui.fullscreenSuppressed
  state.ui.wechatEnabled = ui.wechatEnabled
  state.ui.wechatStatus = ui.wechatStatus
  state.ui.wechatAttention = ui.wechatAttention
  state.ui.wechatEvent = ui.wechatEvent
  state.ui.panel = ui.panel
  state.ui.settingsOpen = ui.settingsOpen
  state.ui.paused = ui.paused
  state.ui.activeCategoryId = ui.activeCategoryId
  state.settings.theme = ui.theme as HubSettings['theme']
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
  const offs = await Promise.all([
    subscribeUiState(), subscribeApplications(),
    listenHost('clipboard-changed', () => { void refreshClipboard().catch(() => pushToast('读取剪贴板历史失败', 'error')) }),
    listenHost<string>('clipboard-notice', (message) => pushToast(message, 'error')),
    listenHost('translation-settings-changed', () => { void refreshTranslationSettings().catch(() => pushToast('读取翻译设置失败', 'error')) })
  ])
  return () => offs.forEach((off) => off())
}

/* ---------- 轻提示 ---------- */

let toastSeq = 0

export function pushToast(text: string, kind: ToastKind = 'info'): void {
  if (isDesktop) {
    // 桌面版提示由独立的 toast 窗口渲染：图标条窗口只有图标条大小，放不下提示。
    void requestToast(text, kind)
    return
  }
  const toast: Toast = { id: ++toastSeq, text, kind }
  state.toasts.push(toast)
  window.setTimeout(() => {
    const index = state.toasts.findIndex((item) => item.id === toast.id)
    if (index >= 0) state.toasts.splice(index, 1)
  }, 2600)
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

/* ---------- 弹层与导航 ---------- */

export function togglePanel(panel: Exclude<PanelName, null>): void {
  state.ui.detailId = null
  state.ui.query = ''
  state.ui.activeIndex = 0
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
  if (isDesktop) {
    void dismissPanel()
    return
  }
  state.ui.panel = null
}

export function openSettings(section: SettingsSection = 'apps'): void {
  state.ui.settingsSection = section
  if (isDesktop) {
    void requestSettings()
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
  state.ui.activeCategoryId = id
  if (isDesktop) void requestCategory(id)
}

export function moveActive(delta: number): void {
  const total = filteredClips.value.length
  if (!total) return
  state.ui.activeIndex = (state.ui.activeIndex + delta + total) % total
}

/* ---------- 应用 ---------- */

export async function launchApp(app: HubApp): Promise<void> {
  if (app.broken) {
    pushToast(`目标失效：${app.name}`, 'error')
    return
  }
  if (!isDesktop) {
    pushToast(`已提交启动请求：${app.name}（浏览器样稿，未真的启动）`, 'success')
    return
  }
  try {
    const result = await invokeHost<{ launched: boolean; reason: string }>('open_application', {
      id: app.id
    })
    if (result.launched) {
      pushToast(`已提交打开请求：${app.name}`, 'success')
    } else {
      // 阶段 1 明确未启动，不假装成功
      pushToast(result.reason, 'info')
    }
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '启动失败', 'error')
  }
}

export function reorderApps(fromId: string, toId: string): void {
  const from = state.apps.findIndex((app) => app.id === fromId)
  const to = state.apps.findIndex((app) => app.id === toId)
  if (from < 0 || to < 0 || from === to) return
  const [moved] = state.apps.splice(from, 1)
  state.apps.splice(to, 0, moved)
  persistOrder()
}

/** 把当前顺序写回宿主；设置页的上移/下移也走这里，不再只改本地数组。 */
export function persistOrder(): void {
  if (!isDesktop) return
  void invokeHost('reorder_applications', { ids: state.apps.map((app) => app.id) }).catch(() => {
    pushToast('保存应用顺序失败', 'error')
  })
}

/** 上移 / 下移一位，并立刻落盘。 */
export function moveApp(index: number, delta: number): void {
  const target = index + delta
  if (index < 0 || index >= state.apps.length) return
  if (target < 0 || target >= state.apps.length) return
  const [moved] = state.apps.splice(index, 1)
  state.apps.splice(target, 0, moved)
  persistOrder()
}

export function toggleAppHidden(id: string): void {
  const app = state.apps.find((item) => item.id === id)
  if (!app) return
  app.hidden = !app.hidden
  void saveApp(app)
}

export function setAppCategory(id: string, categoryId: string): void {
  const app = state.apps.find((item) => item.id === id)
  if (!app) return
  app.categoryId = categoryId
  void saveApp(app)
}

async function saveApp(app: HubApp): Promise<void> {
  if (!isDesktop) return
  try {
    const config = await invokeHost<AppConfig>('save_application', {
      application: { ...app }
    })
    applyApps(config)
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '保存应用设置失败', 'error')
  }
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

/** 从桌面导入快捷方式；返回数量供界面提示。 */
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

export async function copyClip(clip: ClipItem): Promise<void> {
  if (clip.imageMissing) {
    pushToast('图片文件缺失，无法复制', 'error')
    return
  }
  if (!isDesktop) {
    pushToast('已写入系统剪贴板（浏览器样稿，不会真的写入）', 'success')
    return
  }
  try {
    await invokeHost('copy_clipboard_entry', { id: clip.id })
    pushToast('已复制到剪贴板', 'success')
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
    state.clips.splice(0, state.clips.length)
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
  cancelTranslation()
  state.translation.direction = direction
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
    pushToast('译文已复制', 'success')
  } catch (error) {
    pushToast(error instanceof Error ? error.message : '复制失败', 'error')
  }
}

/* ---------- 设置 ---------- */

export function setTheme(theme: HubSettings['theme']): void {
  state.settings.theme = theme
  if (isDesktop) void requestTheme(theme)
}

/* ---------- 主题落地 ---------- */

const prefersDark = window.matchMedia('(prefers-color-scheme: dark)')

function applyTheme(): void {
  const mode = state.settings.theme
  const resolved = mode === 'system' ? (prefersDark.matches ? 'dark' : 'light') : mode
  document.documentElement.dataset.theme = resolved
}

watch(() => state.settings.theme, applyTheme, { immediate: true })
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

export async function saveTranslationSettings(): Promise<boolean> {
  if (!isDesktop) return false
  try {
    state.translationSettings = await invokeHost<TranslationSettings>('save_translation_settings', { settings: { ...state.translationSettings } })
    state.translation.direction = state.translationSettings.direction
    pushToast('翻译设置已保存', 'success')
    return true
  } catch (error) { pushToast(String(error), 'error'); return false }
}

export async function saveTranslationSecret(secret: string): Promise<boolean> {
  if (!await saveTranslationSettings()) return false
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

export async function saveShortcuts(): Promise<void> {
  try {
    state.shortcuts = await invokeHost<typeof state.shortcuts>('save_shortcuts', { settings: { ...state.shortcuts } })
    state.settings.clipboardHotkey = state.shortcuts.clipboard
    pushToast(state.shortcuts.errors.length ? '部分快捷键注册失败，请查看提示' : '快捷键已保存', state.shortcuts.errors.length ? 'error' : 'success')
  } catch (error) { pushToast(String(error), 'error') }
}

export async function saveAppCategory(id: string, name: string | null): Promise<void> {
  try {
    applyApps(await invokeHost<AppConfig>('save_application_category', { id, name }))
    if (!state.categories.some((category) => category.id === state.ui.activeCategoryId)) setCategory('all')
  } catch (error) { pushToast(String(error), 'error') }
}

watch(() => state.ui.panel, (panel, previous) => {
  if (!isDesktop) return
  if (panel === 'clipboard') void refreshClipboard().catch(() => pushToast('读取剪贴板历史失败', 'error'))
  if (windowLabel !== 'translation' && previous === 'translation' && panel !== 'translation') cancelTranslation()
})
