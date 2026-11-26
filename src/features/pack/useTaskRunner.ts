import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invokeHost, isDesktop, listenHost } from '@/lib/desktop/desktop'

export type TaskStage = 'idle' | 'running' | 'done' | 'error' | 'cancelled'

export interface TaskState {
  stage: TaskStage
  scriptId: string
  name: string
  env: 'local' | 'remote'
  kind: 'command' | 'pack'
  message: string
  detail: string
  /** 启动时刻（epoch 毫秒） */
  startedAt: number
  elapsedMs: number
  /** 估算总时长（秒）；null 表示没有历史也没有预估 */
  estimateSec: number | null
  apkPath: string
  cancelable: boolean
}

export const EMPTY_TASK: TaskState = {
  stage: 'idle',
  scriptId: '',
  name: '',
  env: 'local',
  kind: 'command',
  message: '',
  detail: '',
  startedAt: 0,
  elapsedMs: 0,
  estimateSec: null,
  apkPath: '',
  cancelable: false
}

/** 毫秒 → mm:ss */
export function clock(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000))
  return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
}

/**
 * 胶囊中间的实时文案：
 * - 有预估且没超时：任务名 · 约剩 mm:ss
 * - 首次运行（无预估）：任务名 · 已用 mm:ss
 * - 超过预估：收尾中 · 已用 mm:ss（平滑切换，不跳数字）
 */
export function capsuleTaskText(task: TaskState, elapsedMs: number): string {
  const limited = [...task.name].slice(0, 6).join('') + ([...task.name].length > 6 ? '…' : '')
  const name = limited || '任务'
  if (task.stage !== 'running') return name
  if (task.estimateSec && task.estimateSec > 0) {
    const estimateMs = task.estimateSec * 1000
    if (elapsedMs < estimateMs) return `${name} · 约剩 ${clock(estimateMs - elapsedMs)}`
    return `收尾中 · 已用 ${clock(elapsedMs)}`
  }
  return `${name} · 已用 ${clock(elapsedMs)}`
}

/**
 * 任务状态与日志。灵动岛胶囊和打包页共用同一份广播，
 * 所以这里只订阅、不在前端拼状态。
 */
export function useTaskRunner(report: (message: string) => void, options: { withLog?: boolean } = {}) {
  const task = ref<TaskState>({ ...EMPTY_TASK })
  const log = ref<string[]>([])
  const nowMs = ref(Date.now())
  const busy = ref(false)
  let ticker: ReturnType<typeof setInterval> | undefined
  let stopState: (() => void) | undefined
  let stopLog: (() => void) | undefined
  let disposed = false

  function startTicker(): void {
    if (ticker) return
    ticker = setInterval(() => {
      nowMs.value = Date.now()
    }, 1000)
  }

  function stopTicker(): void {
    if (!ticker) return
    clearInterval(ticker)
    ticker = undefined
  }

  watch(
    () => task.value.stage === 'running',
    running => {
      if (running) startTicker()
      else stopTicker()
    },
    { immediate: true }
  )

  async function refresh(): Promise<void> {
    const next = await invokeHost<TaskState>('task_state')
    if (!disposed) task.value = next
    if (options.withLog) {
      const lines = await invokeHost<string[]>('task_log')
      if (!disposed) log.value = lines
    }
  }

  onMounted(async () => {
    if (!isDesktop || getCurrentWindow().label !== 'bar') return
    try {
      stopState = await listenHost<TaskState>('task-state', next => {
        task.value = next
        if (options.withLog && next.stage !== 'running') {
          void invokeHost<string[]>('task_log').then(lines => {
            if (!disposed) log.value = lines
          }).catch(() => {})
        }
      })
      if (options.withLog) {
        stopLog = await listenHost<string | string[]>('task-log', payload => {
          const incoming = Array.isArray(payload) ? payload : [payload]
          if (incoming.length) {
            log.value = [...log.value, ...incoming].slice(-200)
          }
        })
      }
      if (disposed) {
        stopState?.()
        stopLog?.()
        return
      }
      await refresh()
    } catch {
      // 任务状态拿不到时保持空闲，不影响其它功能
    }
  })

  onBeforeUnmount(() => {
    disposed = true
    stopState?.()
    stopLog?.()
    stopTicker()
  })

  const elapsedMs = computed(() =>
    task.value.stage === 'running' && task.value.startedAt
      ? Math.max(0, nowMs.value - task.value.startedAt)
      : task.value.elapsedMs
  )

  async function start(id: string): Promise<boolean> {
    if (task.value.stage === 'running' || busy.value) return false
    busy.value = true
    try {
      await invokeHost('start_pack_task', { id })
      await refresh()
      return true
    } catch (error) {
      report(String(error))
      return false
    } finally {
      busy.value = false
    }
  }

  async function cancel(): Promise<void> {
    try {
      await invokeHost('cancel_pack_task')
    } catch (error) {
      report(String(error))
    }
  }

  return { task, log, elapsedMs, busy, start, cancel, refresh }
}