<script setup lang="ts">
/**
 * `toast` 窗口：一条轻提示独占一个小窗口。
 *
 * 图标条窗口只有图标条大小，放不下提示，所以提示要单独占一个窗口。
 * 收到 Rust 的 `toast` 事件后渲染，量出高度回报给 Rust，再定时自动收起。
 *
 * 宽度固定、只调高度：如果连宽度也按内容自动调，窗口一变窄文本就换行、
 * 换行后量出的宽度又更窄，会陷进「越量越小、文字被裁」的循环。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import GsapPresence from '@/components/GsapPresence.vue'
import HubMotionConfig from '@/components/HubMotionConfig.vue'
import Icon from '@/components/Icon.vue'
import { activeMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
import { dismissToast, onToast, resizeToast } from '@/lib/desktop/desktop'
import { state } from '@/store'

interface ToastPayload {
  message: string
  kind: string
}

/** 提示窗口的固定宽度（逻辑像素）：够放一行常用提示，过长会换行。 */
const TOAST_WIDTH = 360

const payload = ref<ToastPayload | null>(null)
const toastSeq = ref(0)
const boxEl = ref<HTMLElement | null>(null)
let unsubscribe: (() => void) | null = null
let hideTimer: number | null = null
let dismissPending = false

const toastTransition = computed(() =>
  state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.value.popTransition
)

const iconName = computed(() => {
  if (payload.value?.kind === 'error') return 'info'
  if (payload.value?.kind === 'success') return 'check'
  return 'info'
})

function clearTimer(): void {
  if (hideTimer !== null) {
    window.clearTimeout(hideTimer)
    hideTimer = null
  }
}

function onAfterLeave(): void {
  // 仅当本次退场源自自动过期且未有新提示接替时，再通知宿主隐藏窗口，防止截断退场动画或误关新提示。
  if (!dismissPending || payload.value !== null) return
  dismissPending = false
  void dismissToast()
}

async function show(next: ToastPayload): Promise<void> {
  clearTimer()
  dismissPending = false
  payload.value = next
  const currentSeq = toastSeq.value + 1
  toastSeq.value = currentSeq
  // 等 DOM 更新后量出真实高度；宽度固定，避免换行与测量互相影响。
  await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()))
  if (currentSeq !== toastSeq.value) return
  const el = boxEl.value
  if (el) {
    const height = Math.ceil(Math.max(el.scrollHeight, el.offsetHeight)) + 2
    void resizeToast(TOAST_WIDTH, height)
  }
  hideTimer = window.setTimeout(() => {
    hideTimer = null
    if (currentSeq !== toastSeq.value) return
    dismissPending = true
    payload.value = null
  }, 2400)
}

onMounted(async () => {
  unsubscribe = await onToast((next) => {
    void show(next)
  })
})

onBeforeUnmount(() => {
  unsubscribe?.()
  clearTimer()
})
</script>

<template>
  <HubMotionConfig>
    <GsapPresence
      mode="out-in"
      :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: 10, scale: 0.96 }"
      :animate="{ opacity: 1, y: 0, scale: 1 }"
      :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: 6, scale: 0.96 }"
      :transition="toastTransition"
      @after-leave="onAfterLeave"
    >
      <div
        v-if="payload"
        :key="toastSeq"
        ref="boxEl"
        class="toast"
        :class="`is-${payload.kind}`"
        role="status"
      >
        <Icon :name="iconName" :size="14" />
        <span>{{ payload.message }}</span>
      </div>
    </GsapPresence>
  </HubMotionConfig>
</template>

<style scoped>
/*
 * 提示窗口本身按内容收缩，所以 .toast 不能是 inline-flex 加 nowrap 就算了：
 * 窗口初始只有 320×40，文本会被裁掉。这里改成可换行、有上限宽度，
 * 尺寸由 scrollWidth/scrollHeight 量出后回报给 Rust。
 */
.toast {
  display: flex;
  align-items: center;
  gap: 7px;
  box-sizing: border-box;
  width: 100%;
  padding: 7px 12px;
  border-radius: 10px;
  background: var(--pop-bg);
  border: 1px solid var(--line);
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.3);
  font-size: var(--text-sm);
  line-height: 1.5;
  color: var(--text);
  overflow-wrap: anywhere;
}

.toast svg {
  flex: none;
}

.toast span {
  min-width: 0;
}

.toast.is-success {
  color: var(--success);
  border-color: color-mix(in srgb, var(--success) 32%, transparent);
}

.toast.is-error {
  color: var(--danger);
  border-color: color-mix(in srgb, var(--danger) 34%, transparent);
}
</style>
