import { onBeforeUnmount, onMounted, shallowRef } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invokeHost, isDesktop, listenHost } from '@/lib/desktop/desktop'

export type MailProvider = 'qq' | 'gmail'
export const providerName = { qq: 'QQ 邮箱', gmail: 'Gmail' }
export interface MailAccount { provider: MailProvider; address: string; status: string }
export interface MailNotice {
  id: string
  provider: MailProvider
  address: string
  sender: string
  subject: string
  code: string | null
  links: { label: string; domain: string }[]
}

// 事件只传变更信号。窗口加载或重新打开时从宿主取快照，丢弃过期的异步响应。
export function useMailList<T>(label: 'bar' | 'settings', command: string, event: string, report: (message: string) => void) {
  const rows = shallowRef<T[]>([])
  let sequence = 0
  let disposed = false
  let stop: (() => void) | undefined
  async function refresh() {
    const request = ++sequence
    try {
      const result = await invokeHost<T[]>(command)
      if (!disposed && request === sequence) rows.value = result
    } catch (error) {
      if (!disposed && request === sequence) report(String(error))
    }
  }
  onMounted(async () => {
    if (!isDesktop || getCurrentWindow().label !== label) return
    try {
      stop = await listenHost(event, () => { void refresh() })
      if (disposed) { stop(); return }
      await refresh()
    } catch (error) { if (!disposed) report(String(error)) }
  })
  onBeforeUnmount(() => { disposed = true; stop?.() })
  return rows
}
