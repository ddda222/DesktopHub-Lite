import type { ThemeMode } from '@/lib/themes'
export type { ThemeMode }

export interface Category {
  id: string
  name: string
}

export interface HubApp {
  id: string
  name: string
  /** 原目标：exe / 目录 / 网址 / .lnk，样稿只用于展示 */
  target: string
  categoryId: string
  /** 合成图标底色，真实版本使用 Windows 图标读取与缓存 */
  color: string
  initials: string
  hidden: boolean
  /** 目标失效状态 */
  broken?: boolean
}

export type ClipKind = 'text' | 'image'

export interface ClipItem {
  id: string
  kind: ClipKind
  text?: string
  imageUrl?: string
  imageMissing?: boolean
  favorite?: boolean
  note?: string
  group: string
  copiedAt: number
  width?: number
  height?: number
}

export type TranslateDirection = 'auto' | 'zh-en' | 'en-zh'

export interface TranslationResult {
  text: string
  provider: string
  model: string
  direction: TranslateDirection
  elapsedMs: number
  /** 样稿模拟结果：界面必须明确标出，不能当成真实翻译 */
  simulated?: boolean
}

export interface TranslationSettings {
  providerId: string
  model: string
  baseUrl: string
  direction: TranslateDirection
  hasSecret: boolean
}

export interface TranslationProvider {
  id: string
  name: string
  baseUrl: string
  model: string
  note?: string
}

export interface HubSettings {
  theme: ThemeMode
  recordClipboard: boolean
  /** 待确认项：是否继续提供登录自启 */
  launchAtLogin: boolean
  clipboardHotkey: string
}

export type ToastKind = 'info' | 'success' | 'error'

export interface Toast {
  id: number
  text: string
  kind: ToastKind
}

/** Rust 侧 applications.json 的结构，和 Rust 的 AppConfig 一一对应。 */
export interface AppConfig {
  schemaVersion: number
  categories: Category[]
  apps: HubApp[]
}

/** Rust 侧 clipboard-index.json 的一条记录，和 Rust 的 ClipboardEntry 一一对应。 */
export interface ClipboardRecord {
  id: string
  kind: ClipKind
  text?: string | null
  imageFile?: string | null
  favorite?: boolean
  note?: string
  group: string
  copiedAt: number
  width?: number | null
  height?: number | null
}

export interface ShortcutSettings {
  bar: string
  clipboard: string
  translation: string
  errors: string[]
}

/** Rust 侧 translation.json 的结构。 */
export interface TranslationSettingsRecord {
  providerId: string
  model: string
  baseUrl: string
  direction: TranslateDirection
  hasSecret: boolean
}

/** Rust 侧 snippets.json 的单条常用短语结构（列表视图不含密码明文/密文）。 */
export type SnippetKind = 'text' | 'credential'
export type SnippetCopyField = 'account' | 'password'

export interface SnippetItem {
  id: string
  kind?: SnippetKind
  title: string
  content: string
  account?: string
  hasPassword?: boolean
  group: string
  pinned: boolean
  updatedAt: number
}

export interface SnippetPayload {
  groups: string[]
  entries: SnippetItem[]
}

export interface SnippetDraft {
  id?: string
  kind?: SnippetKind
  title: string
  content: string
  account?: string
  password?: string
  group: string
  pinned: boolean
}

export interface AudioDevice {
  id: string
  name: string
}

export interface AudioSession {
  id: string
  name: string
  deviceId: string
  deviceName: string
  appKey: string | null
  pid: number
  volume: number
  muted: boolean
  active: boolean
}

export interface AudioSnapshot {
  devices: AudioDevice[]
  sessions: AudioSession[]
  warnings: string[]
}

/** 各映射只包含手动隐藏的项，保留名称以便离线时恢复。 */
export interface MixerVisibility {
  devices: Record<string, string>
  apps: Record<string, string>
}
