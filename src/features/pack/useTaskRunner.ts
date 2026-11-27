import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invokeHost, isDesktop, listenHost } from '@/lib/desktop/desktop'
import type { PackScript } from './types'

export type TaskStage = 'idle' | 'running' | 'done' | 'error' | 'cancelled'

export type PackPhaseKey =
  | 'idle'
  | 'prepare'
  | 'compile'
  | 'upload'
  | 'queue'
  | 'cloud'
  | 'download'
  | 'install'
  | 'done'
  | 'error'
  | 'cancelled'

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

export const BROWSER_MOCK_SCRIPTS: PackScript[] = [
  {
    id: 'mock-local-debug',
    name: '本地热更联调',
    env: 'local',
    kind: 'command',
    command: 'pnpm dev:android --reverse 11034',
    scriptPath: '',
    workDir: 'D:\\work\\app-client',
    preCommand: '',
    estimateSec: 8,
    history: [7.5, 8.2]
  },
  {
    id: 'mock-cloud-pack',
    name: '正式云打包',
    env: 'remote',
    kind: 'pack',
    command: '',
    scriptPath: '',
    workDir: 'D:\\work\\app-client',
    preCommand: '',
    estimateSec: 12,
    history: [12, 11.4, 1800]
  }
]

export const PACK_PIPELINE_STEPS = [
  { key: 'compile', label: '编译' },
  { key: 'upload', label: '上传' },
  { key: 'cloud', label: '云打包' },
  { key: 'finish', label: '产出' }
] as const

/** 将原始任务阶段/消息收敛为灵动岛与面板共用的核心动词及流水线进度索引。 */
export function resolvePackPhase(task: TaskState): {
  key: PackPhaseKey
  label: string
  stepIndex: number
} {
  if (task.stage === 'idle') return { key: 'idle', label: '', stepIndex: -1 }
  if (task.stage === 'done') return { key: 'done', label: '完成', stepIndex: 4 }
  if (task.stage === 'error') return { key: 'error', label: '出错', stepIndex: -1 }
  if (task.stage === 'cancelled') return { key: 'cancelled', label: '已取消', stepIndex: -1 }

  const msg = (task.message || '').trim()
  if (task.kind === 'command') {
    if (msg.includes('编译')) return { key: 'compile', label: '在编译', stepIndex: 0 }
    if (msg.includes('同步') || msg.includes('推送')) return { key: 'upload', label: '在同步', stepIndex: 1 }
    if (msg.includes('校验') || msg.includes('检查')) return { key: 'cloud', label: '在校验', stepIndex: 2 }
    return { key: 'compile', label: '在执行', stepIndex: 0 }
  }

  if (msg.includes('安装') || msg.includes('端口') || msg.includes('手机')) {
    return { key: 'install', label: '在安装', stepIndex: 3 }
  }
  if (msg.includes('下载') || msg.includes('回写') || msg.includes('保存')) {
    return { key: 'download', label: '在下载', stepIndex: 3 }
  }
  if (msg === '完成' || msg.includes('打包成功') || msg.includes('打包完成')) {
    return { key: 'download', label: '在收尾', stepIndex: 3 }
  }
  if (msg.includes('云端打包') || msg.includes('在打包') || msg.includes('签名')) {
    return { key: 'cloud', label: '在打包', stepIndex: 2 }
  }
  if (msg.includes('等待') || msg.includes('排队')) {
    return { key: 'queue', label: '在等待', stepIndex: 2 }
  }
  if (msg.includes('上传') || msg.includes('压缩') || msg.includes('提交')) {
    return { key: 'upload', label: '在上传', stepIndex: 1 }
  }
  if (msg.includes('唤醒') || msg.includes('启动') || msg.includes('就绪')) {
    return { key: 'prepare', label: '唤醒引擎', stepIndex: 0 }
  }
  if (msg.includes('准备') || msg.includes('切换') || msg.includes('打开') || msg.includes('校验')) {
    return { key: 'prepare', label: '准备配置', stepIndex: 0 }
  }
  return { key: 'compile', label: '在编译', stepIndex: 0 }
}

/** 清洗单行原始 CLI 日志，过滤虚高的排队分钟预测、ANSI 残留及冗余标签。 */
function cleanRawLogLine(raw: string): string {
  let line = raw
    .replace(/^\[(INFO|LOG|WARN|ERR|ERROR)\]\s*/i, '')
    .replace(/^\d{1,2}:\d{2}:\d{2}\s*/, '')
    .trim()
  if (!line) return ''
  // HBuilderX 队列日志常输出脱离实际的「预计 18 分钟内进入打包状态」，直接剔除以免误导用户
  line = line
    .replace(/[,，、;；]?\s*预计\s*\d+\s*分钟(?:内|后|左右)?进入打包(?:状态|队列)?/g, '')
    .replace(/^[,，、;；\s]+|[,，、;；\s]+$/g, '')
    .trim()
  return line
}

function fileBaseName(filePath: string): string {
  const parts = filePath.split(/[\\/]+/).filter(Boolean)
  return parts[parts.length - 1] ?? filePath
}

/** 提取单行核心摘要：灵动岛与控制台 L2 区共用，绝不倾倒冗长构建刷屏输出。 */
export function extractKeyLogSummary(task: TaskState, logLines: string[]): string {
  if (task.stage === 'error') {
    const errLine = [...logLines].reverse().find(line => /^\[(ERR|ERROR)\]/i.test(line) || /失败|error|exception/i.test(line))
    const cleaned = errLine ? cleanRawLogLine(errLine) : ''
    const summary = task.detail || cleaned || task.message || '执行出错，请展开日志排查'
    return [...summary].length > 76 ? `${[...summary].slice(0, 75).join('')}…` : summary
  }
  if (task.stage === 'done') {
    const file = task.apkPath ? fileBaseName(task.apkPath) : ''
    if (file) return `已生成 ${file}`
    return task.message || (task.kind === 'pack' ? 'APK 已保存至输出目录' : '命令执行成功')
  }
  if (task.stage === 'cancelled') {
    return '任务已手动取消'
  }

  for (let i = logLines.length - 1; i >= 0; i--) {
    const candidate = cleanRawLogLine(logLines[i] ?? '')
    if (!candidate) continue
    // 跳过纯资源块清单与无信息分隔符
    if (/^(chunk|asset|built at|---+|===+)/i.test(candidate)) continue
    return [...candidate].length > 56 ? `${[...candidate].slice(0, 55).join('')}…` : candidate
  }
  return resolvePackPhase(task).label || task.message || '准备执行…'
}

/** 毫秒 → mm:ss */
export function clock(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000))
  return `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(seconds % 60).padStart(2, '0')}`
}

/** 毫秒 → 分钟友好时长（如「1分24秒」「42秒」），适合打包完成胶囊汇报 */
export function formatMinuteDuration(ms: number): string {
  const totalSec = Math.max(1, Math.round(ms / 1000))
  const mins = Math.floor(totalSec / 60)
  const secs = totalSec % 60
  if (mins <= 0) return `${totalSec}秒`
  return secs > 0 ? `${mins}分${secs}秒` : `${mins}分钟`
}

/** 计算当前任务进度（0~1）；无预估且未完成时返回 null。 */
export function calcTaskProgress(task: TaskState, elapsedMs: number): number | null {
  if (task.stage === 'done') return 1
  if (task.stage === 'running') {
    if (!task.estimateSec || task.estimateSec <= 0) return null
    return Math.min(0.99, Math.max(0, elapsedMs / (task.estimateSec * 1000)))
  }
  if (task.estimateSec && task.estimateSec > 0) {
    return Math.min(1, Math.max(0, task.elapsedMs / (task.estimateSec * 1000)))
  }
  return null
}

/**
 * 胶囊中间的实时关键文案（始终优先显示关键动词 + 简洁时间）：
 * - 运行中（prep/唤醒阶段或无预估）：`阶段动词 · mm:ss`
 * - 有正常预估且未超时：`阶段动词 · 约剩 mm:ss`
 * - 超出预估：`阶段动词 · mm:ss`（不输出虚高倒计时）
 */
export function capsuleTaskText(task: TaskState, elapsedMs: number): string {
  const phase = resolvePackPhase(task)
  if (task.stage === 'done') {
    const verb = task.kind === 'pack' ? '打包完成' : '执行完成'
    return `${verb} · ${formatMinuteDuration(task.elapsedMs || elapsedMs)}`
  }
  if (task.stage === 'error') return task.kind === 'pack' ? '打包出错' : '执行出错'
  if (task.stage === 'cancelled') return '已取消'
  if (task.stage !== 'running') return task.name || '任务'

  const label = phase.label || '在执行'
  const showEstimate =
    phase.key !== 'prepare' &&
    !!task.estimateSec &&
    task.estimateSec > 0 &&
    elapsedMs < task.estimateSec * 1000
  if (showEstimate && task.estimateSec) {
    return `${label} · 约剩 ${clock(task.estimateSec * 1000 - elapsedMs)}`
  }
  return `${label} · ${clock(elapsedMs)}`
}

// 共享单例响应式任务状态与日志，保证灵动岛胶囊、副条与打包控制台零延迟同步
const sharedTask = ref<TaskState>({ ...EMPTY_TASK })
const sharedLog = ref<string[]>([])
const sharedNowMs = ref(Date.now())
let sharedTicker: ReturnType<typeof setInterval> | undefined
let listenerCount = 0
let stopSharedState: (() => void) | undefined
let stopSharedLog: (() => void) | undefined
let mockTimers: ReturnType<typeof setTimeout>[] = []

function ensureTicker(running: boolean): void {
  if (running && !sharedTicker) {
    sharedNowMs.value = Date.now()
    sharedTicker = setInterval(() => {
      sharedNowMs.value = Date.now()
    }, 500)
  } else if (!running && sharedTicker) {
    clearInterval(sharedTicker)
    sharedTicker = undefined
  }
}

export function stopBrowserPackSimulation(resetToIdle = false): void {
  for (const t of mockTimers) clearTimeout(t)
  mockTimers = []
  if (resetToIdle) {
    sharedTask.value = { ...EMPTY_TASK }
    sharedLog.value = []
    ensureTicker(false)
  }
}

/** 网页预览专属：模拟一条完整关键状态的打包/命令流程（编译 → 上传 → 等待 → 打包 → 下载/完成或出错）。 */
export function startBrowserPackSimulation(scenario: 'pack' | 'error' | 'command' = 'pack', customName?: string): void {
  stopBrowserPackSimulation(false)
  const startedAt = Date.now()
  const isError = scenario === 'error'
  const isCommand = scenario === 'command'
  const estimateSec = isCommand ? 6 : 9
  const name = customName || (isCommand ? '本地热更联调' : '正式云打包')

  sharedLog.value = [
    `[INFO] ${new Date().toLocaleTimeString()} [打包] Windows 下 HBuilderX CLI 需依附主程序 IPC 通道，已唤醒并最小化至后台`,
    isCommand
      ? '[INFO] 正在编译本地热更资源并检测设备通道…'
      : '[INFO] 正在编译打包资源与 remoteIgnoreFiles 列表'
  ]
  sharedTask.value = {
    stage: 'running',
    scriptId: isCommand ? 'mock-local-debug' : 'mock-cloud-pack',
    name,
    env: isCommand ? 'local' : 'remote',
    kind: isCommand ? 'command' : 'pack',
    message: '在编译',
    detail: '',
    startedAt,
    elapsedMs: 0,
    estimateSec,
    apkPath: '',
    cancelable: true
  }
  ensureTicker(true)

  const pushMockLog = (delayMs: number, fn: () => void) => {
    mockTimers.push(setTimeout(fn, delayMs))
  }

  pushMockLog(1600, () => {
    if (sharedTask.value.stage !== 'running') return
    sharedLog.value = [
      ...sharedLog.value,
      isCommand ? '[INFO] adb reverse tcp:11034 tcp:11034 已连接' : '[INFO] 压缩打包资源并上传至云端编译节点 (2.4 MB)'
    ]
    sharedTask.value = {
      ...sharedTask.value,
      message: isCommand ? '在同步' : '在上传'
    }
  })

  if (!isCommand) {
    pushMockLog(3100, () => {
      if (sharedTask.value.stage !== 'running') return
      sharedLog.value = [
        ...sharedLog.value,
        '[INFO] 已提交云端排队，预计 18 分钟内进入打包状态（队列等待中）'
      ]
      sharedTask.value = {
        ...sharedTask.value,
        message: '在等待'
      }
    })
  }

  pushMockLog(4600, () => {
    if (sharedTask.value.stage !== 'running') return
    sharedLog.value = [
      ...sharedLog.value,
      isCommand ? '[INFO] 设备端热更服务已唤醒，正在校验健康度' : '[INFO] 云端 Gradle assembleRelease 构建签名中 (3/4)'
    ]
    sharedTask.value = {
      ...sharedTask.value,
      message: isCommand ? '在校验' : '在打包'
    }
  })

  if (isError) {
    pushMockLog(6200, () => {
      if (sharedTask.value.stage !== 'running') return
      const elapsedMs = Date.now() - startedAt
      sharedLog.value = [
        ...sharedLog.value,
        '[WARN] 密钥别名 desktophub-release 读取异常',
        '[ERR] Build failed: 证书签名密码校验失败 (Exit 403)'
      ]
      sharedTask.value = {
        ...sharedTask.value,
        stage: 'error',
        elapsedMs,
        message: '出错',
        detail: '证书签名密码校验失败 (Exit 403)，请前往设置核对打包证书。',
        cancelable: false
      }
      ensureTicker(false)
    })
    return
  }

  pushMockLog(6800, () => {
    if (sharedTask.value.stage !== 'running') return
    sharedLog.value = [
      ...sharedLog.value,
      isCommand ? '[INFO] 热更资源同步校验完毕' : '[INFO] 正在下载签名包 app-release-0.1.3.apk 至输出目录'
    ]
    sharedTask.value = {
      ...sharedTask.value,
      message: isCommand ? '在收尾' : '在下载'
    }
  })

  pushMockLog(isCommand ? 7200 : 8800, () => {
    if (sharedTask.value.stage !== 'running') return
    const reportedElapsed = isCommand ? 7000 : 84_000
    sharedLog.value = [
      ...sharedLog.value,
      isCommand
        ? '[INFO] 本地热更联调完成，服务运行正常'
        : '[INFO] 打包成功！产出 D:\\output\\app-release-0.1.3.apk (耗时 1分24秒)'
    ]
    sharedTask.value = {
      ...sharedTask.value,
      stage: 'done',
      elapsedMs: reportedElapsed,
      message: '完成',
      detail: '',
      apkPath: isCommand ? '' : 'D:\\output\\app-release-0.1.3.apk',
      cancelable: false
    }
    ensureTicker(false)
  })
}

async function refreshShared(withLog: boolean): Promise<void> {
  if (!isDesktop) return
  const next = await invokeHost<TaskState | null>('task_state')
  if (!next) return
  sharedTask.value = next
  ensureTicker(next.stage === 'running')
  if (withLog) {
    const lines = await invokeHost<string[] | null>('task_log')
    if (Array.isArray(lines)) sharedLog.value = lines
  }
}

/**
 * 任务状态与日志。灵动岛胶囊和打包页共用同一份状态与实时日志流。
 */
export function useTaskRunner(report: (message: string) => void, options: { withLog?: boolean } = {}) {
  const busy = ref(false)
  let disposed = false

  onMounted(async () => {
    listenerCount++
    if (!isDesktop || getCurrentWindow().label !== 'bar') return
    try {
      if (!stopSharedState) {
        stopSharedState = await listenHost<TaskState>('task-state', next => {
          if (!next) return
          sharedTask.value = next
          ensureTicker(next.stage === 'running')
          if (next.stage !== 'running') {
            void invokeHost<string[] | null>('task_log').then(lines => {
              if (Array.isArray(lines)) sharedLog.value = lines
            }).catch(() => {})
          }
        })
      }
      if (options.withLog && !stopSharedLog) {
        stopSharedLog = await listenHost<string | string[]>('task-log', payload => {
          const incoming = Array.isArray(payload) ? payload : [payload]
          if (incoming.length) {
            sharedLog.value = [...sharedLog.value, ...incoming].slice(-200)
          }
        })
      }
      if (!disposed) await refreshShared(!!options.withLog)
    } catch {
      // 任务状态拿不到时保持空闲，不影响其它功能
    }
  })

  onBeforeUnmount(() => {
    disposed = true
    listenerCount = Math.max(0, listenerCount - 1)
    if (listenerCount === 0) {
      stopSharedState?.()
      stopSharedLog?.()
      stopSharedState = undefined
      stopSharedLog = undefined
      ensureTicker(false)
    }
  })

  watch(
    () => sharedTask.value.stage === 'running',
    running => ensureTicker(running),
    { immediate: true }
  )

  const elapsedMs = computed(() =>
    sharedTask.value.stage === 'running' && sharedTask.value.startedAt
      ? Math.max(0, sharedNowMs.value - sharedTask.value.startedAt)
      : sharedTask.value.elapsedMs
  )

  async function refresh(): Promise<void> {
    await refreshShared(!!options.withLog)
  }

  async function start(id: string): Promise<boolean> {
    if (sharedTask.value.stage === 'running' || busy.value) return false
    if (!isDesktop) {
      const target = BROWSER_MOCK_SCRIPTS.find(item => item.id === id)
      startBrowserPackSimulation(target?.kind === 'command' ? 'command' : 'pack', target?.name)
      return true
    }
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
    if (!isDesktop) {
      const elapsed = sharedTask.value.startedAt ? Math.max(0, Date.now() - sharedTask.value.startedAt) : 0
      stopBrowserPackSimulation(false)
      sharedLog.value = [...sharedLog.value, '[INFO] 用户已手动取消当前任务']
      sharedTask.value = {
        ...sharedTask.value,
        stage: 'cancelled',
        elapsedMs: elapsed,
        message: '已取消',
        cancelable: false
      }
      ensureTicker(false)
      return
    }
    try {
      await invokeHost('cancel_pack_task')
    } catch (error) {
      report(String(error))
    }
  }

  return { task: sharedTask, log: sharedLog, elapsedMs, busy, start, cancel, refresh }
}
