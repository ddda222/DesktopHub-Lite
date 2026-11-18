import { computed, onBeforeUnmount, onMounted, ref, shallowRef, watch, type Ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isDesktop } from '@/lib/desktop/desktop'
import { createIslandPhysics } from './physics'

export type IslandCueKind = 'launch' | 'notice' | 'loading' | 'complete'
export type IslandCue = { kind: IslandCueKind; sequence: number }

export function useIslandEffects(panel: Ref<HTMLElement | null>, tucked: Readonly<Ref<boolean>>, reduced: Readonly<Ref<boolean>>, material: Readonly<Ref<boolean>>) {
  const inView = ref(false)
  const documentVisible = ref(!document.hidden)
  const nativeVisible = ref(!isDesktop)
  const contrast = matchMedia('(forced-colors: active), (prefers-contrast: more), (prefers-reduced-transparency: reduce)')
  const plainMaterial = ref(contrast.matches)
  const quality: 'low' | 'full' = navigator.hardwareConcurrency > 0 && navigator.hardwareConcurrency <= 4 ||
    ((navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? 8) <= 4 ? 'low' : 'full'
  const active = computed(() => documentVisible.value && nativeVisible.value && inView.value && !tucked.value && !reduced.value && !plainMaterial.value)
  const cue = shallowRef<IslandCue | null>(null)
  let sequence = 0
  let disposed = false
  let unlisten: (() => void) | undefined
  let observer: IntersectionObserver | undefined
  let physics: ReturnType<typeof createIslandPhysics> | undefined

  function signal(kind: IslandCueKind) {
    if (active.value && material.value) cue.value = { kind, sequence: ++sequence }
  }
  function updateVisibility() { documentVisible.value = !document.hidden }
  function updateContrast() { plainMaterial.value = contrast.matches }
  watch(active, enabled => {
    physics?.setEnabled(enabled && quality === 'full')
    if (!enabled) cue.value = null
  }, { flush: 'sync' })
  watch(material, enabled => { if (!enabled) cue.value = null }, { flush: 'sync' })

  onMounted(async () => {
    if (!panel.value) return
    physics = createIslandPhysics(panel.value)
    observer = new IntersectionObserver(entries => { inView.value = entries.at(-1)?.isIntersecting ?? false })
    observer.observe(panel.value)
    document.addEventListener('visibilitychange', updateVisibility)
    contrast.addEventListener('change', updateContrast)
    if (!isDesktop) return
    let generation = 0
    try {
      const bar = getCurrentWindow()
      const stop = await bar.listen<boolean>('island-visibility', ({ payload: shown }) => {
        generation++
        if (!disposed) nativeVisible.value = shown
      })
      if (disposed) { stop(); return }
      unlisten = stop
      const current = generation
      const shown = await bar.isVisible()
      if (!disposed && current === generation) nativeVisible.value = shown
    } catch {
      if (!disposed && !generation) nativeVisible.value = !document.hidden
    }
  })
  onBeforeUnmount(() => {
    disposed = true
    observer?.disconnect()
    unlisten?.()
    physics?.dispose()
    document.removeEventListener('visibilitychange', updateVisibility)
    contrast.removeEventListener('change', updateContrast)
  })
  return { active, cue, signal, quality, plainMaterial }
}
