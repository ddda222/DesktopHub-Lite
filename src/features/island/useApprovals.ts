import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invokeHost, isDesktop, listenHost } from '@/lib/desktop/desktop'

export interface ApprovalCard {
  id: string
  source: 'Codex' | 'DSH' | 'Orca'
  tool: string
  title: string
  details: string
  context: string
  kind: 'approval' | 'questions' | 'notice'
  questions?: IslandQuestion[]
}

export interface IslandQuestion {
  id: string
  header: string
  question: string
  detail: string
  options: { label: string; description: string }[]
  multiSelect: boolean
  fixed: boolean
}

export interface QuestionResponse { id: string; selected: string[]; custom?: string }
export type ApprovalAnswer = 'allow' | 'deny' | 'delegate' | { answers: QuestionResponse[] }

interface Snapshot {
  revision: number
  count: number
  current: ApprovalCard | null
}

export function useApprovals(report: (message: string) => void) {
  const snapshot = ref<Snapshot>({ revision: -1, count: 0, current: null })
  const busy = ref(false)
  let disposed = false
  let stop: (() => void) | undefined

  async function refresh() {
    const next = await invokeHost<Snapshot>('approval_snapshot')
    if (!disposed && next.revision >= snapshot.value.revision) snapshot.value = next
  }

  onMounted(async () => {
    if (!isDesktop || getCurrentWindow().label !== 'bar') return
    try {
      stop = await listenHost('approvals-changed', () => {
        void refresh().catch(() => report('无法同步待处理请求'))
      })
      if (disposed) { stop(); return }
      await refresh()
    } catch { report('灵动岛审批暂不可用，请在原应用处理') }
  })
  onBeforeUnmount(() => { disposed = true; stop?.() })

  async function answer(id: string, decision: ApprovalAnswer) {
    if (busy.value || id !== snapshot.value.current?.id) return
    busy.value = true
    try {
      await invokeHost('answer_approval', { id, answer: decision })
    } catch (error) {
      report(String(error))
    } finally {
      try { await refresh() } catch { report('无法同步审批结果') }
      busy.value = false
    }
  }

  return { current: computed(() => snapshot.value.current), count: computed(() => snapshot.value.count), busy, answer }
}
