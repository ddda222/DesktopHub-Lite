<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import GsapPresence from '@/components/GsapPresence.vue'
import { animateFlipFromState, captureFlipSnapshot, Flip, gsap, playFlipSnapshot, vGsapPress } from '@/lib/gsapMotion'
import { activeMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import Icon from '@/components/Icon.vue'
import ThemedSelect from '@/components/ThemedSelect.vue'
import { clipSummary, relativeTime } from '@/lib/format'
import type { ClipItem, SnippetCopyField, SnippetDraft, SnippetItem, SnippetKind } from '@/lib/desktop/types'
import { formatJson } from './format-json'
import { useErrorExplanation } from './error-explanation'
import {
  activeClip,
  assignClipGroup,
  closeDetail,
  closePanel,
  copyClip,
  copySnippet,
  deleteClip,
  deleteSelectedClips,
  deleteSnippet,
  editClip,
  ensureClipThumbnails,
  filteredClips,
  filteredSnippets,
  groupFilters,
  hasSnippetVariables,
  moveActive,
  moveSnippetActive,
  openDetail,
  openSettings,
  openTranslation,
  pushToast,
  renderSnippetPreview,
  reorderVisibleSnippets,
  revealSnippetPassword,
  runClipboardOcr,
  saveClipAsSnippet,
  saveSnippet,
  setClipboardView,
  setGroup,
  setPaused,
  setQuery,
  setSnippetGroup,
  setSnippetQuery,
  snippetGroupFilters,
  state
} from '@/store'

const searchEl = ref<HTMLInputElement | null>(null)
const listEl = ref<HTMLElement | null>(null)
const snippetListEl = ref<HTMLElement | null>(null)
const snippetTitleEl = ref<HTMLInputElement | null>(null)
const snippetAccountEl = ref<HTMLInputElement | null>(null)
const snippetBodyEl = ref<HTMLTextAreaElement | null>(null)

const isHistoryView = computed(() => state.ui.clipboardView === 'history')
const modeBarEl = ref<HTMLElement | null>(null)
watch(
  isHistoryView,
  () => {
    const prev = modeBarEl.value?.querySelector<HTMLElement>('.mode-seg-pill')
    if (!prev || state.ui.islandReducedMotion) return
    const flipState = Flip.getState(prev)
    void nextTick(() => {
      const next = modeBarEl.value?.querySelector<HTMLElement>('.mode-seg-pill')
      if (next) animateFlipFromState(flipState, next, activeMotionPreset.value.pill, { absolute: true })
    })
  },
  { flush: 'pre' }
)
const CLIP_PAGE_SIZE = 20
const clipPage = ref(1)
const totalClipPages = computed(() => Math.max(1, Math.ceil(filteredClips.value.length / CLIP_PAGE_SIZE)))
const pageOffset = computed(() => (clipPage.value - 1) * CLIP_PAGE_SIZE)
const pagedClips = computed(() =>
  filteredClips.value.slice(pageOffset.value, pageOffset.value + CLIP_PAGE_SIZE)
)
const visiblePageNumbers = computed<number[]>(() => {
  const total = totalClipPages.value
  const current = clipPage.value
  if (total <= 5) return Array.from({ length: total }, (_, idx) => idx + 1)
  const start = Math.max(1, Math.min(current - 2, total - 4))
  return Array.from({ length: 5 }, (_, idx) => start + idx)
})

function setClipPage(nextPage: number): void {
  const target = Math.max(1, Math.min(totalClipPages.value, nextPage))
  if (target === clipPage.value) return
  clipPage.value = target
  const minIdx = (target - 1) * CLIP_PAGE_SIZE
  const maxIdx = Math.min(filteredClips.value.length - 1, minIdx + CLIP_PAGE_SIZE - 1)
  if (filteredClips.value.length > 0 && (state.ui.activeIndex < minIdx || state.ui.activeIndex > maxIdx)) {
    state.ui.activeIndex = minIdx
  }
  void nextTick(() => {
    listEl.value?.scrollTo({ top: 0 })
  })
}

watch(
  [() => state.ui.query, () => state.ui.group, () => state.ui.favoritesOnly],
  () => {
    clipPage.value = 1
    void nextTick(() => listEl.value?.scrollTo({ top: 0 }))
  }
)

watch(totalClipPages, total => {
  if (clipPage.value > total) clipPage.value = total
})

watch(
  () => state.ui.activeIndex,
  idx => {
    if (!isHistoryView.value || !filteredClips.value.length) return
    const targetPage = Math.floor(Math.max(0, idx) / CLIP_PAGE_SIZE) + 1
    if (targetPage >= 1 && targetPage <= totalClipPages.value && targetPage !== clipPage.value) {
      clipPage.value = targetPage
    }
  }
)

watch(
  [pagedClips, isHistoryView, () => state.ui.panel],
  ([items, historyView, panel]) => {
    if (!isDesktop || !historyView || (panel && panel !== 'clipboard')) return
    void ensureClipThumbnails(items)
  },
  { immediate: true }
)

const clipGroupOptions = computed(() =>
  state.groups.map(group => ({ value: group, label: group }))
)
const snippetGroupOptions = computed(() =>
  state.snippetGroups.map(group => ({ value: group, label: group }))
)

let lastFlippedPage = clipPage.value
watch(
  () => pagedClips.value.map(item => item.id).join('|'),
  () => {
    // 翻页时直接切页避免跨页全量 DOM 测量闪烁，同页内部删减记录时保留平滑补位。
    if (lastFlippedPage !== clipPage.value) {
      lastFlippedPage = clipPage.value
      return
    }
    const snap = captureFlipSnapshot(listEl.value, '.row', state.ui.islandReducedMotion, 24)
    if (!snap) return
    void nextTick(() => playFlipSnapshot(snap, listEl.value, '.row', activeMotionPreset.value.pill, { stagger: 0.01, maxItems: 24 }))
  },
  { flush: 'pre' }
)

watch(
  () => filteredSnippets.value.map(item => `${item.id}:${item.pinned ? 1 : 0}`).join('|'),
  () => {
    const snap = captureFlipSnapshot(snippetListEl.value, '.snippet-row', state.ui.islandReducedMotion, 36)
    if (!snap) return
    void nextTick(() => playFlipSnapshot(snap, snippetListEl.value, '.snippet-row', activeMotionPreset.value.pill, { stagger: 0.012, maxItems: 36 }))
  },
  { flush: 'pre' }
)

function pulseSnippetPin(snippet: SnippetItem, event: MouseEvent): void {
  if (!snippet.pinned && !state.ui.islandReducedMotion) {
    const icon = (event.currentTarget as HTMLElement | null)?.querySelector<HTMLElement>('.pin-icon-wrap')
    if (icon) gsap.fromTo(icon, { scale: 1 }, { scale: 1.26, duration: 0.14, yoyo: true, repeat: 1, ease: 'power2.out', clearProps: 'transform' })
  }
  void onTogglePin(snippet)
}
const detailClip = computed(() => (isHistoryView.value ? activeClip.value : null))
const { explanation, start: explainError, submit: submitExplanation, close: closeExplanation } = useErrorExplanation()
const emit = defineEmits<{ 'reading-explanation': [active: boolean] }>()
watch(() => !!explanation.value, active => emit('reading-explanation', active), { flush: 'sync' })
const explanationHeading = ref<HTMLElement | null>(null)
const explanationButton = ref<HTMLButtonElement | null>(null)
const followUpHeading = ref<HTMLElement | null>(null)

function explainClip(clip: ClipItem) {
  if (!isDesktop || clip.kind !== 'text' || !clip.text?.trim()) return
  openDetail(clip.id)
  void explainError(clip.id)
  void nextTick(() => explanationHeading.value?.focus())
}
function returnToOriginal() {
  closeExplanation()
  void nextTick(() => explanationButton.value?.focus())
}
watch(() => explanation.value?.answer, answer => {
  if (answer) void nextTick(() => followUpHeading.value?.focus())
})
watch(() => detailClip.value?.id, id => {
  if (id !== explanation.value?.id) closeExplanation()
}, { flush: 'sync' })
onBeforeUnmount(() => { closeExplanation(); emit('reading-explanation', false) })

/* ---------- 历史记录详情与批量操作 ---------- */

const formattedText = ref<string | null>(null)
const formattedView = ref(false)
const formatError = ref('')
const showLineNumbers = ref(true)
const wrapLines = ref(true)
const previewText = computed(() =>
  formattedView.value && formattedText.value !== null
    ? formattedText.value
    : detailClip.value?.text ?? ''
)
const previewLines = computed(() => previewText.value.split(/\r\n|\r|\n/))

function showOriginal(): void {
  formattedView.value = false
  formatError.value = ''
}

function showFormattedJson(): void {
  if (detailClip.value?.kind !== 'text') return
  try {
    formattedText.value = formatJson(detailClip.value.text ?? '')
    formattedView.value = true
    formatError.value = ''
  } catch {
    formattedView.value = false
    formattedText.value = null
    formatError.value = '这条内容不是有效的 JSON，已保留原文。'
  }
}

async function previewImage(id: string): Promise<void> {
  if (!isDesktop) {
    pushToast('请在桌面版预览和钉图', 'info')
    return
  }
  try {
    await invokeHost('open_image_preview', { source: 'clipboard', id })
  } catch (error) {
    pushToast(String(error), 'error')
  }
}

function onCopyFormatted(): void {
  if (detailClip.value && formattedText.value !== null) {
    void copyClip(detailClip.value, formattedText.value)
  }
}

watch([() => detailClip.value?.id, () => detailClip.value?.text], () => {
  formattedText.value = null
  showOriginal()
})

const noteDraft = ref('')
const savingNote = ref(false)
const selecting = ref(false)
const selectedIds = ref(new Set<string>())
const deletePrompt = ref<{ ids: string[]; favorites: number } | null>(null)
const deleting = ref(false)
const savingClipSnippetId = ref<string | null>(null)

function toggleSelected(id: string): void {
  deletePrompt.value = null
  const next = new Set(selectedIds.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  selectedIds.value = next
}

function stopSelecting(): void {
  selecting.value = false
  selectedIds.value = new Set()
  deletePrompt.value = null
}

function selectCurrentPage(): void {
  selectedIds.value = new Set([...selectedIds.value, ...pagedClips.value.map((clip) => clip.id)])
  deletePrompt.value = null
}

function selectVisible(): void {
  selectedIds.value = new Set([...selectedIds.value, ...filteredClips.value.map((clip) => clip.id)])
  deletePrompt.value = null
}

function prepareDeletion(ids: string[]): void {
  const wanted = new Set(ids)
  const captured = state.clips.filter((clip) => wanted.has(clip.id))
  if (!captured.length) return
  deletePrompt.value = {
    ids: captured.map((clip) => clip.id),
    favorites: captured.filter((clip) => clip.favorite).length
  }
}

async function confirmDeletion(): Promise<void> {
  const pending = deletePrompt.value
  if (!pending || deleting.value) return
  deleting.value = true
  try {
    if (await deleteSelectedClips(pending.ids, pending.favorites)) {
      if (state.ui.detailId && pending.ids.includes(state.ui.detailId)) closeDetail()
      stopSelecting()
    } else deletePrompt.value = null
  } finally {
    deleting.value = false
  }
}

async function saveNote(clip: ClipItem, favorite = clip.favorite ?? false, quiet = false): Promise<void> {
  if (savingNote.value) return
  savingNote.value = true
  try {
    await editClip(clip, favorite, noteDraft.value)
    if (quiet) pushToast('已自动保存备注', 'info')
  } finally {
    savingNote.value = false
  }
}

async function autoSaveNoteIfDirty(clip: ClipItem | null): Promise<void> {
  if (!clip || savingNote.value) return
  if (noteDraft.value.trim() === (clip.note ?? '').trim()) return
  await saveNote(clip, clip.favorite ?? false, true)
}

async function leaveDetail(): Promise<void> {
  await autoSaveNoteIfDirty(detailClip.value)
  closeDetail()
}

async function onSaveAsSnippet(clip: ClipItem | null): Promise<void> {
  if (!clip || clip.kind !== 'text' || savingClipSnippetId.value) return
  savingClipSnippetId.value = clip.id
  try {
    await saveClipAsSnippet(clip)
  } finally {
    savingClipSnippetId.value = null
  }
}

watch(
  () => detailClip.value?.id,
  () => {
    noteDraft.value = detailClip.value?.note ?? ''
  },
  { immediate: true }
)

/** 文件缺失或没有可用地址时都不渲染 <img>，避免出现破图 */
function isImageBroken(clip: ClipItem | null): boolean {
  if (!clip || clip.kind !== 'image') return false
  return clip.imageMissing === true || !clip.imageUrl
}

function onInput(event: Event): void {
  const target = event.target as HTMLInputElement | null
  if (!target) return
  if (isHistoryView.value) setQuery(target.value)
  else setSnippetQuery(target.value)
}

function onClear(): void {
  if (isHistoryView.value) setQuery('')
  else setSnippetQuery('')
  searchEl.value?.focus()
}

function onTogglePause(): void {
  void setPaused(!state.ui.paused)
}

function setActive(index: number): void {
  state.ui.activeIndex = index
}

function clampActive(): void {
  const total = filteredClips.value.length
  if (state.ui.activeIndex > total - 1) {
    state.ui.activeIndex = Math.max(0, total - 1)
  }
}

function onCopy(clip: ClipItem | null): void {
  if (!clip) return
  void copyClip(clip)
  // 行不可聚焦，点击后把焦点交回搜索框，键盘操作才能继续
  searchEl.value?.focus()
}

function onDelete(clip: ClipItem | null): void {
  if (!clip) return
  if (clip.favorite) {
    prepareDeletion([clip.id])
    return
  }
  void deleteClip(clip).then(clampActive)
}

function onTranslate(clip: ClipItem | null): void {
  if (!clip || clip.kind !== 'text') return
  openTranslation(clip.text ?? '', true)
}

/* ---------- 常用短语列表、动态变量与内联编辑 ---------- */

const variablePresets = [
  { token: '{{date}}', label: '日期', title: '插入当前日期（YYYY-MM-DD）' },
  { token: '{{time}}', label: '时间', title: '插入当前时间（HH:MM:SS）' },
  { token: '{{datetime}}', label: '完整时间', title: '插入日期与时间' },
  { token: '{{weekday}}', label: '星期', title: '插入星期几（如 周三）' },
  { token: '{{clipboard}}', label: '剪贴板', title: '复制时嵌入当前系统剪贴板文本' }
] as const

const editingSnippet = ref<SnippetDraft | null>(null)
const customGroupMode = ref(false)
const customGroupName = ref('')
const lastTextGroup = ref('未分组')
const showDraftPassword = ref(false)
const loadingDraftPassword = ref(false)
const draftPasswordDirty = ref(false)
const draftPasswordError = ref('')
const copiedBadgeKey = ref<string | null>(null)
let copiedBadgeTimer: number | null = null
let revealPasswordSeq = 0
const savingSnippet = ref(false)
const snippetDeletePrompt = ref<SnippetItem | null>(null)
const deletingSnippet = ref(false)
const previewNow = ref(new Date())
let previewTickTimer: number | null = null

function clearCredentialEditorSecrets(): void {
  revealPasswordSeq++
  showDraftPassword.value = false
  loadingDraftPassword.value = false
  draftPasswordDirty.value = false
  draftPasswordError.value = ''
}

function clearCopiedBadgeTimer(): void {
  if (copiedBadgeTimer !== null) {
    window.clearTimeout(copiedBadgeTimer)
    copiedBadgeTimer = null
  }
}

function markCopiedBadge(snippetId: string, field: SnippetCopyField): void {
  clearCopiedBadgeTimer()
  copiedBadgeKey.value = `${snippetId}:${field}`
  copiedBadgeTimer = window.setTimeout(() => {
    copiedBadgeKey.value = null
    copiedBadgeTimer = null
  }, 1400)
}

function startPreviewClock(): void {
  stopPreviewClock()
  previewNow.value = new Date()
  previewTickTimer = window.setInterval(() => {
    previewNow.value = new Date()
  }, 1000)
}

function stopPreviewClock(): void {
  if (previewTickTimer !== null) {
    window.clearInterval(previewTickTimer)
    previewTickTimer = null
  }
}

onBeforeUnmount(() => {
  stopPreviewClock()
  clearCopiedBadgeTimer()
  clearCredentialEditorSecrets()
})

const editingOriginal = computed(() =>
  editingSnippet.value?.id
    ? state.snippets.find((item) => item.id === editingSnippet.value?.id) ?? null
    : null
)

const isEditingCredential = computed(() => {
  const draft = editingSnippet.value
  if (!draft) return false
  const effectiveGroup = customGroupMode.value ? customGroupName.value.trim() : draft.group
  return draft.kind === 'credential' || effectiveGroup === '账密'
})

const draftHasVariables = computed(() =>
  editingSnippet.value && !isEditingCredential.value
    ? hasSnippetVariables(editingSnippet.value.content)
    : false
)

const draftPreviewText = computed(() => {
  if (!editingSnippet.value || !draftHasVariables.value) return ''
  const rendered = renderSnippetPreview(
    editingSnippet.value.content,
    '「当前剪贴板文本」',
    previewNow.value
  )
  const chars = [...rendered]
  return chars.length > 180 ? `${chars.slice(0, 180).join('')}…` : rendered
})

const canSubmitSnippet = computed(() => {
  const draft = editingSnippet.value
  if (!draft || savingSnippet.value || !draft.title.trim()) return false
  if (isEditingCredential.value) {
    const accountOk = Boolean((draft.account ?? '').trim())
    const needPassword = !editingOriginal.value?.hasPassword || draftPasswordDirty.value
    const passwordOk = !needPassword || Boolean(draft.password)
    return accountOk && passwordOk
  }
  return Boolean(draft.content.trim())
})

function snippetSummary(content: string): string {
  const flat = content.replace(/\s+/g, ' ').trim()
  if (!flat) return '（空内容）'
  const chars = [...flat]
  return chars.length > 56 ? `${chars.slice(0, 56).join('')}…` : flat
}

function switchView(view: 'history' | 'snippets'): void {
  if (state.ui.clipboardView === view) return
  stopSelecting()
  snippetDeletePrompt.value = null
  editingSnippet.value = null
  clearCredentialEditorSecrets()
  stopPreviewClock()
  setClipboardView(view)
  void nextTick(() => searchEl.value?.focus())
}

function switchDraftKind(nextKind: SnippetKind): void {
  const draft = editingSnippet.value
  if (!draft) return
  if (nextKind === 'credential') {
    if (draft.group && draft.group !== '账密') {
      lastTextGroup.value = draft.group
    }
    customGroupMode.value = false
    customGroupName.value = ''
    draft.kind = 'credential'
    draft.group = '账密'
    if (draft.account === undefined) draft.account = ''
    if (draft.password === undefined) draft.password = ''
  } else {
    draft.kind = 'text'
    if (draft.group === '账密') {
      const fallback = state.snippetGroups.find((g) => g !== '账密') ?? '未分组'
      draft.group =
        lastTextGroup.value && lastTextGroup.value !== '账密' ? lastTextGroup.value : fallback
    }
    showDraftPassword.value = false
  }
}

watch(
  () => editingSnippet.value?.group,
  (group) => {
    if (editingSnippet.value && group === '账密' && editingSnippet.value.kind !== 'credential') {
      switchDraftKind('credential')
    }
  }
)

function openCreateSnippet(): void {
  snippetDeletePrompt.value = null
  customGroupMode.value = false
  customGroupName.value = ''
  clearCredentialEditorSecrets()
  const activeGroup = state.ui.snippetGroup
  const startCredential = activeGroup === '账密'
  const fallbackTextGroup = state.snippetGroups.find((g) => g !== '账密') ?? '未分组'
  const defaultGroup =
    activeGroup !== '全部' && state.snippetGroups.includes(activeGroup)
      ? activeGroup
      : fallbackTextGroup
  lastTextGroup.value = defaultGroup !== '账密' ? defaultGroup : fallbackTextGroup
  editingSnippet.value = {
    kind: startCredential ? 'credential' : 'text',
    title: '',
    content: '',
    account: '',
    password: '',
    group: defaultGroup,
    pinned: false
  }
  startPreviewClock()
  void nextTick(() => snippetTitleEl.value?.focus())
}

function openEditSnippet(snippet: SnippetItem): void {
  snippetDeletePrompt.value = null
  customGroupMode.value = false
  customGroupName.value = ''
  clearCredentialEditorSecrets()
  const isCred = snippet.kind === 'credential'
  lastTextGroup.value = snippet.group !== '账密' ? snippet.group : '未分组'
  editingSnippet.value = {
    id: snippet.id,
    kind: isCred ? 'credential' : 'text',
    title: snippet.title,
    content: snippet.content,
    account: snippet.account ?? '',
    password: '',
    group: isCred ? '账密' : snippet.group,
    pinned: snippet.pinned
  }
  if (isCred && snippet.hasPassword) {
    const ticket = ++revealPasswordSeq
    loadingDraftPassword.value = true
    void revealSnippetPassword(snippet.id)
      .then((secret) => {
        if (ticket !== revealPasswordSeq || !editingSnippet.value) return
        if (!draftPasswordDirty.value) {
          editingSnippet.value.password = secret
        }
      })
      .catch((error) => {
        if (ticket !== revealPasswordSeq) return
        draftPasswordError.value =
          error instanceof Error ? error.message : '读取已保存密码失败，留空将保留原密码'
      })
      .finally(() => {
        if (ticket === revealPasswordSeq) loadingDraftPassword.value = false
      })
  }
  startPreviewClock()
  void nextTick(() => snippetTitleEl.value?.focus())
}

function cancelEditSnippet(): void {
  editingSnippet.value = null
  customGroupMode.value = false
  customGroupName.value = ''
  clearCredentialEditorSecrets()
  stopPreviewClock()
  void nextTick(() => searchEl.value?.focus())
}

function insertVariable(token: string): void {
  const draft = editingSnippet.value
  if (!draft) return
  const area = snippetBodyEl.value
  if (!area) {
    draft.content += token
    return
  }
  const start = area.selectionStart ?? draft.content.length
  const end = area.selectionEnd ?? start
  draft.content = draft.content.slice(0, start) + token + draft.content.slice(end)
  const cursor = start + token.length
  void nextTick(() => {
    area.focus()
    area.setSelectionRange(cursor, cursor)
  })
}

async function submitSnippet(): Promise<void> {
  const draft = editingSnippet.value
  if (!draft || savingSnippet.value) return
  const rawGroup = customGroupMode.value ? customGroupName.value.trim() : draft.group
  if (customGroupMode.value && !rawGroup) {
    pushToast('请填写新分组名称，或切回已有分组', 'error')
    return
  }
  const credentialMode = draft.kind === 'credential' || rawGroup === '账密'
  const shouldKeepExistingPassword =
    credentialMode && Boolean(editingOriginal.value?.hasPassword) && !draftPasswordDirty.value
  const payload: SnippetDraft = credentialMode
    ? {
        id: draft.id,
        kind: 'credential',
        title: draft.title,
        content: draft.content,
        account: draft.account ?? '',
        password: shouldKeepExistingPassword ? undefined : (draft.password ?? ''),
        group: '账密',
        pinned: draft.pinned
      }
    : {
        id: draft.id,
        kind: 'text',
        title: draft.title,
        content: draft.content,
        group: rawGroup || '未分组',
        pinned: draft.pinned
      }
  savingSnippet.value = true
  try {
    if (await saveSnippet(payload)) {
      cancelEditSnippet()
      clampSnippetActive()
    }
  } finally {
    savingSnippet.value = false
  }
}

function clampSnippetActive(): void {
  const total = filteredSnippets.value.length
  if (state.ui.snippetActiveIndex > total - 1) {
    state.ui.snippetActiveIndex = Math.max(0, total - 1)
  }
}

async function onCopySnippet(
  snippet: SnippetItem | null,
  raw = false,
  field?: SnippetCopyField
): Promise<void> {
  if (!snippet) return
  const targetField: SnippetCopyField | undefined =
    snippet.kind === 'credential' ? (field ?? 'account') : undefined
  const ok = await copySnippet(snippet, raw, targetField)
  if (ok && targetField) {
    markCopiedBadge(snippet.id, targetField)
  }
  if (!editingSnippet.value) searchEl.value?.focus()
}

async function onTogglePin(snippet: SnippetItem): Promise<void> {
  await saveSnippet({
    id: snippet.id,
    kind: snippet.kind ?? 'text',
    title: snippet.title,
    content: snippet.content,
    account: snippet.account,
    password: undefined,
    group: snippet.group,
    pinned: !snippet.pinned
  })
  clampSnippetActive()
}

function onRequestDeleteSnippet(snippet: SnippetItem): void {
  if (snippet.pinned) {
    snippetDeletePrompt.value = snippet
    return
  }
  void performDeleteSnippet(snippet.id)
}

async function performDeleteSnippet(id: string): Promise<void> {
  if (deletingSnippet.value) return
  deletingSnippet.value = true
  try {
    if (await deleteSnippet(id)) {
      if (editingSnippet.value?.id === id) cancelEditSnippet()
      snippetDeletePrompt.value = null
      clampSnippetActive()
    }
  } finally {
    deletingSnippet.value = false
  }
}

function samePartitionList(snippet: SnippetItem): SnippetItem[] {
  const source = editingSnippet.value ? state.snippets : filteredSnippets.value
  return source.filter((item) => item.pinned === snippet.pinned)
}

function canMoveSnippet(snippet: SnippetItem | null, delta: -1 | 1): boolean {
  if (!snippet) return false
  const partition = samePartitionList(snippet)
  const index = partition.findIndex((item) => item.id === snippet.id)
  return index >= 0 && index + delta >= 0 && index + delta < partition.length
}

async function moveSnippetOrder(snippet: SnippetItem | null, delta: -1 | 1): Promise<void> {
  if (!snippet) return
  const partition = samePartitionList(snippet)
  const index = partition.findIndex((item) => item.id === snippet.id)
  const target = partition[index + delta]
  if (!target) return
  const visibleIds = (editingSnippet.value ? state.snippets : filteredSnippets.value).map(
    (item) => item.id
  )
  if (await reorderVisibleSnippets(visibleIds, snippet.id, target.id)) {
    if (!editingSnippet.value) {
      const nextIdx = filteredSnippets.value.findIndex((item) => item.id === snippet.id)
      if (nextIdx >= 0) state.ui.snippetActiveIndex = nextIdx
      await nextTick()
      snippetListEl.value
        ?.querySelector<HTMLElement>(`[data-index="${state.ui.snippetActiveIndex}"]`)
        ?.scrollIntoView({ block: 'nearest' })
    }
  }
}

/* ---------- 键盘导航 ---------- */

async function moveAndReveal(delta: number): Promise<void> {
  if (isHistoryView.value) {
    moveActive(delta)
    await nextTick()
    const row = listEl.value?.querySelector<HTMLElement>(`[data-index="${state.ui.activeIndex}"]`)
    row?.scrollIntoView({ block: 'nearest' })
  } else {
    moveSnippetActive(delta)
    await nextTick()
    const row = snippetListEl.value?.querySelector<HTMLElement>(
      `[data-index="${state.ui.snippetActiveIndex}"]`
    )
    row?.scrollIntoView({ block: 'nearest' })
  }
}

function onKeydown(event: KeyboardEvent): void {
  // 中文输入法候选确认期间（isComposing / 229）不能把 Enter 当成复制
  if (event.isComposing || event.keyCode === 229) return

  if (event.key === 'Escape') {
    event.stopPropagation()
    if (explanation.value) { returnToOriginal(); return }
    if (deletePrompt.value) {
      deletePrompt.value = null
      return
    }
    if (snippetDeletePrompt.value) {
      snippetDeletePrompt.value = null
      return
    }
    if (editingSnippet.value) {
      cancelEditSnippet()
      return
    }
    if (selecting.value) {
      stopSelecting()
      return
    }
    if (detailClip.value) {
      void leaveDetail()
      return
    }
    closePanel()
    return
  }

  // 详情视图或短语编辑视图内由表单自身处理按键
  if (state.ui.detailId || editingSnippet.value || selecting.value) return

  if (!isHistoryView.value && event.altKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
    const current = filteredSnippets.value[state.ui.snippetActiveIndex] ?? null
    if (current) {
      event.preventDefault()
      void moveSnippetOrder(current, event.key === 'ArrowUp' ? -1 : 1)
    }
    return
  }

  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    void moveAndReveal(event.key === 'ArrowDown' ? 1 : -1)
    return
  }

  if (isHistoryView.value && (event.key === 'PageDown' || event.key === 'PageUp')) {
    if (totalClipPages.value > 1) {
      event.preventDefault()
      setClipPage(clipPage.value + (event.key === 'PageDown' ? 1 : -1))
    }
    return
  }

  // Alt+1~9 序号快拷（历史记录按当前页内序号快拷，常用短语同前）
  if (event.altKey && !event.ctrlKey && !event.metaKey && event.key >= '1' && event.key <= '9') {
    const idx = Number(event.key) - 1
    if (isHistoryView.value) {
      const clip = pagedClips.value[idx]
      if (clip) {
        event.preventDefault()
        event.stopPropagation()
        onCopy(clip)
      }
    } else {
      const snippet = filteredSnippets.value[idx]
      if (snippet) {
        event.preventDefault()
        event.stopPropagation()
        void onCopySnippet(snippet)
      }
    }
    return
  }

  if (event.key === 'Enter') {
    const target = event.target as HTMLElement | null
    if (target?.closest('button')) return
    if (isHistoryView.value) {
      const clip = filteredClips.value[state.ui.activeIndex]
      if (!clip) return
      event.preventDefault()
      onCopy(clip)
    } else {
      const snippet = filteredSnippets.value[state.ui.snippetActiveIndex]
      if (!snippet) return
      event.preventDefault()
      if (snippet.kind === 'credential' && event.shiftKey) {
        void onCopySnippet(snippet, false, 'password')
      } else {
        void onCopySnippet(snippet)
      }
    }
  }
}

onMounted(() => {
  requestAnimationFrame(() => {
    searchEl.value?.focus({ preventScroll: true })
  })
})

// 从详情返回列表后焦点回到搜索框
watch(
  () => state.ui.detailId,
  (id) => {
    if (id) return
    void nextTick(() => {
      searchEl.value?.focus()
    })
  }
)

const activeSnippet = computed(() =>
  !isHistoryView.value ? filteredSnippets.value[state.ui.snippetActiveIndex] ?? null : null
)

const emptySnippetHint = computed(() => {
  if (state.snippets.length) {
    return state.ui.snippetGroup === '账密'
      ? '还没有账密记录，点击右上角「新建」即可分别保存账号和密码。'
      : '换个关键词，或切到「全部」分组再试。'
  }
  return '把常用回复、命令、账密或带 {{date}} / {{clipboard}} 的模板钉在这里，单击即可复制。'
})

const footerHint = computed(() => {
  if (explanation.value) return 'Esc 返回原文'
  if (detailClip.value?.kind === 'text') return '格式化仅改变预览，原文始终保留'
  if (detailClip.value) return 'Esc 返回列表'
  if (editingSnippet.value) {
    return isEditingCredential.value
      ? '密码在本机加密保存，复制不会写入剪贴板历史 · Esc 取消编辑'
      : '支持 {{date}} / {{time}} / {{clipboard}} · Esc 取消编辑'
  }
  if (!isHistoryView.value) {
    return activeSnippet.value?.kind === 'credential'
      ? 'Enter 复制账号 · Shift+Enter 复制密码 · Alt+↑↓ 排序 · Esc 关闭'
      : '方向键选择 · Enter/Alt+1~9 复制 · Alt+↑↓ 排序 · Esc 关闭'
  }
  return '方向键选择 · Enter/Alt+1~9 复制 · Esc 关闭'
})

/** 智能识别剪贴板内容类型：颜色值、网址、JSON 或代码片段，提供直觉微标签与快捷动作。 */
interface ClipBadgeInfo {
  type: 'color' | 'url' | 'json' | 'code'
  label: string
  color?: string
  url?: string
}

const COLOR_HEX_RE = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/
const COLOR_RGB_RE = /^rgba?\(\s*\d{1,3}\s*,\s*\d{1,3}\s*,\s*\d{1,3}(?:\s*,\s*(?:0|1|0?\.\d+))?\s*\)$/i
const URL_RE = /^https?:\/\/[^\s<>"{}|\\^`[\]]+$/i
const clipKindCache = new WeakMap<ClipItem, { text: string | undefined; info: ClipBadgeInfo | null }>()

function inspectClipKind(clip: ClipItem): ClipBadgeInfo | null {
  if (clip.kind !== 'text' || !clip.text) return null
  const cached = clipKindCache.get(clip)
  if (cached && cached.text === clip.text) return cached.info
  const trimmed = clip.text.trim()
  let info: ClipBadgeInfo | null = null
  if (trimmed) {
    if (COLOR_HEX_RE.test(trimmed) || COLOR_RGB_RE.test(trimmed)) {
      info = { type: 'color', label: '色值', color: trimmed }
    } else if (!trimmed.includes('\n') && URL_RE.test(trimmed)) {
      info = { type: 'url', label: '链接', url: trimmed }
    } else if ((trimmed.startsWith('{') && trimmed.endsWith('}')) || (trimmed.startsWith('[') && trimmed.endsWith(']'))) {
      try {
        JSON.parse(trimmed)
        info = { type: 'json', label: 'JSON' }
      } catch { /* 非完整 JSON */ }
    } else if (/^(?:SELECT|INSERT|UPDATE|DELETE|export\s+|import\s+|function\s+|const\s+|class\s+)/i.test(trimmed)) {
      info = { type: 'code', label: '代码' }
    }
  }
  clipKindCache.set(clip, { text: clip.text, info })
  return info
}

function openDetectedUrl(url: string): void {
  if (!URL_RE.test(url)) return
  if (isDesktop) {
    void invokeHost('open_external_url', { url }).catch((error) => {
      pushToast(String(error), 'error')
    })
    return
  }
  window.open(url, '_blank', 'noopener,noreferrer')
}
</script>

<template>
  <div
    class="pop"
    role="dialog"
    :aria-label="isHistoryView ? '剪贴板历史' : '常用短语'"
    @keydown="onKeydown"
  >
    <!-- 顶部双模式切换栏（详情与短语编辑态隐藏） -->
    <div v-if="!detailClip && !editingSnippet" ref="modeBarEl" class="mode-bar">
      <div class="seg mode-seg" role="tablist" aria-label="剪贴板功能切换">
        <button
          class="seg-btn"
          type="button"
          role="tab"
          :aria-selected="isHistoryView"
          :class="{ 'is-active': isHistoryView }"
          @click="switchView('history')"
        >
          <span
            v-if="isHistoryView"
            data-flip-id="clip-mode-pill"
            class="mode-seg-pill"
            aria-hidden="true"
          />
          <span class="seg-label">历史记录</span>
          <span class="seg-count">{{ state.clips.length }}</span>
        </button>
        <button
          class="seg-btn"
          type="button"
          role="tab"
          :aria-selected="!isHistoryView"
          :class="{ 'is-active': !isHistoryView }"
          @click="switchView('snippets')"
        >
          <span
            v-if="!isHistoryView"
            data-flip-id="clip-mode-pill"
            class="mode-seg-pill"
            aria-hidden="true"
          />
          <span class="seg-label">常用短语</span>
          <span class="seg-count">{{ state.snippets.length }}</span>
        </button>
      </div>
    </div>

    <!-- 搜索与主动作栏 -->
    <header v-if="!detailClip && !editingSnippet" class="head">
      <div class="search">
        <Icon class="search-icon" name="search" :size="14" />
        <input
          ref="searchEl"
          type="text"
          spellcheck="false"
          autocomplete="off"
          :placeholder="isHistoryView ? '搜索正文、备注、图片…' : '搜索短语标题、账号、正文、分组…'"
          :aria-label="isHistoryView ? '搜索剪贴板历史' : '搜索常用短语'"
          :value="isHistoryView ? state.ui.query : state.ui.snippetQuery"
          @input="onInput"
        />
        <button
          v-if="isHistoryView ? state.ui.query : state.ui.snippetQuery"
          class="clear"
          type="button"
          aria-label="清除搜索"
          @click="onClear"
        >
          <Icon name="close" :size="13" />
        </button>
      </div>

      <button
        v-if="isHistoryView"
        class="pause"
        type="button"
        :class="{ 'is-paused': state.ui.paused }"
        :aria-pressed="state.ui.paused"
        :aria-label="state.ui.paused ? '已暂停记录，点击恢复' : '暂停记录剪贴板'"
        @click="onTogglePause"
      >
        <Icon :name="state.ui.paused ? 'play' : 'pause'" :size="14" />
        <span v-if="state.ui.paused">已暂停</span>
      </button>
      <button
        v-else
        class="pause create-snippet-btn"
        type="button"
        aria-label="新建常用短语"
        @click="openCreateSnippet"
      >
        <Icon name="plus" :size="14" />
        <span>新建</span>
      </button>
    </header>

    <!-- ==================== 分支一：历史记录模式 ==================== -->
    <template v-if="isHistoryView">
      <div v-if="!detailClip" class="filter-bar">
        <div class="chips" role="group" aria-label="按分组筛选">
          <button
            v-for="group in groupFilters"
            :key="group"
            class="chip"
            type="button"
            :class="{ 'is-on': group === state.ui.group }"
            :aria-pressed="group === state.ui.group"
            @click="setGroup(group)"
          >
            {{ group }}
          </button>
        </div>
        <div class="filter-actions" role="group" aria-label="收藏与管理">
          <button
            class="chip"
            type="button"
            :aria-pressed="state.ui.favoritesOnly"
            :class="{ 'is-on': state.ui.favoritesOnly }"
            @click="state.ui.favoritesOnly = !state.ui.favoritesOnly"
          >
            只看收藏
          </button>
          <button
            class="chip"
            type="button"
            :aria-pressed="selecting"
            :class="{ 'is-on': selecting }"
            @click="selecting ? stopSelecting() : (selecting = true)"
          >
            {{ selecting ? '退出管理' : '批量管理' }}
          </button>
        </div>
      </div>

      <GsapPresence
        :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: -4 }"
        :animate="{ opacity: 1, y: 0 }"
        :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: -4 }"
        :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
      >
        <div
          v-if="!detailClip && selecting"
          key="selection-actions"
          class="management selection-actions"
          role="group"
          aria-label="批量选择操作"
        >
          <button class="chip" type="button" @click="selectCurrentPage">全选本页</button>
          <button class="chip" type="button" @click="selectVisible">全选筛选（{{ filteredClips.length }}）</button>
          <button class="chip" type="button" @click="stopSelecting">取消选择</button>
          <button
            class="chip"
            type="button"
            :disabled="!selectedIds.size"
            @click="prepareDeletion([...selectedIds])"
          >
            删除已选（{{ selectedIds.size }}）
          </button>
        </div>
      </GsapPresence>

      <GsapPresence
        :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: -4 }"
        :animate="{ opacity: 1, y: 0 }"
        :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: -4 }"
        :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
      >
        <div
          v-if="deletePrompt"
          key="delete-prompt"
          class="management"
          role="alert"
        >
          <span>
            删除 {{ deletePrompt.ids.length }} 条记录
            <span v-if="deletePrompt.favorites">（含 {{ deletePrompt.favorites }} 条收藏）</span>？
          </span>
          <button class="chip" type="button" :disabled="deleting" @click="confirmDeletion">
            确认删除
          </button>
          <button class="chip" type="button" :disabled="deleting" @click="deletePrompt = null">
            取消
          </button>
        </div>
      </GsapPresence>

      <section v-if="detailClip && explanation" class="detail explanation" aria-label="AI 解读">
        <div class="detail-head">
          <h3 ref="explanationHeading" tabindex="-1">AI 解读</h3>
          <button class="back" type="button" @click="returnToOriginal"><Icon name="back" :size="14" />返回原文</button>
        </div>
        <div class="detail-body scroll-y explanation-body" tabindex="0" aria-label="解释内容">
          <p v-if="explanation.result" class="model-response" role="status">{{ explanation.result }}</p>
          <div v-if="explanation.answer" class="follow-up model-response">
            <h4 ref="followUpHeading" tabindex="-1">{{ explanation.question }}</h4>
            <p>{{ explanation.answer }}</p>
          </div>
          <p v-if="explanation.busy" class="reading-status" role="status">{{ explanation.result ? '正在回答追问…' : '正在解读这段文字…' }}</p>
          <p v-if="explanation.error" class="format-error" role="alert">{{ explanation.error }}</p>
          <div v-if="explanation.error" class="detail-actions">
            <button v-if="!explanation.result" class="chip" type="button" @click="submitExplanation">重试</button>
            <button class="chip" type="button" @click="openSettings('translation')">打开模型设置</button>
          </div>
        </div>
        <form v-if="explanation.result && !explanation.answer" class="management" @submit.prevent="submitExplanation">
          <input v-model="explanation.draft" aria-label="针对这段内容追问" placeholder="针对这段内容追问一句" :disabled="explanation.busy" />
          <button class="chip" type="submit" :disabled="explanation.busy || !explanation.draft.trim()">发送追问</button>
        </form>
      </section>
      <section v-else-if="detailClip" class="detail" aria-label="记录详情">
        <div class="detail-head">
          <button class="back" type="button" @click="leaveDetail()">
            <Icon name="back" :size="14" />
            <span>返回列表</span>
          </button>
          <span class="detail-meta">
            {{ detailClip.group }} · {{ relativeTime(detailClip.copiedAt) }}
          </span>
        </div>

        <div v-if="detailClip.kind === 'text'" class="preview-controls" aria-label="开发预览选项">
          <div class="preview-modes" role="group" aria-label="文本预览方式">
            <button
              class="chip"
              type="button"
              :aria-pressed="!formattedView"
              :class="{ 'is-on': !formattedView }"
              @click="showOriginal"
            >
              原文
            </button>
            <button
              class="chip"
              type="button"
              :aria-pressed="formattedView"
              :class="{ 'is-on': formattedView }"
              @click="showFormattedJson"
            >
              JSON 格式化
            </button>
          </div>
          <button ref="explanationButton" class="chip" type="button" :disabled="!isDesktop || !detailClip.text?.trim()"
            title="用已配置的模型解读这段文字" @click="explainClip(detailClip)">AI 解读</button>
          <label><input v-model="showLineNumbers" type="checkbox" />行号</label>
          <label><input v-model="wrapLines" type="checkbox" />自动换行</label>
        </div>
        <p v-if="formatError" class="format-error" role="status">{{ formatError }}</p>

        <div
          class="detail-body scroll-y"
          :class="{ 'code-body': detailClip.kind === 'text', 'no-wrap': !wrapLines }"
          :tabindex="detailClip.kind === 'text' ? 0 : undefined"
          :aria-label="detailClip.kind === 'text' ? '代码预览' : undefined"
        >
          <template v-if="detailClip.kind === 'image'">
            <div v-if="isImageBroken(detailClip)" class="detail-missing">
              <Icon name="image" :size="22" />
              <p class="detail-missing-title">图片文件已缺失</p>
              <p class="detail-missing-sub">
                原文件可能被移动或删除，这条记录无法预览，也无法复制。
              </p>
            </div>
            <img
              v-else
              class="detail-img"
              :src="detailClip.imageUrl"
              :alt="`剪贴板图片 ${detailClip.width ?? 0} × ${detailClip.height ?? 0}`"
            />
          </template>
          <div
            v-else
            class="code-lines"
            :class="{ numbered: showLineNumbers }"
            :style="{ '--line-number-width': `${String(previewLines.length).length + 1}ch` }"
          >
            <div v-for="(line, index) in previewLines" :key="index" class="code-line">
              <span
                v-if="showLineNumbers"
                class="line-number"
                :data-line="index + 1"
                aria-hidden="true"
              />
              <span class="line-text">{{ line }}</span>
            </div>
          </div>
        </div>

        <div class="management">
          <input
            v-model="noteDraft"
            maxlength="200"
            aria-label="剪贴板备注"
            placeholder="添加备注，回车或返回自动保存（最多 200 字）"
            @keydown.enter.prevent="saveNote(detailClip)"
            @blur="autoSaveNoteIfDirty(detailClip)"
          />
          <button
            class="chip"
            type="button"
            :disabled="savingNote || noteDraft.trim() === (detailClip.note ?? '')"
            @click="saveNote(detailClip)"
          >
            保存备注
          </button>
          <button
            class="chip"
            type="button"
            :disabled="savingNote"
            :aria-pressed="!!detailClip.favorite"
            @click="saveNote(detailClip, !detailClip.favorite)"
          >
            {{ detailClip.favorite ? '取消收藏' : '收藏' }}
          </button>
        </div>

        <div class="detail-actions">
          <ThemedSelect
            class="snippet-select-wrap"
            size="sm"
            :model-value="detailClip.group"
            :options="clipGroupOptions"
            aria-label="记录分组"
            @change="val => assignClipGroup(detailClip!.id, String(val))"
          />
          <button class="btn btn-ghost is-danger" type="button" @click="onDelete(detailClip)">
            删除
          </button>
          <span class="spacer" />
          <button
            v-if="detailClip.kind === 'image'"
            class="btn btn-ghost"
            type="button"
            :disabled="isImageBroken(detailClip)"
            @click="runClipboardOcr(detailClip.id)"
          >
            识别文字
          </button>
          <button
            v-if="detailClip.kind === 'image'"
            class="btn btn-ghost"
            type="button"
            :disabled="isImageBroken(detailClip)"
            @click="previewImage(detailClip.id)"
          >
            预览 / 钉图
          </button>
          <button
            v-if="detailClip.kind === 'text'"
            class="btn btn-ghost"
            type="button"
            :disabled="savingClipSnippetId === detailClip.id"
            @click="onSaveAsSnippet(detailClip)"
          >
            存为短语
          </button>
          <button
            v-if="detailClip.kind === 'text'"
            class="btn btn-ghost"
            type="button"
            @click="onTranslate(detailClip)"
          >
            翻译这条
          </button>
          <button
            v-if="detailClip.kind === 'text' && formattedView && formattedText !== null"
            class="btn btn-ghost"
            type="button"
            @click="onCopyFormatted"
          >
            复制格式化结果
          </button>
          <button
            class="btn btn-primary"
            type="button"
            :disabled="isImageBroken(detailClip)"
            @click="onCopy(detailClip)"
          >
            {{ detailClip.kind === 'text' ? '复制原文' : '复制' }}
          </button>
        </div>
      </section>

      <template v-else>
        <div v-if="state.ui.paused" class="banner banner-paused" role="status">
          <Icon name="pause" :size="13" />
          <span>已暂停记录，新内容不会写入历史</span>
          <button class="banner-btn" type="button" @click="onTogglePause">恢复记录</button>
        </div>

        <div
          v-if="filteredClips.length"
          ref="listEl"
          class="list scroll-y"
          role="group"
          aria-label="剪贴板记录"
        >
          <div
            v-for="(clip, localIdx) in pagedClips"
            :key="clip.id"
            class="row"
            :data-index="pageOffset + localIdx"
            :class="{ 'is-active': pageOffset + localIdx === state.ui.activeIndex }"
            @mouseenter="setActive(pageOffset + localIdx)"
          >
            <input
              v-if="selecting"
              type="checkbox"
              :checked="selectedIds.has(clip.id)"
              :aria-label="`选择：${clipSummary(clip)}`"
              @change="toggleSelected(clip.id)"
            />
            <button
              class="row-trigger"
              type="button"
              tabindex="-1"
              :aria-label="`${clipSummary(clip)}，${clip.group}，${relativeTime(clip.copiedAt)}`"
              @click="selecting ? toggleSelected(clip.id) : onCopy(clip)"
            >
              <div class="thumb" :class="{ 'is-broken': isImageBroken(clip) }">
                <img v-if="clip.kind === 'image' && !isImageBroken(clip)" :src="clip.imageUrl" alt="" />
                <span
                  v-else-if="inspectClipKind(clip)?.type === 'color'"
                  class="color-swatch"
                  :style="{ background: inspectClipKind(clip)?.color }"
                  :title="`颜色预览：${inspectClipKind(clip)?.color}`"
                />
                <Icon v-else-if="clip.kind === 'image'" name="image" :size="16" />
                <Icon v-else name="text" :size="16" />
                <span v-if="!selecting && localIdx < 9" class="index-kbd" :title="`按 Alt+${localIdx + 1} 快捷复制`">{{ localIdx + 1 }}</span>
              </div>

              <div class="row-main">
                <div class="row-title">{{ clip.favorite ? '★ ' : '' }}{{ clipSummary(clip) }}</div>
                <div class="row-meta">
                  <span v-if="inspectClipKind(clip)" class="smart-tag" :class="`is-${inspectClipKind(clip)?.type}`">{{ inspectClipKind(clip)?.label }}</span>
                  <span class="row-meta-text">
                    {{ clip.group }} · {{ relativeTime(clip.copiedAt) }}
                  </span>
                  <span v-if="clip.note" class="row-meta-text" :title="clip.note">
                    {{ clip.note }}
                  </span>
                  <span v-if="clip.imageMissing" class="tag-missing">图片缺失</span>
                </div>
              </div>
            </button>

            <div v-if="!selecting" class="row-actions">
              <button
                v-if="inspectClipKind(clip)?.type === 'url'"
                class="icon-btn"
                type="button"
                title="在新窗口打开链接"
                :aria-label="`打开链接：${clipSummary(clip)}`"
                @click.stop="openDetectedUrl(inspectClipKind(clip)!.url!)"
              >
                <Icon name="external" :size="14" />
              </button>
              <button v-if="clip.kind === 'text'" class="chip" type="button" :disabled="!isDesktop || !clip.text?.trim()"
                title="用已配置的模型解读这段文字" @click.stop="explainClip(clip)">AI 解读</button>
              <button
                v-if="clip.kind === 'image'"
                class="icon-btn"
                type="button"
                title="预览 / 钉图"
                :disabled="isImageBroken(clip)"
                :aria-label="`预览图片：${clipSummary(clip)}`"
                @click.stop="previewImage(clip.id)"
              >
                <Icon name="image" :size="14" />
              </button>
              <button
                v-if="clip.kind === 'text'"
                class="icon-btn"
                type="button"
                title="存为常用短语"
                :disabled="savingClipSnippetId === clip.id"
                :aria-label="`存为常用短语：${clipSummary(clip)}`"
                @click.stop="onSaveAsSnippet(clip)"
              >
                <Icon name="bookmark" :size="14" />
              </button>
              <button
                class="icon-btn"
                type="button"
                title="查看详情"
                :aria-label="`查看详情：${clipSummary(clip)}`"
                @click.stop="openDetail(clip.id)"
              >
                <Icon name="info" :size="14" />
              </button>
              <button
                class="icon-btn is-danger"
                type="button"
                title="删除记录"
                :aria-label="`删除这条记录：${clipSummary(clip)}`"
                @click.stop="onDelete(clip)"
              >
                <Icon name="trash" :size="14" />
              </button>
            </div>
          </div>
        </div>

        <div v-else class="empty">
          <Icon :name="state.clips.length ? 'search' : 'clipboard'" :size="26" />
          <p class="empty-title">
            {{ state.clips.length ? '没有匹配的记录' : '还没有剪贴板记录' }}
          </p>
          <p class="empty-sub">
            {{
              state.clips.length
                ? '换个关键词，或切到「全部」分组再试。'
                : '复制任意文本、网址或图片，历史会按时间倒序出现在这里。'
            }}
          </p>
        </div>

        <nav v-if="totalClipPages > 1" class="clip-pager" aria-label="剪贴板分页">
          <span class="pager-summary">共 {{ filteredClips.length }} 条 · {{ clipPage }}/{{ totalClipPages }} 页</span>
          <div class="pager-controls">
            <button
              class="pager-btn"
              type="button"
              :disabled="clipPage <= 1"
              aria-label="上一页"
              @click="setClipPage(clipPage - 1)"
            >
              ‹
            </button>
            <button
              v-for="num in visiblePageNumbers"
              :key="num"
              class="pager-btn"
              type="button"
              :class="{ 'is-on': num === clipPage }"
              :aria-current="num === clipPage ? 'page' : undefined"
              @click="setClipPage(num)"
            >
              {{ num }}
            </button>
            <button
              class="pager-btn"
              type="button"
              :disabled="clipPage >= totalClipPages"
              aria-label="下一页"
              @click="setClipPage(clipPage + 1)"
            >
              ›
            </button>
          </div>
        </nav>
      </template>
    </template>

    <!-- ==================== 分支二：常用短语模式 ==================== -->
    <template v-else>
      <!-- 新建 / 编辑常用短语内联视图 -->
      <section
        v-if="editingSnippet"
        class="detail snippet-editor-wrap"
        aria-label="编辑常用短语"
      >
        <div class="detail-head snippet-head">
          <button class="back" type="button" @click="cancelEditSnippet">
            <Icon name="back" :size="14" />
            <span>返回常用短语</span>
          </button>
          <span class="detail-meta">
            {{
              editingSnippet.id
                ? isEditingCredential
                  ? '编辑账密'
                  : '编辑模板'
                : isEditingCredential
                  ? '新建账密'
                  : '新建模板'
            }}
          </span>
        </div>

        <div class="snippet-editor scroll-y">
          <div class="snippet-kind-row">
            <span class="snippet-label">类型</span>
            <div class="kind-switch" role="radiogroup" aria-label="短语类型">
              <button
                class="kind-switch-btn"
                type="button"
                role="radio"
                :aria-checked="!isEditingCredential"
                :class="{ 'is-active': !isEditingCredential }"
                @click="switchDraftKind('text')"
              >
                普通短语
              </button>
              <button
                class="kind-switch-btn"
                type="button"
                role="radio"
                :aria-checked="isEditingCredential"
                :class="{ 'is-active': isEditingCredential }"
                @click="switchDraftKind('credential')"
              >
                <Icon name="key" :size="12" />
                <span>账密</span>
              </button>
            </div>
          </div>

          <label class="snippet-field">
            <span class="snippet-label">{{ isEditingCredential ? '账密名称' : '短语标题' }}</span>
            <input
              ref="snippetTitleEl"
              v-model="editingSnippet.title"
              class="snippet-input"
              type="text"
              maxlength="80"
              spellcheck="false"
              :placeholder="
                isEditingCredential
                  ? '例如：公司邮箱、测试服务器、GitHub 账号'
                  : '例如：研发日报抬头、收件地址、常用命令'
              "
            />
          </label>

          <div class="snippet-meta-row">
            <div class="snippet-group-picker">
              <span class="snippet-label">{{ isEditingCredential ? '类别' : '分组' }}</span>
              <template v-if="isEditingCredential">
                <span class="tag-credential">账密（内置）</span>
              </template>
              <template v-else-if="!customGroupMode">
                <ThemedSelect
                  v-model="editingSnippet.group"
                  class="snippet-select-wrap"
                  size="sm"
                  :options="snippetGroupOptions"
                  aria-label="常用短语分组"
                />
                <button
                  class="chip"
                  type="button"
                  @click="customGroupMode = true; customGroupName = ''"
                >
                  + 新分组
                </button>
              </template>
              <template v-else>
                <input
                  v-model="customGroupName"
                  class="snippet-input compact-input"
                  type="text"
                  maxlength="40"
                  placeholder="输入新分组名"
                  aria-label="新分组名称"
                />
                <button class="chip" type="button" @click="customGroupMode = false">
                  选已有
                </button>
              </template>
            </div>

            <label class="pin-toggle">
              <input v-model="editingSnippet.pinned" type="checkbox" />
              <span>{{ isEditingCredential ? '置顶此账密' : '置顶此短语' }}</span>
            </label>
          </div>

          <template v-if="isEditingCredential">
            <label class="snippet-field">
              <span class="snippet-label">账号</span>
              <input
                ref="snippetAccountEl"
                v-model="editingSnippet.account"
                class="snippet-input"
                type="text"
                maxlength="300"
                autocomplete="off"
                spellcheck="false"
                placeholder="输入登录账号、用户名或邮箱"
                @keydown.enter.prevent
              />
            </label>

            <label class="snippet-field">
              <span class="snippet-label">
                密码
                <span v-if="loadingDraftPassword" class="snippet-inline-note">正在读取已保存密码…</span>
              </span>
              <div class="password-input-wrap">
                <input
                  v-model="editingSnippet.password"
                  class="snippet-input password-input"
                  :type="showDraftPassword ? 'text' : 'password'"
                  maxlength="1000"
                  autocomplete="new-password"
                  spellcheck="false"
                  :placeholder="
                    editingOriginal?.hasPassword
                      ? '输入新密码（留空或保持现状将保留原密码）'
                      : '输入登录密码'
                  "
                  @input="draftPasswordDirty = true; draftPasswordError = ''"
                  @keydown.enter.prevent="submitSnippet"
                />
                <button
                  class="pw-toggle"
                  type="button"
                  :aria-pressed="showDraftPassword"
                  :aria-label="showDraftPassword ? '隐藏密码' : '显示密码'"
                  :title="showDraftPassword ? '隐藏密码' : '显示密码'"
                  @click="showDraftPassword = !showDraftPassword"
                >
                  <Icon :name="showDraftPassword ? 'eye-off' : 'eye'" :size="14" />
                </button>
              </div>
              <p v-if="draftPasswordError" class="field-warn" role="status">{{ draftPasswordError }}</p>
            </label>

            <label class="snippet-field">
              <span class="snippet-label">备注（可选）</span>
              <input
                v-model="editingSnippet.content"
                class="snippet-input"
                type="text"
                maxlength="500"
                spellcheck="false"
                placeholder="例如：系统地址、环境说明（不建议写明文口令）"
              />
            </label>

            <p class="cred-security-hint">
              密码仅在本机加密保存，列表隐藏明文且复制自动跳过剪贴板历史，不会自动粘贴。
            </p>
          </template>

          <template v-else>
            <div class="var-bar" role="group" aria-label="插入动态占位符">
              <span class="var-hint">插入变量：</span>
              <button
                v-for="preset in variablePresets"
                :key="preset.token"
                class="chip var-chip"
                type="button"
                :title="preset.title"
                @click="insertVariable(preset.token)"
              >
                + {{ preset.label }}
              </button>
            </div>

            <label class="snippet-field snippet-body-field">
              <span class="snippet-label">短语内容</span>
              <textarea
                ref="snippetBodyEl"
                v-model="editingSnippet.content"
                class="textarea snippet-textarea"
                rows="4"
                maxlength="20000"
                spellcheck="false"
                placeholder="输入短语正文，支持点击上方按钮插入 {{date}}、{{time}}、{{clipboard}} 等变量；复制时仅写入剪贴板，不会自动发送。"
              />
            </label>

            <div v-if="draftHasVariables" class="var-preview" role="status">
              <div class="var-preview-head">动态拼装实时预览</div>
              <div class="var-preview-body">{{ draftPreviewText }}</div>
            </div>
          </template>
        </div>

        <div class="detail-actions snippet-actions">
          <template v-if="editingOriginal">
            <button
              class="btn btn-ghost is-danger"
              type="button"
              :disabled="deletingSnippet"
              @click="performDeleteSnippet(editingOriginal.id)"
            >
              删除
            </button>
            <button
              v-if="canMoveSnippet(editingOriginal, -1)"
              class="btn btn-ghost"
              type="button"
              @click="moveSnippetOrder(editingOriginal, -1)"
            >
              上移
            </button>
            <button
              v-if="canMoveSnippet(editingOriginal, 1)"
              class="btn btn-ghost"
              type="button"
              @click="moveSnippetOrder(editingOriginal, 1)"
            >
              下移
            </button>
            <button
              v-if="!isEditingCredential && hasSnippetVariables(editingOriginal.content)"
              class="btn btn-ghost"
              type="button"
              title="不展开变量，复制原始模板花括号"
              @click="onCopySnippet(editingOriginal, true)"
            >
              原样复制
            </button>
          </template>
          <span class="spacer" />
          <button class="btn btn-ghost" type="button" @click="cancelEditSnippet">取消</button>
          <button
            class="btn btn-primary"
            type="button"
            :disabled="!canSubmitSnippet"
            @click="submitSnippet"
          >
            {{ savingSnippet ? '保存中…' : isEditingCredential ? '保存账密' : '保存短语' }}
          </button>
        </div>
      </section>

      <!-- 常用短语列表视图 -->
      <template v-else>
        <div class="filter-bar">
          <div class="chips" role="group" aria-label="按短语分组筛选">
            <button
              v-for="group in snippetGroupFilters"
              :key="group"
              class="chip"
              type="button"
              :class="{ 'is-on': group === state.ui.snippetGroup }"
              :aria-pressed="group === state.ui.snippetGroup"
              @click="setSnippetGroup(group)"
            >
              {{ group }}
            </button>
          </div>
          <div class="filter-actions" role="group" aria-label="置顶筛选">
            <button
              class="chip"
              type="button"
              :aria-pressed="state.ui.snippetPinnedOnly"
              :class="{ 'is-on': state.ui.snippetPinnedOnly }"
              @click="state.ui.snippetPinnedOnly = !state.ui.snippetPinnedOnly; state.ui.snippetActiveIndex = 0"
            >
              只看置顶
            </button>
          </div>
        </div>

        <div v-if="snippetDeletePrompt" class="management" role="alert">
          <span>删除置顶短语「{{ snippetDeletePrompt.title }}」？</span>
          <button
            class="chip"
            type="button"
            :disabled="deletingSnippet"
            @click="performDeleteSnippet(snippetDeletePrompt.id)"
          >
            确认删除
          </button>
          <button
            class="chip"
            type="button"
            :disabled="deletingSnippet"
            @click="snippetDeletePrompt = null"
          >
            取消
          </button>
        </div>

        <div
          v-if="filteredSnippets.length"
          ref="snippetListEl"
          class="list scroll-y"
          role="group"
          aria-label="常用短语列表"
        >
          <div
            v-for="(snippet, index) in filteredSnippets"
            :key="snippet.id"
            class="row snippet-row"
            :data-index="index"
            :class="{
              'is-active': index === state.ui.snippetActiveIndex,
              'is-credential': snippet.kind === 'credential'
            }"
            @mouseenter="state.ui.snippetActiveIndex = index"
          >
            <div class="row-body">
              <button
                class="row-trigger"
                type="button"
                tabindex="-1"
                :aria-label="
                  snippet.kind === 'credential'
                    ? `${snippet.title}，账密，账号 ${snippet.account ?? ''}`
                    : `${snippet.title}，${snippet.group}`
                "
                @click="onCopySnippet(snippet)"
              >
                <div
                  class="thumb snippet-thumb"
                  :class="{
                    'is-pinned': snippet.pinned,
                    'is-credential': snippet.kind === 'credential'
                  }"
                >
                  <Icon
                    :name="snippet.kind === 'credential' ? 'key' : snippet.pinned ? 'pin' : 'bookmark'"
                    :size="15"
                  />
                </div>

                <div class="row-main">
                  <div class="row-title snippet-title-line">
                    <span class="snippet-title-text">{{ snippet.title }}</span>
                    <span v-if="snippet.kind === 'credential'" class="tag-credential">账密</span>
                    <span v-else-if="hasSnippetVariables(snippet.content)" class="tag-dynamic">动态</span>
                  </div>
                  <div v-if="snippet.kind === 'credential'" class="row-meta cred-row-meta">
                    <span class="row-meta-text" :title="`账号：${snippet.account || ''}`">
                      账号：{{ snippet.account || '未填写'
                      }}<template v-if="snippet.content"> · {{ snippetSummary(snippet.content) }}</template>
                    </span>
                  </div>
                  <div v-else class="row-meta">
                    <span class="row-meta-text">
                      {{ snippet.group }} · {{ snippetSummary(snippet.content) }}
                    </span>
                  </div>
                </div>
              </button>

              <div v-if="snippet.kind === 'credential'" class="cred-quick-bar">
                <button
                  class="cred-copy-btn"
                  :class="{ 'is-copied': copiedBadgeKey === `${snippet.id}:account` }"
                  type="button"
                  :aria-label="`复制账号：${snippet.title}`"
                  @click="onCopySnippet(snippet, false, 'account')"
                >
                  <Icon
                    :name="copiedBadgeKey === `${snippet.id}:account` ? 'check' : 'copy'"
                    :size="11"
                  />
                  <span>{{
                    copiedBadgeKey === `${snippet.id}:account` ? '已复制账号' : '复制账号'
                  }}</span>
                </button>
                <button
                  class="cred-copy-btn is-secret"
                  :class="{ 'is-copied': copiedBadgeKey === `${snippet.id}:password` }"
                  type="button"
                  title="复制密码（不记入剪贴板历史）"
                  :aria-label="`复制密码：${snippet.title}`"
                  @click="onCopySnippet(snippet, false, 'password')"
                >
                  <Icon
                    :name="copiedBadgeKey === `${snippet.id}:password` ? 'check' : 'key'"
                    :size="11"
                  />
                  <span>{{
                    copiedBadgeKey === `${snippet.id}:password` ? '已复制密码' : '复制密码'
                  }}</span>
                </button>
              </div>
            </div>

            <div class="row-actions">
              <button
                v-gsap-press="state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap"
                class="icon-btn"
                type="button"
                :class="{ 'is-pinned-btn': snippet.pinned }"
                :title="snippet.pinned ? '取消置顶' : '置顶短语'"
                :aria-label="`${snippet.pinned ? '取消置顶' : '置顶'}：${snippet.title}`"
                @click.stop="pulseSnippetPin(snippet, $event)"
              >
                <span class="pin-icon-wrap">
                  <Icon name="pin" :size="14" />
                </span>
              </button>
              <button
                class="icon-btn"
                type="button"
                title="编辑短语"
                :aria-label="`编辑短语：${snippet.title}`"
                @click.stop="openEditSnippet(snippet)"
              >
                <Icon name="edit" :size="14" />
              </button>
              <button
                class="icon-btn is-danger"
                type="button"
                title="删除短语"
                :aria-label="`删除短语：${snippet.title}`"
                @click.stop="onRequestDeleteSnippet(snippet)"
              >
                <Icon name="trash" :size="14" />
              </button>
            </div>
          </div>
        </div>

        <div v-else class="empty">
          <Icon
            :name="
              state.snippets.length
                ? state.ui.snippetGroup === '账密'
                  ? 'key'
                  : 'search'
                : 'bookmark'
            "
            :size="26"
          />
          <p class="empty-title">
            {{
              state.snippets.length
                ? state.ui.snippetGroup === '账密'
                  ? '暂无账密记录'
                  : '没有匹配的常用短语'
                : '还没有常用短语'
            }}
          </p>
          <p class="empty-sub">{{ emptySnippetHint }}</p>
          <button
            v-if="!state.snippets.length || state.ui.snippetGroup === '账密'"
            class="btn btn-primary empty-cta"
            type="button"
            @click="openCreateSnippet"
          >
            {{ state.ui.snippetGroup === '账密' ? '新建第一条账密' : '新建第一条短语' }}
          </button>
        </div>
      </template>
    </template>

    <footer class="foot">{{ footerHint }}</footer>
  </div>
</template>

<style scoped>
.management {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
}

.management input:not([type]) {
  min-width: 8rem;
  flex: 1;
  color: var(--text);
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: 5px;
  padding: 5px 8px;
}

.pop {
  display: flex;
  flex-direction: column;
  width: 384px;
  max-height: 480px;
  background: var(--pop-bg);
  border: 1px solid var(--pop-border);
  border-radius: var(--r-pop);
  box-shadow: var(--pop-shadow);
  color: var(--text);
  overflow: hidden;
}

.pop button:focus-visible,
.pop .row:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

/* 顶部模式切换栏 */
.mode-bar {
  flex: none;
  padding: 6px 10px 0;
}

.mode-seg {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 2px;
  padding: 3px;
  background: var(--well-bg);
  box-shadow: var(--well-shadow);
  border: 1px solid var(--line);
  border-radius: calc(var(--r-chip) + 2px);
}

.seg-btn {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  height: 24px;
  padding: 0 10px;
  border-radius: calc(var(--r-chip) - 2px);
  font-size: var(--text-xs);
  font-weight: 500;
  color: var(--text-2);
  transition: color var(--dur-fast) var(--ease);
}

.mode-seg-pill {
  position: absolute;
  inset: 0;
  z-index: 0;
  border-radius: calc(var(--r-chip) - 2px);
  background: var(--tile-surface);
  box-shadow: var(--tile-shadow);
  border: 1px solid var(--line);
  pointer-events: none;
}

.seg-label,
.seg-count {
  position: relative;
  z-index: 1;
}

.seg-btn:hover {
  color: var(--text);
}

.seg-btn.is-active {
  color: var(--text);
  font-weight: 600;
}

.seg-count {
  padding: 0 5px;
  border-radius: 999px;
  background: var(--hover);
  color: var(--text-3);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  line-height: 1.45;
}

.seg-btn.is-active .seg-count {
  background: var(--accent-weak);
  color: var(--accent);
}

/* 顶部：搜索与动作按钮 */
.head {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 6px 10px;
}

.search {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 8px;
  background: var(--well-bg);
  box-shadow: var(--well-shadow);
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease), box-shadow var(--dur-fast) var(--ease);
}

.search:hover {
  border-color: var(--line-strong);
}

.search:focus-within {
  border-color: var(--accent);
}

.search-icon {
  flex: none;
  color: var(--text-3);
}

.search input {
  flex: 1 1 auto;
  min-width: 0;
  height: 100%;
  padding: 0;
  border: 0;
  background: none;
  font-size: var(--text-base);
  color: var(--text);
}

.search input::placeholder {
  color: var(--text-3);
}

/* 焦点落在输入框上时由外层描边提示，避免框内出现第二道轮廓 */
.search input:focus-visible {
  outline: none;
}

.clear {
  flex: none;
  width: 20px;
  height: 20px;
  display: grid;
  place-items: center;
  border-radius: 6px;
  color: var(--text-3);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.clear:hover {
  background: var(--hover);
  color: var(--text);
}

.clear:active {
  background: var(--active);
}

.pause {
  flex: none;
  display: flex;
  align-items: center;
  gap: 5px;
  height: 30px;
  padding: 0 9px;
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  font-size: var(--text-sm);
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.pause:hover {
  background: var(--hover);
  border-color: var(--line-strong);
  color: var(--text);
}

.pause:active {
  background: var(--active);
}

.pause.is-paused,
.create-snippet-btn {
  background: var(--accent-weak);
  border-color: var(--accent-weak);
  color: var(--accent);
  font-weight: 500;
}

.pause.is-paused:hover,
.create-snippet-btn:hover {
  border-color: var(--accent);
  color: var(--accent-hover);
}

/* 提示条 */
.banner {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0 10px 8px;
  padding: 6px 8px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  font-size: var(--text-xs);
  color: var(--text-2);
}

.banner-btn {
  margin-left: auto;
  flex: none;
  height: 22px;
  padding: 0 8px;
  border: 1px solid var(--line-strong);
  border-radius: 6px;
  font-size: var(--text-xs);
  color: var(--text);
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.banner-btn:hover {
  background: var(--hover);
  border-color: var(--accent);
  color: var(--accent);
}

.banner-btn:active {
  background: var(--active);
}

/* 筛选与管理共用一行；分组独立滚动，操作按钮保持可见。 */
.filter-bar {
  flex: none;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 8px;
  padding: 0 10px 6px;
}

.chips {
  flex: 1 1 140px;
  min-width: 0;
  display: flex;
  gap: 4px;
  padding: 1px;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: none;
}

.filter-actions {
  flex: none;
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
  padding-left: 8px;
  border-left: 1px solid var(--line-strong);
}

.selection-actions {
  padding-top: 0;
}

.chips::-webkit-scrollbar {
  display: none;
}

.chip {
  flex: none;
  height: 22px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  font-size: var(--text-xs);
  color: var(--text-2);
  white-space: nowrap;
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.chip:hover {
  background: var(--hover);
  border-color: var(--line-strong);
  color: var(--text);
}

.chip:active {
  background: var(--active);
}

.chip.is-on {
  background: var(--accent-weak);
  border-color: var(--accent-weak);
  color: var(--accent);
}

.chip.is-on:hover {
  border-color: var(--accent);
}

/* 列表 */
.list {
  flex: 1 1 auto;
  min-height: 0;
  padding: 0 8px 8px;
  overflow-x: hidden;
  scrollbar-gutter: stable;
}

.row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: var(--r-tile);
  user-select: none;
  transition: background var(--dur-fast) var(--ease);
}

.row-body {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.row-trigger {
  flex: 1 1 auto;
  width: 100%;
  max-width: 100%;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;
}

.row-trigger:active {
  transform: none;
}

.row-body > .cred-quick-bar {
  padding-left: 36px;
}

.row + .row {
  margin-top: 1px;
}

.row:hover {
  background: var(--hover);
}

.row:active {
  background: var(--active);
}

.row.is-active {
  background: var(--accent-weak);
}

.row.is-active:active {
  background: var(--active);
}

.thumb {
  position: relative;
  flex: none;
  width: 28px;
  height: 28px;
  display: grid;
  place-items: center;
  overflow: hidden;
  background: var(--thumb-bg);
  border: 1px solid var(--line);
  border-radius: 7px;
  color: var(--text-2);
}

.color-swatch {
  width: 18px;
  height: 18px;
  border-radius: 5px;
  border: 1px solid rgba(255, 255, 255, 0.35);
  box-shadow: 0 1px 4px rgba(0, 0, 0, 0.2);
}

.index-kbd {
  position: absolute;
  right: 1px;
  bottom: 1px;
  min-width: 13px;
  height: 13px;
  padding: 0 3px;
  border-radius: 4px;
  background: var(--surface);
  border: 1px solid var(--line-strong);
  color: var(--text-2);
  font-family: var(--font-mono);
  font-size: 9px;
  font-weight: 700;
  line-height: 11px;
  text-align: center;
  opacity: 0.78;
}

.smart-tag {
  flex: none;
  padding: 0 5px;
  height: 16px;
  line-height: 16px;
  border-radius: 4px;
  font-family: var(--font-mono);
  font-size: 10px;
  font-weight: 650;
  background: var(--accent-weak);
  color: var(--accent);
}

.smart-tag.is-color {
  border: 1px solid var(--line-strong);
}

.thumb img {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: cover;
  border-radius: 8px;
}

.thumb.is-broken {
  background: var(--surface-2);
  color: var(--text-3);
}

.snippet-thumb.is-pinned {
  background: var(--accent-weak);
  border-color: var(--accent-weak);
  color: var(--accent);
}

.row-main {
  flex: 1 1 auto;
  min-width: 0;
}

.row-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-base);
  font-weight: 500;
  color: var(--text);
}

.snippet-title-line {
  display: flex;
  align-items: center;
  gap: 6px;
}

.snippet-title-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}

.tag-dynamic {
  flex: none;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--accent-weak);
  color: var(--accent);
  font-size: 11px;
  line-height: 1.4;
}

.row-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  margin-top: 2px;
}

.row-meta-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-xs);
  font-variant-numeric: tabular-nums;
  color: var(--text-3);
}

.tag-missing {
  flex: none;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--danger-weak);
  color: var(--danger);
  font-size: var(--text-xs);
  line-height: 1.35;
}

.row-actions {
  flex: none;
  display: flex;
  gap: 2px;
  opacity: 0;
  visibility: hidden;
  transition: opacity var(--dur-fast) var(--ease);
}

.row:hover .row-actions,
.row.is-active .row-actions,
.row:focus-within .row-actions {
  opacity: 1;
  visibility: visible;
}

.icon-btn {
  width: 26px;
  height: 26px;
  display: grid;
  place-items: center;
  border-radius: 7px;
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.icon-btn:hover {
  background: var(--active);
  color: var(--text);
}

.icon-btn:active {
  background: var(--line-strong);
}

.icon-btn.is-pinned-btn {
  color: var(--accent);
}
.pin-icon-wrap {
  display: grid;
  place-items: center;
}

.icon-btn.is-danger:hover {
  background: var(--danger-weak);
  color: var(--danger);
}

/* 空态 */
.empty {
  flex: 1 1 auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 28px 24px 32px;
  text-align: center;
  color: var(--text-3);
}

.empty-title {
  margin: 8px 0 0;
  font-size: var(--text-base);
  color: var(--text-2);
}

.empty-sub {
  margin: 4px 0 0;
  max-width: 268px;
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--text-3);
}

.empty-cta {
  margin-top: 12px;
}

/* 详情与短语编辑器 */
.detail {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 8px 10px 10px;
}

.detail-head {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding-bottom: 8px;
}

.explanation .detail-head { justify-content: space-between; }
.explanation h3 { margin: 0; font-size: var(--text-md); font-weight: 600; }
.explanation-body { max-height: 260px; overflow-x: hidden; scrollbar-gutter: stable; overflow-wrap: anywhere; line-height: 1.65; user-select: text; }
.explanation-body :is(p, h4) { white-space: pre-wrap; }
.explanation-body p { margin: 0; }
.follow-up { margin-top: var(--space-3); padding-top: var(--space-2); border-top: 1px solid var(--line-strong); }
.follow-up h4 { margin: 0 0 var(--space-1); color: var(--text); font-size: var(--text-base); }
.reading-status { display: flex; align-items: center; gap: var(--space-2); color: var(--text-2); }
.reading-status::before { content: ''; flex: none; width: 6px; height: 6px; border-radius: 50%; background: var(--accent); }
.explanation .management { padding: var(--space-2) 0 0; }
.explanation-body p + p { margin-top: 8px; }

.back {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 8px 0 5px;
  border-radius: 7px;
  font-size: var(--text-sm);
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.back:hover {
  background: var(--hover);
  color: var(--text);
}

.back:active {
  background: var(--active);
}

.detail-meta {
  margin-left: auto;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-xs);
  color: var(--text-3);
}

.detail-body {
  flex: 1 1 auto;
  min-height: 0;
  padding: 10px;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-card);
}

/* 常用短语内联编辑区 */
.snippet-editor {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 9px;
  padding-right: 2px;
}

.snippet-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.snippet-body-field {
  flex: 1 1 auto;
  min-height: 0;
}

.snippet-label {
  font-size: var(--text-xs);
  color: var(--text-2);
  white-space: nowrap;
}

.snippet-input,
.snippet-select {
  height: 30px;
  padding: 0 9px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  font-size: var(--text-sm);
  color: var(--text);
  transition: border-color var(--dur-fast) var(--ease);
}

.snippet-input::placeholder {
  color: var(--text-3);
}

.snippet-input:focus-visible,
.snippet-select:focus-visible {
  outline: none;
  border-color: var(--accent);
}

.compact-input {
  width: 118px;
}

.snippet-meta-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.snippet-group-picker {
  display: flex;
  align-items: center;
  gap: 6px;
}

.pin-toggle {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: var(--text-xs);
  color: var(--text-2);
  cursor: pointer;
  user-select: none;
}

.pin-toggle input {
  margin: 0;
  accent-color: var(--accent);
}

.var-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
}

.var-hint {
  font-size: var(--text-xs);
  color: var(--text-3);
  margin-right: 2px;
}

.var-chip {
  height: 22px;
  padding: 0 7px;
  font-family: var(--font-mono);
  font-size: 11px;
}

.snippet-textarea {
  width: 100%;
  min-height: 74px;
  flex: 1 1 auto;
  resize: vertical;
  padding: 8px 10px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-card);
  font-family: var(--font-sans);
  font-size: var(--text-sm);
  line-height: 1.55;
  color: var(--text);
}

.snippet-textarea::placeholder {
  color: var(--text-3);
}

.snippet-textarea:focus-visible {
  outline: none;
  border-color: var(--accent);
}

.var-preview {
  flex: none;
  margin-top: 2px;
  padding: 7px 9px;
  background: var(--surface-2);
  border: 1px dashed var(--line-strong);
  border-radius: var(--r-chip);
}

.var-preview-head {
  font-size: 11px;
  color: var(--accent);
  margin-bottom: 3px;
}

.var-preview-body {
  font-family: var(--font-mono);
  font-size: 11px;
  line-height: 1.5;
  color: var(--text-2);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.preview-controls {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px 12px;
  margin-bottom: 8px;
}

.preview-modes {
  display: flex;
  gap: 6px;
  margin-right: auto;
}

.preview-controls label {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--text-2);
  white-space: nowrap;
}

.preview-controls input {
  margin: 0;
  accent-color: var(--accent);
}

.format-error {
  margin: 0 0 8px;
  color: var(--danger);
  font-size: 12px;
}

.detail-body.code-body {
  flex: 0 1 auto;
  min-height: 90px;
  max-height: 224px;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  padding: 10px 8px;
}
.detail-body.code-body.no-wrap {
  overflow: auto;
  scrollbar-gutter: auto;
}

.code-body:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.code-lines {
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.75;
  tab-size: 4;
  color: var(--text);
}

.code-line {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  min-height: 1.75em;
}

.numbered .code-line {
  grid-template-columns: var(--line-number-width) minmax(0, 1fr);
  column-gap: 12px;
}

.line-number {
  text-align: right;
  color: var(--text-3);
  user-select: none;
}

.line-number::before {
  content: attr(data-line);
}

.line-text {
  min-width: 0;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
}

.no-wrap .code-lines {
  width: max-content;
  min-width: 100%;
}

.no-wrap .line-text {
  white-space: pre;
  overflow-wrap: normal;
}

.detail-img {
  display: block;
  width: auto;
  height: auto;
  max-width: 100%;
  max-height: 300px;
  margin: 0 auto;
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
}

.detail-missing {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 26px 12px;
  text-align: center;
  color: var(--text-3);
}

.detail-missing p {
  margin: 0;
}

.detail-missing-title {
  font-size: var(--text-base);
  color: var(--danger);
}

.detail-missing-sub {
  max-width: 250px;
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--text-3);
}

.detail-actions {
  flex-wrap: wrap;
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  padding-top: 10px;
}

.spacer {
  flex: 1 1 auto;
}

.btn {
  white-space: nowrap;
  height: 28px;
  padding: 0 12px;
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  font-size: var(--text-sm);
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.btn-ghost {
  color: var(--text-2);
}

.btn-ghost:hover {
  background: var(--hover);
  border-color: var(--line-strong);
  color: var(--text);
}

.btn-ghost:active {
  background: var(--active);
}

.btn-ghost.is-danger {
  border-color: var(--danger-weak);
  color: var(--danger);
}

.btn-ghost.is-danger:hover {
  background: var(--danger-weak);
  border-color: var(--danger);
  color: var(--danger);
}

.btn-primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-text);
}

.btn-primary:hover {
  background: var(--accent-hover);
  border-color: var(--accent-hover);
}

.btn-primary:active {
  background: var(--accent);
  border-color: var(--accent);
}

.btn:disabled {
  background: var(--surface-2);
  border-color: var(--line);
  color: var(--text-3);
  cursor: not-allowed;
}

/* 账密列表与编辑器样式 */
.snippet-row.is-credential {
  align-items: flex-start;
}

.snippet-row.is-credential .row-actions {
  margin-top: 4px;
}

.snippet-thumb.is-credential {
  background: var(--surface-3);
  border-color: var(--line-strong);
  color: var(--accent);
}

.tag-credential {
  flex: none;
  display: inline-flex;
  align-items: center;
  height: 18px;
  padding: 0 6px;
  border-radius: 4px;
  background: var(--accent-weak);
  border: 1px solid var(--accent-weak);
  color: var(--accent);
  font-size: 11px;
  font-weight: 500;
  line-height: 1;
}

.cred-quick-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 5px;
  margin-top: 5px;
}

.cred-copy-btn {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  background: var(--surface-2);
  font-size: 11px;
  color: var(--text-2);
  white-space: nowrap;
  touch-action: manipulation;
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease), transform 140ms var(--ease-out);
}

.cred-copy-btn:hover {
  background: var(--hover);
  border-color: var(--line-strong);
  color: var(--text);
}

.cred-copy-btn.is-secret {
  border-color: var(--line-strong);
  color: var(--text);
}

.cred-copy-btn.is-copied {
  background: var(--accent-weak);
  border-color: var(--accent);
  color: var(--accent);
}

.cred-copy-btn:active:not(:disabled) {
  transform: scale(0.965);
}

.snippet-kind-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.kind-switch {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 2px;
  padding: 2px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
}

.kind-switch-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  height: 24px;
  padding: 0 10px;
  border-radius: calc(var(--r-chip) - 2px);
  font-size: var(--text-xs);
  font-weight: 500;
  color: var(--text-2);
  white-space: nowrap;
  touch-action: manipulation;
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease),
    transform 140ms var(--ease-out);
}

.kind-switch-btn:hover {
  color: var(--text);
}

.kind-switch-btn.is-active {
  background: var(--surface-3);
  color: var(--text);
  font-weight: 600;
  box-shadow: inset 0 0 0 1px var(--line-strong);
}

.kind-switch-btn:active:not(:disabled) {
  transform: scale(0.965);
}

.snippet-inline-note {
  margin-left: 6px;
  font-size: 11px;
  color: var(--accent);
}

.password-input-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.password-input {
  width: 100%;
  padding-right: 34px;
}

.pw-toggle {
  position: absolute;
  right: 4px;
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
  border-radius: 6px;
  color: var(--text-2);
  touch-action: manipulation;
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease),
    transform 140ms var(--ease-out);
}

.pw-toggle:hover {
  background: var(--hover);
  color: var(--text);
}

.pw-toggle:active {
  transform: scale(0.95);
}

.field-warn {
  margin: 0;
  font-size: 11px;
  color: var(--danger);
}

.cred-security-hint {
  margin: 0;
  padding: 6px 8px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  font-size: 11px;
  line-height: 1.5;
  color: var(--text-3);
}

@media (prefers-reduced-motion: reduce) {
  .cred-copy-btn,
  .kind-switch-btn,
  .pw-toggle {
    transition-duration: 0.001ms;
  }

  .cred-copy-btn:active:not(:disabled),
  .kind-switch-btn:active:not(:disabled),
  .pw-toggle:active {
    transform: none;
  }
}

/* 分页器 */
.clip-pager {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 5px 10px;
  border-top: 1px solid var(--line);
  background: var(--pane-surface, var(--surface));
}
.pager-summary {
  font-size: 11px;
  color: var(--text-3);
  font-variant-numeric: tabular-nums;
}
.pager-controls {
  display: inline-flex;
  align-items: center;
  gap: 3px;
}
.pager-btn {
  min-width: 22px;
  height: 22px;
  padding: 0 5px;
  border-radius: var(--r-chip);
  border: 1px solid var(--line);
  background: var(--surface-2);
  color: var(--text-2);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  cursor: pointer;
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}
.pager-btn:hover:not(:disabled):not(.is-on) {
  background: var(--hover);
  border-color: var(--line-strong);
  color: var(--text);
}
.pager-btn.is-on {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-text);
  font-weight: 600;
}
.pager-btn:disabled {
  opacity: 0.42;
  cursor: not-allowed;
}
.snippet-select-wrap {
  min-width: 98px;
  width: auto;
}

/* 底部提示 */
.foot {
  flex: none;
  padding: 5px 10px;
  border-top: 1px solid var(--line);
  font-size: 10.5px;
  color: var(--text-3);
  opacity: 0.82;
}
</style>
