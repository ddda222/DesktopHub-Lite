<script setup lang="ts">
/** 当前灵动岛的浏览器预览，只使用既有合成数据与 MockHost。 */
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import IslandEntry from '@/features/island/IslandEntry.vue'
import type { TelemetrySnapshot } from '@/features/island/telemetryReadout'
import SettingsWindow from '@/features/settings/SettingsWindow.vue'
import ImagePreviewWindow from './ImagePreviewWindow.vue'
import { browserImagePreview } from '@/mock/image-preview'
import { closeSettings, setTheme, state } from '@/store'
import { previewApps, previewIcons } from '@/mock/preview-icons'
import { makeShelfSamples } from '@/mock/shelf-samples'
import { closeShelfPreview, loadBrowserShelfSamples, openShelfPreview, removeShelfFiles, shelf, undoShelfRemove } from '@/features/files/shelf'

const plain = ref(false)
const previewMode = new URLSearchParams(location.search).get('preview')
const settingsStandalone = previewMode === 'settings-window'
const scene = ref('normal')
const keepOpen = ref(false)
const feedback = ref({ kind: '', sequence: 0 })
const telemetryDemoSample = ref<TelemetrySnapshot | null>(null)
const demoNote = ref('先试任务胶囊，再载入文件体验预览与撤销。')
const imageDialog = ref<HTMLDialogElement | null>(null)
const imageDemoHeader = ref<HTMLElement | null>(null)
const imageDemoSize = ref({ width: 720, height: 540 })
watch(browserImagePreview, async content => {
  await nextTick()
  if (content) {
    if (!imageDialog.value?.open) imageDialog.value?.showModal()
    imageDialog.value?.querySelector<HTMLButtonElement>('.close-button')?.focus({ preventScroll: true })
  } else imageDialog.value?.close()
})
function resizeImageDemo(size: { width: number; height: number }): void {
  imageDemoSize.value = { width: size.width + 2, height: size.height + (imageDemoHeader.value?.offsetHeight ?? 36) + 2 }
}
function closeImageDemo(): void { imageDialog.value?.close(); browserImagePreview.value = null }
state.apps = previewApps
state.icons = previewIcons
state.ui.islandExpanded = false
state.ui.fullscreenSuppressed = false
setTheme('light')
if (previewMode === 'settings' || settingsStandalone) state.ui.settingsOpen = true

// 只模拟宿主通知，不改变真实 Windows 前台窗口或显示模式。
watch([scene, () => state.ui.islandAppearance.avoidFullscreen], () => {
  state.ui.fullscreenSuppressed = state.ui.islandAppearance.avoidFullscreen && scene.value !== 'normal'
  state.ui.islandExpanded = false
  keepOpen.value = false
  closeShelfPreview()
})

function clearTask(): void {
  state.ui.translationPending = 0
  state.translation.status = 'idle'
  state.ui.paused = false
  state.settings.recordClipboard = true
  shelf.busy = false; shelf.activity = ''
  telemetryDemoSample.value = null
  feedback.value = { kind: '', sequence: feedback.value.sequence + 1 }
}
async function capsuleDemo(kind: string): Promise<void> {
  const wasPaused = state.ui.paused
  const wasTelemetry = telemetryDemoSample.value !== null
  clearTask(); closeShelfPreview(); closeSettings()
  keepOpen.value = false; scene.value = 'normal'
  await nextTick()
  state.ui.islandExpanded = false
  if (kind === 'working') {
    state.ui.translationPending = 1
    demoNote.value = '正在模拟翻译中；点击完成、失败或结束任务切换状态。'
  } else if (kind === 'paused') {
    state.ui.paused = !wasPaused
    state.settings.recordClipboard = wasPaused
    demoNote.value = state.ui.paused ? '模拟暂停记录；再次点击可恢复。' : '已恢复演示记录状态。'
  } else if (kind === 'telemetry') {
    if (!wasTelemetry) {
      telemetryDemoSample.value = { netState: 'ready', downBps: 12_980_000, upBps: 368_000, cpuPercent: 42, memoryPercent: 68 }
      demoNote.value = '正在演示合成微读数样本（非真实系统数据）；再次点击或点结束任务可关闭。'
    } else {
      demoNote.value = '已关闭合成微读数演示，胶囊恢复时间显示。'
    }
  } else if (kind) {
    feedback.value = { kind, sequence: feedback.value.sequence + 1 }
    demoNote.value = '结果提示保持约 2.6 秒，随后恢复暂存数量或时间。'
  } else demoNote.value = '已结束模拟任务，胶囊显示暂存数量或时间。'
}
async function filesDemo(id?: string, reload = false): Promise<void> {
  clearTask(); closeSettings(); closeShelfPreview(); scene.value = 'normal'
  await nextTick()
  if (reload || !shelf.entries.length || (id && !shelf.entries.some(item => item.id === `preview-${id}`))) loadBrowserShelfSamples(makeShelfSamples())
  keepOpen.value = true
  state.ui.islandPage = 'files'; state.ui.panel = 'files'; state.ui.islandExpanded = true
  if (id) {
    const item = shelf.entries.find(item => item.id === `preview-${id}`)
    if (item) await openShelfPreview(item)
  }
  demoNote.value = '示例文件只存在网页内存；列表中点击、空格、移出和撤销都可以试。'
}
async function removeDemo(): Promise<void> {
  await filesDemo()
  const item = shelf.entries[0]
  if (item) await removeShelfFiles([item.id])
  demoNote.value = '已移出一个示例条目，可以点击撤销或在列表内按 Ctrl+Z。'
}
async function undoDemo(): Promise<void> {
  // 不重新载入样例，否则会清空当前撤销栈。
  closeShelfPreview(); keepOpen.value = true
  state.ui.islandPage = 'files'; state.ui.islandExpanded = true
  await undoShelfRemove()
  demoNote.value = '已撤销移出，示例条目恢复到原来的位置。'
}
async function resetDemo(): Promise<void> {
  clearTask(); closeSettings(); loadBrowserShelfSamples([])
  keepOpen.value = false; scene.value = 'normal'
  await nextTick()
  state.ui.islandExpanded = false; state.ui.islandPage = 'applications'; state.ui.panel = null
  demoNote.value = '演示已重置；保留当前主题、尺寸和 WebGL 选择。'
}

function keydown(event: KeyboardEvent): void {
  if (event.key === 'Escape' && !event.isComposing && state.ui.settingsOpen) closeSettings()
}
onMounted(() => window.addEventListener('keydown', keydown))
onBeforeUnmount(() => { window.removeEventListener('keydown', keydown); browserImagePreview.value = null })
</script>

<template>
  <div class="preview" :class="{ plain, dark: ['dark', 'midnight', 'mecha', 'pixel'].includes(state.settings.theme) }">
    <header class="preview-tools">
      <span>DesktopHub <small>实底高对比预览</small></span>
      <div class="preview-options">
        <a href="?preview=styles" style="color: inherit; font-weight: 600">十款样式实验室 ↗</a>
        <button type="button" @click="state.ui.settingsOpen = true">体验设置</button>
        <button type="button" :aria-pressed="state.settings.theme === 'light'" @click="setTheme('light')">浅色</button>
        <button type="button" :aria-pressed="state.settings.theme === 'dark'" @click="setTheme('dark')">深色</button>
        <button type="button" :aria-pressed="state.settings.theme === 'midnight'" @click="setTheme('midnight')">星舰仪表</button>
        <button type="button" :aria-pressed="state.settings.theme === 'mecha'" @click="setTheme('mecha')">琥珀重工</button>
        <button type="button" :aria-pressed="state.settings.theme === 'scroll'" @click="setTheme('scroll')">海棠书卷</button>
        <button type="button" :aria-pressed="state.settings.theme === 'pixel'" @click="setTheme('pixel')">像素掌机</button>
        <label>尺寸<select v-model.number="state.ui.islandAppearance.scale"><option :value="75">75%</option><option :value="100">100%</option><option :value="125">125%</option></select></label>
        <label><input v-model="plain" type="checkbox" />纯色背景</label>
        <label>场景<select v-model="scene" aria-label="模拟窗口状态"><option value="normal">桌面／普通窗口</option><option value="maximized">窗口最大化</option><option value="fullscreen">游戏／全屏</option></select></label>
        <label><input v-model="state.ui.islandAppearance.webgl" type="checkbox" />WebGL 边缘光效</label>
        <button type="button" @click="state.ui.islandExpanded = !state.ui.islandExpanded">{{ state.ui.islandExpanded ? '收起' : '展开' }}</button>
      </div>
    </header>
    <div class="preview-workbench">
    <main class="preview-stage">
      <IslandEntry :preview-feedback="feedback" :preview-keep-open="keepOpen" :preview-telemetry="telemetryDemoSample" />
    </main>
    <aside class="preview-lab" aria-label="效果测试">
      <header><div><span class="lab-eyebrow">交互演示</span><h1>点一下，看效果</h1></div><button type="button" class="reset-demo" @click="resetDemo">重置演示</button></header>
      <fieldset><legend>任务胶囊</legend><div class="demo-buttons">
        <button type="button" :aria-pressed="telemetryDemoSample !== null" @click="capsuleDemo('telemetry')">模拟微读数</button>
        <button type="button" :aria-pressed="state.ui.translationPending > 0" @click="capsuleDemo('working')">翻译中</button>
        <button type="button" @click="capsuleDemo('translated')">翻译完成</button>
        <button type="button" @click="capsuleDemo('translation-error')">翻译失败</button>
        <button type="button" @click="capsuleDemo('copied')">已复制</button>
        <button type="button" :aria-pressed="state.ui.paused" @click="capsuleDemo('paused')">{{ state.ui.paused ? '恢复记录' : '暂停记录' }}</button>
        <button type="button" @click="capsuleDemo('')">结束任务</button>
      </div></fieldset>
      <fieldset><legend>文件预览与撤销</legend><div class="demo-buttons">
        <button type="button" @click="filesDemo(undefined, true)">载入示例</button>
        <button type="button" @click="filesDemo('text')">文本预览</button>
        <button type="button" @click="filesDemo('image')">图片预览</button>
        <button type="button" @click="filesDemo('missing')">文件失效</button>
        <button type="button" @click="removeDemo">移出一项</button>
        <button type="button" :disabled="!shelf.undoCount" @click="undoDemo">撤销移出</button>
      </div></fieldset>
      <label class="keep-open"><input v-model="keepOpen" type="checkbox" />保持面板展开</label>
      <p class="lab-note" role="status">{{ demoNote }}</p>
      <p class="lab-help">试细线：顶部「场景」选最大化或全屏，鼠标移入立即展开，移开后收回。</p>
      <small class="lab-scope">光轨舱门 · 中转文件与声音为模拟，不改变系统音量。</small>
    </aside>
    </div>
    <p class="preview-note">拖动胶囊或展开后的顶部握柄调整位置。<br />黑白实底、主题跟随与局部 WebGL 光晕。</p>
    <SettingsWindow v-if="state.ui.settingsOpen" :standalone="settingsStandalone" />
    <dialog ref="imageDialog" class="image-demo" aria-label="独立图片预览演示" :style="{ width: `${imageDemoSize.width}px`, height: `${imageDemoSize.height}px` }" @cancel.prevent="closeImageDemo" @close="browserImagePreview = null">
      <header ref="imageDemoHeader" class="image-demo-header"><span>网页演示 · 置顶仅展示状态</span><div><button type="button" :aria-pressed="state.settings.theme === 'light'" @click="setTheme('light')">浅色</button><button type="button" :aria-pressed="state.settings.theme === 'dark'" @click="setTheme('dark')">深色</button><button type="button" :aria-pressed="state.settings.theme === 'midnight'" @click="setTheme('midnight')">星舰仪表</button></div></header>
      <ImagePreviewWindow v-if="browserImagePreview" :demo-content="browserImagePreview" @close="closeImageDemo" @resize="resizeImageDemo" />
    </dialog>
    <div class="toasts" role="status" aria-live="polite">
      <div v-for="toast in state.toasts" :key="toast.id" class="toast"><Icon :name="toast.kind === 'success' ? 'check' : 'info'" :size="16" /><span>{{ toast.text }}</span></div>
    </div>
  </div>
</template>

<style scoped>
.preview { --wallpaper: url('../mock/liquid-wallpaper.svg'); position: relative; min-height: 100%; height: 100%; overflow: auto; background: var(--wallpaper) center / cover fixed; color: #252b3c; }
.preview.dark { background-color: #101a31; background-image: linear-gradient(rgba(10,18,40,.7),rgba(10,18,40,.7)),var(--wallpaper); color: #e3e8f2; }
.preview.plain { background: #d9dfe9; }
.preview.dark.plain { background: #171e2d; }
.preview-tools { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; padding: 20px 28px; }
.preview-tools > span { font-size: 14px; font-weight: 600; }
.preview-tools small { margin-left: 10px; font-size: 12px; font-weight: 400; opacity: .65; }
.preview-options { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; font-size: 12px; }
.preview-options button { padding: 5px 10px; border-radius: 16px; }
.preview-options button[aria-pressed='true'] { background: rgba(255,255,255,.3); }
.preview-options label { display: flex; align-items: center; gap: 6px; }
select { font: inherit; color: inherit; background: transparent; border: 0; }
select option { color: #252b3c; }
.preview-stage { display: flex; justify-content: center; align-items: flex-start; padding: 20px 0 60px; min-height: 560px; }
.preview-workbench { display: grid; grid-template-columns: 252px minmax(0, 1fr); gap: 24px; padding: 0 28px; }
.preview-stage { grid-column: 2; grid-row: 1; min-width: 0; }
.preview-lab { grid-column: 1; grid-row: 1; align-self: start; margin-top: 20px; padding: 18px; border: 1px solid var(--line-strong); border-radius: 20px; background: var(--surface); color: var(--text); box-shadow: 0 10px 36px rgb(0 0 0 / .08); }
.preview-lab header { display: flex; align-items: flex-start; justify-content: space-between; gap: 8px; }
.lab-eyebrow { font-size: 10px; color: var(--text-2); letter-spacing: 1px; }
.preview-lab h1 { font-size: 16px; line-height: 1.5; margin: 4px 0 16px; }
.preview-lab fieldset { min-width: 0; padding: 0; border: 0; margin: 0 0 16px; }
.preview-lab legend { padding: 0; margin-bottom: 8px; font-size: 12px; color: var(--text-2); }
.demo-buttons { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px; }
.demo-buttons button { min-height: 32px; border-radius: 9px; padding: 6px; font-size: 12px; color: var(--text); background: var(--surface-2); border: 1px solid var(--line); }
.demo-buttons button:hover { background: var(--hover); }
.demo-buttons button[aria-pressed='true'] { background: var(--accent-weak); border-color: var(--accent); }
.demo-buttons button:disabled { color: var(--text-3); cursor: default; border-style: dashed; }
.preview-lab button:focus-visible, .preview-lab input:focus-visible, .preview-options button:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.reset-demo { flex: none; padding: 4px 0; font-size: 11px; color: var(--text-2); }
.keep-open { display: flex; align-items: center; gap: 6px; font-size: 12px; }
.keep-open input { accent-color: var(--accent); }
.lab-note, .lab-help { font-size: 11px; line-height: 1.7; color: var(--text-2); margin: 12px 0 0; }
.lab-help { padding-top: 10px; border-top: 1px solid var(--line); }
.lab-scope { display: block; margin-top: 12px; font-size: 10px; line-height: 1.6; color: var(--text-3); }
@media (max-width: 900px) {
  .preview-workbench { display: flex; flex-direction: column; gap: 0; }
  .preview-lab { width: 100%; margin: 0 0 24px; }
  .preview-stage { min-height: 180px; }
  .demo-buttons { grid-template-columns: repeat(3, minmax(0, 1fr)); }
}
.preview-note { margin: 0 28px 24px; font-size: 11px; line-height: 1.8; opacity: .7; }
.toasts { position: fixed; bottom: 30px; left: 50%; transform: translateX(-50%); display: grid; gap: 8px; z-index: var(--z-toast); pointer-events: none; }
.toast { display: flex; align-items: center; gap: 8px; padding: 10px 16px; border-radius: 20px; background: var(--surface); color: var(--text); box-shadow: 0 4px 18px rgba(0,0,0,.1); font-size: 13px; }
.image-demo { max-width: calc(100vw - 32px); max-height: calc(100vh - 32px); padding: 0; border: 1px solid var(--line-strong); border-radius: 18px; background: var(--surface); color: var(--text); box-shadow: 0 24px 80px rgb(0 0 0 / .25); overflow: hidden; }
.image-demo[open] { display: flex; flex-direction: column; }
.image-demo::backdrop { background: rgb(8 12 22 / .4); }
.image-demo-header { display: flex; flex: none; justify-content: space-between; align-items: center; gap: 8px; padding: 6px 12px; background: var(--surface-2); color: var(--text-2); font-size: 11px; }
.image-demo-header > div { display: flex; gap: 4px; }
.image-demo-header button { padding: 3px 6px; border-radius: 5px; white-space: nowrap; }
.image-demo-header button[aria-pressed='true'] { color: var(--text); background: var(--surface); }
.image-demo :deep(.image-preview-window) { flex: 1; height: auto; }
</style>
