const maxBytes = 128 * 1024

function context(session, id) {
  return [session?.header?.cwd, id || session?.id].filter(Boolean).join(' · ')
}

function questionItem(item) {
  const intent = item.intent
  const fixed = intent?.kind === 'approval'
  const options = (item.options ?? []).map(option => {
    let description = option.description ?? ''
    if (fixed && option.label === intent.always) description = ['持续授权', description].filter(Boolean).join(' · ')
    return { label: option.label, description }
  })
  return {
    id: item.id, header: item.header ?? '', question: item.question,
    detail: [intent?.subject?.name, item.detail].filter(Boolean).join('\n'),
    options, fixed, multiSelect: item.multiSelect === true
  }
}

function canAnswer(items, originals) {
  const ids = new Set()
  return items.length > 0 && items.every((q, i) => {
    if (!q.id?.trim() || ids.has(q.id) || !q.question?.trim()) return false
    ids.add(q.id)
    const labels = q.options.map(option => option.label)
    if (labels.some(label => !label?.trim()) || new Set(labels).size !== labels.length) return false
    // 需看完整范围或计划才能作决定。主题名称不能替代原始说明。
    return !originals[i].intent || (originals[i].detail?.trim() && labels.length > 0)
  })
}

function noticePayload(title, details, location) {
  // 过长的普通消息在 DSH 保留全文，提醒只传与提问有关的内容。
  const excerpt = details.length > 12000 ? details.slice(0, 6000) + '\n…\n' + details.slice(-6000) : details
  return JSON.stringify({ kind: 'notice', title, details: excerpt, context: location })
}

function asksForReply(text) {
  // 链接、引用、代码和推理内容中的问号不触发对话提醒。
  const prose = text.replace(/^`{3,}[^\n]*\n[\s\S]*?(?:^`{3,}[^\n]*$|(?![\s\S]))/gm, '')
    .replace(/^~{3,}[^\n]*\n[\s\S]*?(?:^~{3,}[^\n]*$|(?![\s\S]))/gm, '')
    .replace(/^\s*>[^\n]*$/gm, '').replace(/`[^`\n]*`|https?:\/\/[^\s)）]+/gi, '')
  const directQuestion = /[？?]|(?:吗|么|呢)[。！!]?\s*$|(?:你|您)(?:希望|想要|更倾向|是否|能否)/
  const requestForInput = /请(?:问|(?:你|您)?(?:确认|选择|告诉我|提供|回复|决定|补充|说明|告知))|(?:需要|想请)(?:你|您)(?:确认|选择|提供|回复|决定|补充)|\b(?:please (?:confirm|choose|select|specify|provide|tell me)|let me know (?:which|whether|if))\b/i
  return directQuestion.test(prose) || requestForInput.test(prose)
}

export function registerQuestions(ctx, lifetime, send, report) {
  ctx.on('user-questions/request', async (request, next) => {
    request.signal?.throwIfAborted()
    const signal = request.signal ? AbortSignal.any([lifetime, request.signal]) : lifetime
    const items = request.questions.map(questionItem)
    const location = context(request.agent?.session, request.agent?.id)
    const title = items.length === 1 ? items[0].question : `DSH 有 ${items.length} 个问题`
    const payload = JSON.stringify({ source: 'DSH', kind: 'questions', tool: '提问', title,
      context: location, details: '', questions: items })
    if (Buffer.byteLength(payload) > maxBytes || !canAnswer(items, request.questions)) {
      // 完整问题由宿主处理，岛上仍然提示。看到摘要不能授予权限。
      const closed = new AbortController()
      const preview = items.map(q => q.question).join('\n')
      void send(noticePayload('请在 DSH 查看完整问题', preview, location), AbortSignal.any([signal, closed.signal])).catch(() => {})
      try { return await next() } finally { closed.abort() }
    }
    report('question-forwarding')
    const result = await send(payload, signal).catch(() => 'unreachable')
    // 定时提问使用 DSH 的原始截止时间；撤回、超时和用户跳过是三种不同结果。
    request.signal?.throwIfAborted()
    if (!lifetime.aborted && Array.isArray(result?.answers)) {
      report('question-answered')
      return { answers: result.answers }
    }
    if (!lifetime.aborted) report('question-delegated')
    return next()
  }, { prepend: true })

  const messages = new WeakMap()
  const notices = new WeakMap()
  function clear(session) {
    notices.get(session)?.abort()
    notices.delete(session)
    messages.delete(session)
  }
  ctx.on('session/disposed', clear)
  ctx.on('session/event', (session, event) => {
    if (session.header?.origin === 'subagent') return
    if (event.type === 'turn/start' || event.type === 'user/message') {
      clear(session)
    } else if (event.type === 'assistant/message') {
      messages.delete(session)
      const blocks = event.data.message.content
      if (event.data.interrupted || blocks.some(block => block.type === 'tool-call')) return
      const text = blocks.filter(block => block.type === 'text').map(block => block.text).join('\n').trim()
      if (text && asksForReply(text)) messages.set(session, { turn: event.data.turn, text })
    } else if (event.type === 'turn/end') {
      const pending = messages.get(session)
      messages.delete(session)
      if (!pending || pending.turn !== event.data.turn || event.data.reason.kind !== 'completed') return
      const closed = new AbortController()
      notices.set(session, closed)
      const signal = AbortSignal.any([lifetime, closed.signal])
      const payload = noticePayload('DSH 在对话中向你提问', pending.text, context(session))
      void send(payload, signal).catch(() => {}).finally(() => {
        if (notices.get(session) === closed) notices.delete(session)
      })
    }
  })
}
