<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import type { FilePreview, ShelfItem } from './shelf'

const props = defineProps<{ item: ShelfItem; preview: FilePreview | null; loading: boolean; error: string }>()
const emit = defineEmits<{ close: []; reveal: []; pin: []; retry: [] }>()
const back = ref<HTMLButtonElement | null>(null)
const imageFailed = ref(false)
const imageSize = ref({ width: 0, height: 0 })
const imageMode = ref<'fit' | 'actual'>('fit')
const showInfo = ref(false)
const isImage = computed(() => props.preview?.kind === 'image')
const imageType = computed(() => props.item.name.split('.').pop()?.toUpperCase() || '图片')
watch(() => props.preview?.dataUri, () => {
  imageFailed.value = false
  imageSize.value = { width: 0, height: 0 }
  imageMode.value = 'fit'
})
function imageLoaded(event: Event): void {
  const image = event.target as HTMLImageElement
  imageSize.value = { width: image.naturalWidth, height: image.naturalHeight }
}
function retry(): void {
  imageFailed.value = false
  emit('retry')
  void nextTick(() => back.value?.focus({ preventScroll: true }))
}
const size = computed(() => {
  const value = props.preview?.size
  if (value == null) return props.item.isDir ? '文件夹' : '文件'
  if (value < 1024) return `${value} B`
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`
  return `${(value / 1024 / 1024).toFixed(1)} MiB`
})
const modified = computed(() => props.preview?.modifiedMs == null ? '' : new Date(props.preview.modifiedMs).toLocaleString('zh-CN', { hour12: false }))
onMounted(() => back.value?.focus({ preventScroll: true }))
function keydown(event: KeyboardEvent): void {
  if (event.isComposing || event.key !== 'Escape') return
  event.preventDefault(); event.stopPropagation(); emit('close')
}
</script>

<template>
  <section class="file-preview" :class="{ 'image-preview': isImage }" aria-label="文件预览" :aria-busy="loading" @keydown="keydown">
    <header :class="{ 'image-header': isImage }">
      <button ref="back" type="button" class="back" aria-label="返回文件列表" title="返回文件列表（Esc）" @click="emit('close')"><Icon name="back" :size="isImage ? 18 : 15" /><template v-if="!isImage">返回列表</template></button>
      <div v-if="isImage" class="image-heading">
        <h2 :title="item.name">{{ item.name }}</h2>
        <p><span>{{ imageType }}</span><span v-if="imageSize.width">{{ imageSize.width }} × {{ imageSize.height }}</span><span>{{ size }}</span></p>
      </div>
      <span v-else>只读预览</span>
      <kbd aria-label="按 Escape 返回">Esc</kbd>
    </header>
    <div v-if="!isImage" class="file-heading">
      <span class="file-icon"><Icon :name="item.isDir ? 'folder' : preview?.kind === 'image' ? 'image' : 'text'" :size="23" /></span>
      <div><h2>{{ item.name }}</h2><p>{{ size }}<template v-if="modified"> · {{ modified }}</template></p></div>
    </div>
    <div v-if="loading" class="preview-placeholder" role="status">正在读取文件…</div>
    <div v-else-if="error" class="preview-placeholder">
      <p class="preview-error" role="alert">{{ error }}</p>
      <button type="button" @click="retry">重新读取</button>
    </div>
    <template v-else-if="preview">
      <div v-if="preview.kind === 'image'" class="image-viewer">
        <div v-if="imageFailed" class="image-error" role="alert">
          <Icon name="image" :size="28" /><strong>暂时无法显示这张图片</strong>
          <span>可以重新读取，或打开所在位置查看。</span>
          <button type="button" @click="retry">重新读取</button>
        </div>
        <div v-else class="image-canvas" :class="{ 'actual-size': imageMode === 'actual' }" :tabindex="imageMode === 'actual' ? 0 : undefined" aria-label="图片画布，可在原尺寸模式滚动查看">
          <div class="image-frame"><img :src="preview.dataUri ?? undefined" :alt="item.name" draggable="false" @load="imageLoaded" @error="imageFailed = true" /></div>
        </div>
        <div v-if="!imageFailed" class="image-toolbar">
          <span>图片预览</span>
          <div class="view-modes" role="group" aria-label="图片显示比例">
            <button type="button" :aria-pressed="imageMode === 'fit'" @click="imageMode = 'fit'">适应窗口</button>
            <button type="button" :aria-pressed="imageMode === 'actual'" @click="imageMode = 'actual'">原尺寸</button>
          </div>
        </div>
      </div>
      <pre v-else-if="preview.kind === 'text'" class="text-content" tabindex="0" aria-label="文件文本内容">{{ preview.text || '（空文件）' }}</pre>
      <div v-else class="preview-placeholder"><Icon :name="item.isDir ? 'folder' : 'info'" :size="30" /><p>{{ preview.notice }}</p></div>
      <p v-if="preview.notice && ['image', 'text'].includes(preview.kind)" class="notice">{{ preview.notice }}</p>
    </template>
    <footer v-if="isImage" class="image-actions">
      <button type="button" class="info-toggle" :aria-expanded="showInfo" :aria-controls="`preview-info-${item.id}`" @click="showInfo = !showInfo"><Icon name="info" :size="15" />文件信息</button>
      <div>
        <button type="button" class="location-action" title="打开文件所在位置" @click="emit('reveal')"><Icon name="folder" :size="16" />所在位置</button>
        <button v-if="!imageFailed && !loading && !error" type="button" class="pin-action" title="打开独立图片窗口，可缩放与钉图" @click="emit('pin')"><Icon name="external" :size="15" />独立预览</button>
      </div>
    </footer>
    <div v-if="isImage" v-show="showInfo" :id="`preview-info-${item.id}`" class="image-details">
      <dl><div><dt>位置</dt><dd>{{ item.path }}</dd></div><div v-if="modified"><dt>修改时间</dt><dd>{{ modified }}</dd></div></dl>
      <p>仅预览，不会修改原文件</p>
    </div>
    <div v-if="!isImage" class="file-location"><span>文件位置</span><p class="file-path scroll-y">{{ item.path }}</p></div>
    <footer v-if="!isImage">
      <span>原文件保持不变</span>
      <div>
        <button type="button" @click="emit('reveal')">打开所在位置<Icon name="external" :size="13" /></button>
      </div>
    </footer>
  </section>
</template>

<style scoped>
.file-preview { color: var(--text); padding: 4px 0; }
header, footer, footer > div { display: flex; align-items: center; gap: 8px; }
header { padding-bottom: 14px; }
header > span { margin-left: auto; font-size: 11px; color: var(--text-2); }
kbd { font: inherit; font-size: 10px; color: var(--text-2); border: 1px solid var(--line); border-radius: var(--r-chip); padding: 2px 4px; }
button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 30px; padding: 6px 9px; border-radius: var(--r-chip); border: 1px solid var(--line); background: var(--surface-2); font-size: 12px; transition: background 120ms ease, border-color 120ms ease; }
.back { padding-left: 5px; }
button:hover:not(:disabled) { background: var(--hover); border-color: var(--line-strong); }
button:focus-visible, pre:focus-visible, .image-canvas:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.image-canvas:focus-visible { outline-offset: -2px; }
.file-heading { display: flex; align-items: center; gap: 11px; margin-bottom: 12px; }
.file-icon { display: grid; place-items: center; width: 42px; height: 42px; flex: none; border-radius: var(--r-tile); background: var(--surface-2); color: var(--accent); }
.file-heading > div { min-width: 0; }
h2 { font-size: 14px; line-height: 1.5; margin: 0; overflow-wrap: anywhere; }
.file-heading p { font-size: 11px; color: var(--text-2); margin: 3px 0 0; }
.text-content, .preview-placeholder { border: 1px solid var(--line); border-radius: var(--r-tile); background: var(--surface-2); }
.text-content { max-height: 220px; min-height: 100px; overflow: auto; margin: 0; padding: 14px; font: 12px/1.75 var(--font-mono); white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; }
.image-header { gap: 12px; padding: 0 0 12px; min-height: 52px; }
.image-header .back { width: 32px; height: 32px; flex: none; padding: 0; }
.image-heading { flex: 1; min-width: 0; }
.image-heading h2 { font-weight: 600; user-select: text; }
.image-heading p { display: flex; flex-wrap: wrap; gap: 4px 8px; margin: 3px 0 0; font-size: 11px; color: var(--text-2); font-variant-numeric: tabular-nums; }
.image-heading p span + span::before { content: '·'; margin-right: 8px; color: var(--text-3); }
.image-header kbd { flex: none; }
.image-viewer { border: 1px solid var(--line); border-radius: var(--r-card); overflow: hidden; background: var(--surface-2); }
.image-canvas { height: 252px; overflow: auto; background-color: var(--surface-2); background-image: conic-gradient(var(--surface-3) 25%, transparent 0 50%, var(--surface-3) 0 75%, transparent 0); background-size: 16px 16px; }
.image-frame { height: 100%; display: grid; place-items: center; padding: 12px; }
.image-frame img { display: block; max-width: 100%; max-height: 228px; object-fit: contain; box-shadow: 0 2px 8px rgb(0 0 0 / .12); }
.actual-size .image-frame { width: max-content; min-width: 100%; height: max-content; min-height: 100%; }
.actual-size img { max-width: none; max-height: none; }
.image-toolbar { min-height: 40px; padding: 4px 8px 4px 12px; display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 8px; border-top: 1px solid var(--line); background: var(--surface-2); }
.image-toolbar > span { font-size: 11px; color: var(--text-2); }
.view-modes { display: flex; gap: 2px; }
.view-modes button { min-height: 28px; padding: 4px 10px; color: var(--text-2); background: transparent; font-size: 11px; }
.view-modes button[aria-pressed='true'] { background: var(--accent-weak); border-color: var(--accent); color: var(--text); font-weight: 600; }
.image-actions { margin-top: 12px; padding: 0; border: 0; gap: 8px; }
.image-actions button { min-height: 34px; padding: 6px 10px; white-space: nowrap; }
.image-actions .info-toggle { padding-left: 0; background: transparent; color: var(--text-2); }
.image-actions .info-toggle[aria-expanded='true'] { color: var(--text); }
.image-actions .location-action { background: var(--surface-2); }
.image-actions .pin-action { background: var(--text); color: var(--surface); font-weight: 600; }
.image-actions .pin-action:hover { background: var(--text-2); }
.image-details { border-top: 1px solid var(--line); margin-top: 12px; padding-top: 12px; font-size: 11px; line-height: 1.6; }
.image-details dl { margin: 0; }
.image-details dl > div { display: grid; grid-template-columns: 56px minmax(0, 1fr); gap: 8px; margin-bottom: 4px; }
.image-details dt, .image-details p { color: var(--text-2); }
.image-details dd { margin: 0; overflow-wrap: anywhere; user-select: text; }
.image-details p { margin: 8px 0 0; }
.image-error { min-height: 252px; padding: 24px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; text-align: center; font-size: 12px; color: var(--text-2); }
.image-error strong { font-size: 13px; color: var(--text); }
.preview-placeholder { min-height: 140px; padding: 18px; display: flex; flex-direction: column; justify-content: center; align-items: center; gap: 10px; text-align: center; font-size: 12px; color: var(--text-2); }
.preview-placeholder p { margin: 0; line-height: 1.7; }
.preview-error { color: var(--danger); }
.notice, .file-location { color: var(--text-2); font-size: 11px; line-height: 1.6; margin: 8px 0; }
.file-location { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 8px; }
.file-path { margin: 0; max-height: 6.4em; overflow-wrap: anywhere; user-select: text; color: var(--text); }
footer { flex-wrap: wrap; justify-content: space-between; border-top: 1px solid var(--line); padding-top: 10px; margin-top: 10px; }
footer > span { color: var(--text-2); font-size: 11px; }
footer > div { flex-wrap: wrap; margin-left: auto; }
</style>
