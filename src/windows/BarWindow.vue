<script setup lang="ts">
/**
 * `bar` 窗口承载顶部灵动岛。
 *
 * 界面将触发区域尺寸回报给 Rust，收起时窗口限制为顶部 140×14 热区。
 */
import { onBeforeUnmount, onMounted } from 'vue'
import IslandEntry from '@/features/island/IslandEntry.vue'
import { subscribeHostEvents } from '@/store'

let unsubscribe: (() => void) | null = null

onMounted(async () => {
  // 订阅 UI 状态与应用配置：别的窗口改了应用列表，图标条要立刻跟着变
  unsubscribe = await subscribeHostEvents()
})

onBeforeUnmount(() => {
  unsubscribe?.()
})

// 桌面窗口根节点透明：真正的背景由图标条自己画。
</script>

<template>
  <div class="bar-root">
    <IslandEntry />
  </div>
</template>

<style scoped>
.bar-root {
  display: inline-block;
  background: transparent;
}
</style>
