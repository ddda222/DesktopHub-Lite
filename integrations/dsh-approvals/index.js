import { spawn } from 'node:child_process'
import { readFileSync, renameSync, writeFileSync } from 'node:fs'
import { isAbsolute } from 'node:path'
import { registerQuestions } from './questions.js'

export const name = 'desktophub-approvals'
export const inject = ['approval', 'tools']

const pointer = new URL('./executable.json', import.meta.url)
const status = new URL('./runtime-status.json', import.meta.url)

function report(registration, stage) {
  // 切换便携版后，旧宿主不能用自己的进度覆盖新登记。状态文件只含登记编号和阶段。
  const temporary = new URL('./runtime-status.' + process.pid + '.tmp', import.meta.url)
  try {
    if (JSON.parse(readFileSync(pointer, 'utf8')).registration !== registration) return
    writeFileSync(temporary, JSON.stringify({ registration, stage }))
    renameSync(temporary, status)
  } catch { /* 状态写入失败不影响人工审批。 */ }
}

function askIsland(executable, payload, signal, channel = '--dsh-approval') {
  return new Promise(resolve => {
    const child = spawn(executable, [channel, String(process.pid)], {
      stdio: ['pipe', 'pipe', 'ignore'], windowsHide: true, signal,
      timeout: 250_000
    })
    let response = ''
    let length = 0
    const limit = channel === '--dsh-question' ? 128 * 1024 + 2 : 64
    child.stdout.setEncoding('utf8')
    child.stdout.on('data', chunk => {
      length += Buffer.byteLength(chunk)
      if (length > limit) child.kill()
      else response += chunk
    })
    child.once('error', () => resolve('unreachable'))
    child.stdin.on('error', () => child.kill())
    child.once('close', code => {
      if (code !== 0 || signal.aborted || length > limit) return resolve('unreachable')
      const value = response.trim()
      if (channel === '--dsh-question') {
        if (value === '"delegate"') return resolve('delegated')
        try {
          const result = JSON.parse(value)
          if (Array.isArray(result?.answers)) return resolve(result)
        } catch { /* 仅接受完整的一批结构化回答。 */ }
        return resolve('unreachable')
      }
      if (value === '"allowed-once"') return resolve('allowed-once')
      if (value === '"rejected"') return resolve('rejected')
      resolve(value === '"delegate"' ? 'delegated' : 'unreachable')
    })
    child.stdin.end(payload, 'utf8')
  })
}

export function apply(ctx) {
  let config
  try { config = JSON.parse(readFileSync(pointer, 'utf8')) }
  catch { return }
  const { path: executable, registration } = config
  if (typeof executable !== 'string' || !isAbsolute(executable)) return

  const lifetime = new AbortController()
  ctx.effect(() => () => {
    lifetime.abort()
    report(registration, 'stopped')
  })
  // DSH 在预执行和实际工具体中都可能询问。会话日志是可选的，执行对象才持有本次参数。
  const calls = new WeakMap()
  ctx.on('tools/pre-execute', (exec, next) => {
    if (exec.agent && exec.callId) {
      if (!calls.has(exec.agent)) calls.set(exec.agent, new Set())
      calls.get(exec.agent).add(exec)
    }
    return next()
  }, { prepend: true })
  ctx.on('tools/result', exec => {
    calls.get(exec.agent)?.delete(exec)
  })
  ctx.on('approval/request', async (request, next) => {
    if (request.signal?.aborted) return 'cancelled'
    const matches = Array.from(calls.get(request.agent) ?? []).filter(call => call.callId === request.callId)
    const [call] = matches
    if (matches.length !== 1 || call.name !== request.toolName || call.arguments === undefined) {
      report(registration, 'unmatched')
      return next()
    }
    const signal = request.signal ? AbortSignal.any([request.signal, lifetime.signal]) : lifetime.signal
    let decision
    try {
      const { session, id } = request.agent
      const payload = JSON.stringify({
        tool: call.name,
        title: request.displayReason?.['zh-CN'] || request.displayReason?.zh || request.reason || ('请求使用 ' + call.name),
        context: [session.header?.cwd, id || session.id].filter(Boolean).join(' · '),
        details: JSON.stringify(call.arguments, null, 2)
      })
      if (Buffer.byteLength(payload) > 128 * 1024) {
        decision = 'oversize'
      } else {
        report(registration, 'forwarding')
        decision = await askIsland(executable, payload, signal)
      }
    } catch { decision = 'unreachable' }
    if (request.signal?.aborted) {
      report(registration, 'cancelled')
      return 'cancelled'
    }
    if (lifetime.signal.aborted) return next()
    if (decision === 'allowed-once' || decision === 'rejected') {
      report(registration, 'answered')
      return decision
    }
    report(registration, decision)
    return next()
  }, { prepend: true })
  registerQuestions(ctx, lifetime.signal,
    (payload, signal) => askIsland(executable, payload, signal, '--dsh-question'),
    stage => report(registration, stage))
  report(registration, 'loaded')
}
