import { ref } from 'vue'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'

interface Explanation {
  id: string
  result: string
  draft: string
  question: string
  answer: string
  busy: boolean
  error: string
}

export function useErrorExplanation() {
  const explanation = ref<Explanation | null>(null)

  async function submit() {
    const current = explanation.value
    if (!isDesktop || !current || current.busy || current.answer) return
    const question = current.draft.trim()
    if (current.result && !question) return
    current.busy = true
    current.error = ''
    try {
      const reply = await invokeHost<string>('explain_clipboard_error', {
        id: current.id,
        followUp: current.result ? { explanation: current.result, question } : null,
      })
      if (explanation.value !== current) return
      if (current.result) {
        current.question = question
        current.answer = reply
      } else {
        current.result = reply
      }
    } catch (error) {
      if (explanation.value === current) current.error = String(error)
    } finally {
      current.busy = false
    }
  }

  function start(id: string) {
    if (!isDesktop || explanation.value?.id === id) return
    explanation.value = { id, result: '', draft: '', question: '', answer: '', busy: false, error: '' }
    return submit()
  }

  function close() { explanation.value = null }
  return { explanation, start, submit, close }
}
