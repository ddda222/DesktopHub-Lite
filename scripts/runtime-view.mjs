import fs from 'node:fs/promises'
const targets = await fetch('http://127.0.0.1:9346/json/list').then(response => response.json())
const views = []
for (const target of targets.filter(item => item.type === 'page')) {
  const socket = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject })
  const result = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('界面读取超时')), 5000)
    socket.onmessage = event => {
      const message = JSON.parse(event.data)
      if (message.id === 1) { clearTimeout(timer); resolve(message.result.result.value) }
    }
    socket.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: {
      expression: `({url:location.href, text:document.body.innerText.slice(0,400), buttons:[...document.querySelectorAll('button')].map(button=>button.getAttribute('aria-label')||button.title||button.textContent.trim()).slice(0,12)})`, returnByValue: true
    } }))
  })
  views.push(result)
  socket.close()
}
if (!views.some(view => view.buttons.some(button => button.includes('剪贴板')))) throw new Error('未找到图标条剪贴板按钮')
if (!views.some(view => view.text.includes('应用管理'))) throw new Error('设置界面未渲染')
await fs.writeFile('D:/codex/DesktopHub-work/runtime-20261116/views.json', JSON.stringify(views, null, 2))
console.log(JSON.stringify(views, null, 2))
