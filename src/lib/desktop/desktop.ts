/**
 * 桌面宿主（Tauri）与浏览器样稿之间的最小适配层。
 *
 * 设计约束（对应实施方案第 5 节）：
 * - 只在桌面运行时才真的调用宿主命令；在浏览器里打开时全部降级为无副作用，
 *   这样同一套界面代码既能做交互样稿，也能跑真实桌面窗口。
 * - 每个窗口是独立的 WebView，各自有独立的 JS 上下文。跨窗口共享的状态
 *   （当前打开哪个弹层、设置窗口是否打开）由 Rust 持有，这里只做转发。
 * - 不在这里拼接任意 Shell 或文件路径，命令名与实施方案保持一一对应。
 */
import { invoke as tauriInvoke } from '@tauri-apps/api/core'
import { listen as tauriListen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { AppConfig } from '@/lib/desktop/types'

/** 是否运行在 Tauri 桌面外壳里；false 表示浏览器里的交互样稿。 */
export const isDesktop: boolean =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in (window as unknown as Record<string, unknown>)

/** 当前窗口标识：bar / pop / settings；浏览器样稿里是 browser。 */
export let windowLabel: string = isDesktop ? getCurrentWindow().label : 'browser'

/** 调用宿主命令。宿主返回 Err(String) 时抛出，错误文案已经可直接展示给用户。 */
export async function invokeHost<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return (await tauriInvoke<T>(command, args)) as T
}

/** 订阅宿主事件，返回取消订阅函数。页面卸载时必须调用，避免重复注册。 */
export async function listenHost<T>(
  event: string,
  handler: (payload: T) => void
): Promise<() => void> {
  if (!isDesktop) return () => {}
  return tauriListen<T>(event, (e) => handler(e.payload))
}

export interface IslandAppearance {
  scale: number
  opacity: number
  tint: string
  preset: 'classic' | 'glass'
  avoidFullscreen: boolean
  autoCheckUpdates: boolean
  webgl: boolean
  hideFromCapture: boolean
}

export function defaultIslandAppearance(): IslandAppearance {
  return { scale: 100, opacity: 100, tint: '', preset: 'classic', avoidFullscreen: true, autoCheckUpdates: false, webgl: true, hideFromCapture: false }
}

/** 宿主当前打开的弹层与设置窗口状态，由 Rust 持有并广播。 */
export interface UiState {
  islandExpanded: boolean
  islandAnchor: string
  islandPage: 'applications' | 'clipboard' | 'translation' | 'files' | 'compare' | 'audio'
  islandHoverMs: number
  islandLeaveMs: number
  islandReducedMotion: boolean
  islandAppearance: IslandAppearance
  islandCaptureError: string | null
  translationPending: number
  fullscreenSuppressed: boolean
  wechatEnabled: boolean
  wechatStatus: string
  wechatAttention: boolean
  wechatEvent: number
  panel: 'clipboard' | 'translation' | 'files' | 'compare' | 'audio' | 'category' | 'overflow' | null
  settingsOpen: boolean
  paused: boolean
  /** 图标条当前分类；弹层窗口也按这个分类展示 */
  activeCategoryId: string
  /** dark / light / system */
  theme: string
}

const EMPTY_UI: UiState = {
  islandExpanded: false,
  islandAnchor: 'center',
  islandPage: 'applications',
  islandHoverMs: 320,
  islandLeaveMs: 450,
  islandReducedMotion: false,
  islandAppearance: defaultIslandAppearance(),
  islandCaptureError: null,
  translationPending: 0,
  fullscreenSuppressed: false,
  wechatEnabled: false,
  wechatStatus: 'off',
  wechatAttention: false,
  wechatEvent: 0,
  panel: null,
  settingsOpen: false,
  paused: false,
  activeCategoryId: 'all',
  theme: 'dark',
}

export async function readUiState(): Promise<UiState> {
  if (!isDesktop) return { ...EMPTY_UI }
  try {
    return await invokeHost<UiState>('ui_state')
  } catch {
    return { ...EMPTY_UI }
  }
}

/** 订阅宿主广播的界面状态（哪个弹层打开、设置是否打开、是否暂停）。 */
export async function onUiState(handler: (state: UiState) => void): Promise<() => void> {
  return listenHost<UiState>('ui-state', handler)
}

/** 打开/切换一个弹层窗口。同一个弹层再次点击时收起。 */
export async function requestPanel(panel: Exclude<UiState['panel'], null>): Promise<void> {
  if (!isDesktop) return
  await invokeHost('toggle_panel', { panel })
}

/** 收起当前弹层窗口。 */
export async function dismissPanel(): Promise<void> {
  if (!isDesktop) return
  await invokeHost('close_panel')
}

export async function requestSettings(): Promise<void> {
  if (!isDesktop) return
  await invokeHost('open_settings')
}

export async function dismissSettings(): Promise<void> {
  if (!isDesktop) return
  await invokeHost('close_settings')
}

/**
 * 图标条窗口按内容自适应尺寸：窗口等于图标条本身，透明区域才不会挡住桌面点击。
 * 只在 bar 窗口调用。
 */
export interface BarRegion {
  x: number
  y: number
  width: number
  height: number
  radius: number
}

export async function resizeBar(width: number, height: number, regions?: BarRegion[]): Promise<void> {
  if (!isDesktop) return
  await invokeHost('set_bar_size', { width: Math.round(width), height: Math.round(height), regions })
}

/**
 * 弹层窗口按内容自适应尺寸，并让 Rust 重新把它对齐到图标条上方。
 * 只在 pop 窗口调用。
 */
export async function resizePop(width: number, height: number): Promise<void> {
  if (!isDesktop) return
  await invokeHost('set_pop_size', { width: Math.round(width), height: Math.round(height) })
}

/** 让图标条窗口跟随鼠标移动（原生拖动，比 CSS 位移更稳）。 */
export async function startBarDrag(): Promise<void> {
  if (!isDesktop) return
  await invokeHost('start_bar_drag')
}

/** 切换图标条分类（跨窗口状态，Rust 广播给弹层窗口）。 */
export async function requestCategory(categoryId: string): Promise<void> {
  if (!isDesktop) return
  await invokeHost('set_category', { categoryId })
}

/** 切换主题（跨窗口状态）。 */
export async function requestTheme(theme: string): Promise<void> {
  if (!isDesktop) return
  await invokeHost('set_theme', { theme })
}

/** 弹一条轻提示。提示由独立的 toast 窗口渲染。 */
export async function requestToast(message: string, kind: 'info' | 'error' | 'success' = 'info'): Promise<void> {
  if (!isDesktop) return
  try {
    await invokeHost('show_toast', { message, kind })
  } catch {
    // 提示发不出去不该打断主流程
  }
}

/** 收起轻提示窗口。 */
export async function dismissToast(): Promise<void> {
  if (!isDesktop) return
  await invokeHost('hide_toast')
}

/** 提示窗口按内容自适应尺寸。只在 toast 窗口调用。 */
export async function resizeToast(width: number, height: number): Promise<void> {
  if (!isDesktop) return
  await invokeHost('set_toast_size', { width: Math.round(width), height: Math.round(height) })
}

/** 订阅轻提示事件；返回取消订阅函数。 */
export async function onToast(
  handler: (payload: { message: string; kind: string }) => void
): Promise<() => void> {
  return listenHost<{ message: string; kind: string }>('toast', handler)
}

/** 窗口失焦时回调；返回取消订阅函数。浏览器里无操作。 */
export function onWindowBlur(handler: () => void): () => void {
  if (!isDesktop) return () => {}
  let unlisten: (() => void) | null = null
  void getCurrentWindow()
    .onFocusChanged(({ payload: focused }) => {
      if (!focused) handler()
    })
    .then((fn) => {
      unlisten = fn
    })
  return () => unlisten?.()
}

/** 把窗口尺寸报给界面，便于界面在真实窗口里按像素布局（样稿里返回窗口尺寸）。 */
export async function currentWindowSize(): Promise<{ width: number; height: number }> {
  if (!isDesktop) return { width: window.innerWidth, height: window.innerHeight }
  const size = await getCurrentWindow().innerSize()
  const scale = await getCurrentWindow().scaleFactor()
  return { width: size.width / scale, height: size.height / scale }
}

// ---------------------------------------------------------------- 应用（阶段 2）

/** 应用配置变化事件：任意窗口改了应用列表，其它窗口据此重新拉取。 */
export async function onApplicationsChanged(
  handler: (config: AppConfig) => void
): Promise<() => void> {
  return listenHost<AppConfig>('applications-changed', handler)
}

/** 单个目标的图标读取结果。`dataUri` 为空时 `error` 说明原因。 */
export interface AppIconPayload {
  target: string
  dataUri: string | null
  error: string | null
}

/**
 * 批量读取目标图标（EXE / LNK / 目录）。读不到会带 error 回来，
 * 界面据此回退到首字母色块——不伪造图标。
 */
export async function readAppIcons(targets: string[]): Promise<AppIconPayload[]> {
  if (!isDesktop || targets.length === 0) return []
  try {
    return await invokeHost<AppIconPayload[]>('read_app_icons', { targets })
  } catch {
    return []
  }
}

/** 打开系统的文件 / 文件夹选择框；用户取消时返回 null。 */
export async function pickApplicationTarget(
  kind: 'file' | 'folder' = 'file'
): Promise<string | null> {
  if (!isDesktop) return null
  return await invokeHost<string | null>('pick_application_target', { kind })
}

/** 导入结果：新增 / 跳过 / 发现的快捷方式数量，以及导入后的完整配置。 */
export interface ImportShortcutsResult {
  added: number
  skipped: number
  total: number
  config: AppConfig
}

/** 从当前用户桌面与公共桌面导入快捷方式（只读这两个目录）。 */
export async function importDesktopShortcuts(
  categoryId?: string
): Promise<ImportShortcutsResult> {
  if (!isDesktop) {
    return { added: 0, skipped: 0, total: 0, config: { schemaVersion: 1, categories: [], apps: [] } }
  }
  return await invokeHost<ImportShortcutsResult>('import_desktop_shortcuts', {
    categoryId: categoryId ?? null,
  })
}

/** 在资源管理器里定位目标，便于用户确认路径是否还有效。 */
export async function revealApplicationTarget(target: string): Promise<void> {
  if (!isDesktop) return
  await invokeHost('reveal_application_target', { target })
}

/** 目标信息：类型、是否存在、可用的默认名称。 */
export interface TargetDescription {
  kind: 'exe' | 'shortcut' | 'folder' | 'url' | 'other'
  exists: boolean
  suggestedName: string
}

export async function describeTarget(target: string): Promise<TargetDescription | null> {
  if (!isDesktop || !target.trim()) return null
  try {
    return await invokeHost<TargetDescription>('describe_target', { target })
  } catch {
    return null
  }
}

/** 由宿主确认窗口身份，动态窗口不依赖页面初始化时的元数据快照。 */
export async function resolveWindowLabel(): Promise<void> {
  if (isDesktop) windowLabel = await invokeHost<string>("current_window_label")
}
