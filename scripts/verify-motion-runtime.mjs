import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { spawn } from 'node:child_process'
import { performance } from 'node:perf_hooks'

const { chromium } = createRequire(import.meta.url)('playwright')
const base = 'http://127.0.0.1:4317/motion-study.html'
const native = Boolean(process.env.HUB_QA_WEBVIEW_HOST)
const out = process.env.HUB_QA_OUTPUT
if (!out) throw Error('请指定隔离输出目录 HUB_QA_OUTPUT')
const media = process.env.HUB_QA_MEDIA
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
const round = x => Math.round(x * 100) / 100
const log = { host: native ? 'WRY + 系统 WebView2' : '独立 Edge', recorded: new Date().toISOString() }
let host, browser, page
try {
  await mkdir(out, { recursive: true })
  if (media) await mkdir(media, { recursive: true })
  if (native) {
    const profile = await mkdtemp(join(out, 'wv2-'))
    host = spawn(process.env.HUB_QA_WEBVIEW_HOST, [], {
      stdio: 'ignore', windowsHide: true, env: { ...process.env, WEBVIEW2_USER_DATA_FOLDER: profile }
    })
    let port
    for (let count = 0; count < 100 && !port; count++) {
      if (host.exitCode !== null) throw Error('独立测试宿主退出：' + host.exitCode)
      for (const path of [join(profile, 'EBWebView', 'DevToolsActivePort'), join(profile, 'DevToolsActivePort')]) {
        try { port = Number((await readFile(path, 'utf8')).split(/\r?\n/)[0]); break } catch {}
      }
      if (!port) await sleep(170)
    }
    if (!port) throw Error('没有在本次新建的 WebView2 用户目录发现调试端口')
    browser = await chromium.connectOverCDP('http://127.0.0.1:' + port)
    for (let count = 0; count < 40 && !page; count++) {
      page = browser.contexts()[0]?.pages().find(item => item.url() === base)
      if (!page) await sleep(200)
    }
    if (!page || !await page.evaluate(() => window.__hubMotionQA === true)) throw Error('未识别本次专用宿主')
  } else {
    browser = await chromium.launch({ executablePath: process.env.HUB_QA_BROWSER, headless: true })
    page = await browser.newPage({ viewport: { width: 960, height: 750 } })
  }
  const errors = []
  page.on('pageerror', e => errors.push(e.message))
  await page.addInitScript(() => {
    const state = { frames: 0, gl: 0, dots: 0, pointer: 0, resize: 0, restored: 0, lost: 0, hidden: [], buffers: 0, programs: 0 }
    window.__probe = state
    const raf = requestAnimationFrame.bind(window)
    window.__qaFrame = raf
    window.requestAnimationFrame = callback => raf(now => { state.frames++; callback(now) })
    const proto = WebGLRenderingContext.prototype
    for (const [method, key, increment] of [['drawArrays','gl',1],['createBuffer','buffers',1],['deleteBuffer','buffers',-1],['createProgram','programs',1],['deleteProgram','programs',-1]]) {
      const original = proto[method]
      proto[method] = function(...args) { if (this.canvas.classList.contains('glass-light') && (increment > 0 || args[0])) state[key] += increment; return original.apply(this,args) }
    }
    const arc = CanvasRenderingContext2D.prototype.arc
    CanvasRenderingContext2D.prototype.arc = function(...args) { if (this.canvas.classList.contains('island-particles')) state.dots++; return arc.apply(this, args) }
    for (const [name, key] of [['webglcontextlost','lost'],['webglcontextrestored','restored']]) document.addEventListener(name, () => state[key]++, true)
    document.addEventListener('visibilitychange', () => state.hidden.push({ hidden: document.hidden, draws: state.gl, dots: state.dots, time: performance.now() }))
    document.addEventListener('pointermove',()=>state.pointer++,{capture:true,passive:true})
    window.addEventListener('resize',()=>state.resize++)
  })
  await page.goto(base, { waitUntil: 'networkidle' })
  await page.locator('.liquid-skin').waitFor()
  const client = await page.context().newCDPSession(page)
  const top = await browser.newBrowserCDPSession()
  const hw = (await top.send('SystemInfo.getInfo')).gpu
  log.runtime = { browser: browser.version(), gpu: hw.devices.map(item => item.deviceString || item.vendorString),
    webgl: hw.featureStatus.webgl, compositor: hw.featureStatus.gpu_compositing }
  const stat = () => page.evaluate(() => ({ ...window.__probe }))
  const metrics = async () => Object.fromEntries((await client.send('Performance.getMetrics')).metrics.map(item => [item.name,item.value]))
  await client.send('Performance.enable')
  const cpu = async () => (await top.send('SystemInfo.getProcessInfo')).processInfo
  const cpuDelta = (a,b,ms) => round(b.reduce((sum,p) => { const prev = a.find(q => p.id === q.id); return sum + (prev ? Math.max(0,p.cpuTime-prev.cpuTime):0) },0) * 100000/ms)
  const act = async selector => page.locator(selector).evaluate(node => node.click())
  const state = shape => act('[data-state="'+shape+'"]')
  const wait = ms => page.waitForTimeout(ms)
  const shot = async name => { if (media) await page.locator('.stage').screenshot({ path: join(media,'motion-'+name+'.png') }) }
  await page.mouse.move(1,1)
  await state('line'); await wait(700); assert(await page.locator('.island.tucked').count()); await shot('line')
  await state('compact'); await wait(700); assert.equal(await page.locator('.island.expanded,.island.tucked').count(),0); await shot('compact')
  await state('expanded'); await wait(720); assert(await page.locator('.island.expanded').count()); assert.equal(await page.locator('.mode-nav button').count(),5)
  log.shapes = true
  const glState = await page.evaluate(() => {
    const gl = document.querySelector('.glass-light')?.getContext('webgl')
    if (!gl) return null
    const info = gl.getExtension('WEBGL_debug_renderer_info')
    return { renderer: info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL):gl.getParameter(gl.RENDERER), pixels: [gl.drawingBufferWidth,gl.drawingBufferHeight] }
  })
  log.gl = glState
  log.events = {}
  for (const kind of ['launch','notice','loading','complete']) {
    const before = await stat()
    await act('[data-cue="'+kind+'"]'); await wait(180)
    const after = await stat()
    log.events[kind] = after.dots-before.dots
    assert(log.events[kind]>0,'粒子事件未绘制：'+kind)
    if(kind==='loading') await shot('expanded')
  }
  await wait(1800)
  const idleStart = await stat(), idleCpu = await cpu(), idleT = performance.now()
  await wait(2200)
  const idleWall = performance.now()-idleT, idleEnd = await stat()
  log.idle = { wallMs:round(idleWall),raf:idleEnd.frames-idleStart.frames,gl:idleEnd.gl-idleStart.gl,dots:idleEnd.dots-idleStart.dots, pointer:idleEnd.pointer-idleStart.pointer,resize:idleEnd.resize-idleStart.resize,cpuOneCorePct:cpuDelta(idleCpu,await cpu(),idleWall) }
  const beforeCpu=await cpu(), beforeMain=await metrics(), benchStart=performance.now()
  log.active = await page.evaluate(() => new Promise(resolve => {
    const intervals=[],latency=[], started=performance.now()
    let prev=0,next=started,phase=0,pending=false
    function step(now) {
      if(prev) intervals.push(now-prev)
      prev=now
      if(pending!==false){latency.push(now-pending);pending=false}
      if(now>next){
        document.querySelector('[data-state="'+(phase%2?'expanded':'compact')+'"]').click()
        document.querySelector('[data-cue="'+['loading','complete','launch','notice'][phase%4]+'"]').click()
        pending=performance.now();phase++;next+=330
      }
      if(now-started<5600){window.__qaFrame(step);return}
      const sorted=intervals.slice().sort((a,b)=>a-b), sortedLatency=latency.slice().sort((a,b)=>a-b)
      const pct=(xs)=>+(xs[Math.ceil(xs.length*.95)-1]||0).toFixed(2)
      resolve({frames:intervals.length,events:phase,fps:+(1000*intervals.length/intervals.reduce((a,b)=>a+b,0)).toFixed(2),p95FrameMs:pct(sorted),over33:intervals.filter(v=>v>33.4).length,p95NextFrameMs:pct(sortedLatency)})
    }
    window.__qaFrame(step)
  }))
  const benchWall=performance.now()-benchStart
  log.active.cpuOneCorePct=cpuDelta(beforeCpu,await cpu(),benchWall)
  log.active.mainTaskPct=round(((await metrics()).TaskDuration-beforeMain.TaskDuration)*100000/benchWall)
  if(native){
    await state('expanded');await wait(650);await act('[data-cue="loading"]')
    await page.evaluate(()=>window.ipc.postMessage('hide-briefly'));await wait(1900)
    const transitions=(await stat()).hidden;log.nativeHidden=transitions
    const samples=await page.evaluate(()=>[window.__nativeBefore,window.__nativeAfter])
    assert(samples.every(Boolean),'真实窗口未处理隐藏和恢复')
    log.hostHidden={browserSignals:transitions.length,wallMs:round(samples[1].time-samples[0].time),gl:samples[1].gl-samples[0].gl,dots:samples[1].dots-samples[0].dots}
  }
  if(glState) {
    await state('expanded');await wait(600)
    await page.evaluate(()=>{const gl=document.querySelector('.glass-light').getContext('webgl'),ext=gl.getExtension('WEBGL_lose_context');if(!ext)throw Error('WEBGL_lose_context 不可用');ext.loseContext();setTimeout(()=>ext.restoreContext(),100)})
    await page.waitForFunction(()=>window.__probe.restored>0)
    const before=await stat();await act('[data-cue="loading"]');await page.locator('.glass-light').evaluate(n=>n.parentElement.dispatchEvent(new PointerEvent('pointermove',{isPrimary:true,clientX:60,clientY:40})))
    await wait(250);log.restoredDraws=(await stat()).gl-before.gl;assert(log.restoredDraws>0)
  }
  await act('#reduced');await wait(300);log.quietCanvases=await page.locator('.glass-light,.island-particles').count();assert.equal(log.quietCanvases,0)
  await act('[data-cue="loading"]');await wait(220);await act('#reduced');await state('expanded');await wait(800)
  await client.send('HeapProfiler.collectGarbage')
  const beforeMem=await metrics(),beforeDOM=await client.send('Memory.getDOMCounters'),beforeResources=await stat()
  for(let i=0;i<10;i++){await act('#light');await wait(40);await act('#light');await wait(60)}
  await wait(850);await client.send('HeapProfiler.collectGarbage')
  const afterMem=await metrics(),afterDOM=await client.send('Memory.getDOMCounters'),afterResources=await stat()
  log.remount={cycles:10,gcHeapMiB:[beforeMem.JSHeapUsedSize,afterMem.JSHeapUsedSize].map(n=>round(n/1048576)),nodes:[beforeDOM.nodes,afterDOM.nodes],listeners:[beforeDOM.jsEventListeners,afterDOM.jsEventListeners],programDelta:afterResources.programs-beforeResources.programs,bufferDelta:afterResources.buffers-beforeResources.buffers}
  assert.equal(log.remount.programDelta,0);assert.equal(log.remount.bufferDelta,0)
  if(!native) {
    const low=await browser.newPage({ viewport:{width:320,height:670} })
    try {
      await low.addInitScript(()=>Object.defineProperty(navigator,'hardwareConcurrency',{get:()=>2}))
      await low.goto(base,{waitUntil:'networkidle'})
      await low.locator('[data-cue="loading"]').click()
      await low.waitForFunction(()=>document.querySelector('.island-particles')?.width===384)
      const lowBuffer=await low.locator('.island-particles').evaluate(c=>c.width)
      await low.locator('.mode-nav [aria-label="调音台"]').click()
      assert.equal(await low.locator('.mode-nav [aria-label="调音台"]').getAttribute('aria-pressed'),'true')
      await low.emulateMedia({forcedColors:'active'})
      await low.waitForFunction(()=>!document.querySelector('.glass-light,.island-particles'))
      log.low={simulatedCores:2,bufferWidth:lowBuffer,forcedColorsStatic:true,clickableAudio:true}
    } finally {await low.close()}
  }
  assert.equal(log.idle.raf,0,'空闲 RAF 未停止')
  log.errors=errors;assert.deepEqual(errors,[])
} catch(e) {log.failure=e.message;process.exitCode=1}
finally {
  if(native && page) await page.evaluate(()=>{if(window.__hubMotionQA)window.ipc.postMessage('finish')}).catch(()=>{})
  if(browser) await browser.close().catch(()=>{})
  if(host?.exitCode===null) host.kill()
  console.log(JSON.stringify(log,null,2));await writeFile(join(out,native?'webview2.json':'edge.json'),JSON.stringify(log,null,2))
}
