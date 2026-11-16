/**
 * 界面与 Tauri 宿主之间的受限命令契约（对应实施方案第 5 节）。
 *
 * 规则：
 * - 只暴露具体命令，不提供任意 Shell、任意路径读取或任意本地资源访问。
 * - 每个请求带请求编号，响应回带同一编号与可读错误。
 * - 列表只传摘要，全文/原图/缩略图按需读取。
 * - 剪贴板正文、图片、翻译原文与译文、密钥都不进日志。
 *
 * 当前只实现 MockHost（阶段 0 静态样稿）：不触碰系统剪贴板、不启动外部程序、
 * 不发网络请求。Tauri 落地后新增 TauriHost 实现同一接口，界面层无需改动。
 */
import type {
  ClipItem,
  HubApp,
  TranslateDirection,
  TranslationResult,
  TranslationSettings
} from './types'

export interface HostCommandMap {
  list_applications: { input: Record<string, never>; output: HubApp[] }
  open_application: { input: { id: string }; output: null }
  save_application: { input: { app: HubApp }; output: HubApp }
  remove_application: { input: { id: string }; output: null }
  reorder_applications: { input: { ids: string[] }; output: null }

  list_clipboard_entries: { input: { query?: string; group?: string }; output: ClipItem[] }
  get_clipboard_entry: { input: { id: string }; output: ClipItem }
  copy_clipboard_entry: { input: { id: string; formattedText?: string }; output: null }
  delete_clipboard_entry: { input: { id: string }; output: null }
  clear_clipboard_entries: { input: Record<string, never>; output: null }
  set_clipboard_paused: { input: { paused: boolean }; output: null }
  assign_clipboard_group: { input: { id: string; group: string }; output: null }

  translate_text: {
    input: { text: string; direction: TranslateDirection }
    output: TranslationResult
  }
  copy_translation_result: { input: { text: string }; output: null }
  get_translation_settings: { input: Record<string, never>; output: TranslationSettings }
  save_translation_settings: { input: { settings: TranslationSettings }; output: null }
  has_translation_secret: { input: Record<string, never>; output: { present: boolean } }
  save_translation_secret: { input: { secret: string }; output: null }
}

export type HostCommand = keyof HostCommandMap

export type HostErrorCode =
  | 'not_found'
  | 'target_missing'
  | 'readonly'
  | 'busy'
  | 'cancelled'
  | 'timeout'
  | 'no_secret'
  | 'provider_error'
  | 'internal'

export interface HostError {
  code: HostErrorCode
  message: string
}

export interface HostResponse<T = unknown> {
  requestId: string
  ok: boolean
  data?: T
  error?: HostError
}

/** 宿主主动推给界面的事件。页面卸载时取消订阅，不重复注册。 */
export type HostEvent =
  | { type: 'clipboard-changed'; count: number }
  | { type: 'settings-changed' }

export type HostEventListener = (event: HostEvent) => void

export interface Host {
  invoke<C extends HostCommand>(
    command: C,
    input: HostCommandMap[C]['input'],
    requestId: string
  ): Promise<HostResponse<HostCommandMap[C]['output']>>
  subscribe(listener: HostEventListener): () => void
}

export function newRequestId(): string {
  return `req-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

/** 样稿用的模拟故障开关，仅用于让各种失败状态可被观察。 */
export const mockFaults = {
  copyFails: false,
  translateTimesOut: false,
  readOnly: false
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms)
  })
}

export class MockHost implements Host {
  private listeners = new Set<HostEventListener>()

  async invoke<C extends HostCommand>(
    command: C,
    input: HostCommandMap[C]['input'],
    requestId: string
  ): Promise<HostResponse<HostCommandMap[C]['output']>> {
    await delay(command === 'translate_text' ? 900 : 70)

    const fail = (error: HostError) => ({ requestId, ok: false, error }) as HostResponse<
      HostCommandMap[C]['output']
    >
    const done = (data: unknown) =>
      ({ requestId, ok: true, data }) as HostResponse<HostCommandMap[C]['output']>

    switch (command) {
      case 'translate_text':
        if (mockFaults.translateTimesOut) {
          return fail({ code: 'timeout', message: '翻译服务没有在规定时间内返回' })
        }
        return done({
          text:
            (input as { direction: TranslateDirection }).direction === 'en-zh'
              ? '这是样稿里的模拟译文，用来检查排版与状态，不是真实翻译结果。'
              : 'This is placeholder sample output used to review layout and states, not a real translation.',
          provider: '样例服务商',
          model: 'sample-model',
          direction: (input as { direction: TranslateDirection }).direction,
          elapsedMs: 900,
          simulated: true
        })
      case 'copy_clipboard_entry':
        if (mockFaults.copyFails) {
          return fail({ code: 'internal', message: '写入系统剪贴板失败，可重试' })
        }
        return done(null)
      case 'clear_clipboard_entries':
        if (mockFaults.readOnly) {
          return fail({ code: 'readonly', message: '历史目录只读，无法清空' })
        }
        return done(null)
      case 'has_translation_secret':
        return done({ present: true })
      default:
        return done(null)
    }
  }

  subscribe(listener: HostEventListener): () => void {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }
}

export const host: Host = new MockHost()
