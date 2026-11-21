<script setup lang="ts">
import { computed, ref } from 'vue'
import type { ApprovalCard, IslandQuestion, QuestionResponse } from './useApprovals'

const props = defineProps<{ questions: IslandQuestion[]; source: ApprovalCard['source']; disabled: boolean }>()
const emit = defineEmits<{ submit: [answers: QuestionResponse[]]; delegate: [] }>()
// 父组件以请求编号作为 key；每张新卡都从空白回答开始。
const drafts = ref(props.questions.map(() => ({ selected: [] as string[], text: '', skipped: false })))
const completed = computed(() => drafts.value.filter(d => d.skipped || d.selected.length || d.text.trim()).length)
const ready = computed(() => completed.value === drafts.value.length)

function choose(index: number, label: string) {
  const d = drafts.value[index]
  d.skipped = false
  if (props.questions[index].multiSelect) {
    d.selected = d.selected.includes(label) ? d.selected.filter(value => value !== label) : [...d.selected, label]
  } else {
    d.selected = [label]
    d.text = ''
  }
}

function write(index: number, event: Event) {
  const d = drafts.value[index]
  d.text = (event.target as HTMLTextAreaElement).value
  d.skipped = false
  if (d.text.trim() && !props.questions[index].multiSelect) d.selected = []
}

function skip(index: number) {
  const d = drafts.value[index]
  d.skipped = !d.skipped
  d.selected = []
  d.text = ''
}

function submit() {
  if (!ready.value || props.disabled) return
  emit('submit', drafts.value.map((d, i) => ({
    id: props.questions[i].id, selected: [...d.selected], ...(d.text.trim() ? { custom: d.text } : {})
  })))
}
</script>

<template>
  <form @submit.prevent="submit">
    <div class="questions scroll-y">
      <fieldset v-for="(q, i) in questions" :key="q.id">
        <legend>
          <small v-if="questions.length > 1 || q.header || q.options.length" class="question-meta">
            <span v-if="questions.length > 1">第 {{ i + 1 }} 题</span><span v-if="q.header">{{ q.header }}</span><span v-if="q.options.length">{{ q.multiSelect ? '多选' : '单选' }}</span>
          </small>
          {{ q.question }}
        </legend>
        <p v-if="q.detail">{{ q.detail }}</p>
        <label v-for="option in q.options" :key="option.label" class="choice">
          <input :type="q.multiSelect ? 'checkbox' : 'radio'" :name="'question-' + i"
            :checked="drafts[i].selected.includes(option.label)" @change="choose(i, option.label)" />
          <span>{{ option.label }}<small v-if="option.description">{{ option.description }}</small></span>
        </label>
        <template v-if="!q.fixed">
          <textarea :value="drafts[i].text" :aria-label="q.question + '，文字回答'"
            :placeholder="q.options.length ? '其他回答…' : '输入回答…'" rows="2" @input="write(i, $event)" />
          <label class="choice skip"><input type="checkbox" :checked="drafts[i].skipped" @change="skip(i)" />跳过此题</label>
        </template>
      </fieldset>
    </div>
    <footer>
      <button type="button" class="delegate" :disabled="disabled" @click="emit('delegate')">回 {{ source }} 回答</button>
      <span v-if="questions.length > 1" class="progress">已处理 {{ completed }}/{{ questions.length }}</span>
      <button type="submit" class="submit" :disabled="disabled || !ready">提交回答</button>
    </footer>
  </form>
</template>

<style scoped>
form { display: grid; gap: 10px; min-width: 0; }
.questions { max-height: 260px; }
fieldset { min-width: 0; margin: 0; border: 0; padding: 2px; }
fieldset + fieldset { margin-top: 14px; }
legend, p, .choice { overflow-wrap: anywhere; white-space: pre-wrap; }
legend { max-width: 100%; padding: 0; line-height: 1.45; font-size: 15px; font-weight: 650; }
.question-meta { display: flex; flex-wrap: wrap; gap: 2px 8px; margin-bottom: 3px; }
small { display: block; font-weight: 400; font-size: 12px; color: var(--text-2); }
p { margin: 4px 0 6px; font-size: 13px; user-select: text; }
.choice { display: flex; align-items: flex-start; gap: 8px; padding: 5px 7px; margin-top: 3px; border: 1px solid transparent; border-radius: var(--r-chip); cursor: pointer; }
.choice:hover { background: var(--hover); }
.choice:has(input:checked) { border-color: var(--accent); background: var(--accent-weak); color: var(--text); }
.choice:has(input:focus-visible) { outline: 2px solid var(--accent); outline-offset: -2px; }
.choice input { flex: none; width: 15px; height: 15px; margin: 3px 0 0; appearance: auto; accent-color: var(--accent); }
.choice span { min-width: 0; }
.skip { font-size: 12px; color: var(--text-2); }
textarea { width: 100%; margin-top: 6px; padding: 7px 9px; border: 1px solid var(--line-strong); border-radius: var(--r-tile); font: inherit; color: inherit; background: var(--surface-2); resize: vertical; user-select: text; }
footer { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; padding-top: 8px; border-top: 1px solid var(--line); }
.progress { font-size: 12px; color: var(--text-2); font-variant-numeric: tabular-nums; }
button { padding: 5px 10px; min-height: 32px; border: 1px solid var(--line-strong); border-radius: var(--r-chip); }
button.delegate { margin-inline-end: auto; color: var(--text-2); border-color: transparent; font-size: 12px; }
button.submit { background: var(--surface-2); font-weight: 650; }
button:hover:enabled { background: var(--hover); }
button:disabled { opacity: .55; cursor: default; }
button:focus-visible, textarea:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
</style>
