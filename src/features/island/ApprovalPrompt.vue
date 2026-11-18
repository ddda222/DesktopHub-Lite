<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import Icon from '@/components/Icon.vue'
import QuestionForm from './QuestionForm.vue'
import type { ApprovalAnswer, ApprovalCard } from './useApprovals'

const props = defineProps<{ card: ApprovalCard; count: number; busy: boolean }>()
const emit = defineEmits<{ answer: [id: string, decision: ApprovalAnswer] }>()
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
onMounted(() => { guard = setTimeout(() => { accepting.value = true }, 500) })
onBeforeUnmount(() => clearTimeout(guard))
function submit(decision: ApprovalAnswer) {
  if (accepting.value && !props.busy) emit('answer', props.card.id, decision)
}
</script>

<template>
  <section class="approval" :aria-label="card.title" @keydown.enter="event => { if (event.repeat) event.preventDefault() }">
    <header><strong>{{ card.source }}</strong><span>{{ card.tool }}</span><small v-if="count > 1">共 {{ count }} 项待处理</small></header>
    <template v-if="card.kind === 'questions' && card.questions">
      <h2 v-if="card.questions.length > 1" id="approval-title">{{ card.title }}</h2>
      <p v-if="card.context" class="context">{{ card.context }}</p>
      <QuestionForm :questions="card.questions" :disabled="busy || !accepting"
        @submit="values => submit({ answers: values })" @delegate="submit('delegate')" />
    </template>
    <div v-else class="approval-body">
      <h2 id="approval-title">{{ card.title }}</h2>
      <p v-if="card.context" class="context">{{ card.context }}</p>
      <pre tabindex="0" :aria-label="card.kind === 'notice' ? '提问内容' : '本次操作内容'">{{ card.kind === 'notice' ? card.details : details }}</pre>
    </div>
    <footer v-if="card.kind === 'notice'">
      <span class="context">可在 DSH 对话中回答</span>
      <button type="button" :disabled="busy || !accepting" @click="submit('delegate')">知道了</button>
    </footer>
    <footer v-else-if="card.kind === 'approval'">
      <button type="button" class="delegate" :disabled="busy || !accepting" @click="submit('delegate')">回原应用审批</button>
      <button type="button" :disabled="busy || !accepting" @click="submit('deny')"><Icon name="close" :size="16" />拒绝本次</button>
      <button type="button" class="allow" :disabled="busy || !accepting" @click="submit('allow')"><Icon name="check" :size="16" />允许本次</button>
    </footer>
  </section>
</template>

<style scoped>
.approval { display: grid; gap: 10px; color: var(--text); }
header, footer, button { display: flex; align-items: center; gap: 6px; }
header { font-size: 12px; overflow-wrap: anywhere; }
header span, header small, .context, .delegate { color: var(--text-2); }
header small { margin-inline-start: auto; flex: none; }
.approval-body { max-height: 216px; overflow-y: auto; overflow-wrap: anywhere; }
h2 { margin: 0; font-size: 16px; line-height: 1.45; }
.context { margin: 5px 0; font-size: 12px; }
pre { margin: 8px 0 0; padding: 8px 10px; background: var(--surface-3); border-radius: 6px; white-space: pre-wrap; overflow-wrap: anywhere; font: 12px/1.5 Consolas, monospace; user-select: text; }
footer { justify-content: flex-end; flex-wrap: wrap; }
button { justify-content: center; min-height: 32px; padding: 4px 9px; border: 1px solid var(--line-strong); border-radius: 8px; }
button:hover:enabled { background: var(--hover); }
button:focus-visible, pre:focus-visible { outline: 2px solid currentColor; outline-offset: -2px; }
.allow { background: var(--surface-3); font-weight: 650; }
.delegate { margin-inline-end: auto; border-color: transparent; font-size: 12px; }
button:disabled { opacity: .55; }
</style>
