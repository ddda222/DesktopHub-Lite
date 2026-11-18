import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'

const require = createRequire(import.meta.url)
const ts = require('typescript')
const src = readFileSync(new URL('../src/features/island/motion/physics.ts', import.meta.url), 'utf8')
const js = ts.transpileModule(src, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText
const { advanceSpring } = await import('data:text/javascript;base64,' + Buffer.from(js).toString('base64'))
function at(rate, steps) {
  const axis = { value: 0, velocity: 40, target: 2 }
  for (let step = 0; step < steps; step++) advanceSpring(axis, 1 / rate)
  return axis
}
const sixty = at(60, 10), fast = at(120, 20)
assert.ok(Math.abs(sixty.value - fast.value) < .000001)
assert.ok(Math.abs(sixty.velocity - fast.velocity) < .000001)
const interrupted = at(60, 4), instant = interrupted.value, velocity = interrupted.velocity
interrupted.target = -2
advanceSpring(interrupted, 0)
assert.equal(interrupted.value, instant)
assert.equal(interrupted.velocity, velocity)
for (let step = 0; step < 240; step++) advanceSpring(interrupted, 1 / 120)
assert.equal(interrupted.value, -2)
assert.equal(advanceSpring(interrupted, 1 / 60), false)
console.log('通过：刷新率无关、反向时位置与速度连续、收敛停帧')

if (process.argv.includes('--browser')) {
  const browser = await require('playwright').chromium.launch({
    executablePath: process.env.HUB_QA_BROWSER || undefined, headless: true
  })
  try {
    const page = await browser.newPage()
    await page.setContent('<main><button class="app-tile" style="width:80px;height:50px"><span class="fallback">A</span></button><button class="capsule"><span class="capsule-text">12:30</span></button></main>')
    await page.addScriptTag({ type: 'module', content: js + `
      const panel = document.querySelector('main')
      let calls = 0
      const raf = window.requestAnimationFrame.bind(window)
      window.requestAnimationFrame = (fn) => raf((now) => { calls++; fn(now) })
      window.readFrames = () => calls
      window.physics = createIslandPhysics(panel)
      window.physics.setEnabled(true)
    ` })
    await page.waitForFunction(() => Boolean(window.physics))
    const btn = page.locator('.app-tile'), visual = btn.locator('span')
    const before = await btn.boundingBox()
    await page.mouse.move(before.x + before.width - 3, before.y + 4)
    await page.waitForFunction(() => document.querySelector('.fallback').style.transform.includes('rotateY'))
    assert.deepEqual(await btn.boundingBox(), before)
    await page.mouse.down()
    await page.waitForFunction(() => /scale\(0\.9/.test(document.querySelector('.fallback').style.transform))
    await page.mouse.up()
    await page.waitForFunction(() => document.querySelector('.fallback').style.transform === '')
    const grip = await page.locator('.capsule').boundingBox()
    await page.mouse.move(grip.x + 5, grip.y + 5)
    await page.mouse.down()
    await page.mouse.move(grip.x + 20, grip.y + 9)
    await page.waitForFunction(() => document.querySelector('.capsule-text').style.transform.includes('1.95px'))
    await page.mouse.up()
    await page.waitForFunction(() => document.querySelector('.capsule-text').style.transform === '')
    await page.mouse.move(before.x + 1, before.y + 1)
    await page.waitForFunction(() => document.querySelector('.fallback').style.transform !== '')
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')))
    assert.equal(await visual.evaluate(el => el.style.transform), '')
    await page.evaluate(() => window.physics.dispose())
    const stopped = await page.evaluate(() => window.readFrames())
    await page.mouse.move(0, 0); await page.mouse.move(before.x + 2, before.y + 2)
    await page.waitForTimeout(100)
    assert.equal(await page.evaluate(() => window.readFrames()), stopped)
    assert.equal(await visual.evaluate(el => el.style.transform), '')
    console.log('通过：磁吸/倾斜不移动实际按钮、按压回弹、拖动释放、可见性复位、卸载后无新帧')
  } finally { await browser.close() }
}
