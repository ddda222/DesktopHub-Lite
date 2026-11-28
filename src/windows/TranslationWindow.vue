<script setup lang="ts">
import { onBeforeUnmount, onMounted } from 'vue'
import HubMotionConfig from '@/components/HubMotionConfig.vue'
import InlineToastBar from '@/components/InlineToastBar.vue'
import TranslationPanel from '@/features/translation/TranslationPanel.vue'
import { listenHost } from '@/lib/desktop/desktop'
import { cancelTranslation, state, subscribeHostEvents } from '@/store'

let disposed = false
let listeners: Array<() => void> = []
onMounted(async () => {
  const next = await Promise.all([
    listenHost('translation-window-hidden', cancelTranslation),
    subscribeHostEvents()
  ])
  if (disposed) next.forEach(stop => stop())
  else listeners = next
})
onBeforeUnmount(() => { disposed = true; listeners.forEach(stop => stop()) })
</script>

<template>
  <HubMotionConfig>
    <main class="translation-window scroll-y">
      <TranslationPanel standalone />
      <InlineToastBar fixed :items="state.toasts" />
    </main>
  </HubMotionConfig>
</template>

<style scoped>
.translation-window { position: relative; min-height: 100vh; padding: 12px; background: var(--surface); }
.translation-window :deep(.panel) { width: 100%; max-height: none; }
</style>
