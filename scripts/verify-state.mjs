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
if(failed)process.exitCode=1
