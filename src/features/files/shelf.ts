import { reactive } from 'vue'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'

export interface ShelfItem { id: string; path: string; name: string; isDir: boolean; available: boolean }
export const shelf = reactive({ entries: [] as ShelfItem[], busy: false, message: '', error: '' })
export async function refreshShelf(): Promise<void> {
  if (!isDesktop) return
  try { shelf.entries = await invokeHost<ShelfItem[]>('list_shelf_files') }
  catch (error) { shelf.error = String(error) }
}
export async function addShelfFiles(paths: string[]): Promise<void> {
  if (!paths.length || !isDesktop) return
  shelf.error = ''; shelf.busy = true
  try {
    const result = await invokeHost<{ entries: ShelfItem[]; added: number }>('add_shelf_files', { paths })
    shelf.entries = result.entries
    shelf.message = result.added ? `已暂存 ${result.added} 项` : '这些文件已在中转中'
  } catch (error) { shelf.error = String(error) }
  finally { shelf.busy = false }
}
