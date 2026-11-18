<script setup lang="ts">
import { computed, ref } from 'vue'
import type { IslandQuestion, QuestionResponse } from './useApprovals'

const props = defineProps<{ questions: IslandQuestion[]; disabled: boolean }>()
const emit = defineEmits<{ submit: [answers: QuestionResponse[]]; delegate: [] }>()
// 父组件以请求编号作为 key；每张新卡都从空白回答开始。
const drafts = ref(props.questions.map(() => ({ selected: [] as string[], text: '', skipped: false })))
const ready = computed(() => drafts.value.every(d => d.skipped || d.selected.length || d.text.trim()))

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
    <div class="questions">
      <fieldset v-for="(q, i) in questions" :key="q.id">
        <legend><small v-if="q.header">{{ q.header }}</small>{{ q.question }}</legend>
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
      <button type="button" :disabled="disabled" @click="emit('delegate')">回 DSH 回答</button>
      <button type="submit" class="submit" :disabled="disabled || !ready">提交回答</button>
    </footer>
  </form>
</template>

<style scoped>
form { display: grid; gap: 10px; min-width: 0; }
.questions { max-height: 260px; overflow-y: auto; overscroll-behavior: contain; }
fieldset { min-width: 0; margin: 0; border: 0; padding: 2px; }
fieldset + fieldset { margin-top: 12px; }
legend, p, .choice { overflow-wrap: anywhere; white-space: pre-wrap; }
legend { font-size: 15px; font-weight: 650; }
small { display: block; font-weight: 400; font-size: 12px; color: var(--text-2); }
p { margin: 4px 0 6px; font-size: 13px; user-select: text; }
.choice { display: flex; align-items: baseline; gap: 6px; padding: 4px 2px; cursor: pointer; }
.choice input { flex: none; width: 15px; height: 15px; appearance: auto; }
.skip { font-size: 12px; color: var(--text-2); }
textarea { box-sizing: border-box; width: 100%; margin-top: 4px; padding: 6px; border: 1px solid var(--line-strong); border-radius: 6px; color: inherit; background: var(--surface-3); resize: vertical; user-select: text; }
footer { display: flex; justify-content: space-between; gap: 8px; }
button { padding: 5px 10px; min-height: 32px; border: 1px solid var(--line-strong); border-radius: 8px; }
button.submit { background: var(--surface-3); font-weight: 650; }
button:disabled { opacity: .55; }
:focus-visible { outline: 2px solid currentColor; outline-offset: -2px; }
</style>
