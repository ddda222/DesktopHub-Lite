<script setup lang="ts">
/**
 * 窗内 / 灵动岛内底部非遮挡轻提示条。
 *
 * 严格遵循「谁发起的操作就在谁的窗口内部就地反馈」原则：
 * - 设置窗：贴于 .win 底部居中，绝不飞出窗口去遮挡灵动岛；
 * - 灵动岛展开态（floating / notch / split）：贴于面板底部拉条上方，零遮挡按钮与分页；
 * - 独立翻译窗：贴于翻译窗底部居中。
 */
import Icon from '@/components/Icon.vue'
import GsapPresence from '@/components/GsapPresence.vue'
import { activeMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
import { state } from '@/store'
import type { Toast } from '@/lib/desktop/types'

withDefaults(defineProps<{
  items: readonly Toast[]
  compact?: boolean
  fixed?: boolean
}>(), {
  compact: false,
  fixed: false
})
</script>

<template>
  <div
    class="inline-toast-stack"
    :class="{ 'is-compact': compact, 'is-fixed': fixed }"
    role="status"
    aria-live="polite"
  >
    <GsapPresence
      group
      tag="div"
      class="inline-toast-list"
      :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: 8, scale: 0.96 }"
      :animate="{ opacity: 1, y: 0, scale: 1 }"
      :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: 6, scale: 0.96 }"
      :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
    >
      <div
        v-for="item in items"
        :key="item.id"
        class="inline-toast toast"
        :class="`is-${item.kind}`"
      >
        <Icon
          :name="item.kind === 'success' ? 'check' : 'info'"
          :size="compact ? 13 : 14"
        />
        <span>{{ item.text }}</span>
      </div>
    </GsapPresence>
  </div>
</template>

<style scoped>
.inline-toast-stack {
  position: absolute;
  left: 50%;
  bottom: 16px;
  transform: translateX(-50%);
  z-index: 42;
  max-width: calc(100% - 32px);
  pointer-events: none;
}

.inline-toast-list {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  max-width: 100%;
}

.inline-toast-stack.is-fixed {
  position: fixed;
  bottom: 16px;
}

.inline-toast-stack.is-compact {
  bottom: 20px;
  max-width: calc(100% - 24px);
}

.inline-toast {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  max-width: 100%;
  padding: 7px 13px;
  border-radius: var(--r-chip, 10px);
  background: var(--surface);
  color: var(--text);
  border: 1px solid var(--line-strong);
  box-shadow:
    inset 3px 0 0 var(--accent),
    0 8px 24px rgb(0 0 0 / 0.28);
  font-size: 12.5px;
  line-height: 1.45;
  pointer-events: none;
}

.inline-toast-stack.is-compact .inline-toast {
  padding: 5px 11px;
  font-size: 11.5px;
  gap: 6px;
}

.inline-toast svg {
  flex: none;
  color: var(--accent);
}

.inline-toast span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.inline-toast.is-success {
  border-color: color-mix(in srgb, var(--success) 50%, var(--line-strong));
  box-shadow:
    inset 3px 0 0 var(--success),
    0 8px 24px rgb(0 0 0 / 0.28);
}

.inline-toast.is-success svg {
  color: var(--success);
}

.inline-toast.is-error {
  border-color: color-mix(in srgb, var(--danger) 56%, var(--line-strong));
  box-shadow:
    inset 3px 0 0 var(--danger),
    0 8px 24px rgb(0 0 0 / 0.3);
}

.inline-toast.is-error svg {
  color: var(--danger);
}
</style>
