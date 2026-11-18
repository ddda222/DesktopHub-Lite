<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { version as appVersion } from '../../../package.json'
import Icon from '@/components/Icon.vue'
import DesktopFeatures from './DesktopFeatures.vue'
import AppManagement from './AppManagement.vue'
import ConfigBackup from './ConfigBackup.vue'
import './settings.css'
import {
  clearClips,
  setCaptureImages,
  saveClipHistoryLimit,
  createClipGroup,
  removeClipGroup,
  createSnippetGroup,
  removeSnippetGroup,
  saveTranslationSettings,
  saveTranslationSecret,
  deleteTranslationSecret,
  saveShortcuts,
  closeSettings,
  pushToast,
  setPaused,
  setTheme,
  state,
} from '@/store'
import type { ThemeMode, TranslateDirection } from '@/lib/desktop/types'
import { translationProviderPresets } from '@/lib/desktop/providers'
import { isDesktop } from '@/lib/desktop/desktop'
import { useWindowDrag } from '@/lib/useWindowDrag'

/**
 * standalone：桌面版里设置独占一个窗口，窗口本身就是「框」，
 * 不能再画一层半透明遮罩，否则桌面会被整块挡住。
 */
const props = withDefaults(defineProps<{ standalone?: boolean }>(), { standalone: false })
const windowEl = ref<HTMLElement | null>(null)
const islandDraftPending = ref(false)
const appsDraftPending = ref(false)
const windowDrag = useWindowDrag(windowEl, {
  native: () => props.standalone && isDesktop,
  onError: error => pushToast(`无法拖动设置：${String(error)}`, 'error')
})

/** 分区 id 直接取自 store，避免本地重复声明后与 store 漂移 */
type SettingsSection = typeof state.ui.settingsSection

const sections: { id: SettingsSection; label: string }[] = [
  { id: 'apps', label: '应用管理' },
  { id: 'clipboard', label: '剪贴板' },
  { id: 'translation', label: '翻译' },
  { id: 'general', label: '通用' }
]

const themeOptions: { value: ThemeMode; label: string }[] = [
  { value: 'system', label: '跟随系统' },
  { value: 'light', label: '浅色' },
  { value: 'dark', label: '深色' },
  { value: 'midnight', label: '星舰仪表' },
  { value: 'mecha', label: '琥珀重工' },
  { value: 'scroll', label: '海棠书卷' },
  { value: 'pixel', label: '像素掌机' }
]

const directionOptions: { value: TranslateDirection; label: string }[] = [
  { value: 'auto', label: '自动判断' },
  { value: 'zh-en', label: '中 → 英' },
  { value: 'en-zh', label: '英 → 中' }
]

const overlayEl = ref<HTMLElement | null>(null)

const currentSectionLabel = computed(
  () => sections.find((item) => item.id === state.ui.settingsSection)?.label ?? '设置'
)

function pickSection(id: SettingsSection): void {
  state.ui.settingsSection = id
  void nextTick(() => contentEl.value?.scrollTo({ top: 0 }))
}

/* ---------- 剪贴板 ---------- */

function togglePause(): void {
  void setPaused(!state.ui.paused)
}

function groupCount(group: string): number {
  return state.clips.filter((clip) => clip.group === group).length
}

function removeGroup(group: string): void {
  if (group === '未分组') return
  void removeClipGroup(group)
}

const historyLimitDraft = ref<number | null>(null)
const historyLimitPrompt = ref<{ limit: number; removed: number } | null>(null)
const savingClipboardPreferences = ref(false)
const clipboardFeedback = ref('')
const proposedHistoryLimit = computed(() =>
  historyLimitDraft.value ?? state.clipboardPreferences.historyLimit ?? state.clipboardPreferences.defaultHistoryLimit)
const ordinaryCount = computed(() => state.clips.filter((clip) => !clip.favorite).length)

async function toggleCaptureImages(): Promise<void> {
  if (savingClipboardPreferences.value) return
  savingClipboardPreferences.value = true
  try { await setCaptureImages(!state.clipboardPreferences.captureImages) }
  finally { savingClipboardPreferences.value = false }
}

async function applyHistoryLimit(): Promise<void> {
  if (savingClipboardPreferences.value) return
  const limit = proposedHistoryLimit.value
  if (!Number.isInteger(limit) || limit < 10 || limit > 5000) return
  const removed = Math.max(0, ordinaryCount.value - limit)
  if (removed && (historyLimitPrompt.value?.limit !== limit || historyLimitPrompt.value?.removed !== removed)) {
    historyLimitPrompt.value = { limit, removed }
    return
  }
  savingClipboardPreferences.value = true
  try {
    if (await saveClipHistoryLimit(limit, removed)) {
      historyLimitDraft.value = null
      clipboardFeedback.value = isDesktop ? '数量上限已应用' : '数量上限已应用（网页模拟，未落盘）'
    } else clipboardFeedback.value = '应用失败，请查看错误提示后重试。'
  } finally {
    historyLimitPrompt.value = null
    savingClipboardPreferences.value = false
  }
}

const newGroup = ref('')
function addGroup(): void {
  if (!isDesktop) return
  if (newGroup.value.trim()) { void createClipGroup(newGroup.value); newGroup.value = '' }
}

const newSnippetGroup = ref('')
function snippetGroupCount(group: string): number {
  return state.snippets.filter((item) => item.group === group).length
}
async function addSnippetGroup(): Promise<void> {
  if (!newSnippetGroup.value.trim()) return
  if (await createSnippetGroup(newSnippetGroup.value)) newSnippetGroup.value = ''
}
function onRemoveSnippetGroup(group: string): void {
  if (group === '未分组') return
  void removeSnippetGroup(group)
}

/** 清空需要二次确认，3 秒后自动回到初始文案 */
const confirmingClear = ref(false)
let clearTimer: number | null = null

function stopClearTimer(): void {
  if (clearTimer !== null) {
    window.clearTimeout(clearTimer)
    clearTimer = null
  }
}

function onClearClick(): void {
  if (!confirmingClear.value) {
    confirmingClear.value = true
    stopClearTimer()
    clearTimer = window.setTimeout(() => {
      confirmingClear.value = false
      clearTimer = null
    }, 3000)
    return
  }
  stopClearTimer()
  confirmingClear.value = false
  void clearClips()
}

/* ---------- 翻译 ---------- */

const translationDraft = ref({ ...state.translationSettings })
const translationSignature = () => JSON.stringify([translationDraft.value.providerId, translationDraft.value.direction, translationDraft.value.baseUrl, translationDraft.value.model])
const savedTranslation = ref(translationSignature())
const translationDirty = computed(() => translationSignature() !== savedTranslation.value)
const translationBusy = ref(false)
const translationFeedback = ref('')
const translationError = ref(false)
const secretSaved = computed(() => translationDraft.value.providerId === state.translationSettings.providerId && state.translationSettings.hasSecret)
watch(() => state.translationSettings, value => {
  if (!translationDirty.value) { translationDraft.value = { ...value }; savedTranslation.value = translationSignature() }
})
async function saveTranslation(): Promise<void> {
  if (translationBusy.value || !isDesktop) return
  translationBusy.value = true
  const submitted = translationSignature()
  try {
    const ok = await saveTranslationSettings(translationDraft.value)
    translationError.value = !ok
    translationFeedback.value = ok ? '翻译设置已保存' : '保存失败，请检查接口地址和模型后重试。'
    if (ok) savedTranslation.value = submitted
  } finally { translationBusy.value = false }
}

function onProviderChange(event: Event): void {
  const el = event.target as HTMLSelectElement | null
  if (!el) return
  const provider = translationProviderPresets.find((item) => item.id === el.value)
  if (!provider) return
  translationDraft.value.providerId = provider.id
  translationDraft.value.baseUrl = provider.baseUrl
  translationDraft.value.model = provider.model
  cancelSecret()
  translationFeedback.value = ''
}

function onDirectionChange(event: Event): void {
  const el = event.target as HTMLSelectElement | null
  if (!el) return
  translationDraft.value.direction = el.value as TranslateDirection
}

/** 密钥输入只存在于内存，保存后立即清空，界面不回显任何密钥内容 */
const secretEditing = ref(false)
const secretDraft = ref('')
const secretBusy = ref(false)

function editSecret(): void {
  secretEditing.value = true
}

function cancelSecret(): void {
  secretEditing.value = false
  secretDraft.value = ''
}

async function saveSecret(): Promise<void> {
  if (!secretDraft.value || !isDesktop || secretBusy.value) return
  const submitted = translationSignature()
  secretBusy.value = true
  try {
    if (!await saveTranslationSecret(secretDraft.value, translationDraft.value)) return
    savedTranslation.value = submitted
    secretDraft.value = ''; secretEditing.value = false
  } finally { secretBusy.value = false }
}

function removeSecret(): void {
  if (!isDesktop) return
  void deleteTranslationSecret()
  cancelSecret()
}

/* ---------- 通用 ---------- */

const shortcutDraft = ref({ ...state.shortcuts })
const shortcutSignature = () => JSON.stringify([shortcutDraft.value.bar, shortcutDraft.value.clipboard, shortcutDraft.value.translation])
const savedShortcuts = ref(shortcutSignature())
const shortcutsDirty = computed(() => shortcutSignature() !== savedShortcuts.value)
const shortcutsBusy = ref(false)
const shortcutsFeedback = ref('')
const shortcutsError = ref(false)
watch(() => state.shortcuts, value => {
  if (!shortcutsDirty.value) { shortcutDraft.value = { ...value }; savedShortcuts.value = shortcutSignature() }
})
async function applyShortcuts(): Promise<void> {
  if (shortcutsBusy.value || !isDesktop) return
  shortcutsBusy.value = true
  const submitted = shortcutSignature()
  try {
    const ok = await saveShortcuts(shortcutDraft.value)
    shortcutsError.value = !ok
    shortcutsFeedback.value = ok ? '快捷键已保存' : '保存或注册失败，请查看下方提示后重试。'
    if (ok) savedShortcuts.value = submitted
  } finally { shortcutsBusy.value = false }
}

function onThemeChange(theme: ThemeMode): void {
  setTheme(theme)
}

const contentEl = ref<HTMLElement | null>(null)
let returnFocus: HTMLElement | null = null
function handleKeys(event: KeyboardEvent): void {
  if (event.isComposing) return
  if (event.key === 'Escape') { event.stopPropagation(); closeSettings(); return }
  if (event.key !== 'Tab') return
  const nodes = Array.from(windowEl.value?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), summary, [tabindex="0"]') ?? []).filter(node => node.getClientRects().length)
  const first = nodes[0], last = nodes.at(-1)
  if (!first || !last) return
  if (event.shiftKey && (document.activeElement === first || !windowEl.value?.contains(document.activeElement))) { event.preventDefault(); last.focus() }
  else if (!event.shiftKey && (document.activeElement === last || !windowEl.value?.contains(document.activeElement))) { event.preventDefault(); first.focus() }
}
onMounted(() => {
  returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
  windowEl.value?.querySelector<HTMLButtonElement>('.nav-item.is-active')?.focus()
})
onBeforeUnmount(() => { stopClearTimer(); returnFocus?.focus({ preventScroll: true }) })
</script>

<template>
  <div
    ref="overlayEl"
    class="overlay settings-ui"
    :class="{ 'is-standalone': props.standalone }"
    tabindex="-1"
    @click.self="closeSettings()"
    @keydown="handleKeys"
  >
    <div ref="windowEl" class="win" role="dialog" :aria-modal="!props.standalone" aria-labelledby="settings-title">
      <header class="win-head" @pointerdown="windowDrag.begin" @click="windowDrag.consumeClick">
        <div class="window-brand"><span class="brand-mark" aria-hidden="true">D</span><h2 id="settings-title" class="win-title">设置</h2><span v-if="!isDesktop" class="preview-tag">网页预览 · 不写入本机配置</span></div>
        <button
          class="icon-btn"
          type="button"
          aria-label="关闭设置"
          title="关闭设置"
          @click="closeSettings()"
        >
          <Icon name="close" :size="16" />
        </button>
      </header>

      <div class="win-body">
        <nav class="nav" aria-label="设置分区">
          <button
            v-for="item in sections"
            :key="item.id"
            class="nav-item"
            :class="{ 'is-active': state.ui.settingsSection === item.id }"
            type="button"
            :aria-current="state.ui.settingsSection === item.id ? 'page' : undefined"
            @click="pickSection(item.id)"
          >
            {{ item.label }}
          </button>
        </nav>

        <section ref="contentEl" class="content scroll-y" :aria-label="currentSectionLabel">
          <AppManagement v-show="state.ui.settingsSection === 'apps'" @dirty-change="appsDraftPending = $event" />

          <!-- 剪贴板 -->
          <div v-show="state.ui.settingsSection === 'clipboard'" class="panel">
            <h3 class="panel-title">剪贴板</h3>
            <p class="hint">管理记录方式、历史数量与内容分组。开关立即生效。</p>

            <div class="card">
              <div class="set-row">
                <span class="set-text">
                  <span class="set-label">暂停记录剪贴板</span>
                  <span class="set-desc">暂停后新的复制内容不再进入历史记录。</span>
                </span>
                <button
                  class="switch"
                  type="button"
                  role="switch"
                  :class="{ 'is-on': state.ui.paused }"
                  :aria-checked="state.ui.paused"
                  aria-label="暂停记录剪贴板"
                  @click="togglePause"
                >
                  <span class="switch-knob" aria-hidden="true" />
                </button>
              </div>
            </div>

            <div class="card">
              <div class="set-row">
                <span class="set-text">
                  <span class="set-label">记录新图片</span>
                  <span class="set-desc">关闭仅影响之后的捕获，已有图片仍可查看和复制。</span>
                </span>
                <button class="switch" type="button" role="switch" aria-label="记录新图片"
                  :class="{ 'is-on': state.clipboardPreferences.captureImages }"
                  :aria-checked="state.clipboardPreferences.captureImages" :disabled="savingClipboardPreferences"
                  @click="toggleCaptureImages"><span class="switch-knob" aria-hidden="true" /></button>
              </div>
              <div class="set-row">
                <label class="set-text" for="history-limit">
                  <span class="set-label">普通记录数量上限</span>
                  <span class="set-desc">10–5000 条；超出时淘汰最旧普通记录，收藏不计入。当前：{{ state.clipboardPreferences.historyLimit ?? '未限制（保留旧历史）' }}</span>
                </label>
                <input id="history-limit" class="control" style="width: 90px" type="number" min="10" max="5000"
                  :value="proposedHistoryLimit" @input="historyLimitDraft = ($event.target as HTMLInputElement).valueAsNumber; historyLimitPrompt = null" />
                <button class="btn" type="button" :disabled="savingClipboardPreferences || !Number.isInteger(proposedHistoryLimit) || proposedHistoryLimit < 10 || proposedHistoryLimit > 5000"
                  @click="applyHistoryLimit">{{ historyLimitPrompt ? '确认应用' : '应用上限' }}</button>
              </div>
              <p v-if="historyLimitDraft !== null && !historyLimitPrompt" class="hint" role="status">{{ Number.isInteger(proposedHistoryLimit) && proposedHistoryLimit >= 10 && proposedHistoryLimit <= 5000 ? '数量上限尚未应用' : '请输入 10–5000 之间的整数' }}</p>
              <p v-else-if="clipboardFeedback" class="hint" role="status">{{ clipboardFeedback }}</p>
              <p v-if="historyLimitPrompt" class="hint" role="alert">将删除最旧的 {{ historyLimitPrompt.removed }} 条普通记录，不会删除收藏。
                <button class="btn" type="button" @click="historyLimitPrompt = null">取消</button>
              </p>
            </div>

            <h4 class="sub-title">历史记录分组</h4>
            <label class="field-label" for="new-clip-group">新分组名称</label>
            <div class="secret-edit">
              <input id="new-clip-group" v-model="newGroup" class="control" maxlength="40" placeholder="例如：工作资料" @keydown.enter.prevent="addGroup" />
              <button class="btn" type="button" :disabled="!isDesktop || !newGroup.trim()" @click="addGroup">添加分组</button>
            </div>
            <p class="hint">删除分组后，记录会回到“未分组”。</p>
            <div class="card">
              <div v-for="group in state.groups" :key="group" class="group-row">
                <span class="group-name">{{ group }}</span>
                <span v-if="group === '未分组'" class="tag tag-muted">默认组</span>
                <span class="count">{{ groupCount(group) }}</span>
                <button
                  class="icon-btn sm"
                  type="button"
                  :disabled="group === '未分组' || !isDesktop"
                  :aria-label="`删除分组 ${group}`"
                  :title="group === '未分组' ? '默认组不可删除' : '删除分组'"
                  @click="removeGroup(group)"
                >
                  <Icon name="trash" :size="13" />
                </button>
              </div>
            </div>

            <h4 class="sub-title">常用短语分组</h4>
            <label class="field-label" for="new-snippet-group">新短语分组名称</label>
            <div class="secret-edit">
              <input id="new-snippet-group" v-model="newSnippetGroup" class="control" maxlength="40" placeholder="例如：快捷回复、常用脚本" @keydown.enter.prevent="addSnippetGroup" />
              <button class="btn" type="button" :disabled="!newSnippetGroup.trim()" @click="addSnippetGroup">添加分组</button>
            </div>
            <p class="hint">删除短语分组后，该组短语会自动归入“未分组”，正文不会丢失。</p>
            <div class="card">
              <div v-for="group in state.snippetGroups" :key="group" class="group-row">
                <span class="group-name">{{ group }}</span>
                <span v-if="group === '未分组'" class="tag tag-muted">默认组</span>
                <span class="count">{{ snippetGroupCount(group) }}</span>
                <button
                  class="icon-btn sm"
                  type="button"
                  :disabled="group === '未分组'"
                  :aria-label="`删除短语分组 ${group}`"
                  :title="group === '未分组' ? '默认组不可删除' : '删除短语分组'"
                  @click="onRemoveSnippetGroup(group)"
                >
                  <Icon name="trash" :size="13" />
                </button>
              </div>
            </div>

            <div class="danger-row">
              <span class="set-text">
                <span class="set-label">全部历史记录</span>
                <span class="set-desc">包括所有收藏，但不会删除常用短语。清空后无法恢复，需要再点一次确认。</span>
              </span>
              <button
                class="btn btn-danger"
                :class="{ 'is-confirming': confirmingClear }"
                type="button"
                @click="onClearClick"
              >
                {{ confirmingClear ? '确认清空（3 秒内有效）' : '清空全部记录' }}
              </button>
            </div>
          </div>

          <!-- 翻译 -->
          <div v-show="state.ui.settingsSection === 'translation'" class="panel">
            <h3 class="panel-title">翻译</h3>
            <p class="hint">翻译默认手动提交，不会自动读取或上传剪贴板内容。</p>

            <div class="field-grid">
              <label class="field">
                <span class="field-label">服务商</span>
                <span class="select-wrap">
                  <select
                    class="control select"
                    :value="translationDraft.providerId"
                    @change="onProviderChange"
                  >
                    <option
                      v-for="provider in translationProviderPresets"
                      :key="provider.id"
                      :value="provider.id"
                    >
                      {{ provider.name }}
                    </option>
                  </select>
                  <Icon class="chev" name="chevron-down" :size="12" />
                </span>
              </label>

              <label class="field">
                <span class="field-label">默认方向</span>
                <span class="select-wrap">
                  <select
                    class="control select"
                    :value="translationDraft.direction"
                    @change="onDirectionChange"
                  >
                    <option v-for="opt in directionOptions" :key="opt.value" :value="opt.value">
                      {{ opt.label }}
                    </option>
                  </select>
                  <Icon class="chev" name="chevron-down" :size="12" />
                </span>
              </label>

              <label class="field field-wide">
                <span class="field-label">接口地址</span>
                <input
                  v-model="translationDraft.baseUrl"
                  class="control"
                  type="text"
                  spellcheck="false"
                  placeholder="https://api.example.com/v1"
                />
              </label>

              <label class="field field-wide">
                <span class="field-label">模型</span>
                <input
                  v-model="translationDraft.model"
                  class="control"
                  type="text"
                  spellcheck="false"
                  placeholder="填写服务商提供的模型名"
                />
              </label>
            </div>

            <div class="save-row"><p role="status" :class="{ error: translationError }">{{ translationDirty ? '有未保存的修改' : translationFeedback || '更改后点击保存' }}<span v-if="translationDirty && translationError"> · {{ translationFeedback }}</span></p><button class="btn btn-primary" type="button" :disabled="translationBusy || !isDesktop || !translationDraft.baseUrl.trim() || !translationDraft.model.trim()" @click="saveTranslation">{{ translationBusy ? '保存中…' : '保存翻译设置' }}</button></div>
            <div class="card">
              <div class="set-row">
                <span class="set-text">
                  <span class="set-label">API 密钥</span>
                  <span class="set-desc">
                    密钥保存在 Windows 凭据管理器；界面只读取是否存在；不进入日志。
                  </span>
                </span>
                <span
                  class="tag"
                  :class="secretSaved ? 'tag-ok' : 'tag-muted'"
                >
                  {{ secretSaved ? '已保存' : '未保存' }}
                </span>
              </div>

              <div class="secret-actions">
                <template v-if="secretSaved">
                  <button class="btn" type="button" :disabled="!isDesktop" @click="editSecret">替换密钥</button>
                  <button class="btn btn-quiet-danger" type="button" :disabled="!isDesktop" @click="removeSecret">
                    删除密钥
                  </button>
                </template>
                <button v-else class="btn btn-accent" type="button" :disabled="!isDesktop" @click="editSecret">
                  设置密钥
                </button>
              </div>

              <div v-if="secretEditing" class="secret-edit">
                <label class="field"><span class="field-label">新 API 密钥</span>
                <input
                  v-model="secretDraft"
                  class="control"
                  type="password"
                  aria-label="API 密钥"
                  placeholder="粘贴密钥，保存后立即清空"
                  autocomplete="off"
                  spellcheck="false"
                  @keydown.enter.prevent="saveSecret"
                /></label>
                <button
                  class="btn btn-accent"
                  type="button"
                  :disabled="secretDraft.length === 0 || !isDesktop || secretBusy"
                  @click="saveSecret"
                >
                  {{ secretBusy ? '保存中…' : '保存' }}
                </button>
                <button class="btn" type="button" :disabled="secretBusy" @click="cancelSecret">取消</button>
              </div>
            </div>
          </div>

          <!-- 通用 -->
          <div v-show="state.ui.settingsSection === 'general'" class="panel">
            <h3 class="panel-title">通用</h3><p class="page-description">设置外观、灵动岛与日常操作习惯。</p><h4 class="sub-title">显示主题 <span class="save-mode">即时生效</span></h4>

            <div class="card">
              <div class="set-row set-row-stack">
                <span class="set-text">
                  <span class="set-label">主题</span>
                  <span class="set-desc">立即应用到设置、小胶囊和展开面板；不同器物主题拥有独立的轮廓结构与控件形态。</span>
                </span>
                <span class="seg" role="radiogroup" aria-label="主题">
                  <label
                    v-for="opt in themeOptions"
                    :key="opt.value"
                    class="seg-item"
                    :class="{ 'is-on': state.settings.theme === opt.value }"
                  >
                    <input
                      class="sr-only"
                      type="radio"
                      name="settings-theme"
                      :value="opt.value"
                      :checked="state.settings.theme === opt.value"
                      @change="onThemeChange(opt.value)"
                    />
                    <span>{{ opt.label }}</span>
                  </label>
                </span>
              </div>

            </div>

            <DesktopFeatures @dirty-change="islandDraftPending = $event" />
            <ConfigBackup :blocked="islandDraftPending || appsDraftPending" />
            <h4 class="sub-title">全局快捷键</h4><div class="shortcut-fields">
              <label class="field"><span class="field-label">唤出图标条</span><input v-model="shortcutDraft.bar" class="control" /></label>
              <label class="field"><span class="field-label">剪贴板</span><input v-model="shortcutDraft.clipboard" class="control" /></label>
              <label class="field"><span class="field-label">翻译</span><input v-model="shortcutDraft.translation" class="control" /></label>
              <div class="save-row"><p role="status" :class="{ error: shortcutsError }">{{ shortcutsDirty ? '有未保存的修改' : shortcutsFeedback || '更改后点击保存' }}<span v-if="shortcutsDirty && shortcutsError"> · {{ shortcutsFeedback }}</span></p><button class="btn btn-primary" :disabled="shortcutsBusy || !isDesktop" @click="applyShortcuts">{{ shortcutsBusy ? '保存中…' : '保存快捷键' }}</button></div>
              <p v-for="error in state.shortcuts.errors" :key="error" class="hint">{{ error }}</p>
            </div>
            <p class="about">版本 {{ appVersion }} · DesktopHub Lite · 数据保存在本机独立目录</p>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>
