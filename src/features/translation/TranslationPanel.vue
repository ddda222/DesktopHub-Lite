<script setup lang="ts">
/**
 * 翻译面板（阶段 0 静态样稿）
 *
 * 安全与产品约束：
 * - 手动提交：输入、粘贴都不会自动发请求，只有点「翻译」或按 Ctrl/Cmd + Enter 才提交。
 * - 原文去向常驻可见：提交按钮下方固定一行，写明当前服务商名称与地址。
 * - 结果来自 MockHost，界面必须标「样稿模拟结果」，不能让人以为真实翻译成功。
 * - 密钥正文永不读取、永不显示，界面只知道「已保存 / 未保存」。
 */
import { computed } from 'vue'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import Icon from '@/components/Icon.vue'
import {
  closePanel,
  cancelTranslation,
  copyTranslation,
  setTranslationDirection,
  setTranslationSource,
  state,
  submitTranslation
} from '@/store'
import { findProvider } from '@/lib/desktop/providers'
import type { TranslateDirection } from '@/lib/desktop/types'

/** 方向三段式：自动判断 / 中→英 / 英→中 */
const props = defineProps<{ standalone?: boolean }>()
function dismiss(): void {
  if (props.standalone) {
    cancelTranslation()
    void invokeHost('close_translation_window')
  } else closePanel()
}

const directions: ReadonlyArray<{ value: TranslateDirection; label: string }> = [
  { value: 'auto', label: '自动判断' },
  { value: 'zh-en', label: '中→英' },
  { value: 'en-zh', label: '英→中' }
]

/** 与 store 的截断长度保持一致：setTranslationSource 按 5000 截断 */
const MAX_CHARS = 5000
/** 接近上限的提示阈值 */
const NEAR_LIMIT = 4500

/**
 * 服务商展示信息来自预设表：当前配置里的 providerId 找不到时回退到最后一个预设，
 * 保证模板里读 name / baseUrl 不会碰到 undefined。
 */
const FALLBACK_PROVIDER = { name: '未选择服务商', baseUrl: '未配置地址' }
const provider = computed(() => {
  const id = state.translationSettings.providerId
  if (!id) return FALLBACK_PROVIDER
  const item = findProvider(id)
  return { name: item.name, baseUrl: state.translationSettings.baseUrl || item.baseUrl }
})
const status = computed(() => state.translation.status)
const isLoading = computed(() => state.translation.status === 'loading')
const count = computed(() => state.translation.source.length)
const hasSource = computed(() => state.translation.source.length > 0)
const nearLimit = computed(() => count.value > NEAR_LIMIT)
const atLimit = computed(() => count.value >= MAX_CHARS)
const hasSecret = computed(() => state.translationSettings.hasSecret)
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
      <!-- 方向 -->
      <div class="seg" role="group" aria-label="翻译方向">
        <button
          v-for="item in directions"
          :key="item.value"
          class="seg-btn"
          :class="{ 'is-active': state.translation.direction === item.value }"
          type="button"
          :aria-pressed="state.translation.direction === item.value"
          @click="setTranslationDirection(item.value)"
        >
          {{ item.label }}
        </button>
      </div>

      <!-- 原文 -->
      <div class="source">
        <div class="source-head">
          <span class="label">原文</span>
          <button v-if="hasSource" class="link-btn" type="button" @click="onClear">清空</button>
        </div>

        <div v-if="state.translation.filledFromClip" class="chip-row">
          <span class="chip">
            <Icon name="clipboard" :size="12" />
            来自剪贴板记录 · 仍需手动提交
          </span>
        </div>

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
          <span class="counter" :class="{ 'is-near': nearLimit }">{{ count }} / {{ MAX_CHARS }}</span>
          <span v-if="atLimit" class="limit-note">已到 {{ MAX_CHARS }} 字上限，超出部分会被截断</span>
        </div>
      </div>

      <!-- 提交 -->
      <div class="actions">
        <button class="primary" type="button" :disabled="isLoading" @click="onSubmit">
          <span v-if="isLoading" class="pulse" aria-hidden="true">
            <i class="pulse-dot" />
            <i class="pulse-dot" />
            <i class="pulse-dot" />
          </span>
          {{ isLoading ? '翻译中…' : '翻译' }}
        </button>
        <span class="hint">Ctrl / ⌘ + Enter 提交</span>
      </div>

      <!-- 原文去向：常驻说明，不能只在设置里出现 -->
      <div class="notice">
        <Icon class="notice-icon" name="info" :size="13" />
        <div class="notice-body">
          <p class="notice-main">
            原文会发送到 <strong>{{ provider.name }}</strong>（{{ provider.baseUrl }}）处理，不是在本机翻译。
          </p>
          <p class="notice-sub">
            <i class="state-dot" :class="{ 'is-on': hasSecret }" aria-hidden="true" />
            密钥{{ hasSecret ? '已保存' : '未保存' }}<span v-if="!isDesktop"> · 浏览器样稿不会发出真实网络请求</span>
          </p>
        </div>
      </div>

      <!-- 结果：四种状态共用同一高度，切换不跳版 -->
      <div class="result">
        <div class="result-head">
          <span class="label">译文</span>
          <span
            v-if="status === 'done' && !isDesktop"
            class="badge"
            title="译文由 MockHost 生成，只用于检查排版与状态，不是真实翻译"
          >
            样稿模拟结果
          </span>
        </div>

        <div class="result-body">
          <p v-if="status === 'idle'" class="placeholder">译文会显示在这里</p>
          <p v-else-if="status === 'loading'" class="placeholder">正在翻译，请稍候…</p>
          <div v-else-if="status === 'error'" class="error">
            <p class="error-text">{{ state.translation.error }}</p>
            <button class="ghost" type="button" @click="onRetry">重试</button>
          </div>
          <p v-else class="result-text">{{ resultText }}</p>
        </div>

        <!-- 底栏始终占位：从 idle / loading 切到 done 时不会把面板顶高 -->
        <div class="result-foot" :class="{ 'is-ghost': status !== 'done' }">
          <span class="meta">{{ status === 'done' ? resultMeta : '' }}</span>
          <button v-if="status === 'done'" class="ghost copy" type="button" @click="onCopy">
            <Icon name="copy" :size="12" />
            复制译文
          </button>
        </div>
      </div>
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
  gap: 10px;
  min-height: 0;
  padding: var(--space-3) var(--space-4);
  overflow-y: auto;
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

/* 方向 */
.seg {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 2px;
  padding: 2px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
}

.seg-btn {
  height: 26px;
  border-radius: 6px;
  color: var(--text-2);
  font-size: var(--text-sm);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.seg-btn:hover {
  background: var(--hover);
  color: var(--text);
}

.seg-btn:active {
  background: var(--active);
}

.seg-btn.is-active {
  background: var(--accent-weak);
  color: var(--accent);
  font-weight: 600;
}

.seg-btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

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
  height: 84px;
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

.limit-note {
  position: absolute;
  left: 10px;
  bottom: 5px;
  color: var(--danger);
  font-size: var(--text-xs);
  pointer-events: none;
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

/* 克制的加载指示：170ms 一轮，只在按钮内 */
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
  animation: dot-pulse var(--dur) var(--ease) infinite;
}

.pulse-dot:nth-child(2) {
  animation-delay: 55ms;
}

.pulse-dot:nth-child(3) {
  animation-delay: 110ms;
}

@keyframes dot-pulse {
  0%,
  100% {
    opacity: 0.3;
  }
  50% {
    opacity: 1;
  }
}

@media (prefers-reduced-motion: reduce) {
  .pulse-dot {
    opacity: 0.7;
    animation-name: none;
  }
}

/* 原文去向说明 */
.notice {
  display: flex;
  gap: 7px;
  padding: 7px 9px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
}

.notice-icon {
  flex: none;
  margin-top: 2px;
  color: var(--text-3);
}

.notice-body {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}

.notice-main {
  margin: 0;
  color: var(--text-2);
  font-size: var(--text-xs);
  line-height: 1.6;
  overflow-wrap: anywhere;
}

.notice-main strong {
  color: var(--text);
  font-weight: 600;
}

.notice-sub {
  display: flex;
  align-items: center;
  gap: 5px;
  margin: 0;
  color: var(--text-3);
  font-size: var(--text-xs);
  line-height: 1.5;
}

.state-dot {
  display: inline-block;
  width: 5px;
  height: 5px;
  background: var(--text-3);
  border-radius: 50%;
}

.state-dot.is-on {
  background: var(--success);
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
  justify-content: space-between;
  gap: var(--space-2);
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

.result-body {
  min-height: 60px;
}

.placeholder {
  display: flex;
  align-items: center;
  min-height: 60px;
  margin: 0;
  color: var(--text-3);
  font-size: var(--text-sm);
}

.result-text {
  min-height: 60px;
  max-height: 132px;
  margin: 0;
  overflow-y: auto;
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
  min-height: 60px;
}

.error-text {
  margin: 0;
  color: var(--danger);
  font-size: var(--text-sm);
  line-height: 1.6;
}

.result-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  min-height: 25px;
  padding-top: 8px;
  border-top: 1px solid var(--line);
}

/* 非 done 时底栏只占高度不显示内容，避免状态切换时结果区跳版 */
.result-foot.is-ghost {
  border-top-color: transparent;
}

.meta {
  min-width: 0;
  overflow: hidden;
  color: var(--text-3);
  font-size: var(--text-xs);
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
