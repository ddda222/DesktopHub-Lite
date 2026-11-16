<script setup lang="ts">
/**
 * 浏览器样稿窗口：把图标条、弹层、设置和提示都放在一页里，
 * 用一块模拟桌面背景演示交互。桌面版不会用到这个组件。
 */
import { computed, onBeforeUnmount, onMounted } from 'vue'
import Icon from '@/components/Icon.vue'
import IconBar from '@/features/applications/IconBar.vue'
import ClipboardPopover from '@/features/clipboard/ClipboardPopover.vue'
import TranslationPanel from '@/features/translation/TranslationPanel.vue'
import SettingsWindow from '@/features/settings/SettingsWindow.vue'
import { closePanel, closeSettings, state } from '@/store'

const dockStyle = computed(() => ({
  transform: `translate(${state.ui.dockOffset.x}px, ${state.ui.dockOffset.y}px)`
}))

function onGlobalKeydown(event: KeyboardEvent): void {
  if (event.key !== 'Escape') return
  if (event.isComposing || event.keyCode === 229) return
  if (state.ui.settingsOpen) {
    closeSettings()
    return
  }
  if (state.ui.panel) closePanel()
}

/* 点击图标条与弹层之外的空白处收起弹层，设置窗口不在此列 */
function onStagePointerDown(event: PointerEvent): void {
  const target = event.target as HTMLElement | null
  if (!target) return
  if (target.closest('.dock')) return
  if (target.closest('.pop, .panel')) return
  if (state.ui.settingsOpen) return
  if (state.ui.panel) closePanel()
}

onMounted(() => {
  window.addEventListener('keydown', onGlobalKeydown)
  window.addEventListener('pointerdown', onStagePointerDown, true)
})

onBeforeUnmount(() => {
  window.removeEventListener('keydown', onGlobalKeydown)
  window.removeEventListener('pointerdown', onStagePointerDown, true)
})
</script>

<template>
  <div class="stage">
    <p class="hint">浏览器样稿 · 合成数据 · 不读写真实剪贴板、不启动外部程序</p>

    <div class="dock" :style="dockStyle">
      <Transition name="pop">
        <div v-if="state.ui.panel === 'clipboard'" class="pop-slot">
          <ClipboardPopover />
        </div>
      </Transition>
      <Transition name="pop">
        <div v-if="state.ui.panel === 'translation'" class="pop-slot">
          <TranslationPanel />
        </div>
      </Transition>
      <IconBar />
    </div>

    <SettingsWindow v-if="state.ui.settingsOpen" />

    <div class="toasts" role="status" aria-live="polite">
      <TransitionGroup name="toast">
        <div v-for="toast in state.toasts" :key="toast.id" class="toast" :class="`is-${toast.kind}`">
          <Icon
            :name="toast.kind === 'error' ? 'info' : toast.kind === 'success' ? 'check' : 'info'"
            :size="14"
          />
          <span>{{ toast.text }}</span>
        </div>
      </TransitionGroup>
    </div>
  </div>
</template>

<style scoped>
.stage {
  position: relative;
  height: 100%;
  overflow: hidden;
  background: radial-gradient(120% 90% at 50% 0%, var(--stage-1) 0%, var(--stage-2) 100%);
}

.hint {
  position: absolute;
  top: 14px;
  left: 50%;
  transform: translateX(-50%);
  margin: 0;
  font-size: var(--text-xs);
  color: var(--text-3);
  letter-spacing: 0.2px;
  user-select: none;
  pointer-events: none;
}

.dock {
  position: absolute;
  left: 50%;
  bottom: 56px;
  margin-left: -260px;
  width: 520px;
  z-index: var(--z-bar);
}

.pop-slot {
  position: absolute;
  bottom: calc(100% + 10px);
  left: 0;
  transform-origin: bottom left;
}

/* 弹层出现/消失：短、可中断，不做大幅位移 */
.pop-enter-active,
.pop-leave-active {
  transition:
    opacity var(--dur) var(--ease),
    transform var(--dur) var(--ease);
}

.pop-enter-from,
.pop-leave-to {
  opacity: 0;
  transform: translateY(6px) scale(0.985);
}

.toasts {
  position: absolute;
  left: 50%;
  bottom: 130px;
  transform: translateX(-50%);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: var(--space-2);
  z-index: var(--z-toast);
  pointer-events: none;
}

.toast {
  display: flex;
  align-items: center;
  gap: 7px;
  max-width: 420px;
  padding: 7px 12px;
  border-radius: 10px;
  background: var(--pop-bg);
  border: 1px solid var(--line);
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.3);
  font-size: var(--text-sm);
  color: var(--text);
}

.toast.is-success {
  color: var(--success);
  border-color: color-mix(in srgb, var(--success) 32%, transparent);
}

.toast.is-error {
  color: var(--danger);
  border-color: color-mix(in srgb, var(--danger) 34%, transparent);
}

.toast-enter-active,
.toast-leave-active {
  transition:
    opacity var(--dur) var(--ease),
    transform var(--dur) var(--ease);
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(5px);
}
</style>
