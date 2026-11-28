<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import Icon from '@/components/Icon.vue'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import { gsap, vGsapPress } from '@/lib/gsapMotion'
import { activeMotionPreset } from '@/lib/motionPresets'
import { state } from '@/store'
import QuestionForm from './QuestionForm.vue'
import type { ApprovalAnswer, ApprovalCard } from './useApprovals'

const props = defineProps<{ card: ApprovalCard; count: number; busy: boolean }>()
const emit = defineEmits<{ answer: [id: string, decision: ApprovalAnswer] }>()
const rootEl = ref<HTMLElement | null>(null)
const details = computed(() => {
  try {
    const args = JSON.parse(props.card.details)
    if (args && typeof args === 'object' && !Array.isArray(args)) {
      const field = ['command', 'cmd', 'patch'].find(key => typeof args[key] === 'string' && args[key].length)
      if (field) {
        const { [field]: operation, ...options } = args
        if (options.description === props.card.title) delete options.description
        return Object.keys(options).length ? operation + '\n\n' + JSON.stringify(options, null, 2) : operation
      }
    }
  } catch { /* 原应用也可以提供非 JSON 的操作正文。 */ }
  return props.card.details
})
// Vue 以请求编号重新挂载卡片，隔开连点的第二次点击和下一项授权。
const accepting = ref(false)
let guard: ReturnType<typeof setTimeout> | undefined
onMounted(() => {
  guard = setTimeout(() => { accepting.value = true }, 500)
  if (rootEl.value && !state.ui.islandReducedMotion) {
    const spec = activeMotionPreset.value.popTransition
    gsap.fromTo(
      rootEl.value,
      { opacity: 0, y: 6, scale: 0.985 },
      { opacity: 1, y: 0, scale: 1, duration: spec.duration, ease: spec.ease, clearProps: 'transform' }
    )
  }
})
onBeforeUnmount(() => clearTimeout(guard))
function submit(decision: ApprovalAnswer) {
  if (accepting.value && !props.busy) emit('answer', props.card.id, decision)
}
async function openOrcaAndDismiss() {
  if (!accepting.value || props.busy) return
  if (isDesktop) {
    try { await invokeHost('focus_orca_window') } catch { /* 切窗失败仍关闭提醒卡 */ }
  }
  emit('answer', props.card.id, 'delegate')
}
</script>

<template>
  <section
    ref="rootEl"
    class="approval"
    :aria-label="card.title"
    :aria-busy="busy"
    @keydown.enter="event => { if (event.repeat) event.preventDefault() }"
  >
    <header><strong>{{ card.source }}</strong><span>{{ card.tool }}</span><small v-if="count > 1">共 {{ count }} 项待处理</small></header>
    <div v-if="card.kind !== 'questions' || card.questions?.length !== 1 || card.context" class="request-summary scroll-y">
      <h2 v-if="card.kind !== 'questions' || card.questions?.length !== 1">{{ card.title }}</h2>
      <p v-if="card.context" class="context">{{ card.context }}</p>
    </div>
    <QuestionForm v-if="card.kind === 'questions' && card.questions" :questions="card.questions" :source="card.source" :disabled="busy || !accepting"
      @submit="values => submit({ answers: values })" @delegate="submit('delegate')" />
    <div v-else-if="card.kind === 'notice'" class="approval-body notice scroll-y" role="region" tabindex="0" aria-label="提问内容">{{ card.details }}</div>
    <pre v-else class="approval-body scroll-y" tabindex="0" aria-label="本次操作内容">{{ details }}</pre>
    <footer v-if="card.kind === 'notice'">
      <span class="context">{{ card.source === 'Orca' ? '可在 Orca 窗口中继续处理' : `可在 ${card.source} 对话中回答` }}</span>
      <button v-gsap-press="busy || !accepting || state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap" type="button" :disabled="busy || !accepting" @click="submit('delegate')">知道了</button>
      <button v-if="card.source === 'Orca'" v-gsap-press="busy || !accepting || state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap" type="button" class="allow" :disabled="busy || !accepting" @click="openOrcaAndDismiss"><Icon name="external" :size="14" />切回 Orca</button>
    </footer>
    <footer v-else-if="card.kind === 'approval'">
      <button v-gsap-press="busy || !accepting || state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap" type="button" class="delegate" :disabled="busy || !accepting" @click="submit('delegate')">回原应用审批</button>
      <button v-gsap-press="busy || !accepting || state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap" type="button" class="deny" :disabled="busy || !accepting" @click="submit('deny')"><Icon name="close" :size="15" />拒绝本次</button>
      <button v-gsap-press="busy || !accepting || state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap" type="button" class="allow" :disabled="busy || !accepting" @click="submit('allow')"><Icon name="check" :size="15" />允许本次</button>
    </footer>
  </section>
</template>

<style scoped>
.approval { display: grid; grid-template-columns: minmax(0, 1fr); gap: 8px; color: var(--text); }
header, footer, button { display: flex; align-items: center; gap: 6px; }
header { flex-wrap: wrap; gap: 3px 6px; font-size: 12px; overflow-wrap: anywhere; }
header strong { font-weight: 600; color: var(--text); }
header span { min-width: 0; color: var(--text-3); }
header small { margin-inline-start: auto; flex: none; color: var(--text-3); font-variant-numeric: tabular-nums; }
.request-summary { max-height: 98px; overflow-wrap: anywhere; user-select: text; }
h2 { margin: 0; font-size: 15px; line-height: 1.4; font-weight: 600; }
.context { margin: 0; font-size: 11.5px; color: var(--text-2); white-space: pre-wrap; }
h2 + .context { margin-top: 4px; }
.approval-body { max-height: 210px; margin: 0; padding: 8px 10px; border: 1px solid var(--line); border-radius: var(--r-tile); background: var(--surface-2); white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
pre { font: 11.5px/1.6 var(--font-mono); tab-size: 4; }
.notice { font-size: 13.5px; line-height: 1.6; }
footer { justify-content: flex-end; flex-wrap: wrap; padding-top: 8px; border-top: 1px solid var(--line); }
button { justify-content: center; min-height: 30px; padding: 4px 10px; border: 1px solid var(--line-strong); border-radius: var(--r-chip); font-size: 12px; transition: background 120ms ease, border-color 120ms ease, color 120ms ease, transform 80ms ease; }
button:hover:enabled { background: var(--hover); }
button:active:enabled { transform: scale(0.97); }
button:focus-visible, .scroll-y:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
.allow { background: var(--accent); border-color: var(--accent); color: var(--accent-text); font-weight: 600; }
.allow:hover:enabled { background: var(--accent-hover); border-color: var(--accent-hover); }
.deny { color: var(--danger); border-color: color-mix(in srgb, var(--danger) 40%, var(--line)); background: transparent; }
.deny:hover:enabled { background: var(--danger-weak); border-color: var(--danger); }
.delegate { margin-inline-end: auto; border-color: transparent; color: var(--text-3); font-size: 11.5px; padding-inline: 6px; }
.delegate:hover:enabled { color: var(--text); }
button:disabled { opacity: .5; cursor: default; }
</style>
