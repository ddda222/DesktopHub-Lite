<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { Window } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import HubMotionConfig from '@/components/HubMotionConfig.vue'
import Icon from '@/components/Icon.vue'
import { invokeHost, isDesktop, onUiState, readUiState, windowLabel, type UiState } from '@/lib/desktop/desktop'
import { state } from '@/store'
import { isThemeMode, normalizeThemeMode } from '@/lib/themes'

const props = defineProps<{ demoContent?: { dataUri: string; name: string } }>()
const emit = defineEmits<{ close: []; resize: [size: { width: number; height: number }] }>()
const preview = ref<{ dataUri: string; name: string } | null>(null)
const error = ref('')
const loaded = ref(false)
const imageFailed = ref(false)
const pinned = ref(false)
const pinning = ref(false)
const resizing = ref(false)
const scale = ref(100)
const original = ref({ width: 0, height: 0 })
const canvas = ref<HTMLElement | null>(null)
const header = ref<HTMLElement | null>(null)
const footer = ref<HTMLElement | null>(null)
const closeButton = ref<HTMLButtonElement | null>(null)
let observer: ResizeObserver | undefined
let stopTheme: (() => void) | undefined
let disposed = false
let resizeVersion = 0
const current = isDesktop ? Window.getByLabel(windowLabel).then(value => {
  if (!value) throw new Error('预览窗口已关闭')
  return value
}) : Promise.resolve(null)
void current.catch(e => { error.value = String(e) })
const dimensions = computed(() => loaded.value ? original.value.width + ' × ' + original.value.height + ' 像素' : '图片预览')

function measureScale(): void {
  if (!loaded.value || !canvas.value) return
  scale.value = Math.max(1, Math.min((canvas.value.clientWidth - 32) / original.value.width,
    (canvas.value.clientHeight - 32) / original.value.height) * 100)
}
function syncTheme(ui: UiState): void {
  state.settings.theme = isThemeMode(ui.theme) ? ui.theme : normalizeThemeMode(ui.theme)
}
async function readImage(): Promise<void> {
  error.value = ''; imageFailed.value = false; loaded.value = false; preview.value = null
  try {
    const content = isDesktop ? await invokeHost<{ dataUri: string; name: string }>('image_preview_content') : props.demoContent
    if (!content) throw new Error('没有可预览的图片')
    if (!disposed) preview.value = content
  } catch (e) { error.value = e instanceof Error ? e.message : String(e) }
}
onMounted(async () => {
  observer = new ResizeObserver(measureScale)
  if (canvas.value) observer.observe(canvas.value)
  if (isDesktop) {
    const stop = await onUiState(syncTheme)
    if (disposed) { stop(); return }
    stopTheme = stop
    syncTheme(await readUiState())
  }
  if (!disposed) await readImage()
})
onBeforeUnmount(() => { disposed = true; resizeVersion++; observer?.disconnect(); stopTheme?.() })
async function close(): Promise<void> {
  if (!isDesktop) { emit('close'); return }
  try { await (await current)?.close() } catch (e) { error.value = String(e) }
}
async function drag(event: PointerEvent): Promise<void> {
  if (!isDesktop || event.button !== 0 || (event.target as Element).closest('button, input')) return
  try { await (await current)?.startDragging() } catch (e) { error.value = String(e) }
}
async function togglePin(): Promise<void> {
  if (pinning.value) return
  pinning.value = true
  try {
    if (isDesktop) await invokeHost('pin_image_preview', { pinned: !pinned.value })
    pinned.value = !pinned.value
  } catch (e) { error.value = String(e) }
  finally { pinning.value = false }
}
function chromeHeight(): number {
  return (header.value?.getBoundingClientRect().height ?? 64) + (footer.value?.getBoundingClientRect().height ?? 56) + 32
}
function availableSize(): { width: number; height: number } {
  return isDesktop ? { width: screen.availWidth - 40, height: screen.availHeight - 80 }
    : { width: window.innerWidth - 48, height: window.innerHeight - 100 }
}
function maximumScale(): number {
  const available = availableSize()
  return Math.min(400, (available.width - 32) / original.value.width * 100,
    (available.height - chromeHeight()) / original.value.height * 100)
}
function imageLoaded(event: Event): void {
  const img = event.target as HTMLImageElement
  original.value = { width: img.naturalWidth, height: img.naturalHeight }
  loaded.value = true
  void fitScreen()
}
function fitScreen(): Promise<void> { return resizeTo(Math.min(100, maximumScale())) }
async function setWindowSize(width: number, height: number): Promise<void> {
  if (isDesktop) await (await current)?.setSize(new LogicalSize(width, height))
  else { emit('resize', { width, height }); await nextTick() }
}
async function resizeTo(value: number): Promise<void> {
  if (!loaded.value || !Number.isFinite(value)) return
  const version = ++resizeVersion
  resizing.value = true
  const requested = Math.max(1, Math.min(value, maximumScale()))
  const width = Math.max(280, original.value.width * requested / 100 + 32)
  const imageHeight = original.value.height * requested / 100
  try {
    await setWindowSize(width, Math.max(180, imageHeight + chromeHeight()))
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())))
    if (disposed || version !== resizeVersion) return
    // 窄窗口工具栏会换行，按实际高度补足画布，保持原有缩放窗口的方式。
    await setWindowSize(width, Math.max(180, imageHeight + chromeHeight()))
    await nextTick()
    measureScale()
  } catch (e) { error.value = String(e) }
  finally { if (version === resizeVersion) resizing.value = false }
}
function changeScale(event: Event): void {
  const input = event.target as HTMLInputElement
  void resizeTo(Number(input.value) || 100).finally(() => { input.value = String(Math.round(scale.value)) })
}
async function retry(): Promise<void> { await readImage(); await nextTick(); closeButton.value?.focus({ preventScroll: true }) }
function keydown(event: KeyboardEvent): void {
  if (event.isComposing || event.key !== 'Escape') return
  event.preventDefault(); event.stopPropagation(); void close()
}
</script>

<template>
  <HubMotionConfig>
    <main class="image-preview-window" :class="{ pinned }" aria-label="独立图片预览" @keydown="keydown">
    <header ref="header" class="viewer-header" @pointerdown="drag">
      <span class="image-mark"><Icon name="image" :size="20" /></span>
      <div class="image-heading"><h1 :title="preview?.name">{{ preview?.name ?? '图片预览' }}</h1><p>{{ dimensions }}</p></div>
      <button type="button" class="pin-button" :disabled="!loaded || pinning" :aria-pressed="pinned" :title="pinned ? '取消窗口置顶' : '让图片保持在其他窗口上方'" @click="togglePin"><Icon name="pin" :size="15" /><span>{{ pinned ? '已置顶' : '置顶' }}</span></button>
      <button ref="closeButton" type="button" class="close-button" aria-label="关闭图片预览" title="关闭（Esc）" @click="close"><Icon name="close" :size="18" /></button>
    </header>
    <p v-if="error && loaded" class="action-error" role="alert">{{ error }}</p>
    <div ref="canvas" class="canvas" :class="{ ready: loaded }" @pointerdown="drag" :aria-busy="!loaded && !error && !imageFailed">
      <img v-if="preview && !imageFailed" :src="preview.dataUri" :alt="preview.name" draggable="false" @load="imageLoaded" @error="imageFailed = true; loaded = false" />
      <div v-if="!loaded" class="canvas-state" :role="error || imageFailed ? 'alert' : 'status'">
        <Icon name="image" :size="32" />
        <strong>{{ error || imageFailed ? '暂时无法显示图片' : '正在读取图片' }}</strong>
        <span v-if="error || imageFailed">{{ error || '图片无法解码或格式不受支持' }}</span>
        <button v-if="error || imageFailed" type="button" @click="retry">重新读取</button>
      </div>
    </div>
    <footer ref="footer" class="viewer-footer">
      <span class="move-hint">{{ isDesktop ? '拖动图片移动窗口' : '桌面版可拖动图片移动窗口' }}</span>
      <div class="viewer-tools" :aria-busy="resizing">
        <div class="zoom-control" role="group" aria-label="图片缩放">
          <button type="button" :disabled="!loaded || scale <= 1" aria-label="缩小图片" @click="resizeTo(scale - 10)"><span aria-hidden="true">−</span></button>
          <label class="zoom-value"><input type="number" min="1" max="400" :value="Math.round(scale)" :disabled="!loaded" aria-label="图片缩放百分比" @change="changeScale" @keydown.enter="changeScale" /><span>%</span></label>
          <button type="button" :disabled="!loaded || scale >= maximumScale() - 0.5" aria-label="放大图片" @click="resizeTo(scale + 10)"><span aria-hidden="true">＋</span></button>
        </div>
        <button type="button" class="fit-button" :disabled="!loaded" title="按屏幕大小完整显示图片" @click="fitScreen">适应屏幕</button>
      </div>
    </footer>
  </main>
  </HubMotionConfig>
</template>

<style scoped>
.image-preview-window { width: 100%; height: 100%; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: var(--surface); color: var(--text); container-type: inline-size; }
.viewer-header, .viewer-footer { display: flex; flex: none; align-items: center; gap: 12px; padding: 12px 16px; }
.viewer-header { min-height: 64px; border-bottom: 1px solid var(--line); user-select: none; }
.image-mark { display: grid; place-items: center; flex: none; width: 36px; height: 36px; border-radius: 10px; background: var(--surface-2); color: var(--text-2); }
.image-heading { flex: 1; min-width: 0; }
.image-heading h1 { margin: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; font-weight: 600; line-height: 1.5; }
.image-heading p { margin: 2px 0 0; font-size: 11px; color: var(--text-2); font-variant-numeric: tabular-nums; }
button { display: inline-flex; align-items: center; justify-content: center; gap: 6px; min-height: 32px; border-radius: 8px; padding: 6px 10px; font-size: 12px; white-space: nowrap; }
button:hover:not(:disabled) { background: var(--hover); }
button:disabled { opacity: .45; cursor: default; }
button:focus-visible, input:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.pin-button { background: var(--surface-2); flex: none; min-width: 74px; }
.pin-button[aria-pressed='true'] { background: var(--text); color: var(--surface); }
.pin-button[aria-pressed='true']:hover { background: var(--text-2); }
.close-button { flex: none; width: 32px; padding: 0; color: var(--text-2); }
.close-button:hover { color: var(--danger); }
.canvas { position: relative; flex: 1; min-height: 0; display: flex; align-items: center; justify-content: center; overflow: hidden; padding: 16px; background-color: var(--surface-2); background-image: conic-gradient(var(--surface-3) 25%, transparent 0 50%, var(--surface-3) 0 75%, transparent 0); background-size: 16px 16px; }
.canvas.ready { cursor: move; }
.canvas img { width: 100%; height: 100%; object-fit: contain; user-select: none; filter: drop-shadow(0 3px 8px rgb(0 0 0 / .12)); }
.canvas-state { position: absolute; inset: 0; padding: 20px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; text-align: center; color: var(--text-2); background: var(--surface-2); font-size: 12px; }
.canvas-state strong { color: var(--text); font-size: 14px; }
.canvas-state button { background: var(--surface-3); }
.viewer-footer { min-height: 56px; justify-content: space-between; flex-wrap: wrap; border-top: 1px solid var(--line); }
.move-hint { font-size: 11px; color: var(--text-2); }
.viewer-tools { display: flex; align-items: center; gap: 8px; margin-left: auto; }
.zoom-control { display: flex; align-items: center; padding: 2px; border-radius: 10px; background: var(--surface-2); }
.zoom-control button { width: 28px; min-height: 28px; padding: 0; font-size: 16px; }
.zoom-value { display: flex; align-items: center; gap: 2px; padding-right: 6px; font-size: 11px; font-variant-numeric: tabular-nums; }
.zoom-value input { width: 42px; padding: 4px 0; border: 0; border-radius: 4px; background: transparent; color: var(--text); text-align: right; font-size: 12px; appearance: textfield; }
.zoom-value input::-webkit-inner-spin-button, .zoom-value input::-webkit-outer-spin-button { appearance: none; margin: 0; }
.zoom-value span { color: var(--text-2); }
.fit-button { border: 1px solid var(--line); }
.action-error { flex: none; margin: 0; padding: 8px 16px; color: var(--danger); font-size: 12px; background: var(--surface-2); }
@container (max-width: 500px) { .move-hint { display: none; } .viewer-tools { width: 100%; justify-content: space-between; } }
@container (max-width: 360px) { .image-mark { display: none; } .viewer-header { gap: 8px; padding-inline: 12px; } .viewer-footer { padding-inline: 12px; } }
</style>
