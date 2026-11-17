import { reactive } from 'vue'
import { invokeHost, isDesktop, listenHost } from '@/lib/desktop/desktop'

export interface ShelfItem { id: string; path: string; name: string; isDir: boolean; available: boolean }
interface ShelfSnapshot { entries: ShelfItem[]; undoCount: number }
export interface FilePreview {
  kind: 'image' | 'text' | 'folder' | 'info'
  size: number | null
  modifiedMs: number | null
  text: string | null
  dataUri: string | null
  notice: string | null
}
export const shelf = reactive({
  entries: [] as ShelfItem[], busy: false, activity: '', message: '', error: '', undoCount: 0,
  previewItem: null as ShelfItem | null, preview: null as FilePreview | null, previewLoading: false, previewError: ''
})
let refreshSeq = 0
let previewSeq = 0
// 浏览器样稿的撤销只存在内存里，不访问用户文件。
const sampleHistory: { index: number; item: ShelfItem }[][] = []
export interface ShelfPreviewSample { item: ShelfItem; preview: FilePreview }
const samplePreviews = new Map<string, FilePreview>()
export function loadBrowserShelfSamples(samples: ShelfPreviewSample[]): void {
  if (isDesktop) return
  closeShelfPreview()
  sampleHistory.length = 0
  samplePreviews.clear()
  for (const sample of samples) samplePreviews.set(sample.item.id, sample.preview)
  Object.assign(shelf, { entries: samples.map(sample => ({ ...sample.item })), undoCount: 0, busy: false, activity: '', message: '', error: '' })
}
function applySnapshot(result: ShelfSnapshot): void {
  shelf.entries = result.entries
  shelf.undoCount = result.undoCount
  if (shelf.previewItem && !result.entries.some(item => item.id === shelf.previewItem?.id)) closeShelfPreview()
}
export async function refreshShelf(): Promise<void> {
  if (!isDesktop) return
  const seq = ++refreshSeq
  try {
    const result = await invokeHost<ShelfSnapshot>('list_shelf_files')
    if (seq === refreshSeq) applySnapshot(result)
  } catch (error) { if (seq === refreshSeq) shelf.error = String(error) }
}
export async function subscribeShelf(): Promise<() => void> {
  const stop = await listenHost('file-shelf-changed', () => { void refreshShelf() })
  await refreshShelf()
  return stop
}
export async function addShelfFiles(paths: string[]): Promise<void> {
  if (!paths.length || !isDesktop || shelf.busy) return
  shelf.error = ''; shelf.busy = true; shelf.activity = '正在暂存'
  try {
    const result = await invokeHost<{ entries: ShelfItem[]; added: number }>('add_shelf_files', { paths })
    ++refreshSeq
    shelf.entries = result.entries
    shelf.message = result.added ? `已暂存 ${result.added} 项` : '这些文件已在中转中'
  } catch (error) { shelf.error = String(error) }
  finally { shelf.busy = false; shelf.activity = '' }
}
export async function removeShelfFiles(ids: string[]): Promise<void> {
  if (shelf.busy || !ids.length) return
  shelf.error = ''; shelf.busy = true; shelf.activity = '正在移出'
  try {
    let result: ShelfSnapshot
    if (isDesktop) result = await invokeHost<ShelfSnapshot>('remove_shelf_files', { ids })
    else {
      const removed = shelf.entries.flatMap((item, index) => ids.includes(item.id) ? [{ item, index }] : [])
      if (removed.length) sampleHistory.push(removed)
      if (sampleHistory.length > 10) sampleHistory.shift()
      result = { entries: shelf.entries.filter(item => !ids.includes(item.id)), undoCount: sampleHistory.at(-1)?.length ?? 0 }
    }
    ++refreshSeq
    applySnapshot(result)
    shelf.message = '已移出中转，原文件保留'
  } catch (error) { shelf.error = String(error) }
  finally { shelf.busy = false; shelf.activity = '' }
}
export async function undoShelfRemove(): Promise<void> {
  if (shelf.busy || !shelf.undoCount) return
  shelf.error = ''; shelf.busy = true; shelf.activity = '正在恢复'
  try {
    let result: ShelfSnapshot
    if (isDesktop) result = await invokeHost<ShelfSnapshot>('undo_shelf_remove')
    else {
      const entries = [...shelf.entries]
      for (const { index, item } of sampleHistory.pop() ?? []) {
        if (!entries.some(existing => existing.id === item.id)) entries.splice(Math.min(index, entries.length), 0, item)
      }
      result = { entries, undoCount: sampleHistory.at(-1)?.length ?? 0 }
    }
    ++refreshSeq
    applySnapshot(result)
    shelf.message = '已撤销移出，恢复中转条目'
  } catch (error) { shelf.error = String(error) }
  finally { shelf.busy = false; shelf.activity = '' }
}
export async function readShelfPreview(item: ShelfItem): Promise<FilePreview> {
  if (isDesktop) return invokeHost<FilePreview>('preview_shelf_file', { id: item.id })
  if (!item.available) throw new Error('原文件已移动或删除（模拟状态）')
  const preview = samplePreviews.get(item.id)
  if (!preview) throw new Error('请先载入浏览器示例文件')
  return preview
}

export async function openShelfPreview(item: ShelfItem): Promise<void> {
  const seq = ++previewSeq
  shelf.previewItem = item; shelf.preview = null; shelf.previewError = ''; shelf.previewLoading = true
  try {
    if (!isDesktop && !item.available) throw new Error('原文件已移动或删除（模拟状态）')
    const preview = isDesktop ? await invokeHost<FilePreview>('preview_shelf_file', { id: item.id }) : samplePreviews.get(item.id) ?? {
      kind: item.isDir ? 'folder' : 'info', size: null, modifiedMs: null, text: null, dataUri: null,
      notice: '浏览器样稿不读取本机文件，请在桌面版查看内容'
    } as FilePreview
    if (seq === previewSeq) shelf.preview = preview
  } catch (error) { if (seq === previewSeq) shelf.previewError = error instanceof Error ? error.message : String(error) }
  finally { if (seq === previewSeq) shelf.previewLoading = false }
}
export function closeShelfPreview(): void {
  ++previewSeq
  shelf.previewItem = null; shelf.preview = null; shelf.previewLoading = false; shelf.previewError = ''
}
