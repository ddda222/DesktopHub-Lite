<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import CompareBoard from '@/features/compare/CompareBoard.vue'
import type { CompareSnapshot } from '@/features/compare/state'
import { invokeHost, onUiState, readUiState, type UiState } from '@/lib/desktop/desktop'
import { state } from '@/store'
const snapshot = ref<CompareSnapshot | null>(null)
const error = ref('')
const pinned = ref(false)
const busy = ref(false)
const root = ref<HTMLElement | null>(null)
let disposed = false
let stopTheme: (() => void) | undefined
function syncTheme(ui: UiState): void { if (['light', 'dark', 'system'].includes(ui.theme)) state.settings.theme = ui.theme as typeof state.settings.theme }
onMounted(async () => {
  try {
    const stop = await onUiState(syncTheme)
    if (disposed) { stop(); return }
    stopTheme = stop; syncTheme(await readUiState())
    const content = await invokeHost<CompareSnapshot>('compare_window_content')
    if (!disposed) { snapshot.value = content; await nextTick(); root.value?.focus({ preventScroll: true }) }
  } catch (e) { error.value = String(e) }
})
onBeforeUnmount(() => { disposed = true; stopTheme?.() })
async function pin(): Promise<void> {
  busy.value = true; error.value = ''
  try { pinned.value = await invokeHost<boolean>('pin_compare_window', { pinned: !pinned.value }) } catch (e) { error.value = String(e) }
  finally { busy.value = false }
}
async function close(): Promise<void> { try { await getCurrentWindow().close() } catch (e) { error.value = String(e) } }
async function drag(event: PointerEvent): Promise<void> {
  if (event.button !== 0 || (event.target as HTMLElement).closest('button')) return
  try { await getCurrentWindow().startDragging() } catch (e) { error.value = String(e) }
}
</script>
<template>
  <main ref="root" class="compare-window" tabindex="-1" @keydown.esc="!$event.isComposing && close()">
    <header @pointerdown="drag"><div><h1>浮动对照台</h1><p>拖动标题栏移动 · 内容仅保留在当前窗口</p></div><button :aria-pressed="pinned" :disabled="busy" @click="pin">{{ pinned ? '已置顶' : '置顶' }}</button><button aria-label="关闭对照台" @click="close">✕</button></header>
    <p v-if="error" role="alert" class="error">{{ error }}</p>
    <CompareBoard v-if="snapshot" floating :initial="snapshot" />
    <p v-else-if="!error" role="status">正在读取对照内容…</p>
  </main>
</template>
<style scoped>
.compare-window { height:100%; width:100%; overflow:auto; padding:20px 24px; color:var(--text); background:var(--surface); border:1px solid var(--line-strong); }
header { display:flex; align-items:center; gap:14px; padding-bottom:18px; margin-bottom:18px; border-bottom:1px solid var(--line-strong); user-select:none; cursor:grab; }header > div { margin-right:auto; }h1 { margin:0 0 6px; font-size:16px; font-weight:600; }header p { margin:0; color:var(--text-2); font-size:11px; }header button { padding:8px 12px; border:1px solid var(--line-strong); border-radius:7px; font-size:12px; }header button[aria-pressed=true] { background:var(--text); color:var(--surface); }.error { color:var(--danger); font-size:12px; }
.compare-window :deep(.image-comparison img) { height:clamp(200px,50vh,700px); }.compare-window :deep(.image-comparison.side img) { height:clamp(200px,45vh,600px); }.compare-window :deep(textarea) { height:200px; }.compare-window :deep(.text-diff) { max-height:35vh; }
</style>
