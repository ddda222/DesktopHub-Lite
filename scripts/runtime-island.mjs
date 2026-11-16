import fs from 'node:fs/promises'

const sleep = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds))
const targets = await fetch('http://127.0.0.1:9346/json/list').then(response => response.json())
let connection
async function connect(target) {
  const socket = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject })
  let sequence = 0
  const pending = new Map()
  socket.onmessage = event => {
    const message = JSON.parse(event.data)
    const request = pending.get(message.id)
    if (!request) return
    pending.delete(message.id)
    clearTimeout(request.timer)
    if (message.error || message.result.exceptionDetails) request.reject(new Error(JSON.stringify(message)))
    else request.resolve(message.result.result.value)
  }
  return {
    socket,
    evaluate(expression) {
      return new Promise((resolve, reject) => {
        const id = ++sequence
        const timer = setTimeout(() => { pending.delete(id); reject(new Error('调用超时')) }, 8000)
        pending.set(id, { resolve, reject, timer })
        socket.send(JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } }))
      })
    }
  }
}
for (const target of targets.filter(item => item.type === 'page')) {
  const candidate = await connect(target)
  if (await candidate.evaluate(`!!document.querySelector('.island')`)) { connection = candidate; break }
  candidate.socket.close()
}
if (!connection) throw new Error('灵动岛组件未加载')
const invoke = (command, args = {}) => connection.evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`)
const dimensions = () => connection.evaluate(`(()=>{const element=document.querySelector('.island');const rect=element.getBoundingClientRect();return {width:Math.round(rect.width),height:Math.round(rect.height),expanded:element.classList.contains('expanded'),applications:document.querySelectorAll('.app-tile').length}})()`)
const result = []
try {
  await invoke('set_island_expanded', { expanded: false }); await sleep(550)
  const compact = await dimensions()
  if (compact.width !== 72 || compact.height !== 5) throw new Error('收起尺寸异常 ' + JSON.stringify(compact))
  result.push({ scenario: '细线收起', ...compact })
  await connection.evaluate(`document.querySelector('.island-hit-area').dispatchEvent(new MouseEvent('mouseenter'))`)
  await sleep(1000)
  const opened = await dimensions()
  if (!opened.expanded || opened.height !== 440 || opened.applications === 0) throw new Error('悬停展开异常 ' + JSON.stringify(opened))
  result.push({ scenario: '320ms 悬停展开与应用网格', ...opened })
  for (const panel of ['clipboard', 'translation']) {
    await invoke('toggle_panel', { panel }); await sleep(500)
    const label = panel === 'clipboard' ? '剪贴板历史' : '翻译面板'
    if (!await connection.evaluate(`!!document.querySelector('[aria-label="${label}"]')`)) throw new Error(panel + ' 未渲染')
    result.push({ scenario: panel, passed: true })
  }
  for (const anchor of ['left', 'right', 'center']) {
    await invoke('set_island_anchor', { anchor }); await sleep(150)
    result.push({ scenario: '停靠 ' + anchor, windows: await invoke('selftest_windows') })
  }
  await invoke('set_island_expanded', { expanded: false }); await sleep(500)
  if ((await dimensions()).height !== 5) throw new Error('收起未恢复')
  await fs.writeFile('D:/codex/DesktopHub-work/island-runtime-20261116/report.json', JSON.stringify(result, null, 2))
  console.log(JSON.stringify(result, null, 2))
} finally { connection.socket.close() }
