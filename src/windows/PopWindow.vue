<script setup lang="ts">
/**
 * `pop` 窗口：按 Rust 广播的 `panel` 决定渲染哪个弹层。
 *
 * 窗口尺寸由界面量出后回报给 Rust，Rust 再把它对齐到图标条上方，
 * 所以弹层不会把桌面挡住，也不会被图标条窗口裁掉。
 *
 * 「点击弹层外面收起」由 Rust 监听窗口焦点事件统一处理：
 * 界面自己判断失焦会把「窗口刚显示还没拿到焦点」误当成点了外面，
 * 曾导致弹层显示后立刻消失，所以这里不再自行关闭。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import ClipboardPopover from '@/features/clipboard/ClipboardPopover.vue'
import TranslationPanel from '@/features/translation/TranslationPanel.vue'
import CategoryMenu from '@/features/applications/CategoryMenu.vue'
import OverflowGrid from '@/features/applications/OverflowGrid.vue'
import { resizePop } from '@/lib/desktop/desktop'
import { state, subscribeHostEvents } from '@/store'

const rootEl = ref<HTMLElement | null>(null)
let unsubscribe: (() => void) | null = null

const panel = computed(() => state.ui.panel)

/** 把内容真实尺寸报给 Rust，窗口与位置随之修正。 */
async function syncSize(): Promise<void> {
  await nextTick()
  // 等一帧，避免量到还没排版完的尺寸
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  const rect = rootEl.value?.getBoundingClientRect()
  if (!rect || rect.width < 1 || rect.height < 1) return
  void resizePop(Math.ceil(rect.width), Math.ceil(rect.height))
}

onMounted(async () => {
  unsubscribe = await subscribeHostEvents()
  void syncSize()
})

onBeforeUnmount(() => {
  unsubscribe?.()
})

// 切换弹层时尺寸会变，必须重新上报
watch(panel, async () => {
  await syncSize()
})
</script>

<template>
  <div ref="rootEl" class="pop-root">
    <ClipboardPopover v-if="panel === 'clipboard'" />
    <TranslationPanel v-else-if="panel === 'translation'" />
    <div v-else-if="panel === 'category'" class="pop-shell">
      <CategoryMenu />
    </div>
    <div v-else-if="panel === 'overflow'" class="pop-shell">
      <OverflowGrid />
    </div>
  </div>
</template>

<style scoped>
.pop-root {
  display: inline-block;
  background: transparent;
}

.pop-shell {
  width: 100%;
}
</style>
