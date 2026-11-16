<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import Icon from '@/components/Icon.vue'
import DesktopFeatures from './DesktopFeatures.vue'
import {
  addAppFromPicker,
  addAppByTarget,
  clearClips,
  setCaptureImages,
  saveClipHistoryLimit,
  createClipGroup,
  removeClipGroup,
  saveTranslationSettings,
  saveTranslationSecret,
  deleteTranslationSecret,
  saveShortcuts,
  saveAppCategory,
  closeSettings,
  deleteApp,
  iconFor,
  importShortcuts,
  moveApp,
  reorderApps,
  pushToast,
  setAppCategory,
  setPaused,
  setTheme,
  state,
  toggleAppHidden,
  updateApp
} from '@/store'
import type { ThemeMode, TranslateDirection } from '@/lib/desktop/types'
import { translationProviderPresets } from '@/lib/desktop/providers'

/**
 * standalone：桌面版里设置独占一个窗口，窗口本身就是「框」，
 * 不能再画一层半透明遮罩，否则桌面会被整块挡住。
 */
const props = withDefaults(defineProps<{ standalone?: boolean }>(), { standalone: false })

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
  { value: 'dark', label: '深色' }
]

const directionOptions: { value: TranslateDirection; label: string }[] = [
  { value: 'auto', label: '自动判断' },
  { value: 'zh-en', label: '中 → 英' },
  { value: 'en-zh', label: '英 → 中' }
]

const overlayEl = ref<HTMLElement | null>(null)

/** 分类下拉排除「全部」这个筛选项 */
const categoryOptions = computed(() => state.categories.filter((cat) => cat.id !== 'all'))
const newCategory = ref('')
const linkTarget = ref('')
const linkName = ref('')
const addingLink = ref(false)
async function addLink(): Promise<void> {
  if (!/^https?:\/\//i.test(linkTarget.value.trim())) { pushToast('请输入以 https:// 或 http:// 开头的网址', 'error'); return }
  const result = await addAppByTarget(linkTarget.value, undefined, linkName.value || undefined)
  if (result.ok) { addingLink.value = false; linkTarget.value = ''; linkName.value = '' }
}
function addCategory(): void {
  if (!newCategory.value.trim()) return
  void saveAppCategory(`c-${Date.now().toString(36)}`, newCategory.value.trim())
  newCategory.value = ''
}

const currentSectionLabel = computed(
  () => sections.find((item) => item.id === state.ui.settingsSection)?.label ?? '设置'
)

function pickSection(id: SettingsSection): void {
  state.ui.settingsSection = id
}

/* ---------- 应用管理 ---------- */
const selectedAppId = ref<string | null>(null)
const draggedAppId = ref<string | null>(null)
const dropTarget = ref('')
const selectedApps = computed(() => state.apps.filter(app => app.id === selectedAppId.value))
function beginAppDrag(event: DragEvent, id: string): void {
  draggedAppId.value = id
  event.dataTransfer?.setData('text/plain', id)
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}
function endAppDrag(): void { draggedAppId.value = null; dropTarget.value = '' }
function dropApp(id: string): void {
  if (draggedAppId.value && draggedAppId.value !== id) reorderApps(draggedAppId.value, id)
  endAppDrag()
}
function dropCategory(id: string): void {
  if (draggedAppId.value) setAppCategory(draggedAppId.value, id)
  endAppDrag()
}


/** 导入桌面快捷方式：只读用户桌面与公共桌面，结果由 store 明确提示。 */
function onImport(): void {
  void importShortcuts()
}

function onCategoryChange(event: Event, id: string): void {
  const el = event.target as HTMLSelectElement | null
  if (!el) return
  setAppCategory(id, el.value)
}

/* 添加：走系统文件选择框，目标来自用户的明确选择，不做全盘扫描 */
const adding = ref(false)

async function onAddApp(kind: 'file' | 'folder'): Promise<void> {
  if (adding.value) return
  adding.value = true
  try {
    const outcome = await addAppFromPicker(undefined, kind)
    // 取消选择不提示错误，也不改变列表
    if (!outcome.ok && outcome.cancelled) return
  } finally {
    adding.value = false
  }
}

/* 编辑：行内展开，名称与目标都可改；目标改动后图标会重新读取 */
const editingId = ref<string | null>(null)
const editName = ref('')
const editTarget = ref('')
const saving = ref(false)

function startEdit(app: { id: string; name: string; target: string }): void {
  editingId.value = app.id
  editName.value = app.name
  editTarget.value = app.target
}

function cancelEdit(): void {
  editingId.value = null
  editName.value = ''
  editTarget.value = ''
}

async function submitEdit(): Promise<void> {
  if (!editingId.value || saving.value) return
  saving.value = true
  try {
    const outcome = await updateApp(editingId.value, {
      name: editName.value,
      target: editTarget.value
    })
    if (outcome.ok) cancelEdit()
  } finally {
    saving.value = false
  }
}

/* 删除需要二次确认，避免误点把配置删掉 */
const confirmingId = ref<string | null>(null)
let confirmTimer: number | null = null

function stopConfirmTimer(): void {
  if (confirmTimer !== null) {
    window.clearTimeout(confirmTimer)
    confirmTimer = null
  }
}

function onDeleteClick(id: string): void {
  if (confirmingId.value !== id) {
    confirmingId.value = id
    stopConfirmTimer()
    confirmTimer = window.setTimeout(() => {
      confirmingId.value = null
      confirmTimer = null
    }, 3000)
    return
  }
  stopConfirmTimer()
  confirmingId.value = null
  void deleteApp(id)
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
    if (await saveClipHistoryLimit(limit, removed)) historyLimitDraft.value = null
  } finally {
    historyLimitPrompt.value = null
    savingClipboardPreferences.value = false
  }
}

const newGroup = ref('')
function addGroup(): void {
  if (newGroup.value.trim()) { void createClipGroup(newGroup.value); newGroup.value = '' }
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

function onProviderChange(event: Event): void {
  const el = event.target as HTMLSelectElement | null
  if (!el) return
  const provider = translationProviderPresets.find((item) => item.id === el.value)
  if (!provider) return
  state.translationSettings.providerId = provider.id
  state.translationSettings.baseUrl = provider.baseUrl
  state.translationSettings.model = provider.model
  state.translationSettings.hasSecret = false
}

function onDirectionChange(event: Event): void {
  const el = event.target as HTMLSelectElement | null
  if (!el) return
  state.translationSettings.direction = el.value as TranslateDirection
}

/** 密钥输入只存在于内存，保存后立即清空，界面不回显任何密钥内容 */
const secretEditing = ref(false)
const secretDraft = ref('')

function editSecret(): void {
  secretEditing.value = true
}

function cancelSecret(): void {
  secretEditing.value = false
  secretDraft.value = ''
}

async function saveSecret(): Promise<void> {
  if (!secretDraft.value) return
  if (!await saveTranslationSecret(secretDraft.value)) return
  secretDraft.value = ''
  secretEditing.value = false
}

function removeSecret(): void {
  void deleteTranslationSecret()
  cancelSecret()
}

/* ---------- 通用 ---------- */

function onThemeChange(theme: ThemeMode): void {
  setTheme(theme)
}

onMounted(() => {
  overlayEl.value?.focus()
})

onBeforeUnmount(() => {
  stopClearTimer()
  stopConfirmTimer()
})
</script>

<template>
  <div
    ref="overlayEl"
    class="overlay"
    :class="{ 'is-standalone': props.standalone }"
    tabindex="-1"
    @click.self="closeSettings()"
    @keydown.escape.stop="closeSettings()"
  >
    <div class="win" role="dialog" aria-modal="true" aria-labelledby="settings-title">
      <header class="win-head">
        <h2 id="settings-title" class="win-title">设置</h2>
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

        <section class="content" :aria-label="currentSectionLabel">
          <!-- 应用管理 -->
          <div v-if="state.ui.settingsSection === 'apps'" class="panel panel-fixed">
            <div class="panel-head">
              <h3 class="panel-title">应用管理</h3>
              <span class="head-actions">
                <button
                  class="btn"
                  type="button"
                  :disabled="adding"
                  title="选择一个 EXE / 快捷方式 / 文件作为应用"
                  @click="onAddApp('file')"
                >
                  添加文件…
                </button>
                <button
                  class="btn"
                  type="button"
                  :disabled="adding"
                  title="选择一个文件夹作为应用"
                  @click="onAddApp('folder')"
                >
                  添加文件夹…
                </button>
                <button class="btn btn-accent" type="button" @click="onImport">
                  导入桌面快捷方式
                </button>
                <button class="btn" type="button" @click="addingLink = !addingLink">添加网址</button>
              </span>
            </div>
            <p class="hint">
              目标来自你的手动选择或桌面快捷方式；不扫描全盘。启动请求交给系统，系统接受不代表窗口已打开。
            </p>

            <div v-if="addingLink" class="card">
              <input v-model="linkName" class="control" placeholder="显示名称（可选）" />
              <input v-model="linkTarget" class="control" placeholder="https://example.com" @keydown.enter.prevent="addLink" />
              <button class="btn btn-accent" type="button" @click="addLink">添加</button>
            </div>
            <details class="card">
              <summary>管理分类</summary>
              <div v-for="category in categoryOptions" :key="category.id" class="group-row">
                <input class="control" :value="category.name" maxlength="40" :aria-label="`${category.name} 分类名称`" @change="saveAppCategory(category.id, ($event.target as HTMLInputElement).value)" />
                <button class="btn" type="button" @click="saveAppCategory(category.id, null)">删除分类</button>
              </div>
              <p class="hint">删除分类保留应用，应用仍可在“全部”中找到。</p>
              <div class="secret-edit"><input v-model="newCategory" class="control" maxlength="40" placeholder="新分类名称" @keydown.enter.prevent="addCategory" /><button class="btn" type="button" @click="addCategory">添加分类</button></div>
            </details>
            <p class="hint">拖动图标调整顺序，拖到分类改变归属；点击图标编辑。隐藏的应用也在此显示。</p>
            <div class="category-targets" aria-label="拖放分类">
              <button v-for="category in categoryOptions" :key="category.id" class="btn" type="button"
                :class="{ 'drop-target': dropTarget === category.id }"
                @dragover.prevent="dropTarget = category.id" @dragleave="dropTarget = ''" @drop.prevent="dropCategory(category.id)">
                {{ category.name }}
              </button>
            </div>
            <div class="app-workspace scroll-y">
            <div class="manage-grid" role="group" aria-label="全部应用，拖动排序">
              <button v-for="app in state.apps" :key="app.id" type="button" class="manage-tile" draggable="true"
                :class="{ selected: selectedAppId === app.id, 'is-hidden': app.hidden, 'drop-target': dropTarget === app.id }"
                :title="`${app.name} · ${state.categories.find(c => c.id === app.categoryId)?.name ?? '未分类'}`"
                :aria-pressed="selectedAppId === app.id" @click="selectedAppId = app.id"
                @dragstart="beginAppDrag($event, app.id)" @dragend="endAppDrag"
                @dragover.prevent="dropTarget = app.id" @dragleave="dropTarget = ''" @drop.prevent="dropApp(app.id)">
                <img v-if="iconFor(app.target)" :src="iconFor(app.target)" alt="" draggable="false" />
                <span v-else class="app-icon" :style="{ background: app.color }">{{ app.initials }}</span>
                <span class="manage-name">{{ app.name }}</span>
                <small v-if="app.hidden || app.broken">{{ app.broken ? '失效' : '隐藏' }}</small>
              </button>
            </div>
            <div class="app-list" role="list" aria-label="所选应用设置">
              <div v-if="!state.apps.length" class="app-empty">
                还没有应用。用「添加文件…」选一个程序，或导入桌面快捷方式。
              </div>

              <div
                v-for="app in selectedApps"
                :key="app.id"
                class="app-row"
                :class="{ 'is-hidden': app.hidden, 'is-editing': editingId === app.id }"
                role="listitem"
              >
                <img
                  v-if="iconFor(app.target)"
                  class="app-img"
                  :src="iconFor(app.target)"
                  :alt="app.name"
                  aria-hidden="true"
                />
                <span v-else class="app-icon" :style="{ background: app.color }" aria-hidden="true">
                  {{ app.initials }}
                </span>

                <!-- 编辑态：名称与目标都可改 -->
                <div v-if="editingId === app.id" class="app-edit">
                  <input
                    v-model="editName"
                    class="control input input-sm"
                    type="text"
                    :aria-label="`${app.name} 的名称`"
                    placeholder="显示名称"
                    @keydown.enter.prevent="submitEdit"
                    @keydown.escape.prevent="cancelEdit"
                  />
                  <input
                    v-model="editTarget"
                    class="control input input-sm mono"
                    type="text"
                    :aria-label="`${app.name} 的目标`"
                    placeholder="EXE / 快捷方式 / 文件夹 / 网址"
                    @keydown.enter.prevent="submitEdit"
                    @keydown.escape.prevent="cancelEdit"
                  />
                  <span class="edit-actions">
                    <button class="btn btn-sm btn-accent" type="button" :disabled="saving" @click="submitEdit">
                      {{ saving ? '保存中…' : '保存' }}
                    </button>
                    <button class="btn btn-sm" type="button" :disabled="saving" @click="cancelEdit">
                      取消
                    </button>
                  </span>
                </div>

                <template v-else>
                  <div class="app-meta">
                    <span class="app-name">
                      {{ app.name }}
                      <span v-if="app.broken" class="broken-tag" title="目标不存在">失效</span>
                    </span>
                    <span class="app-target" :title="app.target">{{ app.target }}</span>
                  </div>

                  <span class="select-wrap">
                    <select
                      class="control select select-sm"
                      :value="app.categoryId"
                      :aria-label="`${app.name} 的分类`"
                      @change="onCategoryChange($event, app.id)"
                    >
                      <option v-for="cat in state.categories" :key="cat.id" :value="cat.id">
                        {{ cat.name }}
                      </option>
                    </select>
                    <Icon class="chev" name="chevron-down" :size="12" />
                  </span>

                  <button
                    class="vis"
                    type="button"
                    :class="{ 'is-off': app.hidden }"
                    :aria-pressed="!app.hidden"
                    :aria-label="`在图标条显示 ${app.name}`"
                    :title="app.hidden ? '当前已隐藏，点击显示' : '当前显示中，点击隐藏'"
                    @click="toggleAppHidden(app.id)"
                  >
                    <span class="vis-dot" aria-hidden="true" />{{ app.hidden ? '隐藏' : '显示' }}
                  </button>

                  <span class="row-actions">
                    <button
                      class="icon-btn sm"
                      type="button"
                      :aria-label="`编辑 ${app.name}`"
                      title="编辑名称与目标"
                      @click="startEdit(app)"
                    >
                      <Icon name="edit" :size="13" />
                    </button>
                    <button
                      class="icon-btn sm danger"
                      type="button"
                      :class="{ 'is-confirm': confirmingId === app.id }"
                      :aria-label="
                        confirmingId === app.id ? `确认移除 ${app.name}` : `移除 ${app.name}`
                      "
                      :title="confirmingId === app.id ? '再点一次确认移除' : '从图标条移除（不删文件）'"
                      @click="onDeleteClick(app.id)"
                    >
                      <Icon name="trash" :size="13" />
                    </button>
                  </span>

                  <span class="moves">
                    <button
                      class="icon-btn sm"
                      type="button"
                      :disabled="state.apps.findIndex(item => item.id === app.id) === 0"
                      :aria-label="`上移 ${app.name}`"
                      title="上移"
                      @click="moveApp(state.apps.findIndex(item => item.id === app.id), -1)"
                    >
                      <Icon class="flip" name="chevron-down" :size="13" />
                    </button>
                    <button
                      class="icon-btn sm"
                      type="button"
                      :disabled="state.apps.findIndex(item => item.id === app.id) === state.apps.length - 1"
                      :aria-label="`下移 ${app.name}`"
                      title="下移"
                      @click="moveApp(state.apps.findIndex(item => item.id === app.id), 1)"
                    >
                      <Icon name="chevron-down" :size="13" />
                    </button>
                  </span>
                </template>
              </div>
            </div>
            </div>
          </div>

          <!-- 剪贴板 -->
          <div v-else-if="state.ui.settingsSection === 'clipboard'" class="panel scroll-y">
            <h3 class="panel-title">剪贴板</h3>
            <p class="hint">日志不记录剪贴板正文、图片和密钥；单条删除立即生效。</p>

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
              <p v-if="historyLimitPrompt" class="hint" role="alert">将删除最旧的 {{ historyLimitPrompt.removed }} 条普通记录，不会删除收藏。
                <button class="btn" type="button" @click="historyLimitPrompt = null">取消</button>
              </p>
            </div>

            <h4 class="sub-title">分组</h4>
            <div class="secret-edit">
              <input v-model="newGroup" class="control" maxlength="40" placeholder="新分组名称" @keydown.enter.prevent="addGroup" />
              <button class="btn" type="button" @click="addGroup">添加分组</button>
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
                  :disabled="group === '未分组'"
                  :aria-label="`删除分组 ${group}`"
                  :title="group === '未分组' ? '默认组不可删除' : '删除分组'"
                  @click="removeGroup(group)"
                >
                  <Icon name="trash" :size="13" />
                </button>
              </div>
            </div>

            <div class="danger-row">
              <span class="set-text">
                <span class="set-label">全部记录</span>
                <span class="set-desc">包括所有收藏。清空后无法恢复，需要再点一次确认。</span>
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
          <div v-else-if="state.ui.settingsSection === 'translation'" class="panel scroll-y">
            <h3 class="panel-title">翻译</h3>
            <p class="hint">翻译默认手动提交，不会自动读取或上传剪贴板内容。</p>

            <div class="field-grid">
              <label class="field">
                <span class="field-label">服务商</span>
                <span class="select-wrap">
                  <select
                    class="control select"
                    :value="state.translationSettings.providerId"
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
                    :value="state.translationSettings.direction"
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
                  v-model="state.translationSettings.baseUrl"
                  class="control"
                  type="text"
                  spellcheck="false"
                  placeholder="https://api.example.com/v1"
                />
              </label>

              <label class="field field-wide">
                <span class="field-label">模型</span>
                <input
                  v-model="state.translationSettings.model"
                  class="control"
                  type="text"
                  spellcheck="false"
                  placeholder="填写服务商提供的模型名"
                />
              </label>
            </div>

            <button class="btn btn-accent" type="button" @click="saveTranslationSettings">保存翻译设置</button>
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
                  :class="state.translationSettings.hasSecret ? 'tag-ok' : 'tag-muted'"
                >
                  {{ state.translationSettings.hasSecret ? '已保存' : '未保存' }}
                </span>
              </div>

              <div class="secret-actions">
                <template v-if="state.translationSettings.hasSecret">
                  <button class="btn" type="button" @click="editSecret">替换密钥</button>
                  <button class="btn btn-quiet-danger" type="button" @click="removeSecret">
                    删除密钥
                  </button>
                </template>
                <button v-else class="btn btn-accent" type="button" @click="editSecret">
                  设置密钥
                </button>
              </div>

              <div v-if="secretEditing" class="secret-edit">
                <input
                  v-model="secretDraft"
                  class="control"
                  type="password"
                  aria-label="API 密钥"
                  placeholder="粘贴密钥，保存后立即清空"
                  autocomplete="off"
                  spellcheck="false"
                  @keydown.enter.prevent="saveSecret"
                />
                <button
                  class="btn btn-accent"
                  type="button"
                  :disabled="secretDraft.length === 0"
                  @click="saveSecret"
                >
                  保存
                </button>
                <button class="btn" type="button" @click="cancelSecret">取消</button>
              </div>
            </div>
          </div>

          <!-- 通用 -->
          <div v-else class="panel scroll-y">
            <h3 class="panel-title">通用</h3>

            <div class="card">
              <div class="set-row">
                <span class="set-text">
                  <span class="set-label">主题</span>
                  <span class="set-desc">跟随系统会随 Windows 的浅色 / 深色设置切换。</span>
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

            <DesktopFeatures />
            <div class="card fields">
              <label class="field"><span class="field-label">唤出图标条</span><input v-model="state.shortcuts.bar" class="control" /></label>
              <label class="field"><span class="field-label">剪贴板</span><input v-model="state.shortcuts.clipboard" class="control" /></label>
              <label class="field"><span class="field-label">翻译</span><input v-model="state.shortcuts.translation" class="control" /></label>
              <button class="btn" type="button" @click="saveShortcuts">保存快捷键</button>
              <p v-for="error in state.shortcuts.errors" :key="error" class="hint">{{ error }}</p>
            </div>
            <p class="about">版本 0.1.0 · DesktopHub Lite · 数据保存在本机独立目录</p>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* ---------- 遮罩与窗口 ---------- */
.overlay {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal);
  display: grid;
  place-items: center;
  padding: 24px;
  /* 浅色主题使用同一层遮罩，仅靠透明度区分 */
  background: rgba(0, 0, 0, 0.45);
}

.overlay:focus-visible {
  outline: none;
}

/* 桌面版：窗口本身就是「框」，不画遮罩、不留内边距 */
.overlay.is-standalone {
  padding: 0;
  background: transparent;
}

.overlay.is-standalone .win {
  width: 100%;
  height: 100%;
  max-width: none;
  max-height: none;
  border-radius: 12px;
}

.win {
  width: 760px;
  height: 560px;
  max-width: 100%;
  max-height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--surface);
  border: 1px solid var(--line-strong);
  border-radius: 16px;
  box-shadow: var(--pop-shadow);
  color: var(--text);
}

.win-head {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  height: 48px;
  padding: 0 12px 0 18px;
  border-bottom: 1px solid var(--line);
}

.win-title {
  margin: 0;
  font-size: var(--text-md);
  font-weight: 600;
  letter-spacing: 0.2px;
}

.win-body {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
}

/* ---------- 左导航 ---------- */
.nav {
  flex: none;
  width: 172px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 10px 8px;
  border-right: 1px solid var(--line);
}

.nav-item {
  position: relative;
  height: 34px;
  padding: 0 10px;
  border-radius: 8px;
  text-align: left;
  font-size: var(--text-base);
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.nav-item:hover {
  background: var(--hover);
  color: var(--text);
}

.nav-item:active {
  background: var(--active);
}

.nav-item:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.nav-item.is-active {
  background: var(--accent-weak);
  color: var(--accent);
}

.nav-item.is-active::before {
  content: '';
  position: absolute;
  left: 0;
  top: 8px;
  bottom: 8px;
  width: 2px;
  border-radius: 1px;
  background: var(--accent);
}

/* ---------- 内容区 ---------- */
.content {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.panel {
  flex: 1 1 auto;
  min-height: 0;
  padding: 16px 18px 18px;
}

.panel-fixed {
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  margin-bottom: 6px;
}

.panel-title {
  margin: 0 0 6px;
  font-size: var(--text-md);
  font-weight: 600;
  color: var(--text);
}

.panel-head .panel-title {
  margin: 0;
}

.sub-title {
  margin: 16px 0 8px;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--text-2);
}

.hint {
  margin: 0 0 12px;
  font-size: var(--text-xs);
  line-height: 1.65;
  color: var(--text-3);
}

/* ---------- 通用按钮 ---------- */
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 30px;
  padding: 0 12px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface-2);
  color: var(--text-2);
  font-size: var(--text-sm);
  white-space: nowrap;
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease),
    border-color var(--dur-fast) var(--ease);
}

.btn:hover:not(:disabled) {
  background: var(--surface-3);
  border-color: var(--line-strong);
  color: var(--text);
}

.btn:active:not(:disabled) {
  background: var(--active);
}

.btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.btn-accent {
  border-color: var(--accent);
  background: var(--accent-weak);
  color: var(--accent);
}

.btn-accent:hover:not(:disabled) {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-text);
}

.btn-accent:active:not(:disabled) {
  background: var(--accent-hover);
  border-color: var(--accent-hover);
  color: var(--accent-text);
}

.btn-danger {
  border-color: var(--line);
  background: var(--surface-2);
  color: var(--danger);
}

.btn-danger:hover:not(:disabled) {
  border-color: var(--danger);
  background: var(--danger-weak);
  color: var(--danger);
}

.btn-danger:active:not(:disabled) {
  background: var(--danger-weak);
}

.btn-danger.is-confirming {
  border-color: var(--danger);
  background: var(--danger-weak);
  color: var(--danger);
  font-weight: 600;
}

.btn-quiet-danger {
  color: var(--danger);
}

.btn-quiet-danger:hover:not(:disabled) {
  border-color: var(--danger);
  background: var(--danger-weak);
  color: var(--danger);
}

/* ---------- 图标按钮 ---------- */
.icon-btn {
  flex: none;
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border-radius: 7px;
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease),
    opacity var(--dur-fast) var(--ease);
}

.icon-btn:hover:not(:disabled) {
  background: var(--hover);
  color: var(--text);
}

.icon-btn:active:not(:disabled) {
  background: var(--active);
}

.icon-btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.icon-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.icon-btn.sm {
  width: 24px;
  height: 24px;
  border-radius: 6px;
}

.flip {
  transform: rotate(180deg);
}

/* ---------- 应用管理列表 ---------- */
.app-list {
  flex: 1 1 auto;
  min-height: 0;
  margin: 0;
  padding: 0;
  border: 1px solid var(--line);
  border-radius: var(--r-card);
}

.app-row {
  display: grid;
  grid-template-columns: 28px minmax(0, 1fr) 100px auto auto auto;
  align-items: center;
  gap: 10px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--line);
  transition: background var(--dur-fast) var(--ease);
}

.app-row:last-child {
  border-bottom: 0;
}

.app-row:hover {
  background: var(--hover);
}

.app-icon {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border-radius: 8px;
  color: var(--accent-text);
  font-size: 11px;
  font-weight: 600;
  user-select: none;
  transition: opacity var(--dur-fast) var(--ease);
}

.app-row.is-hidden .app-icon {
  opacity: 0.5;
}

.app-row.is-hidden .app-name {
  color: var(--text-3);
}

.app-meta {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.app-name {
  font-size: var(--text-sm);
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.app-target {
  font-size: var(--text-xs);
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.vis {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 28px;
  padding: 0 9px;
  border: 1px solid var(--line);
  border-radius: 7px;
  background: var(--surface-2);
  color: var(--text-3);
  font-size: var(--text-xs);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease),
    border-color var(--dur-fast) var(--ease);
}

.vis:hover {
  border-color: var(--line-strong);
  color: var(--text-2);
}

.vis:active {
  background: var(--active);
}

.vis:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

/* 显示中是常态：保持低调，只有「已隐藏」才用强调色提示需要处理 */
.vis.is-off {
  border-color: color-mix(in srgb, var(--danger) 42%, transparent);
  background: var(--danger-weak);
  color: var(--danger);
}

.vis.is-off:hover {
  border-color: var(--danger);
}

.vis-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
}

.moves {
  display: flex;
  gap: 2px;
}

/* 行内操作（编辑 / 移除） */
.row-actions {
  display: flex;
  gap: 2px;
}

.icon-btn.danger:hover:not(:disabled) {
  background: var(--danger-weak);
  color: var(--danger);
}

.icon-btn.danger.is-confirm {
  background: var(--danger-weak);
  color: var(--danger);
  opacity: 1;
}

/* 标题右侧的操作组：三个按钮排一行，窄窗口时换行而不是溢出 */
.head-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.btn-sm {
  height: 26px;
  padding: 0 9px;
  font-size: var(--text-xs);
}

.app-empty {
  padding: 22px 12px;
  text-align: center;
  font-size: var(--text-sm);
  line-height: 1.7;
  color: var(--text-3);
}

/* 真实图标与首字母色块同尺寸，列表不会因为有没有图标而跳动 */
.app-img {
  width: 28px;
  height: 28px;
  object-fit: contain;
  user-select: none;
}

.app-row.is-hidden .app-img {
  opacity: 0.5;
}

.broken-tag {
  margin-left: 6px;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--danger-weak);
  color: var(--danger);
  font-size: 10px;
  line-height: 15px;
  vertical-align: 1px;
}

/* 编辑态：整行变成两行输入 + 操作 */
.app-row.is-editing {
  grid-template-columns: 28px minmax(0, 1fr);
  align-items: start;
  background: var(--surface-2);
}

.app-edit {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
  padding: 2px 0;
}

.input-sm {
  height: 28px;
  font-size: var(--text-xs);
}

.mono {
  font-family: var(--font-mono, ui-monospace, SFMono-Regular, Consolas, monospace);
}

.edit-actions {
  display: flex;
  gap: 6px;
  justify-content: flex-end;
}

/* ---------- 表单控件 ---------- */
.select-wrap {
  position: relative;
  display: flex;
  align-items: center;
  min-width: 0;
}

.control {
  width: 100%;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--line);
  border-radius: 8px;
  background: var(--surface-2);
  color: var(--text);
  font: inherit;
  font-size: var(--text-sm);
  transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.control::placeholder {
  color: var(--text-3);
}

.control:hover:not(:disabled) {
  border-color: var(--line-strong);
}

.control:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
  border-color: var(--accent);
}

.control:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.select {
  appearance: none;
  -webkit-appearance: none;
  padding-right: 28px;
  cursor: pointer;
}

.select:disabled {
  cursor: not-allowed;
}

.select-sm {
  height: 30px;
  padding: 0 24px 0 8px;
  font-size: var(--text-xs);
}

.chev {
  position: absolute;
  right: 9px;
  color: var(--text-3);
  pointer-events: none;
}

/* ---------- 卡片与设置行 ---------- */
.card {
  border: 1px solid var(--line);
  border-radius: var(--r-card);
  background: var(--surface-2);
  overflow: hidden;
}

.set-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  padding: 10px 12px;
  border-bottom: 1px solid var(--line);
}

.card > .set-row:last-child {
  border-bottom: 0;
}

.set-row.is-disabled .set-label {
  color: var(--text-2);
}

.set-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.set-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-sm);
  color: var(--text);
}

.set-desc {
  font-size: var(--text-xs);
  line-height: 1.55;
  color: var(--text-3);
}

/* ---------- 开关 ---------- */
.switch {
  position: relative;
  flex: none;
  width: 36px;
  height: 20px;
  border: 1px solid var(--line);
  border-radius: 10px;
  background: var(--surface-3);
  transition: background var(--dur) var(--ease), border-color var(--dur) var(--ease);
}

.switch:hover:not(:disabled) {
  border-color: var(--line-strong);
}

.switch:active:not(:disabled) {
  background: var(--active);
}

.switch:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}

.switch.is-on {
  border-color: var(--accent);
  background: var(--accent);
}

.switch.is-on:active:not(:disabled) {
  background: var(--accent-hover);
  border-color: var(--accent-hover);
}

.switch:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.switch-knob {
  position: absolute;
  top: 50%;
  left: 2px;
  width: 14px;
  height: 14px;
  margin-top: -7px;
  border-radius: 50%;
  background: var(--surface);
  transition: transform var(--dur) var(--ease), background var(--dur) var(--ease);
}

.switch.is-on .switch-knob {
  transform: translateX(16px);
  background: var(--accent-text);
}

/* ---------- 分组 ---------- */
.group-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 8px 12px;
  border-bottom: 1px solid var(--line);
}

.group-row:last-child {
  border-bottom: 0;
}

.group-name {
  flex: 1 1 auto;
  min-width: 0;
  font-size: var(--text-sm);
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.count {
  flex: none;
  min-width: 20px;
  height: 18px;
  padding: 0 6px;
  display: grid;
  place-items: center;
  border-radius: 9px;
  background: var(--surface-3);
  color: var(--text-3);
  font-size: var(--text-xs);
  font-variant-numeric: tabular-nums;
}

/* ---------- 标签 ---------- */
.tag {
  flex: none;
  display: inline-flex;
  align-items: center;
  height: 18px;
  padding: 0 6px;
  border: 1px solid var(--line);
  border-radius: 5px;
  background: var(--surface-3);
  color: var(--text-3);
  font-size: var(--text-xs);
  font-weight: 400;
  line-height: 1;
}

.tag-ok {
  color: var(--success);
}

.tag-muted {
  color: var(--text-3);
}

.tag-warn {
  border-color: var(--line-strong);
  color: var(--text-2);
}

/* ---------- 危险操作 ---------- */
.danger-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-4);
  margin-top: 12px;
  padding: 10px 12px;
  border: 1px solid var(--line);
  border-radius: var(--r-card);
  background: var(--surface-2);
}

/* ---------- 翻译表单 ---------- */
.field-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px 12px;
  margin-bottom: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 5px;
  min-width: 0;
}

.field-wide {
  grid-column: 1 / -1;
}

.field-label {
  font-size: var(--text-xs);
  color: var(--text-2);
}

.secret-actions {
  display: flex;
  gap: var(--space-2);
  padding: 10px 12px;
}

.secret-edit {
  display: flex;
  gap: var(--space-2);
  padding: 0 12px 12px;
}

.secret-edit .control {
  flex: 1 1 auto;
  min-width: 0;
}

/* ---------- 通用 ---------- */
.seg {
  flex: none;
  display: inline-flex;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--line);
  border-radius: 9px;
  background: var(--surface-3);
}

.seg-item {
  display: inline-flex;
  align-items: center;
  height: 26px;
  padding: 0 10px;
  border-radius: 7px;
  font-size: var(--text-xs);
  color: var(--text-2);
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.seg-item:hover {
  background: var(--hover);
  color: var(--text);
}

.seg-item:active {
  background: var(--active);
}

.seg-item.is-on {
  background: var(--surface);
  color: var(--text);
  box-shadow: inset 0 0 0 1px var(--line-strong);
}

.seg-item:has(input:focus-visible) {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.kbd {
  flex: none;
  display: inline-flex;
  align-items: center;
  height: 26px;
  padding: 0 9px;
  border: 1px solid var(--line);
  border-radius: 7px;
  background: var(--surface-3);
  color: var(--text-2);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  letter-spacing: 0.3px;
  white-space: nowrap;
}

.about {
  margin: 14px 0 0;
  font-size: var(--text-xs);
  color: var(--text-3);
}
</style>

<style scoped>
.category-targets { display: flex; flex-wrap: wrap; gap: 6px; flex: none; margin-bottom: 10px; }
.app-workspace { flex: 1; min-height: 0; }
.manage-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(72px, 1fr)); gap: 8px; padding: 4px; }
.manage-tile { display: flex; flex-direction: column; align-items: center; gap: 5px; padding: 8px 4px; border-radius: 10px; border: 1px solid transparent; cursor: grab; }
.manage-tile img { width: 32px; height: 32px; object-fit: contain; }
.manage-tile.selected { background: var(--accent-weak); border-color: var(--accent); }
.manage-tile:hover { background: var(--hover); }
.manage-tile.is-hidden { opacity: .55; }
.manage-name { max-width: 100%; font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.manage-tile small { font-size: 10px; color: var(--text-3); }
.drop-target { outline: 2px solid var(--accent); outline-offset: -2px; }
.app-workspace .app-list { overflow: visible; margin-top: 12px; }
</style>
