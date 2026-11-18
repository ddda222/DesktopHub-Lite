<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { invokeHost, isDesktop, listenHost } from '@/lib/desktop/desktop'

type Agent = 'codex' | 'dsh'
interface AgentRow {
  agent: Agent
  state: 'available' | 'registered' | 'repair' | 'disabled' | 'missing' | 'unknown' | 'unsupported'
  detail: string
}
const names: Record<Agent, string> = { codex: 'Codex', dsh: 'DSH（DeepSeek Harness）' }
const rows = ref<AgentRow[]>([])
const connected = ref(new Set<Agent>())
const agents: Agent[] = ['codex', 'dsh']
const entries = computed(() => agents.map(agent => ({ agent, row: rows.value.find(row => row.agent === agent) })))
const reading = ref(false)
const installing = ref<Agent | null>(null)
const error = ref('')

async function detect() {
  if (!isDesktop || reading.value || installing.value) return
  reading.value = true
  error.value = ''
  try { rows.value = await invokeHost<AgentRow[]>('list_approval_agents') }
  catch (e) { error.value = String(e) }
  finally { reading.value = false }
}

async function connect(row: AgentRow) {
  if (installing.value || reading.value) return
  installing.value = row.agent
  error.value = ''
  try {
    const result = await invokeHost<AgentRow>('connect_approval_agent', { agent: row.agent })
    rows.value = rows.value.map(item => item.agent === result.agent ? result : item)
  } catch (e) { error.value = String(e) }
  finally { installing.value = null }
}

let stop: (() => void) | undefined
let disposed = false
onMounted(async () => {
  if (!isDesktop) return
  try {
    stop = await listenHost<Agent>('approval-agent-connected', agent => connected.value.add(agent))
    if (disposed) { stop(); return }
    const history = await invokeHost<Agent[]>('connected_approval_agents')
    for (const agent of history) connected.value.add(agent)
  } catch (e) { if (!disposed) error.value = String(e) }
})
onBeforeUnmount(() => { disposed = true; stop?.() })
</script>

<template>
  <section class="panel" aria-label="智能体连接">
    <h3 class="panel-title">智能体连接</h3>
    <p class="hint">灵动岛显示 Codex 和 DSH 的工具审批，也会提醒你回答 DSH 的提问。</p>
    <div class="set-row">
      <span class="set-label">本机智能体</span>
      <button class="btn" type="button" :disabled="reading || !!installing || !isDesktop" @click="detect">{{ rows.length ? '重新检测' : '检测本机智能体' }}</button>
    </div>
    <p v-if="!isDesktop" class="hint">请在 Windows 桌面版中检测本机应用。</p>
    <p v-else-if="reading" class="hint" role="status">正在检测本机应用…</p>
    <p v-if="error" class="inline-feedback error" role="alert">{{ error }}</p>
    <div v-for="{ agent, row } in entries" :key="agent" class="set-row">
      <div class="set-text">
        <strong class="set-label">{{ names[agent] }}</strong>
        <span v-if="connected.has(agent)" class="tag tag-ok" role="status">已连接</span>
        <span v-if="installing === agent" class="set-desc" role="status">正在接入…</span>
        <span v-else-if="row && (!connected.has(agent) || row.state !== 'registered' || agent === 'dsh')" class="set-desc" role="status">{{ row.detail }}</span>
        <span v-else-if="!connected.has(agent)" class="set-desc">尚未检测</span>
      </div>
      <button v-if="row && (row.state === 'available' || row.state === 'repair')" class="btn" type="button"
        :disabled="reading || !!installing" :aria-label="(row.state === 'repair' ? '更新接入：' : '添加钩子：') + names[agent]"
        @click="connect(row)">{{ row.state === 'repair' ? '更新接入' : '添加钩子' }}</button>
    </div>
  </section>
</template>

<style scoped>
.set-text .tag { align-self: flex-start; }
</style>
