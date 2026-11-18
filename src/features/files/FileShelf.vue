<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import { pushToast } from '@/store'
import FileShelfPreview from './FileShelfPreview.vue'
import { addShelfFiles, refreshShelf, removeShelfFiles, undoShelfRemove, openShelfPreview, closeShelfPreview, shelf, type ShelfItem } from './shelf'

defineProps<{ dragOver: boolean }>()
const selected = ref<string[]>([])
const availableSelected = computed(() => selected.value.filter(id => shelf.entries.some(e => e.id === id && e.available)))
const root = ref<HTMLElement | null>(null)
const addButton = ref<HTMLButtonElement | null>(null)
let returnId: string | undefined
let dragged = false
let pointer: { id: string; x: number; y: number } | undefined
watch(() => shelf.entries, entries => { selected.value = selected.value.filter(id => entries.some(e => e.id === id)) })
async function pick(): Promise<void> {
  if (!isDesktop) { shelf.message = '请在桌面版添加文件'; return }
  shelf.busy = true; shelf.error = ''; shelf.activity = '选择文件中'
  try {
    const paths = await invokeHost<string[] | null>('pick_shelf_files')
    if (paths) await addShelfFiles(paths)
  } catch (error) { shelf.error = String(error) }
  finally { shelf.busy = false; shelf.activity = '' }
}
async function remove(ids: string[]): Promise<void> {
  await removeShelfFiles([...ids])
  await nextTick()
  root.value?.querySelector<HTMLButtonElement>('.undo-button')?.focus({ preventScroll: true })
}
async function reveal(item: ShelfItem): Promise<void> {
  if (!isDesktop) { pushToast('打开所在位置请在桌面版中使用', 'info'); return }
  try { await invokeHost('reveal_shelf_file', { id: item.id }) }
  catch (error) { shelf.error = String(error); await refreshShelf() }
}
async function previewImage(item: ShelfItem): Promise<void> {
  if (!isDesktop) {
    const dataUri = shelf.preview?.dataUri
    if (dataUri) {
      const { browserImagePreview } = await import('@/mock/image-preview')
      browserImagePreview.value = { dataUri, name: item.name }
    }
    return
  }
  try { await invokeHost('open_image_preview', { source: 'shelf', id: item.id }) }
  catch (error) { shelf.error = String(error) }
}
function preview(item: ShelfItem): void {
  if (dragged || shelf.busy) return
  returnId = item.id
  void openShelfPreview(item)
}
async function closePreview(): Promise<void> {
  closeShelfPreview()
  await nextTick()
  const button = Array.from(root.value?.querySelectorAll<HTMLButtonElement>('.file-main') ?? [])
    .find(element => element.dataset.id === returnId)
  ;(button ?? addButton.value)?.focus({ preventScroll: true })
}
async function undo(): Promise<void> {
  await undoShelfRemove()
  await nextTick()
  if (!shelf.undoCount) addButton.value?.focus({ preventScroll: true })
}
function keydown(event: KeyboardEvent): void {
  if (event.isComposing || event.repeat || shelf.previewItem) return
  const target = event.target as HTMLElement
  if (target.closest('input, textarea, select, [contenteditable="true"]')) return
  if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 'z' && shelf.undoCount) {
    event.preventDefault(); event.stopPropagation(); void undo()
  }
}
function begin(event: PointerEvent, item: ShelfItem): void {
  dragged = false
  if (event.button !== 0 || !item.available || shelf.busy) return
  pointer = { id: item.id, x: event.clientX, y: event.clientY }
}
function release(): void { pointer = undefined }
async function move(event: PointerEvent): Promise<void> {
  if (!pointer || !(event.buttons & 1)) { release(); return }
  if (Math.hypot(event.clientX - pointer.x, event.clientY - pointer.y) < 6) return
  const id = pointer.id
  dragged = true
  release()
  const ids = selected.value.includes(id) ? availableSelected.value : [id]
  if (!isDesktop) { shelf.message = '请在桌面版拖出文件'; return }
  shelf.busy = true; shelf.activity = '正在拖出'; shelf.error = ''; shelf.message = '拖到目标软件或文件夹，松开即可'
  try {
    const dropped = await invokeHost<boolean>('drag_shelf_files', { ids })
    shelf.message = dropped ? '已交给目标，中转条目保留' : '已取消拖出'
  } catch (error) { shelf.error = String(error) }
  finally { shelf.busy = false; shelf.activity = ''; await refreshShelf() }
}
onMounted(() => {
  void refreshShelf()
  window.addEventListener('pointermove', move)
  window.addEventListener('pointerup', release)
  window.addEventListener('pointercancel', release)
})
onBeforeUnmount(() => {
  release(); closeShelfPreview()
  window.removeEventListener('pointermove', move)
  window.removeEventListener('pointerup', release)
  window.removeEventListener('pointercancel', release)
})
</script>

<template>
  <section ref="root" class="file-shelf" :class="{ receiving: dragOver }" aria-label="文件中转" @keydown="keydown">
    <p v-if="shelf.error" class="shelf-error" role="alert">{{ shelf.error }}</p>
    <FileShelfPreview v-if="shelf.previewItem" :key="shelf.previewItem.id" :item="shelf.previewItem" :preview="shelf.preview"
      :loading="shelf.previewLoading" :error="shelf.previewError" @close="closePreview" @reveal="reveal(shelf.previewItem)"
      @pin="previewImage(shelf.previewItem)" @retry="openShelfPreview(shelf.previewItem)" />
    <template v-else>
    <div class="shelf-tools">
      <span>{{ dragOver ? '松开，放入中转' : `${shelf.entries.length} 项暂存` }}</span>
      <div>
        <button ref="addButton" type="button" :disabled="shelf.busy" @click="pick">添加文件</button>
        <button v-if="selected.length" type="button" :disabled="shelf.busy" @click="remove(selected)">移出所选</button>
      </div>
    </div>
    <p class="shelf-help">点击或空格预览 · 按住拖出 · 移出可撤销</p>
    <div v-if="shelf.undoCount" class="shelf-undo">
      <span role="status">上次移出 {{ shelf.undoCount }} 项，原文件保留</span>
      <button class="undo-button" type="button" :disabled="shelf.busy" title="撤销上次移出（Ctrl+Z）" @click="undo">撤销</button>
    </div>
    <p v-else-if="shelf.message" class="shelf-message" role="status">{{ shelf.message }}</p>
    <div v-if="!shelf.entries.length" class="shelf-empty">
      <Icon name="folder" :size="28" />
      <span>{{ dragOver ? '松开即可暂存' : '把文件或文件夹拖到这里' }}</span>
      <small>重启后仍保留，移出不会删除原文件</small>
    </div>
    <ul v-else class="shelf-list">
      <li v-for="item in shelf.entries" :key="item.id" :class="{ missing: !item.available }">
        <input v-model="selected" type="checkbox" :value="item.id" :aria-label="`选择 ${item.name}`" :disabled="shelf.busy" />
        <button class="file-main" type="button" :data-id="item.id" :title="item.path" :disabled="shelf.busy"
          :aria-label="`预览 ${item.name}`" aria-description="单击或按空格预览，按住拖出" @pointerdown="begin($event, item)" @dragstart.prevent
          @keydown.space="dragged = false" @keydown.enter="dragged = false" @click="preview(item)">
          <Icon :name="item.isDir ? 'folder' : 'text'" :size="21" />
          <span><strong>{{ item.name }}</strong><small>{{ item.available ? item.path : '原文件已移动或删除，请重新添加' }}</small></span>
        </button>
        <button class="row-action" type="button" :disabled="!item.available || shelf.busy" title="打开所在位置" :aria-label="`定位 ${item.name}`" @click="reveal(item)"><Icon name="external" :size="16" /></button>
        <button class="row-action" type="button" :disabled="shelf.busy" title="移出中转，保留原文件" :aria-label="`移出 ${item.name}`" @click="remove([item.id])"><Icon name="close" :size="16" /></button>
      </li>
    </ul>
    </template>
  </section>
</template>

<style scoped>
.file-shelf { min-height: 148px; color: var(--text); border-radius: 18px; padding: 4px 0; }
.file-shelf.receiving { background: var(--accent-weak); outline: 1px dashed var(--accent); outline-offset: -1px; }
.shelf-tools { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 13px; }
.shelf-tools > div { display: flex; gap: 6px; }
.shelf-tools button { padding: 6px 10px; min-height: 30px; border-radius: 14px; border: 1px solid var(--line); background: var(--surface-2); transition: background 120ms ease, border-color 120ms ease; }
.shelf-tools button:hover:not(:disabled) { border-color: var(--line-strong); }
.shelf-help, .shelf-message, .shelf-error { font-size: 12px; line-height: 1.6; margin: 8px 0; color: var(--text-2); }
.shelf-error { color: var(--danger, #fa8e8e); }
.shelf-undo { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin: 8px 0; padding: 6px 8px 6px 12px; border-radius: 12px; border: 1px solid var(--accent-weak); background: var(--accent-weak); font-size: 12px; }
.shelf-undo span { color: var(--text); }
.undo-button { flex: none; font-weight: 600; color: var(--accent); min-height: 30px; padding: 5px 10px; border-radius: 8px; }
.shelf-empty { min-height: 120px; display: flex; flex-direction: column; gap: 9px; align-items: center; justify-content: center; padding: 16px 8px; color: var(--text-2); font-size: 13px; text-align: center; }
.shelf-empty small { font-size: 12px; color: var(--text-3); }
.shelf-list { margin: 10px 0 0; padding: 0; list-style: none; display: grid; gap: 6px; max-height: 250px; overflow-y: auto; }
.shelf-list li { display: flex; align-items: center; gap: 6px; padding: 6px 8px; min-width: 0; border-radius: 12px; border: 1px solid var(--line); background: var(--surface-2); transition: background 120ms ease, border-color 120ms ease; }
.shelf-list li:hover { background: var(--surface-3); border-color: var(--line-strong); }
.shelf-list input { accent-color: var(--accent); flex: none; }
.file-main { display: flex; align-items: center; flex: 1; min-width: 0; gap: 9px; text-align: left; cursor: grab; padding: 5px 0; }
.file-main:active { cursor: grabbing; }
.file-main svg { flex: none; color: var(--accent); }
.file-main > span { display: block; min-width: 0; }
.file-main strong, .file-main small { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.file-main strong { font-size: 13px; font-weight: 500; }
.file-main small { font-size: 11px; color: var(--text-3); margin-top: 3px; }
.row-action { display: grid; place-items: center; width: 28px; height: 30px; border-radius: 10px; flex: none; }
button:hover { background: var(--hover); }
button:disabled { color: var(--text-3); cursor: default; }
button:focus-visible, input:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.missing strong { color: var(--text-3); }
</style>
