<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue'
import GsapPresence from '@/components/GsapPresence.vue'
import Icon from '@/components/Icon.vue'
import ThemedSelect from '@/components/ThemedSelect.vue'
import SystemAppPicker from './SystemAppPicker.vue'
import { captureFlipSnapshot, playFlipSnapshot } from '@/lib/gsapMotion'
import { activeMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
import { isDesktop } from '@/lib/desktop/desktop'
import type { HubApp } from '@/lib/desktop/types'
import {
  acceptDiscoveredApp,
  addAppByTarget,
  addAppFromPicker,
  auditApplications,
  deleteApp,
  dismissAllDiscoveredApps,
  dismissDiscoveredApp,
  iconFor,
  importShortcuts,
  refreshDiscoveredApps,
  removeBrokenApplications,
  reorderVisibleApps,
  restoreDismissedApp,
  saveAppCategory,
  scanDiscoveredApps,
  setAppCategory,
  state,
  toggleAppHidden,
  updateApp
} from '@/store'

// null 代表「全部」，'__broken__' 代表「仅看失效应用」，'all' 沿用未分类标记。
const filter = ref<string | null>(null)
const query = ref('')
const categories = computed(() => state.categories.filter(item => item.id !== 'all'))
const categoryKey = (app: HubApp) => categories.value.some(item => item.id === app.categoryId) ? app.categoryId : 'all'
const categoryName = (id: string) => {
  if (id === '__broken__') return '失效应用'
  return categories.value.find(item => item.id === id)?.name ?? '未分类'
}
const brokenApps = computed(() => state.apps.filter(app => Boolean(app.broken)))
const filters = computed(() => [
  { id: null, name: '全部', count: state.apps.length },
  { id: 'all', name: '未分类', count: state.apps.filter(app => categoryKey(app) === 'all').length },
  ...categories.value.map(cat => ({ ...cat, count: state.apps.filter(app => categoryKey(app) === cat.id).length })),
  ...(brokenApps.value.length ? [{ id: '__broken__', name: '失效应用', count: brokenApps.value.length }] : [])
])
const filterName = computed(() => filter.value === null ? '全部应用' : categoryName(filter.value))
// 有效归属分类 ID：当处于「全部」或「失效应用」哨兵视图时统一回落到 'all'（未分类），绝不把 '__broken__' 传给后端或 Picker。
const activeCategoryId = computed(() => (filter.value === null || filter.value === '__broken__' ? 'all' : filter.value))
const categorySelectOptions = computed(() => [
  { value: 'all', label: '未分类' },
  ...categories.value.map(cat => ({ value: cat.id, label: cat.name }))
])
const visible = computed(() => {
  const term = query.value.trim().toLocaleLowerCase()
  return state.apps.filter(app => {
    const matchFilter = filter.value === null
      ? true
      : filter.value === '__broken__'
        ? Boolean(app.broken)
        : categoryKey(app) === filter.value
    return matchFilter && (!term || `${app.name}\n${app.target}\n${categoryName(categoryKey(app))}`.toLocaleLowerCase().includes(term))
  })
})
watch(() => brokenApps.value.length, count => {
  if (count === 0 && filter.value === '__broken__') filter.value = null
})

/* ---------- 新安装应用待分类引导（LG-01）与失效项一键清理（LG-04） ---------- */
const discoveryExpanded = ref(true)
const showDismissed = ref(false)
const discoveryChoices = ref<Record<string, string>>({})
const discoveryBusyTarget = ref('')
const confirmCleanBroken = ref(false)
const cleaningBroken = ref(false)

watch(
  () => state.discovery.pending,
  items => {
    const next: Record<string, string> = { ...discoveryChoices.value }
    for (const item of items) {
      if (!next[item.target]) {
        const valid = item.suggestedCategoryId === 'all' || categories.value.some(c => c.id === item.suggestedCategoryId)
        next[item.target] = valid ? item.suggestedCategoryId : (categories.value[0]?.id ?? 'all')
      }
    }
    discoveryChoices.value = next
  },
  { immediate: true, deep: true }
)

function formatDiscoveredTime(ms: number): string {
  if (!ms) return '刚刚发现'
  const date = new Date(ms)
  const now = new Date()
  const hh = String(date.getHours()).padStart(2, '0')
  const mm = String(date.getMinutes()).padStart(2, '0')
  if (date.toDateString() === now.toDateString()) return `今天 ${hh}:${mm}`
  return `${date.getMonth() + 1}/${date.getDate()} ${hh}:${mm}`
}

async function handleAcceptDiscovered(target: string, name: string): Promise<void> {
  if (discoveryBusyTarget.value) return
  discoveryBusyTarget.value = target
  try {
    const catId = discoveryChoices.value[target] || categories.value[0]?.id || 'all'
    await acceptDiscoveredApp(target, name, catId)
  } finally {
    discoveryBusyTarget.value = ''
  }
}

async function handleDismissDiscovered(target: string): Promise<void> {
  if (discoveryBusyTarget.value) return
  discoveryBusyTarget.value = target
  try {
    await dismissDiscoveredApp(target)
  } finally {
    discoveryBusyTarget.value = ''
  }
}

async function handleCleanBroken(): Promise<void> {
  if (cleaningBroken.value) return
  if (!confirmCleanBroken.value) {
    confirmCleanBroken.value = true
    return
  }
  confirmCleanBroken.value = false
  cleaningBroken.value = true
  try {
    const removed = await removeBrokenApplications()
    if (removed > 0) notify(true, `已清理 ${removed} 个失效应用`)
  } finally {
    cleaningBroken.value = false
  }
}

if (isDesktop) {
  void refreshDiscoveredApps()
  void auditApplications()
}
const groups = computed(() => [...categories.value, { id: 'all', name: '未分类' }]
  .map(cat => ({ ...cat, apps: visible.value.filter(app => categoryKey(app) === cat.id) }))
  .filter(group => group.apps.length))
const groupIds = (app: HubApp) => visible.value.filter(item => categoryKey(item) === categoryKey(app)).map(item => item.id)
const neighbor = (app: HubApp, delta: number) => { const ids = groupIds(app); return ids[ids.indexOf(app.id) + delta] }
const selected = ref<string | null>(null)
const dragged = ref<string | null>(null)
const dropTarget = ref<string | null>(null)
const busy = ref(false)
const feedback = ref('')
const failed = ref(false)
const adding = ref(false)
const showSystemApps = ref(false)
const systemAppsTrigger = ref<HTMLButtonElement | null>(null)
watch(adding, open => { if (!open) showSystemApps.value = false })
function closeSystemApps() {
  showSystemApps.value = false
  void nextTick(() => systemAppsTrigger.value?.focus({ preventScroll: true }))
}
function systemAppsSaved(added: number, skipped: number) {
  closeSystemApps()
  notify(true, `已导入 ${added} 个系统应用${skipped ? `，跳过 ${skipped} 个已添加项` : ''}`)
}
const managing = ref(false)
const linkName = ref('')
const linkTarget = ref('')
const newCategory = ref('')
const categoryDrafts = ref<Record<string, string>>({})
const confirmCategory = ref<string | null>(null)
const confirmApp = ref<string | null>(null)
const editId = ref<string | null>(null)
const editName = ref('')
const editTarget = ref('')
const root = ref<HTMLElement | null>(null)
const emit = defineEmits<{ 'dirty-change': [value: boolean] }>()
const draftPending = computed(() => busy.value || editId.value !== null || Boolean(linkName.value || linkTarget.value || newCategory.value) || categories.value.some(cat => cat.id in categoryDrafts.value && categoryDrafts.value[cat.id] !== cat.name))
watch(draftPending, value => emit('dirty-change', value), { immediate: true })

watch(categories, (value, previous) => {
  if (filter.value && filter.value !== 'all' && filter.value !== '__broken__' && !value.some(cat => cat.id === filter.value)) filter.value = null
  for (const cat of value) if (!(cat.id in categoryDrafts.value) || categoryDrafts.value[cat.id] === previous?.find(old => old.id === cat.id)?.name) categoryDrafts.value[cat.id] = cat.name
}, { immediate: true })
watch(visible, apps => {
  if (selected.value && !apps.some(app => app.id === selected.value)) { selected.value = null; editId.value = null }
})
function selectFilter(id: string | null): void { filter.value = id; selected.value = null; editId.value = null }
function notify(ok: boolean, message: string): void {
  failed.value = !ok
  feedback.value = ok ? `${message}${isDesktop ? '' : '（网页模拟，未落盘）'}` : '操作未保存，请查看错误提示后重试。'
}
async function action(run: () => Promise<boolean>, message: string): Promise<boolean> {
  if (busy.value) return false
  busy.value = true; feedback.value = '正在保存…'; failed.value = false
  try { const ok = await run(); notify(ok, message); return ok }
  catch (error) { failed.value = true; feedback.value = `操作失败：${String(error)}`; return false }
  finally { busy.value = false }
}
async function changeCategory(id: string, value: string): Promise<void> {
  const ok = await action(() => setAppCategory(id, value), `已移到「${categoryName(value)}」`)
  if (ok && selected.value === null) await nextTick(() => root.value?.querySelector<HTMLInputElement>('#app-search')?.focus())
}
async function runAppsFlip(mutate: () => void | Promise<void>): Promise<void> {
  const snap = captureFlipSnapshot(root.value, '.managed-app', state.ui.islandReducedMotion, 48)
  await mutate()
  await nextTick()
  playFlipSnapshot(snap, root.value, '.managed-app', activeMotionPreset.value.pill, { stagger: 0.01, maxItems: 48 })
}

watch([filter, query, () => state.apps.map(a => `${a.id}:${a.categoryId}`).join('|')], () => {
  const snap = captureFlipSnapshot(root.value, '.managed-app', state.ui.islandReducedMotion, 48)
  if (!snap) return
  void nextTick(() => {
    playFlipSnapshot(snap, root.value, '.managed-app', activeMotionPreset.value.pill, { stagger: 0.01, maxItems: 48 })
  })
}, { flush: 'pre' })

async function reorder(app: HubApp, target: string | undefined): Promise<void> {
  if (!target) return
  let ok = false
  await runAppsFlip(async () => {
    ok = await action(() => reorderVisibleApps(groupIds(app), app.id, target), '组内顺序已更新')
  })
  if (ok) {
    await nextTick(() => root.value?.querySelector<HTMLButtonElement>('.managed-app.selected .app-summary')?.focus({ preventScroll: true }))
  }
}
function beginDrag(event: DragEvent, app: HubApp): void {
  if (busy.value) { event.preventDefault(); return }
  dragged.value = app.id; event.dataTransfer?.setData('text/plain', app.id)
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}
function endDrag(): void { dragged.value = null; dropTarget.value = null }
function dropOnApp(app: HubApp): void {
  const from = state.apps.find(item => item.id === dragged.value)
  if (from && categoryKey(from) === categoryKey(app)) void reorder(from, app.id)
  else if (from) { failed.value = false; feedback.value = '跨分类移动请拖到上方分类按钮，或修改应用归属。' }
  endDrag()
}
function dropOnCategory(id: string | null): void {
  if (id !== null && id !== '__broken__' && dragged.value) void changeCategory(dragged.value, id)
  endDrag()
}
async function pickApp(kind: 'file' | 'folder'): Promise<void> {
  if (busy.value) return
  const destination = activeCategoryId.value
  busy.value = true
  try { const result = await addAppFromPicker(destination, kind); if (result.ok) { adding.value = false; notify(true, '应用已添加') } }
  finally { busy.value = false }
}
async function importApps(): Promise<void> {
  if (busy.value) return
  busy.value = true
  try { await importShortcuts(activeCategoryId.value) } finally { busy.value = false }
}
async function addLink(): Promise<void> {
  if (!/^https?:\/\//i.test(linkTarget.value.trim())) { failed.value = true; feedback.value = '网址需以 https:// 或 http:// 开头。'; return }
  if (await action(async () => (await addAppByTarget(linkTarget.value, activeCategoryId.value, linkName.value || undefined)).ok, '网址已添加')) {
    linkName.value = ''; linkTarget.value = ''; adding.value = false
  }
}
async function addCategory(): Promise<void> {
  const name = newCategory.value.trim()
  if (!name) return
  if (await action(() => saveAppCategory(`c-${Date.now().toString(36)}`, name), '分类已添加')) newCategory.value = ''
}
async function removeCategory(id: string): Promise<void> {
  if (confirmCategory.value !== id) { confirmCategory.value = id; return }
  if (await action(() => saveAppCategory(id, null), '分类已删除，应用已保留在「未分类」')) confirmCategory.value = null
}
async function selectApp(app: HubApp): Promise<void> {
  selected.value = selected.value === app.id ? null : app.id
  editId.value = null; confirmApp.value = null
  if (selected.value) await nextTick(() => root.value?.querySelector('.app-inspector')?.scrollIntoView({ block: 'nearest' }))
}
async function startEdit(app: HubApp): Promise<void> {
  editId.value = app.id; editName.value = app.name; editTarget.value = app.target
  await nextTick(() => root.value?.querySelector<HTMLInputElement>('#app-edit-name')?.focus())
}
async function finishEdit(): Promise<void> {
  editId.value = null
  await nextTick(() => (root.value?.querySelector<HTMLButtonElement>('.managed-app.selected .app-summary') ?? root.value?.querySelector<HTMLInputElement>('#app-search'))?.focus({ preventScroll: true }))
}
async function submitEdit(): Promise<void> {
  const id = editId.value
  if (!id) return
  if (await action(async () => (await updateApp(id, { name: editName.value, target: editTarget.value })).ok, '应用信息已保存')) await finishEdit()
}
async function removeApp(id: string): Promise<void> {
  if (confirmApp.value !== id) { confirmApp.value = id; return }
  if (await action(() => deleteApp(id), '应用已移除，目标文件保留')) {
    confirmApp.value = null; selected.value = null
    await nextTick(() => root.value?.querySelector<HTMLInputElement>('#app-search')?.focus())
  }
}
</script>

<template>
  <div ref="root" class="panel app-management">
    <header class="page-heading">
      <div><h3 class="panel-title">应用管理</h3><p class="page-description">按分类整理，快速找到每个应用。</p></div>
      <div class="heading-actions">
        <button class="btn" type="button" :disabled="busy || state.discovery.scanning || !isDesktop" @click="scanDiscoveredApps(true)">
          {{ state.discovery.scanning ? '检查中…' : '检查新装应用' }}
        </button>
        <button class="btn btn-primary" :disabled="busy" :aria-expanded="adding" aria-controls="add-app-options" @click="adding = !adding">{{ adding ? '收起添加' : '＋ 添加应用' }}</button>
      </div>
    </header>

    <!-- 新安装应用待分类高亮引导卡片（仅增量新装，绝不倾倒系统预装） -->
    <section
      v-if="state.discovery.pending.length || state.discovery.dismissed.length"
      class="discovery-card"
      :class="{ 'has-pending': state.discovery.pending.length > 0 }"
      aria-label="新安装应用待分类引导"
    >
      <div class="discovery-head">
        <div class="discovery-title">
          <span class="discovery-dot" aria-hidden="true" />
          <strong v-if="state.discovery.pending.length">
            发现 {{ state.discovery.pending.length }} 个新安装的应用，选择分类一键加入
          </strong>
          <strong v-else>暂无待分类的新安装应用</strong>
        </div>
        <div class="discovery-head-actions">
          <button
            v-if="state.discovery.pending.length"
            type="button"
            class="btn btn-sm"
            :disabled="state.discovery.scanning"
            @click="dismissAllDiscoveredApps()"
          >全部忽略</button>
          <button
            v-if="state.discovery.pending.length"
            type="button"
            class="btn btn-sm"
            @click="discoveryExpanded = !discoveryExpanded"
          >{{ discoveryExpanded ? '折叠' : '展开' }}</button>
        </div>
      </div>

      <ul v-if="state.discovery.pending.length && discoveryExpanded" class="discovery-list">
        <li v-for="item in state.discovery.pending" :key="item.target" class="discovery-row">
          <img v-if="iconFor(item.target)" :src="iconFor(item.target)" alt="" class="discovery-icon" />
          <span v-else class="app-initials discovery-icon" aria-hidden="true">{{ item.name.slice(0, 1).toUpperCase() }}</span>
          <div class="discovery-info">
            <span class="discovery-name" :title="item.name">{{ item.name }}</span>
            <small class="discovery-meta" :title="item.target">{{ formatDiscoveredTime(item.discoveredAtMs) }}</small>
          </div>
          <div class="discovery-select">
            <ThemedSelect
              :model-value="discoveryChoices[item.target] ?? 'all'"
              :options="categorySelectOptions"
              size="sm"
              :disabled="discoveryBusyTarget === item.target"
              :aria-label="`为 ${item.name} 选择分类`"
              @update:model-value="val => { discoveryChoices[item.target] = String(val) }"
            />
          </div>
          <button
            type="button"
            class="btn btn-sm btn-primary"
            :disabled="discoveryBusyTarget === item.target"
            @click="handleAcceptDiscovered(item.target, item.name)"
          >加入分类</button>
          <button
            type="button"
            class="btn btn-sm"
            :disabled="discoveryBusyTarget === item.target"
            @click="handleDismissDiscovered(item.target)"
          >忽略</button>
        </li>
      </ul>

      <div v-if="state.discovery.dismissed.length" class="discovery-dismissed-wrap">
        <button type="button" class="dismissed-toggle" @click="showDismissed = !showDismissed">
          {{ showDismissed ? '收起' : '查看' }}已忽略的新应用（{{ state.discovery.dismissed.length }}）
        </button>
        <ul v-if="showDismissed" class="dismissed-list">
          <li v-for="item in state.discovery.dismissed" :key="item.target" class="dismissed-row">
            <span :title="item.target">{{ item.name }}</span>
            <button type="button" class="btn btn-sm" @click="restoreDismissedApp(item.target)">恢复到待分类</button>
          </li>
        </ul>
      </div>
    </section>

    <div class="app-tools">
      <label class="field search-field" for="app-search"><span class="field-label">搜索应用</span><span class="search-control"><Icon name="search" :size="16" /><input id="app-search" v-model="query" class="control" type="search" placeholder="名称、目标或分类" /><button v-if="query" class="search-clear" type="button" aria-label="清空搜索" @click="query = ''"><Icon name="close" :size="12" /></button></span></label>
      <button class="btn" :aria-expanded="managing" aria-controls="category-manager" @click="managing = !managing">管理分类</button>
    </div>
    <GsapPresence
      :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: -6 }"
      :animate="{ opacity: 1, y: 0 }"
      :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: -6 }"
      :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
    >
      <section
        v-if="adding"
        id="add-app-options"
        key="add-app-options"
        class="disclosure-body"
        aria-label="添加应用"
      >
        <p class="hint">添加到「{{ categoryName(activeCategoryId) }}」<span v-if="!isDesktop"> · 文件选择与桌面导入在桌面版可用</span></p>
        <div class="actions"><button class="btn" :disabled="busy || !isDesktop" @click="pickApp('file')">选择文件</button><button class="btn" :disabled="busy || !isDesktop" @click="pickApp('folder')">选择文件夹</button><button class="btn" :disabled="busy || !isDesktop" @click="importApps">导入桌面快捷方式</button>
          <button ref="systemAppsTrigger" class="btn" :aria-expanded="showSystemApps" :aria-controls="showSystemApps ? 'system-app-picker' : undefined" :disabled="busy || !isDesktop" @click="showSystemApps = !showSystemApps">导入系统应用</button>
        </div>
        <GsapPresence
          :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: -4 }"
          :animate="{ opacity: 1, y: 0 }"
          :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: -4 }"
          :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
        >
          <div
            v-if="showSystemApps"
            key="system-apps"
          >
            <SystemAppPicker :category-id="activeCategoryId" @saving="busy = $event" @saved="systemAppsSaved" @close="closeSystemApps" />
          </div>
        </GsapPresence>
        <form class="link-form" @submit.prevent="addLink">
          <label class="field"><span class="field-label">网址</span><input v-model="linkTarget" class="control" type="url" required placeholder="https://example.com" /></label>
          <label class="field"><span class="field-label">显示名称（可选）</span><input v-model="linkName" class="control" /></label>
          <button class="btn" :disabled="busy || !linkTarget.trim()">添加网址</button>
        </form>
      </section>
    </GsapPresence>
    <GsapPresence
      :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: -6 }"
      :animate="{ opacity: 1, y: 0 }"
      :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: -6 }"
      :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
    >
      <section
        v-if="managing"
        id="category-manager"
        key="category-manager"
        class="disclosure-body"
        aria-label="分类管理"
      >
        <p class="hint">删除分类会把应用移到「未分类」，不会删除应用。</p>
        <div v-for="cat in categories" :key="cat.id" class="category-editor">
          <label class="field"><span class="field-label">分类名称 · {{ cat.name }}</span><input v-model="categoryDrafts[cat.id]" class="control" maxlength="40" :aria-label="`${cat.name} 分类名称`" @keydown.enter.prevent="action(() => saveAppCategory(cat.id, categoryDrafts[cat.id]), '分类名称已保存')" /></label>
          <button class="btn" :disabled="busy || !categoryDrafts[cat.id]?.trim() || categoryDrafts[cat.id] === cat.name" @click="action(() => saveAppCategory(cat.id, categoryDrafts[cat.id]), '分类名称已保存')">保存</button>
          <button class="btn" :class="{ 'btn-danger': confirmCategory === cat.id }" :disabled="busy" :aria-label="`${confirmCategory === cat.id ? '确认删除' : '删除'}分类 ${cat.name}`" @click="removeCategory(cat.id)">{{ confirmCategory === cat.id ? '确认删除' : '删除' }}</button>
          <button v-if="confirmCategory === cat.id" class="btn" @click="confirmCategory = null">取消</button>
        </div>
        <form class="category-editor" @submit.prevent="addCategory"><label class="field"><span class="field-label">新分类名称</span><input v-model="newCategory" class="control" maxlength="40" /></label><button class="btn" :disabled="busy || !newCategory.trim()">添加分类</button></form>
      </section>
    </GsapPresence>
    <div class="category-filters" role="group" aria-label="应用分类筛选">
      <button v-for="cat in filters" :key="cat.id ?? '__everything'" class="category-filter" :class="{ selected: filter === cat.id, 'drop-target': cat.id !== null && cat.id !== '__broken__' && dropTarget === `category:${cat.id}` }"
        :aria-pressed="filter === cat.id" :title="cat.name" @click="selectFilter(cat.id)"
        @dragover.prevent="cat.id !== null && cat.id !== '__broken__' && dragged && (dropTarget = `category:${cat.id}`)" @dragleave="dropTarget = null" @drop.prevent="dropOnCategory(cat.id)">
        <span>{{ cat.name }}</span><span class="count">{{ cat.count }}</span>
      </button>
    </div>
    <div class="result-summary">
      <p role="status">{{ filterName }}<span> · {{ visible.length }} 个{{ query.trim() ? '结果' : '应用' }}</span></p>
      <div class="summary-right">
        <template v-if="brokenApps.length">
          <button
            type="button"
            class="btn btn-sm btn-danger"
            :class="{ 'is-confirming': confirmCleanBroken }"
            :disabled="cleaningBroken"
            @click="handleCleanBroken"
          >{{ confirmCleanBroken ? `确认清理 ${brokenApps.length} 个失效项` : `一键清理失效项 (${brokenApps.length})` }}</button>
          <button v-if="confirmCleanBroken" type="button" class="btn btn-sm" @click="confirmCleanBroken = false">取消</button>
        </template>
        <span class="hint">含隐藏应用</span>
      </div>
    </div>
    <p v-if="feedback" class="inline-feedback" :class="{ error: failed }" role="status">{{ feedback }}</p>
    <div v-if="!visible.length" class="empty-state" role="status"><strong>{{ query.trim() ? '没有匹配的应用' : '这个分类还没有应用' }}</strong><p>{{ query.trim() ? '试试其他名称、目标或分类。' : '添加应用，或将已有应用移到这里。' }}</p><button v-if="query.trim()" class="btn" @click="query = ''">清空搜索</button><button v-else class="btn" @click="adding = true">添加应用</button></div>
    <section v-for="group in groups" :key="group.id" class="app-group" :aria-label="group.name">
      <h4 class="app-group-heading"><span>{{ group.name }}</span><span class="count">{{ group.apps.length }}</span></h4>
      <ul class="managed-apps">
        <li v-for="app in group.apps" :key="app.id" class="managed-app" :class="{ selected: selected === app.id }">
          <button class="app-summary" :class="{ 'drop-target': dropTarget === app.id }" :draggable="!busy" :aria-expanded="selected === app.id" :aria-controls="`details-${app.id}`" :title="`${app.name} · ${categoryName(categoryKey(app))}`"
            @click="selectApp(app)" @dragstart="beginDrag($event, app)" @dragend="endDrag" @dragover.prevent="dragged && (dropTarget = app.id)" @dragleave="dropTarget = null" @drop.prevent="dropOnApp(app)">
            <img v-if="iconFor(app.target)" :src="iconFor(app.target)" alt="" draggable="false" /><span v-else class="app-initials" aria-hidden="true">{{ app.initials }}</span>
            <span class="app-summary-text"><span class="managed-name">{{ app.name }}</span><span class="app-membership">{{ categoryName(categoryKey(app)) }}</span></span>
            <span class="app-status">{{ app.hidden ? '已隐藏' : '显示中' }}<span v-if="app.broken" class="error"> · 目标失效</span></span><Icon name="chevron-down" :size="14" :class="{ flipped: selected === app.id }" />
          </button>
          <GsapPresence
            :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: -4 }"
            :animate="{ opacity: 1, y: 0 }"
            :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: -4 }"
            :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
          >
            <div
              v-if="selected === app.id"
              :id="`details-${app.id}`"
              :key="`details-${app.id}`"
              class="app-inspector"
            >
              <template v-if="editId !== app.id">
                <p class="full-app-name">{{ app.name }}</p><p class="app-path">{{ app.target }}</p>
                <div class="field">
                  <span class="field-label">所属分类</span>
                  <ThemedSelect
                    :model-value="categoryKey(app)"
                    :options="categorySelectOptions"
                    :disabled="busy"
                    aria-label="所属分类"
                    @change="val => changeCategory(app.id, String(val))"
                  />
                </div>
                <div class="actions app-actions">
                  <button class="btn" :disabled="busy" @click="action(() => toggleAppHidden(app.id), app.hidden ? '应用已显示' : '应用已隐藏')">{{ app.hidden ? '显示应用' : '隐藏应用' }}</button>
                  <button class="btn" :disabled="busy" @click="startEdit(app)">编辑</button>
                  <button class="btn" :disabled="busy || !neighbor(app, -1)" :aria-label="`上移 ${app.name}`" @click="reorder(app, neighbor(app, -1))">上移</button>
                  <button class="btn" :disabled="busy || !neighbor(app, 1)" :aria-label="`下移 ${app.name}`" @click="reorder(app, neighbor(app, 1))">下移</button>
                  <button class="btn btn-danger" :disabled="busy" @click="removeApp(app.id)">{{ confirmApp === app.id ? '确认移除应用' : '移除' }}</button><button v-if="confirmApp === app.id" class="btn" @click="confirmApp = null">取消</button>
                </div><p class="hint">排序仅调整当前分类中的可见结果；移除应用不会删除目标文件。</p>
              </template>
              <form v-else class="app-edit-form" @submit.prevent="submitEdit" @keydown.escape.stop.prevent="finishEdit">
                <label class="field" for="app-edit-name"><span class="field-label">显示名称</span><input id="app-edit-name" v-model="editName" class="control" required /></label>
                <label class="field"><span class="field-label">目标 · 文件、文件夹或网址</span><input v-model="editTarget" class="control" required /></label>
                <div class="actions"><button class="btn btn-primary" :disabled="busy || !editName.trim() || !editTarget.trim()">{{ busy ? '保存中…' : '保存修改' }}</button><button type="button" class="btn" :disabled="busy" @click="finishEdit">取消</button></div>
              </form>
            </div>
          </GsapPresence>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped>
.heading-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.discovery-card {
  margin: 16px 0 4px;
  padding: 12px 14px;
  border-radius: 10px;
  border: 1px solid var(--line-strong);
  background: var(--surface-2);
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.discovery-card.has-pending {
  border-color: color-mix(in srgb, var(--accent) 55%, var(--line-strong));
  background: color-mix(in srgb, var(--accent-weak) 45%, var(--surface-2));
  box-shadow: inset 3px 0 0 var(--accent);
}
.discovery-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
.discovery-title { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; }
.discovery-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--accent); flex: none; }
.discovery-card:not(.has-pending) .discovery-dot { background: var(--text-3); }
.discovery-head-actions { display: inline-flex; align-items: center; gap: 6px; }
.discovery-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.discovery-row {
  display: flex; align-items: center; gap: 10px; padding: 8px 10px;
  border-radius: 8px; border: 1px solid var(--line); background: var(--surface);
  flex-wrap: wrap;
}
.discovery-icon { width: 26px; height: 26px; flex: none; object-fit: contain; font-size: 12px; }
.discovery-info { flex: 1 1 130px; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.discovery-name { font-size: 13px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.discovery-meta { font-size: 11px; color: var(--text-3); }
.discovery-select { width: 118px; flex: none; }
.discovery-dismissed-wrap { border-top: 1px dashed var(--line); padding-top: 8px; display: flex; flex-direction: column; gap: 6px; }
.dismissed-toggle { align-self: flex-start; font-size: 12px; color: var(--text-2); padding: 2px 4px; border-radius: 5px; }
.dismissed-toggle:hover { color: var(--text); background: var(--hover); }
.dismissed-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
.dismissed-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 12px; color: var(--text-2); padding: 4px 6px; border-radius: 6px; background: var(--surface); }
.summary-right { display: inline-flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.app-tools { display: flex; align-items: end; gap: 12px; margin: 20px 0 16px; }
.search-field { flex: 1; }
.search-control { position: relative; display: block; }
.search-control > svg { position: absolute; left: 12px; top: 10px; color: var(--text-3); pointer-events: none; }
.search-control .control { padding-left: 36px; padding-right: 30px; }
.search-clear { position: absolute; right: 7px; top: 50%; transform: translateY(-50%); display: grid; place-items: center; width: 22px; height: 22px; border-radius: 50%; color: var(--text-3); }
.search-clear:hover { background: var(--hover); color: var(--text); }
.category-filters { display: flex; flex-wrap: wrap; gap: 8px; }
.category-filter { display: flex; align-items: center; gap: 8px; max-width: 100%; min-height: 36px; padding: 6px 12px; border: 1px solid var(--line-strong); background: var(--surface-2); border-radius: 9px; color: var(--text-2); transition: background 120ms ease, border-color 120ms ease, color 120ms ease; }
.category-filter > span:first-child { overflow-wrap: anywhere; text-align: left; }
.category-filter:hover:not(.selected) { background: var(--surface-3); border-color: var(--text-3); color: var(--text); }
.category-filter.selected { background: var(--text); color: var(--surface); border-color: var(--text); font-weight: 600; }
.category-filter.selected .count { color: inherit; background: transparent; }
.result-summary { display: flex; justify-content: space-between; align-items: baseline; flex-wrap: wrap; gap: 8px; margin: 20px 0 8px; }
.result-summary p { margin: 0; font-weight: 600; overflow-wrap: anywhere; }
.result-summary p span { color: var(--text-2); font-weight: 400; }
.result-summary .hint { margin: 0; }
.app-group { margin: 16px 0 24px; }
.app-group-heading { margin: 0 0 8px; display: flex; align-items: baseline; gap: 8px; font-size: 13px; color: var(--text-2); overflow-wrap: anywhere; }
.managed-apps { list-style: none; padding: 0; margin: 0; }
.managed-app { border-bottom: 1px solid var(--line); }
.managed-app.selected { background: var(--surface-2); border-radius: 10px; box-shadow: inset 0 0 0 1px var(--line-strong); }
.app-summary { display: flex; align-items: center; gap: 12px; padding: 12px 10px; min-height: 64px; width: 100%; text-align: left; border-radius: 10px; transition: background 120ms ease; }
.app-summary:hover { background: var(--hover); }
.app-summary > img, .app-initials { flex: none; width: 32px; height: 32px; object-fit: contain; }
.app-initials { display: grid; place-items: center; background: var(--surface-3); color: var(--text); border: 1px solid var(--line); border-radius: 8px; font-size: 13px; }
.app-summary-text { flex: 1; min-width: 0; display: grid; gap: 2px; }
.managed-name { font-size: 14px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.app-membership { font-size: 12px; color: var(--text-2); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.app-status { flex: none; font-size: 12px; color: var(--text-2); }
.flipped { transform: rotate(180deg); }
.app-inspector { padding: 0 12px 16px; }
.full-app-name { margin: 0 0 4px; font-weight: 600; overflow-wrap: anywhere; }
.app-path { margin: 0 0 16px; overflow-wrap: anywhere; color: var(--text-2); font-size: 12px; user-select: text; }
.app-actions { margin: 12px 0; }
.app-edit-form { display: grid; gap: 12px; }
.drop-target { outline: 2px dashed var(--text); outline-offset: -2px; }
.link-form { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) auto; gap: 8px; align-items: end; margin-top: 16px; }
.category-editor { display: flex; align-items: end; gap: 8px; margin-top: 12px; flex-wrap: wrap; }
.category-editor .field { flex: 1; min-width: 140px; }
@container settings (max-width: 580px) {
  .link-form { grid-template-columns: 1fr; }
  .app-status { max-width: 70px; text-align: right; }
}
</style>
