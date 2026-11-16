import fs from 'node:fs/promises'
import http from 'node:http'
import { execFileSync } from 'node:child_process'

const report = []
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const targets = await fetch('http://127.0.0.1:9346/json/list').then(response => response.json())
const page = targets.find(target => target.type === 'page')
if (!page) throw new Error('没有可连接的界面')
const socket = new WebSocket(page.webSocketDebuggerUrl)
await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject })
let sequence = 0
const pending = new Map()
socket.onmessage = event => {
  const result = JSON.parse(event.data)
  const entry = pending.get(result.id)
  if (!entry) return
  pending.delete(result.id)
  clearTimeout(entry.timer)
  result.error ? entry.reject(new Error(JSON.stringify(result.error))) : entry.resolve(result.result)
}
function call(method, params) {
  return new Promise((resolve, reject) => {
    const id = ++sequence
    const timer = setTimeout(() => { pending.delete(id); reject(new Error('调试调用超时')) }, 12000)
    pending.set(id, { resolve, reject, timer })
    socket.send(JSON.stringify({ id, method, params }))
  })
}
async function evaluate(expression) {
  const result = await call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true })
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text + ' ' + JSON.stringify(result.exceptionDetails.exception))
  return result.result.value
}
const invoke = (command, args = {}) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`)
async function scenario(name, action) {
  try { const detail = await action(); report.push({ name, passed: true, detail }); console.log('通过 ' + name) }
  catch (error) { report.push({ name, passed: false, error: String(error) }); console.log('失败 ' + name + ' ' + error) }
}
function assert(condition, message) { if (!condition) throw new Error(message) }
await scenario('内嵌界面与宿主通信', async () => {
  const loaded = await evaluate(`({url:location.href,bridge:!!window.__TAURI_INTERNALS__,text:document.body.innerText.slice(0,300)})`)
  assert(loaded.bridge && !loaded.url.startsWith('file:'), JSON.stringify(loaded))
  return { loaded, info: await invoke('app_info') }
})
if (!await evaluate('!!window.__TAURI_INTERNALS__')) { socket.close(); throw new Error('宿主桥未加载') }
await scenario('窗口显示与关闭', async () => {
  for (const panel of ['clipboard', 'translation', 'category', 'overflow']) {
    await invoke('toggle_panel', { panel }); await sleep(950)
    assert((await invoke('ui_state')).panel === panel, panel + ' 未打开')
    await invoke('close_panel')
  }
  await invoke('open_settings'); await sleep(400)
  assert((await invoke('ui_state')).settingsOpen, '设置未打开')
  await invoke('close_settings')
  return await invoke('selftest_windows')
})
await scenario('主题保存与快捷键状态', async () => {
  await invoke('set_theme', { theme: 'light' })
  assert((await invoke('ui_state')).theme === 'light', '主题未生效')
  await invoke('set_theme', { theme: 'dark' })
  const settings = await invoke('get_shortcuts')
  const saved = await invoke('save_shortcuts', { settings })
  assert(Array.isArray(saved.errors), '快捷键冲突状态缺失')
  return saved
})
await scenario('文字捕获、去重、分组及复制', async () => {
  execFileSync('pwsh', ['-NoProfile','-Command', "Set-Clipboard -Value 'DesktopHub runtime sample 20261116'"])
  await sleep(1300)
  let entries = await invoke('list_clipboard_entries')
  const entry = entries.find(item => item.text === 'DesktopHub runtime sample 20261116')
  assert(entry, '文字未捕获')
  await invoke('create_clipboard_group', { group: '运行测试' })
  await invoke('assign_clipboard_group', { id: entry.id, group: '运行测试' })
  await invoke('copy_clipboard_entry', { id: entry.id }); await sleep(700)
  entries = await invoke('list_clipboard_entries')
  assert(entries.filter(item => item.id === entry.id).length === 1, '去重异常')
  assert(entries.find(item => item.id === entry.id).group === '运行测试', '分组未保存')
  return { count: entries.length }
})
await scenario('暂停记录与恢复', async () => {
  await invoke('set_clipboard_paused', { paused: true })
  execFileSync('pwsh', ['-NoProfile','-Command', "Set-Clipboard -Value 'DesktopHub paused sample'"])
  await sleep(1000)
  assert(!(await invoke('list_clipboard_entries')).some(item => item.text === 'DesktopHub paused sample'), '暂停期间仍捕获')
  await invoke('set_clipboard_paused', { paused: false })
})
await scenario('图片捕获、缩略图及写回复制', async () => {
  execFileSync('pwsh', ['-STA','-NoProfile','-Command', "Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $bitmap = [System.Drawing.Bitmap]::new(96,64); $graphics = [System.Drawing.Graphics]::FromImage($bitmap); $graphics.Clear([System.Drawing.Color]::CornflowerBlue); [System.Windows.Forms.Clipboard]::SetImage($bitmap); $graphics.Dispose(); $bitmap.Dispose()"])
  await sleep(1500)
  const entry = (await invoke('list_clipboard_entries')).find(item => item.kind === 'image')
  assert(entry && entry.width === 96 && entry.height === 64, '图片未捕获')
  const image = await invoke('read_clipboard_thumbnail', { id: entry.id })
  assert(image.startsWith('data:image/png;base64,'), '缩略图无效')
  await invoke('copy_clipboard_entry', { id: entry.id })
  return { width: entry.width, height: entry.height }
})
const server = http.createServer((request, response) => {
  let body = ''
  request.on('data', data => { body += data })
  request.on('end', () => {
    const payload = JSON.parse(body)
    response.writeHead(200, { 'Content-Type': 'application/json' })
    response.end(JSON.stringify({ choices: [{ finish_reason: 'stop', message: { content: '运行测试译文' } }] }))
    assert(payload.messages.at(-1).content === 'Hello runtime', '请求原文不匹配')
  })
})
await new Promise(resolve => server.listen(9357, '127.0.0.1', resolve))
await scenario('翻译真实 HTTP 链路与复制译文', async () => {
  await invoke('save_translation_settings', { settings: { providerId: 'custom', model: 'runtime-model', baseUrl: 'http://127.0.0.1:9357/v1', direction: 'auto', hasSecret: false } })
  const result = await invoke('translate_text', { text: 'Hello runtime', direction: 'en-zh' })
  assert(result.text === '运行测试译文', '译文不匹配')
  await invoke('copy_translation_result', { text: result.text })
  return { provider: result.provider, direction: result.direction }
})
server.close()
await scenario('删除记录与分组', async () => {
  await invoke('remove_clipboard_group', { group: '运行测试' })
  assert(!(await invoke('clipboard_status')).groups.includes('运行测试'), '分组仍存在')
  await invoke('clear_clipboard_entries')
  assert((await invoke('list_clipboard_entries')).length === 0, '清空失败')
})
await fs.writeFile('D:/codex/DesktopHub-work/runtime-20261116/report.json', JSON.stringify(report, null, 2))
socket.close()
console.log(JSON.stringify(report, null, 2))
process.exitCode = report.some(item => !item.passed) ? 1 : 0
