<script setup lang="ts">
/**
 * `settings` 窗口：设置独占一个窗口，不再画遮罩。
 *
 * 浏览器样稿里 SettingsWindow 自带遮罩与居中，桌面窗口本身就是那个「框」，
 * 所以这里用 shell 标记把遮罩层去掉，只保留窗口内容。
 */
import { onBeforeUnmount, onMounted } from 'vue'
import HubMotionConfig from '@/components/HubMotionConfig.vue'
import SettingsWindow from '@/features/settings/SettingsWindow.vue'
import { subscribeHostEvents } from '@/store'

let unsubscribe: (() => void) | null = null
let disposed = false

// main.ts 已在挂载前完成 store.bootstrap()，此处仅订阅跨窗口状态广播，避免冷启动双倍拉取 IPC。
onMounted(async () => {
  const stop = await subscribeHostEvents()
  if (disposed) stop()
  else unsubscribe = stop
})

onBeforeUnmount(() => {
  disposed = true
  unsubscribe?.()
})
</script>

<template>
  <HubMotionConfig>
    <div class="settings-root">
      <SettingsWindow standalone />
    </div>
  </HubMotionConfig>
</template>

<style scoped>
.settings-root {
  width: 100%;
  height: 100%;
  background: transparent;
}
</style>
