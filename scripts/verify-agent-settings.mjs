import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { test } from 'node:test'
import { runInNewContext } from 'node:vm'
import { compileScript, parse } from 'vue/compiler-sfc'
import ts from 'typescript'
import { createRenderer, h, KeepAlive, nextTick, ref } from 'vue'

const require = createRequire(import.meta.url)
const source = readFileSync(new URL('../src/features/settings/ApprovalSettings.vue', import.meta.url), 'utf8')
const script = compileScript(parse(source).descriptor, { id: 'agent-settings', inlineTemplate: true })
const { outputText } = ts.transpileModule(script.content, {
  compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.CommonJS },
})

// 使用 Vue 的真实挂载、缓存和卸载生命周期，节点仅存在于内存中。
function node(type, text = '') {
  return { type, text, props: {}, children: [], parent: null }
}
function remove(child) {
  if (child.parent) child.parent.children.splice(child.parent.children.indexOf(child), 1)
  child.parent = null
}
function insert(child, parent, anchor = null) {
  remove(child)
  const index = anchor ? parent.children.indexOf(anchor) : parent.children.length
  parent.children.splice(index, 0, child)
  child.parent = parent
}
const renderer = createRenderer({
  createElement: tag => node(tag), createText: text => node('text', text),
  createComment: () => node('comment'), remove, insert,
  setText: (n, text) => { n.text = text }, setElementText: (n, text) => { n.text = text; n.children = [] },
  parentNode: n => n.parent, nextSibling: n => n.parent?.children[n.parent.children.indexOf(n) + 1],
  patchProp: (n, key, _previous, value) => { n.props[key] = value },
  insertStaticContent(content, parent, anchor) {
    const n = node('static', content)
    insert(n, parent, anchor)
    return [n, n]
  },
})
function descendants(n) { return [n, ...n.children.flatMap(descendants)] }
const settle = async () => { await new Promise(resolve => setImmediate(resolve)); await nextTick() }

test('检测由按钮触发，切换标签保留结果，成功记录按智能体独立显示', async () => {
  const calls = []
  let resolveHistory, connected, unsubscribed = false, failDetection = false
  const history = new Promise(resolve => { resolveHistory = resolve })
  const host = {
    isDesktop: true,
    async listenHost(event, handler) {
      assert.equal(event, 'approval-agent-connected')
      connected = handler
      return () => { unsubscribed = true }
    },
    async invokeHost(command) {
      calls.push(command)
      if (command === 'connected_approval_agents') return history
      assert.equal(command, 'list_approval_agents')
      if (failDetection) throw new Error('本次检测失败')
      return [{ agent: 'codex', state: 'registered', detail: '已登记' }, { agent: 'dsh', state: 'repair', detail: '需要更新插件' }]
    },
  }
  const exports = {}
  runInNewContext(outputText, { exports, require: id => id === '@/lib/desktop/desktop' ? host : require(id) })
  const shown = ref(true)
  const app = renderer.createApp({ render: () => h(KeepAlive, () => shown.value ? h(exports.default) : null) })
  const root = node('root')
  const nodes = () => descendants(root)
  const button = text => nodes().find(n => n.type === 'button' && n.text === text)
  const badges = () => nodes().filter(n => n.text === '已连接').map(n => n.parent.children.find(c => c.type === 'strong').text)
  app.mount(root)
  try {
    await settle()
    assert.deepEqual(calls, ['connected_approval_agents'])
    assert.deepEqual(badges(), [])
    await button('检测本机智能体').props.onClick()
    await settle()
    assert.deepEqual(badges(), []) // 仅登记或可修复都没有成功回执。
    connected('codex')
    await settle()
    assert.deepEqual(badges(), ['Codex'])
    resolveHistory(['dsh']) // 迟到的历史快照也应保留刚发生的成功事件。
    await settle()
    assert.deepEqual(badges(), ['Codex', 'DSH（DeepSeek Harness）'])
    shown.value = false
    await nextTick()
    shown.value = true
    await settle()
    assert.ok(button('更新接入'))
    assert.deepEqual(calls, ['connected_approval_agents', 'list_approval_agents'])
    failDetection = true
    await button('重新检测').props.onClick()
    await settle()
    assert.ok(nodes().some(n => n.props.role === 'alert' && n.text.includes('本次检测失败')))
    assert.ok(button('更新接入'))
    assert.equal(badges().length, 2)
  } finally { app.unmount() }
  assert.ok(unsubscribed)
})
