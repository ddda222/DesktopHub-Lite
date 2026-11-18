import assert from 'node:assert/strict'
import { createRequire } from 'node:module'

const { chromium } = createRequire(import.meta.url)('playwright')
const browser = await chromium.launch({ executablePath: process.env.HUB_QA_BROWSER || undefined, headless: true })
try {
  const page = await browser.newPage({ viewport: { width: 900, height: 700 } })
  const errors = []
  page.on('pageerror', err => errors.push(err.message))
  await page.addInitScript(() => {
    Object.defineProperty(navigator, 'hardwareConcurrency', { get: () => 2 })
    const getContext = HTMLCanvasElement.prototype.getContext
    HTMLCanvasElement.prototype.getContext = function (type, ...rest) {
      return type === 'webgl' ? null : getContext.call(this, type, ...rest)
    }
  })
  await page.goto('http://127.0.0.1:4317/motion-study.html')
  const action = selector => page.locator(selector).click()
  const particles = page.locator('.island-particles')
  await particles.waitFor()
  await page.waitForFunction(() => !document.querySelector('.glass-light'))
  assert.equal(await page.locator('.island .mode-nav button').count(), 5)
  assert.equal(await page.locator('.liquid-skin .skin').evaluate(el => getComputedStyle(el).fill !== 'none'), true)
  await action('[data-cue=launch]')
  await page.waitForFunction(() => document.querySelector('.island-particles')?.width === 384)
  await action('#light')
  await action('[data-cue=complete]')
  await action('#light')
  await particles.waitFor()
  assert.equal(await particles.evaluate(c => c.width), 1, '禁用期间的旧事件不应在重新启用时播放')
  await page.locator('.app-tile').first().hover()
  assert.equal(await page.locator('.app-tile').first().evaluate(btn => btn.firstElementChild.style.transform), '', '低档不应用图标磁吸')
  await action('[data-state=line]')
  await page.emulateMedia({ reducedMotion: 'reduce' })
  await action('[data-state=expanded]')
  assert.equal(await page.locator('.island-particles,.glass-light').count(), 0)
  assert.equal(await page.locator('.liquid-skin .skin').evaluate(el => parseFloat(getComputedStyle(el).transitionDuration)), 0)
  await page.locator('.mode-nav button[aria-label="文件中转"]').click()
  assert.equal(await page.locator('.mode-nav button[aria-label="文件中转"]').getAttribute('aria-pressed'), 'true')
  assert.deepEqual(errors, [])
  console.log('通过：无 WebGL 回退、低档粒子与物理策略、禁用时不补播、系统减少动画及五项导航。')
} finally {
  await browser.close()
}
