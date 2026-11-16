<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { Window } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { invokeHost, windowLabel } from '@/lib/desktop/desktop'
const preview = ref<{ dataUri: string; name: string } | null>(null)
const error = ref('')
const loaded = ref(false)
const canvas = ref<HTMLElement | null>(null)
let observer: ResizeObserver | undefined
function measureScale(): void {
  if (!loaded.value || !canvas.value) return
  scale.value = Math.min(canvas.value.clientWidth / original.value.width, canvas.value.clientHeight / original.value.height) * 100
}
onBeforeUnmount(() => observer?.disconnect())
const pinned = ref(false)
const scale = ref(100)
const original = ref({ width: 640, height: 400 })
const current = Window.getByLabel(windowLabel).then(value => {
  if (!value) throw new Error('预览窗口已关闭')
  return value
})
void current.catch(e => { error.value = String(e) })
const dimensions = computed(() => `${original.value.width} × ${original.value.height}`)
onMounted(async () => {
  observer = new ResizeObserver(measureScale)
  if (canvas.value) observer.observe(canvas.value)
  try { preview.value = await invokeHost('image_preview_content') }
  catch (e) { error.value = String(e) }
})
async function close(): Promise<void> {
  try { await (await current)?.close() } catch (e) { error.value = String(e) }
}
async function drag(event: PointerEvent): Promise<void> {
  if (event.button !== 0) return
  try { await (await current)?.startDragging() } catch (e) { error.value = String(e) }
}
async function togglePin(): Promise<void> {
  try { await invokeHost('pin_image_preview', { pinned: !pinned.value }); pinned.value = !pinned.value }
  catch (e) { error.value = String(e) }
}
function imageLoaded(event: Event): void {
  const img = event.target as HTMLImageElement
  original.value = { width: img.naturalWidth, height: img.naturalHeight }
  loaded.value = true
  void resizeTo(Math.min(100, Math.floor(Math.min((screen.availWidth - 80) / img.naturalWidth, (screen.availHeight - 150) / img.naturalHeight) * 100)))
}
async function resizeTo(value: number): Promise<void> {
  const maximum = Math.min(400, (screen.availWidth - 40) / original.value.width * 100, (screen.availHeight - 130) / original.value.height * 100)
  const requested = Math.max(1, Math.min(value, maximum))
  const width = Math.max(280, original.value.width * requested / 100)
  const imageHeight = original.value.height * requested / 100
  try {
    const target = await current
    await target.setSize(new LogicalSize(width, Math.max(180, imageHeight + 76)))
    // 先确定宽度，再测量工具栏换行后的真实高度，不能用固定高度挤压图片。
    await new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())))
    const chromeHeight = document.querySelector('header')!.getBoundingClientRect().height +
      document.querySelector('footer')!.getBoundingClientRect().height
    await target.setSize(new LogicalSize(width, Math.max(180, imageHeight + chromeHeight)))
  }
  catch (e) { error.value = String(e) }
}
</script>

<template>
  <main class="image-preview">
    <header>
      <span class="name" :title="preview?.name">{{ preview?.name ?? '图片预览' }}</span>
      <button type="button" :disabled="!loaded" :aria-pressed="pinned" @click="togglePin">{{ pinned ? '取消置顶' : '钉在屏幕上' }}</button>
      <button type="button" @click="close">关闭</button>
    </header>
    <p v-if="error" role="alert">{{ error }}</p>
    <div ref="canvas" class="canvas" @pointerdown="drag">
      <img v-if="preview" :src="preview.dataUri" :alt="preview.name" draggable="false" @load="imageLoaded" @error="error = '图片无法解码或格式不受支持'" />
    </div>
    <footer>
      <span>{{ dimensions }} · 拖动图片移动，拖动窗边调整大小</span>
      <button type="button" :disabled="!loaded" aria-label="缩小图片" @click="resizeTo(scale - 10)">−</button>
      <input type="number" min="1" max="400" :value="Math.round(scale)" :disabled="!loaded" aria-label="图片缩放百分比" @change="resizeTo(Number(($event.target as HTMLInputElement).value) || 100)" />
      <span>%</span><button type="button" :disabled="!loaded" aria-label="放大图片" @click="resizeTo(scale + 10)">＋</button>
    </footer>
  </main>
</template>

<style scoped>
.image-preview { height: 100vh; display: flex; flex-direction: column; background: var(--surface); color: var(--text); }
header, footer { display: flex; flex: none; align-items: center; gap: 8px; padding: 8px 10px; }
header { border-bottom: 1px solid var(--line); }
.name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
button { padding: 4px 8px; border-radius: 5px; background: var(--hover); white-space: nowrap; }
button[aria-pressed='true'] { background: var(--accent-weak); color: var(--accent); }
.canvas { flex: 1; min-height: 0; display: flex; align-items: center; justify-content: center; overflow: hidden; cursor: move; background: var(--surface-2); }
img { width: 100%; height: 100%; max-width: 100%; max-height: 100%; object-fit: contain; user-select: none; }
footer { font-size: 11px; flex-wrap: wrap; }
footer > span:first-child { flex: 1; }
input { width: 54px; background: var(--surface-2); border: 1px solid var(--line); }
p { margin: 6px 10px; color: var(--danger); }
</style>
