import fs from 'node:fs'
import vm from 'node:vm'
import assert from 'node:assert/strict'
import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
const ts = require('typescript')
const source = fs.readFileSync(new URL('../src/store.ts', import.meta.url), 'utf8')
function subject(names, bindings, declarations = '', text = source) {
  const file = ts.createSourceFile('subject.ts', text, ts.ScriptTarget.Latest, true)
  const bodies = file.statements.filter(node => ts.isFunctionDeclaration(node) && names.includes(node.name?.text)).map(node => node.getText(file))
  assert.equal(bodies.length, names.length)
  const script = ts.transpileModule(declarations + '\n' + bodies.join('\n') + '\nexports.test = { ' + names.join(',') + ' }', { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText
  const context = { exports: {}, isDesktop: true, pushToast() {}, ...bindings }
  vm.runInNewContext(script, context)
  return context.exports.test
}
function deferred() { let resolve, reject; const promise = new Promise((a,b) => { resolve=a; reject=b }); return { promise, resolve, reject } }
let failed = 0
async function test(name, run) { try { await run(); console.log('通过：'+name) } catch(e) { failed++; console.error('失败：'+name+' — '+e.message) } }
await test('翻译设置旧响应不能覆盖新响应', async()=> {
  const jobs=[deferred(),deferred()], state={}; let index=0
  const api=subject(['refreshTranslationSettings'],{state,invokeHost:()=>jobs[index++].promise},'let translationSettingsRefresh=0')
  const first=api.refreshTranslationSettings(),second=api.refreshTranslationSettings();jobs[1].resolve({model:'new'});await second;jobs[0].resolve({model:'old'});await first
  assert.equal(state.translationSettings.model,'new')
})
await test('清空返回期间新增的剪贴板记录仍保留', async()=> {
  const state={clips:[{id:'old'}],ui:{detailId:'old'}}
  const api=subject(['clearClips'],{state,invokeHost:async()=>{state.clips=[{id:'new'}]},refreshClipboard:async()=>{state.clips=[{id:'new'}]}})
  await api.clearClips();assert.equal(state.clips[0]?.id,'new')
})
await test('同一应用启动中重复点击只调用一次且允许失效路径重新核对', async()=> {
  const job=deferred();let calls=0;const api=subject(['launchApp'],{invokeHost:()=>{calls++;return job.promise}},'const launchingApps=new Set<string>()')
  const app={id:'a',name:'示例',broken:true};const first=api.launchApp(app);const second=api.launchApp(app);assert.equal(calls,1);job.resolve({launched:true,reason:''});await Promise.all([first,second])
})
await test('旧入口排序在保存成功前不修改当前列表',async()=> {
  const state={apps:[{id:'a'},{id:'b'}]};let requested=false
  const api=subject(['reorderApps'],{state,persistOrder(){},reorderVisibleApps:async()=>{requested=true;return false}})
  api.reorderApps('a','b');assert.equal(state.apps[0].id,'a');assert.equal(requested,true)
})
await test('声音写入期间合并后续音量并最终写入最后值',async()=> {
  const text=fs.readFileSync(new URL('../src/features/audio/AudioMixer.vue',import.meta.url),'utf8').match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1]
  const jobs=[deferred(),deferred()],calls=[],row={id:'a',name:'示例',volume:10};const refs={sessions:{value:[row]},busy:{value:false},exclusive:{value:false},drafts:{value:{a:80}},error:{value:''},notice:{value:''},warnings:{value:[]}}
  const api=subject(['update'],{...refs,invokeHost:(_command,args)=>{calls.push(args);return jobs[calls.length-1].promise}},'let sequence=0, disposed=false, writingSession=false; const queuedUpdates=new Map(); const draftVersions={};',text)
  const first=api.update(row,20);api.update(row,50);api.update(row,80);jobs[0].resolve({...row,volume:20});for(let n=0;n<5;n++)await Promise.resolve();assert.equal(calls.length,2);assert.equal(calls[1].volume,80);jobs[1].resolve({...row,volume:80});await first;assert.equal(row.volume,80);assert.equal(refs.busy.value,false)
})
const configurationSource=fs.readFileSync(new URL('../src/features/settings/ConfigBackup.vue',import.meta.url),'utf8').match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1]
await test('取消配置预览只释放令牌，不提交恢复',async()=> {
  const calls=[],preview={value:{token:'one'}},feedback={value:''}
  const api=subject(['cancel','discard'],{preview,feedback,busy:{value:null},failed:{value:false},importButton:{value:null},nextTick:async(fn)=>fn?.(),invokeHost:async(command,args)=>calls.push([command,args])},'let sequence=0',configurationSource)
  await api.cancel();assert.equal(preview.value,null);assert.equal(calls.length,1);assert.equal(calls[0][0],'cancel_configuration_import');assert.equal(calls[0][1].token,'one')
})
await test('关闭设置后的迟到导入预览被丢弃并释放',async()=> {
  const job=deferred(),calls=[],preview={value:null},busy={value:null}
  const api=subject(['choose','cancel','discard'],{preview,busy,feedback:{value:''},failed:{value:false},props:{blocked:false},importButton:{value:null},previewHeading:{value:null},nextTick:async(fn)=>fn?.(),invokeHost:async(command,args)=>{calls.push([command,args]);if(command==='preview_configuration_import')return job.promise}},'let sequence=0',configurationSource)
  const choosing=api.choose();await api.cancel();job.resolve({token:'late'});await choosing;assert.equal(preview.value,null);assert.equal(busy.value,null);assert.equal(calls.at(-1)[0],'cancel_configuration_import');assert.equal(calls.at(-1)[1].token,'late')
})
await test('配置恢复失败保留预览且可重试',async()=> {
  const preview={value:{token:'one'}},busy={value:null},failed={value:false}
  const api=subject(['apply'],{preview,busy,failed,feedback:{value:''},props:{blocked:false},state:{ui:{settingsOpen:true}},importButton:{value:null},nextTick:async(fn)=>fn?.(),invokeHost:async()=>{throw new Error('synthetic-write-failure')}},'',configurationSource)
  await api.apply();assert.equal(preview.value.token,'one');assert.equal(busy.value,null);assert.equal(failed.value,true)
})
await test('旧页面状态回退应用且不保留空面板，调音台保持可用',()=> {
  const state={ui:{},settings:{}}
  const api=subject(['applyUiState'],{state})
  api.applyUiState({islandPage:'compare',panel:'compare'});assert.equal(state.ui.islandPage,'applications');assert.equal(state.ui.panel,null)
  api.applyUiState({islandPage:'audio',panel:'audio'});assert.equal(state.ui.islandPage,'audio');assert.equal(state.ui.panel,'audio')
})
await test('常用短语刷新旧响应不能覆盖新响应',async()=> {
  const jobs=[deferred(),deferred()],state={snippets:[],snippetGroups:['未分组'],ui:{snippetGroup:'全部'}};let index=0
  const api=subject(['applySnippets','refreshSnippets'],{state,invokeHost:()=>jobs[index++].promise},'let snippetsRefresh=0')
  const first=api.refreshSnippets(),second=api.refreshSnippets()
  jobs[1].resolve({groups:['未分组','代码'],entries:[{id:'new',title:'新',content:'c',group:'代码',pinned:false,updatedAt:2}]});await second
  jobs[0].resolve({groups:['未分组'],entries:[{id:'old',title:'旧',content:'c',group:'未分组',pinned:false,updatedAt:1}]});await first
  assert.equal(state.snippets[0]?.id,'new');assert.deepEqual([...state.snippetGroups],['未分组','代码'])
})
await test('常用短语保存与排序在落盘失败时保持原内存不变',async()=> {
  const state={snippets:[{id:'a',title:'A',content:'1',group:'未分组',pinned:false,updatedAt:1},{id:'b',title:'B',content:'2',group:'未分组',pinned:false,updatedAt:2}],snippetGroups:['未分组'],ui:{snippetGroup:'全部'}}
  const api=subject(['applySnippets','saveSnippet','reorderVisibleSnippets'],{state,invokeHost:async()=>{throw new Error('disk-error')}},'')
  assert.equal(await api.saveSnippet({title:'C',content:'3',group:'未分组',pinned:false}),false)
  assert.equal(await api.reorderVisibleSnippets(['a','b'],'a','b'),false)
  assert.deepEqual(state.snippets.map(s=>s.id),['a','b'])
})
await test('常用短语前端预览单趟替换变量且保留未知语法',()=> {
  const api=subject(['hasSnippetVariables','renderSnippetPreview'],{},'const KNOWN_SNIPPET_VARS = new Set(["date","time","datetime","weekday","clipboard"])')
  assert.equal(api.hasSnippetVariables('{{ item.name }}'),false)
  assert.equal(api.hasSnippetVariables('{{{date}}}'),true)
  const fixed=new Date(2026,10,18,14,5,9)
  assert.equal(api.renderSnippetPreview('[{{date}} {{time}}] {{clipboard}} {{ user.id }}','含 {{date}} 原文',fixed),'[2026-11-18 14:05:09] 含 {{date}} 原文 {{ user.id }}')
})
const telemetrySource=fs.readFileSync(new URL('../src/features/island/telemetryReadout.ts',import.meta.url),'utf8')
await test('胶囊微读数智能浮现具备进入与回落回差及 3 秒最短保持期',()=> {
  const mod=ts.transpileModule(telemetrySource,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText
  const ctx={exports:{}};vm.runInNewContext(mod,ctx);const api=ctx.exports
  const prefs={telemetryMode:'smart',telemetryNet:true,telemetryCpu:true,telemetryMemory:true}
  // 闲置低水位不唤醒
  let step=api.stepSmartTelemetry({netState:'ready',downBps:8192,upBps:2048,cpuPercent:12,memoryPercent:55},prefs,0)
  assert.deepEqual({...step},{active:false,nextHold:0})
  // 下载达到 120 KB/s（>=64 KB/s）立即唤醒并置 3 秒保持
  step=api.stepSmartTelemetry({netState:'ready',downBps:120*1024,upBps:1024,cpuPercent:10,memoryPercent:55},prefs,step.nextHold)
  assert.deepEqual({...step},{active:true,nextHold:3})
  // 回落到 45 KB/s（处于 32~64 KB/s 回差区间）继续稳定保持，不减计数
  step=api.stepSmartTelemetry({netState:'ready',downBps:45*1024,upBps:1024,cpuPercent:10,memoryPercent:55},prefs,step.nextHold)
  assert.deepEqual({...step},{active:true,nextHold:3})
  // 完全跌回 0 B/s 后保持 2 拍可见，第 3 拍才平滑收回，不每秒闪烁
  const calm={netState:'ready',downBps:0,upBps:0,cpuPercent:8,memoryPercent:55}
  step=api.stepSmartTelemetry(calm,prefs,step.nextHold);assert.deepEqual({...step},{active:true,nextHold:2})
  step=api.stepSmartTelemetry(calm,prefs,step.nextHold);assert.deepEqual({...step},{active:true,nextHold:1})
  step=api.stepSmartTelemetry(calm,prefs,step.nextHold);assert.deepEqual({...step},{active:false,nextHold:0})
  // 常驻模式即使 0 流量也保持可见，关闭模式始终隐藏
  assert.equal(api.stepSmartTelemetry(calm,{...prefs,telemetryMode:'always'},0).active,true)
  assert.equal(api.stepSmartTelemetry(calm,{...prefs,telemetryMode:'off'},3).active,false)
})
await test('胶囊微读数按勾选指标数量格式化单行文案且诚实反馈未知状态',()=> {
  const mod=ts.transpileModule(telemetrySource,{compilerOptions:{module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText
  const ctx={exports:{}};vm.runInNewContext(mod,ctx);const api=ctx.exports
  const full=api.buildTelemetryReadout({netState:'ready',downBps:12.4*1024*1024,upBps:200*1024,cpuPercent:42,memoryPercent:68},{telemetryMode:'smart',telemetryNet:true,telemetryCpu:true,telemetryMemory:true})
  assert.equal(full.metricCount,3)
  assert.equal(full.text,'↓ 12.4M · CPU 42% · 内存 68%')
  // 上传占主导时切换 ↑ 指示，单指标使用完整 MB/s 单位
  const upOnly=api.buildTelemetryReadout({netState:'ready',downBps:10*1024,upBps:3.5*1024*1024,cpuPercent:90,memoryPercent:70},{telemetryMode:'always',telemetryNet:true,telemetryCpu:false,telemetryMemory:false})
  assert.equal(upOnly.metricCount,1)
  assert.equal(upOnly.text,'↑ 3.5 MB/s')
  // 网卡不可用时诚实显示未知，不伪造 0 KB/s
  const unavailable=api.buildTelemetryReadout({netState:'unavailable',downBps:null,upBps:null,cpuPercent:null,memoryPercent:null},{telemetryMode:'always',telemetryNet:true,telemetryCpu:false,telemetryMemory:false})
  assert.equal(unavailable.text,'网速 未知')
  // CPU 与内存段提供数值百分比用于进度条/双环/格栅动画
  assert.equal(full.segments.find(s=>s.metric==='cpu')?.percent,42)
  assert.equal(full.segments.find(s=>s.metric==='memory')?.percent,68)
  assert.equal(api.normalizeTelemetryStyle('bar'),'bar')
  assert.equal(api.normalizeTelemetryStyle('ring'),'ring')
  assert.equal(api.normalizeTelemetryStyle('pulse'),'pulse')
  assert.equal(api.normalizeTelemetryStyle('unknown'),'bar')
})
await test('旧版已移除主题在状态同步时平滑降级为深色或浅色主题',()=>{
  const state={ui:{},settings:{}}
  const api=subject(['applyUiState'],{state})
  api.applyUiState({theme:'liquid'});assert.equal(state.settings.theme,'dark')
  api.applyUiState({theme:'scroll'});assert.equal(state.settings.theme,'light')
  api.applyUiState({theme:'clay'});assert.equal(state.settings.theme,'light')
  api.applyUiState({theme:'neon'});assert.equal(state.settings.theme,'neon')
})
await test('普通设置入口打开通用，应用管理入口打开指定页', () => {
  const requested = []
  const api = subject(['openSettings'], { state: { ui: {} }, requestSettings: section => requested.push(section) })
  api.openSettings()
  api.openSettings('apps')
  assert.deepEqual(requested, ['general', 'apps'])
})
await test('切换标签后，重复的宿主状态不会把用户带回默认页', () => {
  const state = { ui: {}, settings: {} }
  const api = subject(['applyUiState'], { state }, 'let lastSettingsRequest = 0')
  const first = { settingsOpen: true, settingsRequest: [1, 'general'] }
  api.applyUiState(first)
  assert.equal(state.ui.settingsSection, 'general')
  state.ui.settingsSection = 'approvals'
  api.applyUiState(first)
  assert.equal(state.ui.settingsSection, 'approvals')
  api.applyUiState({ settingsOpen: true, settingsRequest: [2, 'apps'] })
  assert.equal(state.ui.settingsSection, 'apps')
})
await test('恢复灵动岛默认值只更新草稿，点应用后才提交', async () => {
  const component = fs.readFileSync(new URL('../src/features/settings/DesktopFeatures.vue', import.meta.url), 'utf8')
  const script = component.split('<script setup lang="ts">')[1].split('</script>')[0]
  const field = value => ({ value })
  const calls = []
  const inputs = {
    appearance: field({ scale: 120 }), anchor: field('left'), hoverMs: field(800),
    leaveMs: field(900), reducedMotion: field(true), saving: field(false),
    validSettings: field(true), applied: field(''), failed: field(false), feedback: field(''),
    signature: () => 'defaults', defaultIslandAppearance: () => ({ scale: 100 }),
    invokeHost: async (command, args) => { calls.push({ command, args }) },
  }
  const api = subject(['restore', 'save'], inputs, '', script)
  api.restore()
  assert.equal(inputs.appearance.value.scale, 100)
  assert.equal(calls.length, 0)
  await api.save()
  assert.equal(calls.length, 1)
  assert.equal(calls[0].command, 'save_island_appearance')
  assert.equal(calls[0].args.appearance.scale, 100)
  assert.equal(calls[0].args.anchor, 'center')
})
const explanationSource = fs.readFileSync(new URL('../src/features/clipboard/error-explanation.ts', import.meta.url), 'utf8')
function explanationUi(invokeHost, isDesktop = true) {
  const api = subject(['useErrorExplanation'], { ref: require('vue').ref, invokeHost, isDesktop }, '', explanationSource)
  return api.useErrorExplanation()
}
await test('解释入口只打开所点的文字记录，图片和空文本不发起解释', () => {
  const calls = []
  const popover = fs.readFileSync(new URL('../src/features/clipboard/ClipboardPopover.vue', import.meta.url), 'utf8')
  const source = popover.split('<script setup lang="ts">')[1].split('</script>')[0]
  const api = subject(['explainClip'], {
    openDetail: id => calls.push(['detail', id]),
    explainError: id => calls.push(['explain', id]),
    nextTick: fn => fn(), explanationHeading: { value: null },
  }, '', source)
  api.explainClip({ id: 'image', kind: 'image' })
  api.explainClip({ id: 'blank', kind: 'text', text: '   ' })
  assert.deepEqual(calls, [])
  api.explainClip({ id: 'selected', kind: 'text', text: 'TypeError', note: '私人备注' })
  assert.deepEqual(calls, [['detail', 'selected'], ['explain', 'selected']])
})
await test('只在点击解释时提交选中记录，连点不会重复发送', async () => {
  const calls = [], reply = deferred()
  const ui = explanationUi((command, args) => { calls.push({ command, args }); return reply.promise })
  assert.equal(calls.length, 0)
  const pending = ui.start('selected')
  await ui.start('selected')
  await ui.submit()
  assert.equal(calls.length, 1)
  assert.equal(calls[0].command, 'explain_clipboard_error')
  assert.deepEqual({ ...calls[0].args }, { id: 'selected', followUp: null })
  reply.resolve('第 12 行在读取空值')
  await pending
  assert.equal(ui.explanation.value.result, '第 12 行在读取空值')
})
await test('追问携带本条解释，失败后保留问题并允许重试', async () => {
  const calls = []
  const ui = explanationUi(async (_command, args) => {
    calls.push(args)
    if (calls.length === 2) throw new Error('请求超时')
    return calls.length === 1 ? '变量为空' : '先检查初始化步骤'
  })
  await ui.start('one')
  await ui.submit()
  assert.equal(calls.length, 1)
  ui.explanation.value.draft = ' 哪一步？ '
  await ui.submit()
  assert.match(ui.explanation.value.error, /请求超时/)
  assert.equal(ui.explanation.value.result, '变量为空')
  await ui.submit()
  assert.equal(calls[2].id, 'one')
  assert.deepEqual({ ...calls[2].followUp }, { explanation: '变量为空', question: '哪一步？' })
  assert.equal(ui.explanation.value.question, '哪一步？')
  assert.equal(ui.explanation.value.answer, '先检查初始化步骤')
  await ui.submit()
  assert.equal(calls.length, 3)
})
await test('退出解释并切换记录后，迟到的回复不会出现在新记录上', async () => {
  const first = deferred(), second = deferred()
  const ui = explanationUi((_command, args) => args.id === 'old' ? first.promise : second.promise)
  const oldRequest = ui.start('old')
  ui.close()
  const newRequest = ui.start('new')
  first.resolve('旧解释')
  await oldRequest
  assert.equal(ui.explanation.value.id, 'new')
  assert.equal(ui.explanation.value.result, '')
  assert.equal(ui.explanation.value.busy, true)
  second.resolve('新解释')
  await newRequest
  assert.equal(ui.explanation.value.result, '新解释')
})
await test('首次解释失败可重试，网页预览不会调用模型', async () => {
  let calls = 0
  const service = async () => { if (++calls === 1) throw new Error('请先配置模型'); return '解释成功' }
  const preview = explanationUi(service, false)
  await preview.start('one')
  assert.equal(calls, 0)
  const ui = explanationUi(service)
  await ui.start('one')
  assert.match(ui.explanation.value.error, /请先配置模型/)
  await ui.submit()
  assert.equal(ui.explanation.value.result, '解释成功')
})
const dismissSource = fs.readFileSync(new URL('../src/features/island/dismissGesture.ts', import.meta.url), 'utf8')
const dismissMod = ts.transpileModule(dismissSource, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText
const dismissCtx = { exports: {} }
vm.runInNewContext(dismissMod, dismissCtx)
const dismissApi = dismissCtx.exports
await test('空白翻译页不锁死自动收缩，加载中、有输入、有译文或报错时保持展开', () => {
  assert.equal(dismissApi.shouldKeepTranslationOpen({ status: 'idle', source: '   ', result: null, error: '' }), false)
  assert.equal(dismissApi.shouldKeepTranslationOpen({ status: 'loading', source: '', result: null, error: '' }), true)
  assert.equal(dismissApi.shouldKeepTranslationOpen({ status: 'idle', source: '你好', result: null, error: '' }), true)
  assert.equal(dismissApi.shouldKeepTranslationOpen({ status: 'done', source: '', result: { text: 'Hello' }, error: '' }), true)
  assert.equal(dismissApi.shouldKeepTranslationOpen({ status: 'error', source: '', result: null, error: '请求超时' }), true)
})
await test('窗口失焦保护矩阵：仅保护钉住、审批、文件忙碌、拖入、组字、翻译中与预览保活', () => {
  const base = {
    expanded: true, pinned: false, hasApproval: false, shelfBusy: false,
    receivingFiles: false, composing: false, translationLoading: false, previewKeepOpen: false
  }
  assert.equal(dismissApi.shouldAutoCollapseOnWindowBlur(base), true)
  assert.equal(dismissApi.shouldAutoCollapseOnWindowBlur({ ...base, expanded: false }), false)
  for (const flag of ['pinned', 'hasApproval', 'shelfBusy', 'receivingFiles', 'composing', 'translationLoading', 'previewKeepOpen']) {
    assert.equal(dismissApi.shouldAutoCollapseOnWindowBlur({ ...base, [flag]: true }), false)
  }
})
await test('底部拉条满足 1:1 跟手、边界橡皮筋阻尼、动量投影轻甩与减弱动态降级', () => {
  const linearPull = dismissApi.computeDismissPull(-20)
  assert.equal(linearPull.offsetY, -20)
  assert.equal(linearPull.ready, false)
  assert.ok(linearPull.scaleY < 1 && linearPull.scaleY >= 0.94)
  const overPull = dismissApi.computeDismissPull(-60)
  assert.ok(overPull.offsetY < -36 && overPull.offsetY > -60)
  assert.equal(overPull.ready, true)
  const downRubber = dismissApi.computeDismissPull(20)
  assert.ok(downRubber.offsetY > 0 && downRubber.offsetY < 20)
  assert.equal(downRubber.scaleY, 1)
  assert.equal(downRubber.ready, false)
  assert.equal(dismissApi.shouldCommitDismiss(-28, 0, true), true)
  assert.equal(dismissApi.shouldCommitDismiss(-12, -450, true), true)
  assert.equal(dismissApi.shouldCommitDismiss(-12, -50, true), false)
  assert.equal(dismissApi.shouldCommitDismiss(-12, -450, true, true), false)
  assert.equal(dismissApi.shouldCommitDismiss(-30, 0, true, true), true)
  assert.equal(dismissApi.shouldCommitDismiss(-40, -800, false), false)
})
await test('常用短语账密类别校验、密码保留与禁止删除内置类别', async () => {
  const calls = []
  const toasts = []
  const state = {
    snippets: [{ id: 'c1', kind: 'credential', title: '邮箱', content: '', account: 'u@example.com', hasPassword: true, group: '账密', pinned: false, updatedAt: 1 }],
    snippetGroups: ['未分组', '账密'],
    ui: { snippetGroup: '全部' }
  }
  const api = subject(
    ['applySnippets', 'saveSnippet', 'removeSnippetGroup', 'copySnippet'],
    {
      state,
      pushToast: (msg, kind) => toasts.push({ msg, kind }),
      invokeHost: async (command, args) => {
        calls.push({ command, args })
        if (command === 'save_snippet') {
          return {
            groups: ['未分组', '账密'],
            entries: [{ id: args.snippet.id || 'c-new', kind: args.snippet.kind, title: args.snippet.title, content: args.snippet.content, account: args.snippet.account, hasPassword: true, group: '账密', pinned: args.snippet.pinned, updatedAt: 2 }]
          }
        }
      }
    }
  )
  // 不可删除内置类别“账密”与“未分组”
  assert.equal(await api.removeSnippetGroup('未分组'), false)
  assert.equal(await api.removeSnippetGroup('账密'), false)
  // 新建账密缺少账号或密码均校验拦截，不发 IPC
  assert.equal(await api.saveSnippet({ kind: 'credential', title: '新账密', account: '  ', password: 'abc', content: '', group: '账密', pinned: false }), false)
  assert.equal(await api.saveSnippet({ kind: 'credential', title: '新账密', account: 'alice', password: '', content: '', group: '账密', pinned: false }), false)
  assert.equal(calls.length, 0)
  // 编辑已有账密不传 password（如置顶操作）允许保留原密码，并携带 kind 与 account
  assert.equal(await api.saveSnippet({ id: 'c1', kind: 'credential', title: '邮箱', account: 'u@example.com', content: '', group: '账密', pinned: true }), true)
  assert.equal(calls[0].command, 'save_snippet')
  assert.equal(calls[0].args.snippet.kind, 'credential')
  assert.equal(calls[0].args.snippet.password, undefined)
  // 账密分别复制账号与密码
  assert.equal(await api.copySnippet(state.snippets[0], false, 'account'), true)
  assert.equal(await api.copySnippet(state.snippets[0], false, 'password'), true)
  assert.equal(calls[1].args.field, 'account')
  assert.equal(calls[2].args.field, 'password')
  assert.match(toasts.at(-1).msg, /不记入剪贴板历史/)
})
await test('bootstrap 具备幂等与防重入保护，并发与重复调用只初始化一次', async () => {
  const job = deferred()
  let appCalls = 0
  const state = { ready: false, loadError: '', shortcuts: { bar: '', clipboard: '', translation: '', ocr: '' }, settings: {} }
  const api = subject(
    ['bootstrap'],
    {
      state,
      applyApps() {},
      refreshClipboard: async () => {},
      refreshSnippets: async () => {},
      refreshTranslationSettings: async () => {},
      applyUiState() {},
      invokeHost: async (command) => {
        if (command === 'list_applications') {
          appCalls++
          return job.promise
        }
        if (command === 'get_shortcuts') return { bar: 'Ctrl+Alt+Space', clipboard: 'Ctrl+Shift+V', translation: 'Ctrl+Shift+F', ocr: 'Ctrl+Shift+O', errors: [] }
        return {}
      }
    },
    'let bootstrapPromise: Promise<void> | null = null'
  )
  const first = api.bootstrap()
  const second = api.bootstrap()
  assert.equal(appCalls, 1)
  job.resolve({ categories: [], apps: [] })
  await Promise.all([first, second])
  assert.equal(state.ready, true)
  await api.bootstrap()
  assert.equal(appCalls, 1)
})
if(failed)process.exitCode=1
