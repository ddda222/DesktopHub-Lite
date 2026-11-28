<script setup lang="ts">
/**
 * 翻译面板（阶段 0 静态样稿）
 *
 * 安全与产品约束：
 * - 手动提交：输入、粘贴都不会自动发请求，只有点「翻译」或按 Ctrl/Cmd + Enter 才提交。
 * - 结果来自 MockHost，界面必须标「样稿模拟结果」，不能让人以为真实翻译成功。
 * - 密钥正文永不读取、永不显示。
 */
import { computed, ref, watch } from 'vue'
import GsapPresence from '@/components/GsapPresence.vue'
import { gsap, vGsapPress, vGsapPulse } from '@/lib/gsapMotion'
import { activeMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import Icon from '@/components/Icon.vue'
import ThemedSelect from '@/components/ThemedSelect.vue'
import {
  closePanel,
  cancelTranslation,
  copyTranslation,
  setTranslationDirection,
  setTranslationSource,
  state,
  submitTranslation
} from '@/store'

const props = defineProps<{ standalone?: boolean }>()
const swapIconEl = ref<HTMLElement | null>(null)
function dismiss(): void {
  if (props.standalone) {
    cancelTranslation()
    void invokeHost('close_translation_window')
  } else closePanel()
}

const sourceLanguage = computed(() => state.translation.direction === 'auto' ? 'auto' : state.translation.direction === 'en-zh' ? 'en' : 'zh')
watch(sourceLanguage, (lang) => {
  if (!swapIconEl.value) return
  const spec = state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.value.pill
  gsap.to(swapIconEl.value, { rotation: lang === 'en' ? 180 : 0, duration: spec.duration, ease: spec.ease })
})
const targetLanguage = computed(() => state.translation.direction === 'auto' ? 'auto' : state.translation.direction === 'en-zh' ? 'zh' : 'en')
const sourceLanguageOptions = [
  { value: 'zh', label: '中文' },
  { value: 'en', label: '英语' },
  { value: 'auto', label: '自动检测' }
] as const
const targetLanguageOptions = computed(() => [
  ...(targetLanguage.value === 'auto' ? [{ value: 'auto', label: '自动匹配', disabled: true }] : []),
  { value: 'en', label: '英语' },
  { value: 'zh', label: '中文' }
])
function chooseLanguage(value: string | number, source: boolean) {
  const language = String(value)
  if (language === 'auto') setTranslationDirection('auto')
  else setTranslationDirection((language === 'zh') === source ? 'zh-en' : 'en-zh')
}
function swapLanguages() {
  const direction = state.translation.direction === 'auto' ? state.translation.result?.direction : state.translation.direction
  setTranslationDirection(direction === 'en-zh' ? 'zh-en' : 'en-zh')
}

/** 与 store 的截断长度保持一致：setTranslationSource 按 5000 截断 */
const MAX_CHARS = 5000
/** 接近上限的提示阈值 */
const NEAR_LIMIT = 4500

const status = computed(() => state.translation.status)
const isLoading = computed(() => state.translation.status === 'loading')
const count = computed(() => state.translation.source.length)
const hasSource = computed(() => state.translation.source.length > 0)
const nearLimit = computed(() => count.value > NEAR_LIMIT)
const atLimit = computed(() => count.value >= MAX_CHARS)
const resultText = computed(() => state.translation.result?.text ?? '')
const resultMeta = computed(() => {
  const result = state.translation.result
  if (!result) return ''
  return `${result.provider} · ${result.model} · 耗时 ${(result.elapsedMs / 1000).toFixed(1)}s`
})

/** 读屏播报用：只报状态，不把整段译文念出来 */
const liveStatus = computed(() => {
  if (status.value === 'loading') return '正在翻译'
  if (status.value === 'error') return `翻译失败：${state.translation.error}`
  if (status.value === 'done') return '翻译完成'
  return ''
})

/**
 * 输入只做「写入 store」，不使用 v-model：
 * store 会截断到 5000 字，截断后的值与输入框里的值可能不同，必须手动写回，
 * 否则界面会显示超出上限的内容，与真正提交的原文不一致。
 */
function onSourceInput(event: Event): void {
  const el = event.target as HTMLTextAreaElement
  const typed = el.value
  const caret = el.selectionStart ?? typed.length
  setTranslationSource(typed)
  // 只有被截断时才写回，避免正常输入时光标被重置到末尾
  if (typed !== state.translation.source) {
    el.value = state.translation.source
    const pos = Math.min(caret, state.translation.source.length)
    el.setSelectionRange(pos, pos)
  }
}

const latestClipText = computed(() =>
  state.clips.find(item => item.kind === 'text' && !!item.text?.trim())?.text ?? ''
)

function fillFromLatestClip(): void {
  if (!latestClipText.value) return
  setTranslationSource(latestClipText.value)
  state.translation.filledFromClip = true
}

function onClear(): void {
  setTranslationSource('')
}

function onSubmit(): void {
  if (isLoading.value) return
  void submitTranslation()
}

function onRetry(): void {
  onSubmit()
}

function onCopy(): void {
  void copyTranslation()
}

function useResultAsSource(): void {
  if (!resultText.value.trim()) return
  const text = resultText.value
  const dir = state.translation.direction === 'auto' ? state.translation.result?.direction : state.translation.direction
  if (dir === 'zh-en' || dir === 'en-zh') {
    setTranslationDirection(dir === 'zh-en' ? 'en-zh' : 'zh-en')
  }
  setTranslationSource(text)
}

function onPanelKeydown(event: KeyboardEvent): void {
  // 中文输入法候选确认期间（isComposing / keyCode 229），Enter 与 Escape 都属于输入法，
  // 不能当成提交或关闭，否则会把候选词吞掉。
  if (event.isComposing || event.keyCode === 229) return

  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    dismiss()
    return
  }

  if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
    event.preventDefault()
    event.stopPropagation()
    onSubmit()
  }
}
</script>

<template>
  <section class="panel translation-panel" aria-label="翻译面板" @keydown="onPanelKeydown">
    <header class="head">
      <h2 class="title">翻译</h2>
      <button class="icon-btn" type="button" aria-label="关闭翻译面板" title="关闭" @click="dismiss()">
        <Icon name="close" :size="16" />
      </button>
    </header>

    <div class="content">
      <!-- 方向与快捷填充合一 -->
      <div class="top-bar">
        <div class="languages" role="group" aria-label="翻译方向">
          <ThemedSelect
            class="lang-select"
            size="xs"
            variant="ghost"
            aria-label="原文语言"
            :model-value="sourceLanguage"
            :options="sourceLanguageOptions"
            @change="val => chooseLanguage(val, true)"
          />
          <button
            v-gsap-press="state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap"
            class="swap"
            type="button"
            title="交换翻译方向"
            aria-label="交换翻译方向"
            @click="swapLanguages"
          >
            <span
              ref="swapIconEl"
              class="swap-icon"
            >
              <Icon name="swap" :size="16" />
            </span>
          </button>
          <ThemedSelect
            class="lang-select"
            size="xs"
            variant="ghost"
            aria-label="译文语言"
            :model-value="targetLanguage"
            :options="targetLanguageOptions"
            @change="val => chooseLanguage(val, false)"
          />
        </div>

        <div class="source-head-actions">
          <button
            v-if="latestClipText"
            class="link-btn"
            type="button"
            title="将最近一条文本剪贴板填入原文框（仍需手动点翻译）"
            @click="fillFromLatestClip"
          >填入最新剪贴板</button>
          <button v-if="hasSource" class="link-btn" type="button" @click="onClear">清空</button>
        </div>
      </div>

      <!-- 原文 -->
      <div class="source">
        <div class="field">
          <textarea
            class="textarea"
            :value="state.translation.source"
            rows="3"
            placeholder="粘贴或输入原文，然后点「翻译」"
            aria-label="要翻译的原文"
            spellcheck="false"
            @input="onSourceInput"
          />
          <span v-if="atLimit" class="limit-note">已到 {{ MAX_CHARS }} 字上限，超出部分会被截断</span>
          <span v-else-if="state.translation.filledFromClip" class="clip-note">来自剪贴板 · 需手动提交</span>
          <span class="counter" :class="{ 'is-near': nearLimit }">{{ count }} / {{ MAX_CHARS }}</span>
        </div>
      </div>

      <!-- 提交 -->
      <div class="actions">
        <button
          v-gsap-press="isLoading || state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap"
          class="primary"
          type="button"
          :disabled="isLoading"
          @click="onSubmit"
        >
          <span v-if="isLoading" class="pulse" aria-hidden="true">
            <i
              v-for="idx in 3"
              :key="idx"
              v-gsap-pulse="state.ui.islandReducedMotion ? null : { opacity: [0.3, 1], scale: [0.9, 1.15], duration: 1.05, delay: (idx - 1) * 0.14 }"
              class="pulse-dot"
            />
          </span>
          {{ isLoading ? '翻译中…' : '翻译' }}
        </button>
        <span class="hint">Ctrl / ⌘ + Enter 提交</span>
      </div>

      <GsapPresence
        :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: 6 }"
        :animate="{ opacity: 1, y: 0 }"
        :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: 4 }"
        :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
      >
        <div
          v-if="status !== 'idle'"
          key="translation-result"
          class="result"
        >
          <div class="result-head">
            <span class="label">译文</span>
            <span
              v-if="status === 'done' && !isDesktop"
              class="badge"
              title="译文由 MockHost 生成，只用于检查排版与状态，不是真实翻译"
            >
              样稿模拟结果
            </span>
            <span class="head-spacer" />
            <button
              v-if="status === 'done' && resultText.trim()"
              v-gsap-press="state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap"
              class="ghost"
              type="button"
              title="将当前译文回填到原文框并反转方向，方便回译校验"
              @click="useResultAsSource"
            >
              回填原文
            </button>
            <button
              v-if="status === 'done'"
              v-gsap-press="state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap"
              class="ghost copy"
              type="button"
              @click="onCopy"
            >
              <Icon name="copy" :size="13" />复制译文
            </button>
          </div>

          <div class="result-body">
            <p v-if="status === 'loading'" class="placeholder">正在翻译，请稍候…</p>
            <div v-else-if="status === 'error'" class="error">
              <p class="error-text">{{ state.translation.error }}</p>
              <button class="ghost" type="button" @click="onRetry">重试</button>
            </div>
            <p v-else class="result-text model-response" tabindex="0" aria-label="译文内容">{{ resultText }}</p>
          </div>

          <footer v-if="status === 'done'" class="result-foot">
            <span class="meta" :title="resultMeta">{{ resultMeta }}</span>
          </footer>
        </div>
      </GsapPresence>
    </div>

    <p class="sr-only" aria-live="polite">{{ liveStatus }}</p>
  </section>
</template>

<style scoped>
.panel {
  display: flex;
  flex-direction: column;
  width: 448px;
  max-height: 520px;
  overflow: hidden;
  color: var(--text);
  font-size: var(--text-base);
  background: var(--pop-bg);
  border: 1px solid var(--pop-border);
  border-radius: var(--r-pop);
  box-shadow: var(--pop-shadow);
}

/* 标题栏 */
.head {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  height: 44px;
  padding: 0 var(--space-2) 0 var(--space-4);
  border-bottom: 1px solid var(--line);
}

.title {
  flex: 1 1 auto;
  margin: 0;
  font-size: var(--text-md);
  font-weight: 600;
  letter-spacing: 0.2px;
}

.icon-btn {
  flex: none;
  width: 28px;
  height: 28px;
  display: grid;
  place-items: center;
  border-radius: var(--r-chip);
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.icon-btn:hover {
  background: var(--hover);
  color: var(--text);
}

.icon-btn:active {
  background: var(--active);
}

.icon-btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

/* 内容区：高度受限时只在这里滚动 */
.content {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
  padding: var(--space-3) var(--space-4);
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  overscroll-behavior: contain;
  scrollbar-width: thin;
  scrollbar-color: var(--line-strong) transparent;
}

.content::-webkit-scrollbar {
  width: 6px;
}

.content::-webkit-scrollbar-thumb {
  background: var(--line-strong);
  border-radius: 3px;
}

.content::-webkit-scrollbar-track {
  background: transparent;
}

.top-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-width: 0;
}

.languages {
  flex: none;
  display: inline-flex;
  align-items: center;
  padding: 2px;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-card);
  background: var(--surface-2);
}
.languages :deep(.lang-select), .swap {
  height: 28px; color: var(--text); background: transparent; border: 0; box-shadow: none;
  border-radius: var(--r-chip); font: inherit; font-size: var(--text-sm); cursor: pointer;
}
.languages :deep(.lang-select) { min-width: 6.4em; padding-inline: 7px; font-weight: 600; }
.swap { width: 30px; display: grid; place-items: center; color: var(--accent); touch-action: manipulation; }
.swap-icon { display: grid; place-items: center; }
.languages :deep(.lang-select):hover, .languages .swap:hover { background: var(--hover); }
.languages :focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }

/* 原文 */
.source {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}

.source-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: 20px;
}

.source-head-actions {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.label {
  font-size: var(--text-xs);
  color: var(--text-3);
  letter-spacing: 0.2px;
}

.link-btn {
  height: 20px;
  padding: 0 6px;
  border-radius: 6px;
  color: var(--text-3);
  font-size: var(--text-xs);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.link-btn:hover {
  background: var(--hover);
  color: var(--text);
}

.link-btn:active {
  background: var(--active);
}

.link-btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.chip-row {
  display: flex;
  min-width: 0;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 100%;
  height: 22px;
  padding: 0 8px;
  overflow: hidden;
  color: var(--text-2);
  font-size: var(--text-xs);
  white-space: nowrap;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
}

.field {
  position: relative;
}

.textarea {
  display: block;
  width: 100%;
  height: 76px;
  padding: 9px 10px 24px;
  color: var(--text);
  font: inherit;
  line-height: 1.55;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-card);
  resize: none;
  scrollbar-width: thin;
  scrollbar-color: var(--line-strong) transparent;
  transition: border-color var(--dur-fast) var(--ease);
}

.textarea::placeholder {
  color: var(--text-3);
}

.textarea:hover {
  border-color: var(--line-strong);
}

/* 键盘焦点：边框着色 + 全局 outline 收进边框内侧，避免出现双层焦点环 */
.textarea:focus-visible {
  border-color: var(--accent);
  outline-offset: -1px;
}

.textarea::-webkit-scrollbar {
  width: 6px;
}

.textarea::-webkit-scrollbar-thumb {
  background: var(--line-strong);
  border-radius: 3px;
}

.counter {
  position: absolute;
  right: 10px;
  bottom: 5px;
  color: var(--text-3);
  font-size: var(--text-xs);
  font-variant-numeric: tabular-nums;
  pointer-events: none;
  transition: color var(--dur-fast) var(--ease);
}

.counter.is-near {
  color: var(--danger);
}

.limit-note,
.clip-note {
  position: absolute;
  left: 10px;
  bottom: 5px;
  font-size: var(--text-xs);
  pointer-events: none;
}

.limit-note {
  color: var(--danger);
}

.clip-note {
  color: var(--text-3);
}

/* 提交 */
.actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}

.primary {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  min-width: 96px;
  height: 32px;
  padding: 0 16px;
  color: var(--accent-text);
  font-size: var(--text-sm);
  font-weight: 600;
  background: var(--accent);
  /* 常态就留出 1px 透明边框，禁用态换成可见边框时按钮不会跳动 */
  border: 1px solid transparent;
  border-radius: var(--r-chip);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease),
    border-color var(--dur-fast) var(--ease), transform var(--dur-fast) var(--ease);
}

.primary:hover:not(:disabled) {
  background: var(--accent-hover);
}

.primary:active:not(:disabled) {
  background: var(--accent-hover);
  transform: translateY(1px);
}

.primary:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.primary:disabled {
  color: var(--text-3);
  background: var(--hover);
  border: 1px solid var(--line);
  cursor: not-allowed;
}

.hint {
  color: var(--text-3);
  font-size: var(--text-xs);
}

/* 请求期间缓慢明暗变化。 */
.pulse {
  display: inline-flex;
  align-items: center;
  gap: 3px;
}

.pulse-dot {
  display: block;
  width: 3px;
  height: 3px;
  background: currentColor;
  border-radius: 50%;
}

/* 结果 */
.result {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: 8px 10px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-card);
}

.result-head {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
.result-head .label { flex: none; color: var(--text); font-weight: 600; }
.head-spacer { flex: 1; }
.result-foot {
  display: flex;
  align-items: center;
  padding-top: 4px;
  border-top: 1px solid var(--line);
}

.badge {
  display: inline-flex;
  align-items: center;
  height: 18px;
  padding: 0 7px;
  color: var(--accent);
  font-size: var(--text-xs);
  font-weight: 600;
  white-space: nowrap;
  background: var(--accent-weak);
  border: 1px solid var(--line-strong);
  border-radius: 9px;
}

.placeholder {
  display: flex;
  align-items: center;
  min-height: 22px;
  margin: 0;
  color: var(--text-3);
  font-size: var(--text-sm);
}

.result-text {
  max-height: 7.2em;
  margin: 0;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  color: var(--text);
  font-size: var(--text-base);
  line-height: 1.6;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
  scrollbar-width: thin;
  scrollbar-color: var(--line-strong) transparent;
}

.result-text::-webkit-scrollbar {
  width: 6px;
}

.result-text::-webkit-scrollbar-thumb {
  background: var(--line-strong);
  border-radius: 3px;
}

.error {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  justify-content: center;
  gap: 7px;
  min-height: 28px;
}

.error-text {
  margin: 0;
  color: var(--danger);
  font-size: var(--text-sm);
  line-height: 1.6;
}

.meta {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--text-3);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.ghost {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 24px;
  padding: 0 9px;
  color: var(--text-2);
  font-size: var(--text-xs);
  white-space: nowrap;
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease),
    border-color var(--dur-fast) var(--ease);
}

.ghost:hover {
  color: var(--text);
  background: var(--hover);
  border-color: var(--line-strong);
}

.ghost:active {
  background: var(--active);
}

.ghost:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.ghost.copy {
  flex: none;
  color: var(--accent);
  font-weight: 600;
  background: var(--accent-weak);
  border-color: var(--line-strong);
}

.ghost.copy:hover {
  color: var(--accent-text);
  background: var(--accent);
  border-color: var(--accent);
}

.ghost.copy:active {
  color: var(--accent-text);
  background: var(--accent-hover);
  border-color: var(--accent-hover);
}
</style>
