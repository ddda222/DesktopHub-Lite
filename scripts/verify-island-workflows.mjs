import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'

const { chromium } = createRequire(import.meta.url)('playwright')
const root = fileURLToPath(new URL('../', import.meta.url))
const server = await createServer({ root, server: { host: '127.0.0.1', port: 0, strictPort: true } })
let browser

async function capture(locator, name) {
  if (process.env.HUB_QA_SCREENSHOTS) {
    await locator.screenshot({ path: join(process.env.HUB_QA_SCREENSHOTS, name), animations: 'disabled' })
  }
}

// 通过正常窗口入口挂载页面。宿主命令只读写这个浏览器页面里的样例数据。
async function openWindow(label) {
  const page = await browser.newPage({ viewport: { width: 1040, height: 780 }, reducedMotion: 'reduce' })
  page.setDefaultTimeout(5000)
  await page.addInitScript(({ label }) => {
    const catalog = ['画图', '计算器', '记事本'].map(name => ({ name, target: 'shell:AppsFolder\\' + name }))
    const config = { categories: [{ id: 'work', name: '办公' }], apps: [
      { ...catalog[0], id: 'first', categoryId: 'work', kind: 'shortcut' },
    ] }
    const fixture = window.fixture = { imports: [], reading: [], modelCalls: [], failImport: true }
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label }, currentWebview: { label } },
      transformCallback: () => 1,
      async invoke(command, args) {
        const state = window.__hub?.state
        switch (command) {
          case 'current_window_label': return label
          case 'list_applications': return config
          case 'read_app_icons': return []
          case 'list_clipboard_entries': return [{ id: 'clip', kind: 'text', text: "TypeError: Cannot read properties of undefined (reading 'name')", group: '未分组', copiedAt: 0 }]
          case 'clipboard_status': return { groups: [], paused: false }
          case 'list_snippets': case 'list_shelf_files': return { groups: [], entries: [], undoCount: 0 }
          case 'get_shortcuts': return { clipboard: 'F8' }
          case 'get_translation_settings': return { direction: 'zh-en', hasSecret: true, providerId: 'custom', baseUrl: 'http://localhost', model: 'test' }
          case 'ui_state': return { ...state.ui, theme: 'light', settingsOpen: label === 'settings',
            islandExpanded: true, islandPage: 'translation', islandLeaveMs: 80,
            islandAppearance: { ...state.ui.islandAppearance, webgl: false } }
          case 'select_island_page': state.ui.islandPage = args.page; return
          case 'set_island_expanded': state.ui.islandExpanded = args.expanded; return
          case 'set_island_reading_explanation': fixture.reading.push(args.active); return
          case 'approval_snapshot': return { revision: 0, count: 0, current: null }
          case 'mail_notices': return []
          case 'translate_text': case 'explain_clipboard_error':
            fixture.modelCalls.push({ command, args })
            return new Promise(resolve => { fixture.reply = resolve })
          case 'list_system_apps': return [...catalog]
          case 'import_system_apps':
            fixture.imports.push(args)
            if (fixture.failImport) throw new Error('模拟导入失败')
            for (const target of args.targets) {
              const app = catalog.find(item => item.target === target)
              config.apps.push({ ...app, id: target, categoryId: args.categoryId, kind: 'shortcut' })
            }
            return { added: args.targets.length, skipped: 0, config }
          default: return null
        }
      },
    }
  }, { label })
  await page.goto(server.resolvedUrls.local[0])
  await page.waitForFunction(() => window.__hub?.state.ready)
  return page
}

async function verifyIsland(page) {
  const button = name => page.getByRole('button', { name, exact: true })
  const translation = page.getByRole('region', { name: '翻译面板' })
  const source = page.getByLabel('原文语言')
  const target = page.getByLabel('译文语言')
  assert.equal(await source.inputValue(), 'zh')
  assert.equal(await target.inputValue(), 'en')
  assert.equal(await translation.locator('.result').count(), 0)
  const swap = button('交换翻译方向')
  const arrow = swap.locator('svg')
  const duration = () => arrow.evaluate(el => parseFloat(getComputedStyle(el).transitionDuration))
  const initialWidth = (await translation.getByRole('group', { name: '翻译方向' }).boundingBox()).width
  assert.equal(await duration(), 0)
  await page.emulateMedia({ reducedMotion: 'no-preference' })
  await page.waitForFunction(() => !document.querySelector('.island-hit-area.reduced'))
  assert.ok(await duration() > 0)
  await swap.focus()
  await page.keyboard.press('Enter')
  await page.keyboard.press('Enter')
  assert.equal(await source.inputValue(), 'zh')
  await page.evaluate(() => { window.__hub.state.ui.islandReducedMotion = true })
  assert.equal(await duration(), 0)
  await swap.click()
  assert.equal(await source.inputValue(), 'en')
  assert.equal(await target.inputValue(), 'zh')
  assert.equal((await translation.getByRole('group', { name: '翻译方向' }).boundingBox()).width, initialWidth)
  await target.selectOption('en')
  assert.equal(await source.inputValue(), 'zh')
  await page.evaluate(() => { window.__hub.state.ui.islandReducedMotion = false })
  await page.getByLabel('要翻译的原文').fill('请在关闭窗口前保存更改。')
  assert.deepEqual(await page.evaluate(() => fixture.modelCalls), [])
  await translation.getByRole('button', { name: '翻译', exact: true }).click()
  await page.getByText('正在翻译，请稍候…').waitFor()
  const moveAway = () => page.mouse.move(1000, 760)
  async function staysOpen() {
    // 实际等待时间超过用例宿主的移出延时，焦点不留在输入框中。
    await moveAway()
    await page.waitForTimeout(350)
    assert.equal(await page.locator('.capsule').getAttribute('aria-expanded'), 'true')
  }
  await staysOpen()
  const reply = text => page.evaluate(text => fixture.reply({ text, provider: 'test', model: 'test', elapsedMs: 1, direction: 'zh-en' }), text)
  await reply('Save your changes before closing the window.')
  const result = page.getByLabel('译文内容')
  const shortHeight = (await result.boundingBox()).height
  assert.notEqual(await result.evaluate(el => getComputedStyle(el).animationName), 'none')
  await staysOpen()
  await capture(page.locator('.island.solid-surface'), '翻译.png')
  await page.emulateMedia({ reducedMotion: 'reduce' })
  assert.equal(await result.evaluate(el => getComputedStyle(el).animationName), 'none')
  await translation.getByRole('button', { name: '翻译', exact: true }).click()
  await page.getByText('正在翻译，请稍候…').waitFor()
  await reply('长译文\n'.repeat(50))
  await result.waitFor()
  assert.ok(await result.evaluate((el, short) => short < el.clientHeight && el.clientHeight <= 100 && el.scrollHeight > el.clientHeight, shortHeight))
  assert.ok(await button('复制译文').isVisible())
  await button('交换翻译方向').click()
  assert.equal(await result.count(), 0)

  await page.evaluate(() => { window.__hub.state.settings.theme = 'dark' })
  await page.waitForFunction(() => document.documentElement.dataset.theme === 'dark')
  await button('剪贴板').click()
  await button('AI 解读').click()
  await page.getByText('正在解读这段文字…').waitFor()
  await staysOpen()
  const explanation = '读取 name 时，对象的值是 undefined。先检查对象是否已经赋值，再访问它的属性。'
  await page.evaluate(text => fixture.reply(text), explanation)
  await page.getByText(explanation, { exact: true }).waitFor()
  await staysOpen()
  const followUp = page.getByLabel('针对这段内容追问')
  await followUp.fill('先看哪一行？')
  await button('发送追问').click()
  await page.getByText('正在回答追问…').waitFor()
  await page.evaluate(() => fixture.reply('从堆栈中第一条指向自己代码的行号开始。'))
  await page.getByText('从堆栈中第一条指向自己代码的行号开始。').waitFor()
  assert.ok(await page.getByRole('heading', { name: '先看哪一行？' }).evaluate(el => el === document.activeElement))
  await capture(page.locator('.island.solid-surface'), 'AI解读.png')
  await button('返回原文').click()
  assert.deepEqual(await page.evaluate(() => fixture.reading), [true, false])
  await moveAway()
  await page.waitForFunction(() => !window.__hub.state.ui.islandExpanded)
  await page.evaluate(() => { window.__hub.state.ui.islandExpanded = true })
  await button('翻译').first().click()
  await button('收起灵动岛').click()
  await page.waitForFunction(() => !window.__hub.state.ui.islandExpanded)
}

async function verifyPicker(page) {
  const button = name => page.getByRole('button', { name, exact: true })
  await button('应用管理').click()
  await page.getByTitle('办公', { exact: true }).click()
  await button('＋ 添加应用').click()
  const open = button('导入系统应用')
  await open.click()
  const picker = page.getByRole('region', { name: '选择系统应用' })
  const check = name => picker.getByRole('checkbox', { name })
  const search = picker.getByRole('searchbox')
  assert.ok(await search.evaluate(el => el === document.activeElement))
  assert.ok(await check('画图').isDisabled())
  assert.ok(await button('导入所选（0）').isDisabled())
  await check('计算器').check()
  await picker.getByRole('button', { name: '取消' }).click()
  assert.ok(await open.evaluate(el => el === document.activeElement))
  assert.deepEqual(await page.evaluate(() => fixture.imports), [])
  await open.click()
  assert.ok(await button('导入所选（0）').isDisabled())
  await check('计算器').focus()
  await page.keyboard.press('Space')
  assert.ok(await check('计算器').isChecked())
  const selectedRow = picker.locator('label').filter({ hasText: '计算器' })
  const unselectedRow = picker.locator('label').filter({ hasText: '记事本' })
  const rowColor = el => getComputedStyle(el).backgroundColor
  assert.notEqual(await selectedRow.evaluate(rowColor), await unselectedRow.evaluate(rowColor))
  await capture(picker, '选择系统应用.png')
  await search.fill('记事本')
  assert.equal(await check('计算器').count(), 0)
  await picker.getByText('1 项', { exact: true }).waitFor()
  await button('导入所选（1）').click()
  await picker.getByRole('alert').waitFor()
  await page.evaluate(() => { fixture.failImport = false })
  await button('导入所选（1）').click()
  await page.getByText('已导入 1 个系统应用', { exact: true }).waitFor()
  assert.ok(await open.evaluate(el => el === document.activeElement))
  const expected = { categoryId: 'work', targets: ['shell:AppsFolder\\计算器'] }
  assert.deepEqual(await page.evaluate(() => fixture.imports), [expected, expected])
  assert.deepEqual(await page.getByRole('region', { name: '办公', exact: true }).locator('.managed-name').allTextContents(), ['画图', '计算器'])
}

try {
  await server.listen()
  browser = await chromium.launch({ executablePath: process.env.HUB_QA_BROWSER || undefined })
  for (const [label, verify] of [['bar', verifyIsland], ['settings', verifyPicker]]) {
    const page = await openWindow(label)
    const errors = []
    page.on('pageerror', error => errors.push(error.message))
    await verify(page)
    assert.deepEqual(errors, [])
    await page.close()
  }
  console.log('通过：翻译方向互换、紧凑译文、翻译与解读保持展开、手动收起，以及系统应用的勾选、取消和重试。')
} finally {
  await browser?.close()
  await server.close()
}
