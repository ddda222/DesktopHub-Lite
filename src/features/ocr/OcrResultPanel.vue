<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import GsapPresence from '@/components/GsapPresence.vue'
import Icon from '@/components/Icon.vue'
import { vGsapPress } from '@/lib/gsapMotion'
import { activeMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
import {
  closeOcrPanel,
  copyOcrText,
  sendOcrToTranslation,
  startScreenOcr,
  state
} from '@/store'

const draftText = ref(state.ocr.text)
const copied = ref(false)
let copyTimer: ReturnType<typeof setTimeout> | undefined

watch(
  () => [state.ocr.text, state.ocr.status],
  () => {
    draftText.value = state.ocr.text
    copied.value = false
    clearTimeout(copyTimer)
  },
  { immediate: true }
)

const sourceLabel = computed(() =>
  state.ocr.source === 'clipboard' ? '剪贴板图片' : '屏幕截图'
)
const liveLines = computed(() =>
  draftText.value
    .split(/\r?\n/)
    .filter(line => line.trim().length > 0).length
)
const liveChars = computed(() => [...draftText.value].length)

async function handleCopy(): Promise<void> {
  if (!draftText.value.trim()) return
  const ok = await copyOcrText(draftText.value)
  if (!ok) return
  copied.value = true
  clearTimeout(copyTimer)
  copyTimer = setTimeout(() => {
    copied.value = false
  }, 2000)
}

function handleTranslate(): void {
  if (!draftText.value.trim()) return
  sendOcrToTranslation(draftText.value)
}

onBeforeUnmount(() => {
  clearTimeout(copyTimer)
})
</script>

<template>
  <section class="ocr-panel" aria-labelledby="ocr-panel-title">
    <header class="ocr-head">
      <div class="ocr-title-row">
        <Icon name="scan-text" :size="15" />
        <h3 id="ocr-panel-title">截图识字</h3>
        <span class="ocr-meta-stats">
          {{ sourceLabel }}
          <template v-if="state.ocr.status === 'done'">
            · {{ liveLines }} 行 · {{ liveChars }} 字<template v-if="state.ocr.language"> · {{ state.ocr.language }}</template><template v-if="state.ocr.elapsedMs"> · {{ state.ocr.elapsedMs }} ms</template>
          </template>
        </span>
      </div>
      <button
        class="ocr-close-btn"
        type="button"
        title="返回原页面"
        aria-label="关闭识字结果"
        @click="closeOcrPanel"
      >
        <Icon name="close" :size="14" />
      </button>
    </header>

    <GsapPresence
      mode="out-in"
      :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: 4 }"
      :animate="{ opacity: 1, y: 0 }"
      :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: -4 }"
      :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
    >
      <div
        v-if="state.ocr.status === 'loading'"
        key="ocr-loading"
        class="ocr-state-box"
        role="status"
      >
        <p>正在本地离线识别文字，请稍候…</p>
      </div>

      <div
        v-else-if="state.ocr.status === 'error'"
        key="ocr-error"
        class="ocr-state-box is-error"
        role="alert"
      >
        <p>{{ state.ocr.error || '未能识别出文字' }}</p>
        <div class="ocr-actions">
          <button class="ocr-btn ocr-btn-primary" type="button" @click="startScreenOcr()">
            重新截图
          </button>
        </div>
      </div>

      <div
        v-else-if="state.ocr.status === 'done'"
        key="ocr-done"
      >
        <textarea
          v-model="draftText"
          class="ocr-textarea"
          rows="7"
          spellcheck="false"
          aria-label="识别出的文字内容"
          placeholder="识别结果将显示在这里，可编辑或局部选中复制"
        />
        <div class="ocr-actions">
          <button
            v-gsap-press="!draftText.trim() || state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap"
            class="ocr-btn ocr-btn-primary"
            type="button"
            :disabled="!draftText.trim()"
            @click="handleCopy"
          >
            <Icon :name="copied ? 'check' : 'copy'" :size="14" />
            {{ copied ? '已复制' : '复制全文' }}
          </button>
          <button
            v-gsap-press="!draftText.trim() || state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap"
            class="ocr-btn"
            type="button"
            :disabled="!draftText.trim()"
            @click="handleTranslate"
          >
            <Icon name="translate" :size="14" />
            送去翻译
          </button>
          <button
            v-gsap-press="state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap"
            class="ocr-btn"
            type="button"
            @click="startScreenOcr()"
          >
            <Icon name="scan-text" :size="14" />
            重新截图
          </button>
        </div>
      </div>
    </GsapPresence>
  </section>
</template>

<style scoped>
.ocr-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  color: var(--text);
}
.ocr-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.ocr-title-row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  min-width: 0;
}
.ocr-title-row h3 {
  margin: 0;
  font-size: 13.5px;
  font-weight: 600;
}
.ocr-meta-stats {
  font-size: 11px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.ocr-close-btn {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  flex: none;
  border-radius: 8px;
  border: 1px solid transparent;
  color: var(--text-2);
  touch-action: manipulation;
  transition: transform 150ms var(--ease-out), background-color 140ms ease, border-color 140ms ease, color 140ms ease;
}
.ocr-close-btn:active {
  transform: scale(.965);
  transition-duration: 70ms;
}
.ocr-close-btn:hover {
  background: var(--hover);
  border-color: var(--line);
  color: var(--text);
}
.ocr-state-box {
  padding: 18px 12px;
  border-radius: var(--r-card, 12px);
  border: 1px solid var(--line);
  background: var(--surface-2);
  font-size: 13px;
  color: var(--text-2);
}
.ocr-state-box p {
  margin: 0 0 12px;
  line-height: 1.6;
  overflow-wrap: anywhere;
}
.ocr-state-box p:last-child {
  margin-bottom: 0;
}
.ocr-state-box.is-error p {
  color: var(--danger);
}
.ocr-textarea {
  width: 100%;
  min-height: 148px;
  max-height: 260px;
  resize: vertical;
  padding: 10px 12px;
  border-radius: 12px;
  border: 1px solid var(--line-strong);
  background: var(--surface-2);
  color: var(--text);
  font-family: var(--font-sans);
  font-size: 13px;
  line-height: 1.65;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  user-select: text;
}
.ocr-textarea:focus-visible {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent-weak);
}
.ocr-actions {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.ocr-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-height: 32px;
  padding: 0 12px;
  border-radius: 10px;
  border: 1px solid var(--line-strong);
  background: var(--surface-2);
  color: var(--text);
  font-size: 12.5px;
  font-weight: 500;
  touch-action: manipulation;
  transition: transform 150ms var(--ease-out), background-color 140ms ease, border-color 140ms ease, color 140ms ease;
}
.ocr-btn:active:not(:disabled) {
  transform: scale(.965);
  transition-duration: 70ms;
}
.ocr-btn:hover:not(:disabled) {
  background: var(--surface-3);
  border-color: var(--text-3);
}
.ocr-btn-primary {
  background: var(--accent);
  color: var(--accent-text);
  border-color: var(--accent);
  font-weight: 600;
}
.ocr-btn-primary:hover:not(:disabled) {
  opacity: .92;
  background: var(--accent);
  color: var(--accent-text);
  border-color: var(--accent);
}
.ocr-btn:disabled {
  opacity: .55;
  cursor: not-allowed;
}
@media (prefers-reduced-motion: reduce) {
  .ocr-panel,
  .ocr-btn,
  .ocr-close-btn {
    transition: none;
    transform: none;
  }
}
</style>
