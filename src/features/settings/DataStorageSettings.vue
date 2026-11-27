<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import GsapPresence from '@/components/GsapPresence.vue'
import { activeMotionPreset, INSTANT_TRANSITION } from '@/lib/motionPresets'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import { state } from '@/store'

const props = defineProps<{ blocked: boolean }>()

export interface DataDirSummary {
  exists: boolean
  hasMeaningfulData: boolean
  appCount: number
  categoryCount: number
  clipCount: number
  clipImageCount: number
  snippetCount: number
  mailAccountCount: number
  shelfCount: number
  audioProfileCount: number
  hasTranslationConfig: boolean
  hasUiPreferences: boolean
  hasShortcuts: boolean
  iconCacheCount: number
  totalFiles: number
  totalBytes: number
  corruptedFiles: string[]
}

export interface CandidateOldDir {
  path: string
  label: string
  summary: DataDirSummary
}

export interface BackupRecord {
  path: string
  name: string
  kind: 'config' | 'migration' | string
  modifiedMs: number
  totalFiles: number
  totalBytes: number
}

export interface DataStorageStatus {
  currentDir: string
  defaultSharedDir: string
  mode: 'shared' | 'legacy_portable' | 'isolated' | 'env' | string
  pendingRestart: boolean
  autoMigratedFrom?: string | null
  autoMigratedFiles?: number
  currentSummary: DataDirSummary
  candidates: CandidateOldDir[]
  backups: BackupRecord[]
}

export interface QuickDataPrompt {
  kind: 'legacy' | 'empty-with-candidates'
  title: string
  detail: string
  buttonLabel: string
  targetOrSourcePath: string
}

export interface SwitchDataDirPreview {
  currentDir: string
  targetDir: string
  targetExists: boolean
  targetHasData: boolean
  currentSummary: DataDirSummary
  targetSummary: DataDirSummary
}

export interface ImportOldDataPreview {
  sourceDir: string
  currentDir: string
  sourceSummary: DataDirSummary
  currentSummary: DataDirSummary
}

const status = ref<DataStorageStatus | null>(null)
const loading = ref(false)
const busy = ref<'open' | 'pick-switch' | 'pick-import' | 'apply' | 'restart' | 'delete' | null>(null)
const feedback = ref('')
const failed = ref(false)
const restartInfo = ref<{ message: string; backupDir?: string | null } | null>(null)
const confirmDeleteOldPath = ref<string | null>(null)
const confirmDeleteBackupPath = ref<string | null>(null)
const confirmClearAllBackups = ref(false)

const switchPreview = ref<SwitchDataDirPreview | null>(null)
const importPreview = ref<ImportOldDataPreview | null>(null)
const previewHeading = ref<HTMLElement | null>(null)
const previewBox = ref<HTMLElement | null>(null)
const rootSection = ref<HTMLElement | null>(null)
const emit = defineEmits<{ 'quick-prompt': [value: QuickDataPrompt | null] }>()

const MODE_LABELS: Record<string, string> = {
  shared: '跨版本共享（更新不丢数据）',
  legacy_portable: '旧版随包目录',
  isolated: '便携隔离模式',
  env: '环境变量指定'
}

const modeLabel = computed(() => MODE_LABELS[status.value?.mode ?? ''] ?? '本机目录')
const showLegacyUpgradeBanner = computed(() => {
  const s = status.value
  if (!s || s.mode !== 'legacy_portable') return false
  return s.currentDir.trim().toLowerCase() !== s.defaultSharedDir.trim().toLowerCase()
})
const cleanCandidates = computed(() =>
  (status.value?.candidates ?? []).filter(item => item.summary.corruptedFiles.length === 0)
)
const bestCandidate = computed(() => cleanCandidates.value[0] ?? status.value?.candidates[0] ?? null)
const hasPendingRestart = computed(() => Boolean(restartInfo.value || status.value?.pendingRestart))
const showEmptyCandidateBanner = computed(() => {
  const s = status.value
  if (!s || s.mode !== 'shared' || s.autoMigratedFrom || hasPendingRestart.value) return false
  const nearlyEmpty = !s.currentSummary.hasMeaningfulData ||
    (s.currentSummary.appCount <= 8 && s.currentSummary.clipCount === 0 && s.currentSummary.snippetCount === 0)
  return nearlyEmpty && cleanCandidates.value.length > 0
})
const quickPrompt = computed<QuickDataPrompt | null>(() => {
  const s = status.value
  if (!s || hasPendingRestart.value) return null
  if (showLegacyUpgradeBanner.value) {
    return {
      kind: 'legacy',
      title: '当前仍在使用旧版随包目录',
      detail: '建议迁移至固定共享目录，以后更新或打开新包都不会丢数据。',
      buttonLabel: `迁移到 ${s.defaultSharedDir}`,
      targetOrSourcePath: s.defaultSharedDir
    }
  }
  if (showEmptyCandidateBanner.value && cleanCandidates.value[0]) {
    const best = cleanCandidates.value[0]
    return {
      kind: 'empty-with-candidates',
      title: '本机发现旧版完整数据',
      detail: `来自「${best.label}」（${best.summary.totalFiles} 个文件），可一键预览并搬入当前共享目录。`,
      buttonLabel: '立即查看并搬入',
      targetOrSourcePath: best.path
    }
  }
  return null
})
watch(quickPrompt, value => emit('quick-prompt', value), { immediate: true })
const switchHasCorruption = computed(() => {
  const p = switchPreview.value
  if (!p) return false
  return p.currentSummary.corruptedFiles.length > 0 || p.targetSummary.corruptedFiles.length > 0
})
const importHasCorruption = computed(() => {
  const p = importPreview.value
  if (!p) return false
  return p.sourceSummary.corruptedFiles.length > 0
})

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`
}

function buildDemoStatus(): DataStorageStatus {
  return {
    currentDir: 'D:\\apply\\DesktopHub-Lite\\data',
    defaultSharedDir: 'D:\\DesktopHub-Data',
    mode: 'legacy_portable',
    pendingRestart: false,
    currentSummary: {
      exists: true,
      hasMeaningfulData: true,
      appCount: state.apps.length,
      categoryCount: state.categories.length,
      clipCount: state.clips.length,
      clipImageCount: 2,
      snippetCount: state.snippets.length,
      mailAccountCount: 1,
      shelfCount: 1,
      audioProfileCount: 1,
      hasTranslationConfig: true,
      hasUiPreferences: true,
      hasShortcuts: true,
      iconCacheCount: 18,
      totalFiles: 30,
      totalBytes: 245760,
      corruptedFiles: []
    },
    candidates: [
      {
        path: 'D:\\codex\\DesktopHub-work\\packages\\DesktopHub-Lite-0.1.3-win-x64-old\\data',
        label: '历史版本（示例旧版）',
        summary: {
          exists: true,
          hasMeaningfulData: true,
          appCount: 14,
          categoryCount: 5,
          clipCount: 86,
          clipImageCount: 10,
          snippetCount: 7,
          mailAccountCount: 2,
          shelfCount: 3,
          audioProfileCount: 2,
          hasTranslationConfig: true,
          hasUiPreferences: true,
          hasShortcuts: true,
          iconCacheCount: 42,
          totalFiles: 62,
          totalBytes: 1548000,
          corruptedFiles: []
        }
      }
    ],
    backups: [
      {
        path: 'D:\\DesktopHub-Data\\config-backups\\before-import-demo.json',
        name: 'before-import-demo.json',
        kind: 'config',
        modifiedMs: Date.now() - 86400_000,
        totalFiles: 1,
        totalBytes: 18432
      }
    ]
  }
}

async function loadStatus(): Promise<void> {
  if (!isDesktop) {
    status.value = buildDemoStatus()
    return
  }
  loading.value = true
  try {
    status.value = await invokeHost<DataStorageStatus>('get_data_storage_status')
  } catch (error) {
    failed.value = true
    feedback.value = `无法读取数据目录状态：${String(error)}`
  } finally {
    loading.value = false
  }
}

async function focusPreview(): Promise<void> {
  await nextTick()
  previewBox.value?.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
  previewHeading.value?.focus({ preventScroll: true })
}

function clearPreviews(): void {
  switchPreview.value = null
  importPreview.value = null
  confirmDeleteOldPath.value = null
  confirmDeleteBackupPath.value = null
  confirmClearAllBackups.value = false
}

function formatBackupTime(ms: number): string {
  if (!ms) return '未知时间'
  const date = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

async function confirmDeleteOldDir(targetDir: string): Promise<void> {
  if (busy.value || hasPendingRestart.value) return
  busy.value = 'delete'
  feedback.value = ''
  failed.value = false
  try {
    if (!isDesktop) {
      if (status.value) {
        status.value.candidates = status.value.candidates.filter(item => item.path !== targetDir)
      }
      feedback.value = '演示：已从列表并模拟清理该历史配置目录。'
    } else {
      const removed = await invokeHost<number>('delete_old_data_dir', { targetDir })
      await loadStatus()
      feedback.value = `已从磁盘清理 ${removed} 个旧配置文件，并移除该历史记录。`
    }
    confirmDeleteOldPath.value = null
  } catch (error) {
    failed.value = true
    feedback.value = `删除旧配置失败：${String(error)}`
  } finally {
    busy.value = null
  }
}

async function confirmDeleteBackup(backupPath: string): Promise<void> {
  if (busy.value) return
  busy.value = 'delete'
  feedback.value = ''
  failed.value = false
  try {
    if (!isDesktop) {
      if (status.value) {
        status.value.backups = status.value.backups.filter(item => item.path !== backupPath)
      }
      feedback.value = '演示：已移除该历史备份记录。'
    } else {
      await invokeHost<number>('delete_data_backup', { backupPath, clearAll: false })
      await loadStatus()
      feedback.value = '已删除该历史备份。'
    }
    confirmDeleteBackupPath.value = null
  } catch (error) {
    failed.value = true
    feedback.value = `删除备份失败：${String(error)}`
  } finally {
    busy.value = null
  }
}

async function confirmClearBackups(): Promise<void> {
  if (busy.value) return
  busy.value = 'delete'
  feedback.value = ''
  failed.value = false
  try {
    if (!isDesktop) {
      if (status.value) status.value.backups = []
      feedback.value = '演示：已清空全部历史备份。'
    } else {
      const removed = await invokeHost<number>('delete_data_backup', { backupPath: null, clearAll: true })
      await loadStatus()
      feedback.value = `已清空全部历史备份（共删除 ${removed} 个文件）。`
    }
    confirmClearAllBackups.value = false
    confirmDeleteBackupPath.value = null
  } catch (error) {
    failed.value = true
    feedback.value = `清空历史备份失败：${String(error)}`
  } finally {
    busy.value = null
  }
}

async function openCurrentDir(): Promise<void> {
  if (busy.value || !isDesktop) return
  busy.value = 'open'
  failed.value = false
  try {
    await invokeHost('open_current_data_dir')
  } catch (error) {
    failed.value = true
    feedback.value = String(error)
  } finally {
    busy.value = null
  }
}

async function startPickSwitch(): Promise<void> {
  if (busy.value || hasPendingRestart.value) return
  clearPreviews()
  feedback.value = ''
  failed.value = false
  if (!isDesktop) {
    switchPreview.value = {
      currentDir: status.value?.currentDir ?? 'D:\\apply\\DesktopHub-Lite\\data',
      targetDir: 'D:\\DesktopHub-Data',
      targetExists: false,
      targetHasData: false,
      currentSummary: status.value?.currentSummary ?? buildDemoStatus().currentSummary,
      targetSummary: {
        exists: false, hasMeaningfulData: false, appCount: 0, categoryCount: 0, clipCount: 0,
        clipImageCount: 0, snippetCount: 0, mailAccountCount: 0, shelfCount: 0, audioProfileCount: 0,
        hasTranslationConfig: false, hasUiPreferences: false, hasShortcuts: false, iconCacheCount: 0,
        totalFiles: 0, totalBytes: 0, corruptedFiles: []
      }
    }
    await focusPreview()
    return
  }
  busy.value = 'pick-switch'
  try {
    const result = await invokeHost<SwitchDataDirPreview | null>('pick_data_dir_for_switch')
    if (result) {
      switchPreview.value = result
      await focusPreview()
    } else {
      feedback.value = '已取消更改目录'
    }
  } catch (error) {
    failed.value = true
    feedback.value = String(error)
  } finally {
    busy.value = null
  }
}

async function previewSwitchTo(targetDir: string): Promise<void> {
  if (busy.value || hasPendingRestart.value || !targetDir) return
  clearPreviews()
  feedback.value = ''
  failed.value = false
  if (!isDesktop) {
    const demo = buildDemoStatus()
    const match = demo.candidates.find(c => c.path === targetDir)
    switchPreview.value = {
      currentDir: demo.currentDir,
      targetDir,
      targetExists: Boolean(match),
      targetHasData: Boolean(match),
      currentSummary: demo.currentSummary,
      targetSummary: match?.summary ?? {
        exists: false, hasMeaningfulData: false, appCount: 0, categoryCount: 0, clipCount: 0,
        clipImageCount: 0, snippetCount: 0, mailAccountCount: 0, shelfCount: 0, audioProfileCount: 0,
        hasTranslationConfig: false, hasUiPreferences: false, hasShortcuts: false, iconCacheCount: 0,
        totalFiles: 0, totalBytes: 0, corruptedFiles: []
      }
    }
    await focusPreview()
    return
  }
  busy.value = 'pick-switch'
  try {
    switchPreview.value = await invokeHost<SwitchDataDirPreview>('preview_switch_data_dir', { targetDir })
    await focusPreview()
  } catch (error) {
    failed.value = true
    feedback.value = String(error)
  } finally {
    busy.value = null
  }
}

async function confirmSwitch(strategy: 'copy_current' | 'use_existing'): Promise<void> {
  const preview = switchPreview.value
  if (!preview || busy.value || hasPendingRestart.value) return
  busy.value = 'apply'
  feedback.value = ''
  failed.value = false
  if (!isDesktop) {
    restartInfo.value = {
      message: `演示：已将共享目录设为 ${preview.targetDir}（策略：${strategy === 'copy_current' ? '复制当前数据' : '使用目标现有数据'}），网页预览不改写本机文件。`
    }
    switchPreview.value = null
    busy.value = null
    return
  }
  try {
    const res = await invokeHost<{ targetDir: string; strategy: string; copiedFiles: number; backupDir?: string | null }>('apply_switch_data_dir', {
      targetDir: preview.targetDir,
      strategy
    })
    switchPreview.value = null
    const desc = res.strategy === 'copy_current'
      ? `已将当前 ${res.copiedFiles} 个数据文件迁移并切换至 ${res.targetDir}`
      : `已直接切换共享目录至 ${res.targetDir}`
    restartInfo.value = { message: `${desc}，重启后生效。`, backupDir: res.backupDir }
    await loadStatus()
  } catch (error) {
    failed.value = true
    feedback.value = String(error)
  } finally {
    busy.value = null
  }
}

async function startPickImport(): Promise<void> {
  if (busy.value || hasPendingRestart.value) return
  clearPreviews()
  feedback.value = ''
  failed.value = false
  if (!isDesktop) {
    const first = buildDemoStatus().candidates[0]
    if (first) await previewImportFrom(first.path)
    return
  }
  busy.value = 'pick-import'
  try {
    const result = await invokeHost<ImportOldDataPreview | null>('pick_old_data_dir_for_import')
    if (result) {
      importPreview.value = result
      await focusPreview()
    } else {
      feedback.value = '已取消选择旧版目录'
    }
  } catch (error) {
    failed.value = true
    feedback.value = String(error)
  } finally {
    busy.value = null
  }
}

async function previewImportFrom(sourceDir: string): Promise<void> {
  if (busy.value || hasPendingRestart.value || !sourceDir) return
  clearPreviews()
  feedback.value = ''
  failed.value = false
  if (!isDesktop) {
    const demo = buildDemoStatus()
    const match = demo.candidates.find(c => c.path === sourceDir) ?? demo.candidates[0]
    importPreview.value = {
      sourceDir: match.path,
      currentDir: demo.currentDir,
      sourceSummary: match.summary,
      currentSummary: demo.currentSummary
    }
    await focusPreview()
    return
  }
  busy.value = 'pick-import'
  try {
    importPreview.value = await invokeHost<ImportOldDataPreview>('preview_old_data_import', { sourceDir })
    await focusPreview()
  } catch (error) {
    failed.value = true
    feedback.value = String(error)
  } finally {
    busy.value = null
  }
}

async function confirmImport(): Promise<void> {
  const preview = importPreview.value
  if (!preview || busy.value || hasPendingRestart.value) return
  busy.value = 'apply'
  feedback.value = ''
  failed.value = false
  if (!isDesktop) {
    restartInfo.value = {
      message: `演示：已从旧版目录导入 ${preview.sourceSummary.totalFiles} 个文件（网页预览未改写本机文件）。`,
      backupDir: `${preview.currentDir}\\migration-backups\\demo`
    }
    importPreview.value = null
    busy.value = null
    return
  }
  try {
    const res = await invokeHost<{ sourceDir: string; copiedFiles: number; backupDir?: string | null }>('apply_old_data_import', {
      sourceDir: preview.sourceDir
    })
    importPreview.value = null
    restartInfo.value = {
      message: `已从旧版目录完整导入 ${res.copiedFiles} 个文件，重启后立即生效。`,
      backupDir: res.backupDir
    }
    await loadStatus()
  } catch (error) {
    failed.value = true
    feedback.value = String(error)
  } finally {
    busy.value = null
  }
}

async function triggerQuickMigration(): Promise<void> {
  rootSection.value?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  if (showLegacyUpgradeBanner.value && status.value) {
    await previewSwitchTo(status.value.defaultSharedDir)
  } else if (bestCandidate.value) {
    await previewImportFrom(bestCandidate.value.path)
  }
}

defineExpose({ triggerQuickMigration })

async function restartNow(): Promise<void> {
  if (busy.value || !isDesktop) {
    if (!isDesktop) feedback.value = '网页预览模式不支持自动重启程序。'
    return
  }
  busy.value = 'restart'
  failed.value = false
  try {
    await invokeHost('restart_for_data_change')
  } catch (error) {
    failed.value = true
    feedback.value = `无法重启程序：${String(error)}`
    busy.value = null
  }
}

onMounted(() => {
  void loadStatus()
})

watch(() => state.ui.settingsOpen, open => {
  if (open) void loadStatus()
  else clearPreviews()
})
</script>

<template>
  <section ref="rootSection" class="data-storage" aria-labelledby="data-storage-title" :aria-busy="loading || Boolean(busy)">
    <h4 id="data-storage-title" class="sub-title">
      数据目录与旧版迁移
      <span class="save-mode">{{ modeLabel }}</span>
    </h4>
    <p class="hint">
      新版默认把数据统一保存在跨版本共享目录，更新替换或解压新包自动继承配置与记录（凭据管理器密钥一并延续）。
    </p>
    <p v-if="!isDesktop" class="hint">网页演示仅展示模拟目录与迁移预览，不会读写本机文件。</p>

    <div v-if="status" class="storage-card">
      <div class="storage-head">
        <div class="storage-path-group">
          <span class="field-label">当前数据目录</span>
          <code class="data-path-box">{{ status.currentDir }}</code>
        </div>
        <button
          class="btn"
          type="button"
          :disabled="Boolean(busy) || !isDesktop"
          @click="openCurrentDir"
        >
          打开目录
        </button>
      </div>

      <p class="summary-line" aria-label="当前数据统计">
        应用 <b>{{ status.currentSummary.appCount }}</b> · 剪贴板 <b>{{ status.currentSummary.clipCount }}</b>（图 {{ status.currentSummary.clipImageCount }}）· 短语 <b>{{ status.currentSummary.snippetCount }}</b> · 邮箱 <b>{{ status.currentSummary.mailAccountCount }}</b> · 中转 <b>{{ status.currentSummary.shelfCount }}</b> · 共 {{ status.currentSummary.totalFiles }} 个文件（{{ formatBytes(status.currentSummary.totalBytes) }}）
      </p>

      <div v-if="status.autoMigratedFrom" class="auto-migrated-banner" role="status">
        <strong>本次启动已自动迁入旧版数据（共 {{ status.autoMigratedFiles ?? 0 }} 个文件）</strong>
        <p class="hint">来源：<code>{{ status.autoMigratedFrom }}</code>。以后更新或解压新版都会自动共享当前数据目录。</p>
      </div>

      <div v-else-if="showEmptyCandidateBanner && bestCandidate" class="upgrade-banner" role="region" aria-label="旧版数据搬入建议">
        <div class="upgrade-text">
          <strong>当前共享目录暂无历史数据，已发现本机旧版数据</strong>
          <p class="hint">来自「{{ bestCandidate.label }}」（应用 {{ bestCandidate.summary.appCount }} · 剪贴板 {{ bestCandidate.summary.clipCount }} · 共 {{ bestCandidate.summary.totalFiles }} 个文件）。</p>
        </div>
        <button
          class="btn btn-primary"
          type="button"
          :disabled="Boolean(busy) || hasPendingRestart"
          @click="previewImportFrom(bestCandidate.path)"
        >
          一键预览并搬入旧版数据
        </button>
      </div>

      <div v-else-if="showLegacyUpgradeBanner" class="upgrade-banner" role="region" aria-label="迁移建议">
        <div class="upgrade-text">
          <strong>当前仍在使用旧版随包目录</strong>
          <p class="hint">以后更新如果换了新文件夹打开，这里的数据不会跟着走。建议迁移到共享固定目录 <code>{{ status.defaultSharedDir }}</code>。</p>
        </div>
        <button
          class="btn btn-primary"
          type="button"
          :disabled="Boolean(busy) || hasPendingRestart"
          @click="previewSwitchTo(status.defaultSharedDir)"
        >
          迁移到 {{ status.defaultSharedDir }}
        </button>
      </div>

      <div class="actions storage-actions">
        <button
          class="btn"
          type="button"
          :disabled="Boolean(busy) || hasPendingRestart || status.mode === 'env' || status.mode === 'isolated'"
          @click="startPickSwitch"
        >
          {{ busy === 'pick-switch' ? '正在扫描…' : '更改数据目录…' }}
        </button>
        <button
          class="btn"
          type="button"
          :disabled="Boolean(busy) || hasPendingRestart"
          @click="startPickImport"
        >
          {{ busy === 'pick-import' ? '正在扫描…' : '从旧版本导入数据…' }}
        </button>
        <button
          class="btn"
          type="button"
          :disabled="Boolean(busy) || loading"
          @click="loadStatus"
        >
          {{ loading ? '扫描中…' : '重新扫描旧目录' }}
        </button>
      </div>

      <p v-if="status.mode === 'isolated'" class="hint">当前目录存在 <code>portable-isolated.flag</code>（便携隔离模式），如需改用共享目录请删除该标记文件后重启。</p>
      <p v-else-if="status.mode === 'env'" class="hint">当前正由环境变量 <code>DESKTOPHUB_LITE_DATA</code> 指定隔离目录。</p>
      <p v-if="blocked" class="hint">提醒：灵动岛或应用管理有未保存草稿；现在仍可直接迁移或导入，重启后未保存草稿不会保留。</p>
    </div>

    <!-- 切换数据目录两步确认预览（置于候选列表上方，触发后自动滚入视口） -->
    <GsapPresence
      :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: -6 }"
      :animate="{ opacity: 1, y: 0 }"
      :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: -6 }"
      :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
    >
      <div
        v-if="switchPreview"
        ref="previewBox"
        key="switch-preview"
        class="migration-preview"
        role="region"
        aria-label="切换数据目录预览"
      >
        <h5 ref="previewHeading" tabindex="-1">确认切换共享数据目录</h5>
        <p class="hint">目标目录：<code>{{ switchPreview.targetDir }}</code></p>
        <p class="summary-line">
          当前目录：应用 <b>{{ switchPreview.currentSummary.appCount }}</b> · 剪贴板 <b>{{ switchPreview.currentSummary.clipCount }}</b> · 短语 <b>{{ switchPreview.currentSummary.snippetCount }}</b>
          <br />
          {{ switchPreview.targetHasData
            ? `目标已有数据：应用 ${switchPreview.targetSummary.appCount} · 剪贴板 ${switchPreview.targetSummary.clipCount} · 短语 ${switchPreview.targetSummary.snippetCount}`
            : '目标目录目前为空' }}
        </p>
        <p v-if="switchHasCorruption" class="hint error" role="alert">
          检测到损坏的配置文件（{{ [...switchPreview.currentSummary.corruptedFiles, ...switchPreview.targetSummary.corruptedFiles].join('、') }}），已禁止盲目覆盖或切换。
        </p>
        <p v-else-if="switchPreview.targetHasData" class="hint">
          目标目录中已有真实数据，请选择处理方式（选择覆盖前会自动先备份目标原有文件）：
        </p>
        <p v-else class="hint">
          将把当前目录下的全部配置、剪贴板、图片、短语与邮箱记录完整复制过去（原目录文件会保留不动）。
        </p>
        <p v-if="blocked && !switchHasCorruption" class="hint">当前灵动岛或应用管理有未保存修改，迁移并重启后这些草稿不会保留。</p>
        <div class="actions">
          <button
            v-if="switchPreview.targetHasData"
            class="btn btn-primary"
            type="button"
            :disabled="Boolean(busy) || switchPreview.targetSummary.corruptedFiles.length > 0"
            @click="confirmSwitch('use_existing')"
          >
            {{ busy === 'apply' ? '处理中…' : '直接使用目标已有数据（不覆盖）' }}
          </button>
          <button
            class="btn"
            :class="{ 'btn-primary': !switchPreview.targetHasData }"
            type="button"
            :disabled="Boolean(busy) || switchHasCorruption"
            @click="confirmSwitch('copy_current')"
          >
            {{ busy === 'apply' ? '正在复制迁移…' : switchPreview.targetHasData ? '备份目标后，用当前数据覆盖' : '复制当前数据并切换' }}
          </button>
          <button class="btn" type="button" :disabled="Boolean(busy)" @click="clearPreviews">取消</button>
        </div>
      </div>
    </GsapPresence>

    <!-- 从旧版本完整导入两步确认预览 -->
    <GsapPresence
      :initial="state.ui.islandReducedMotion ? false : { opacity: 0, y: -6 }"
      :animate="{ opacity: 1, y: 0 }"
      :exit="state.ui.islandReducedMotion ? { opacity: 0 } : { opacity: 0, y: -6 }"
      :transition="state.ui.islandReducedMotion ? INSTANT_TRANSITION : activeMotionPreset.popTransition"
    >
      <div
        v-if="importPreview"
        ref="previewBox"
        key="import-preview"
        class="migration-preview"
        role="region"
        aria-label="旧版数据导入预览"
      >
        <h5 ref="previewHeading" tabindex="-1">确认从旧版目录完整导入</h5>
        <p class="hint">来源目录：<code>{{ importPreview.sourceDir }}</code></p>
        <p class="summary-line">
          应用 <b>{{ importPreview.sourceSummary.appCount }}</b>（分类 {{ importPreview.sourceSummary.categoryCount }}）· 剪贴板 <b>{{ importPreview.sourceSummary.clipCount }}</b>（图 {{ importPreview.sourceSummary.clipImageCount }}）· 短语 <b>{{ importPreview.sourceSummary.snippetCount }}</b> · 邮箱 <b>{{ importPreview.sourceSummary.mailAccountCount }}</b> · 中转 <b>{{ importPreview.sourceSummary.shelfCount }}</b> · 调音台 {{ importPreview.sourceSummary.audioProfileCount }} · 翻译模型：{{ importPreview.sourceSummary.hasTranslationConfig ? '有' : '无' }} · 外观与快捷键：{{ importPreview.sourceSummary.hasUiPreferences || importPreview.sourceSummary.hasShortcuts ? '有' : '无' }}
        </p>
        <p v-if="importHasCorruption" class="hint error" role="alert">
          来源目录存在损坏的配置文件（{{ importPreview.sourceSummary.corruptedFiles.join('、') }}），已停止导入以避免污染当前数据。
        </p>
        <p v-else class="hint">
          导入前会自动将当前目录原有数据完整备份到 <code>migration-backups/</code>；翻译密钥与邮箱授权码保存在 Windows 凭据管理器中会自动延续。
        </p>
        <p v-if="blocked && !importHasCorruption" class="hint">当前灵动岛或应用管理有未保存修改，导入并重启后这些草稿不会保留。</p>
        <div class="actions">
          <button
            class="btn btn-primary"
            type="button"
            :disabled="Boolean(busy) || importHasCorruption"
            @click="confirmImport"
          >
            {{ busy === 'apply' ? '正在备份并导入…' : '备份当前并导入全部数据' }}
          </button>
          <button class="btn" type="button" :disabled="Boolean(busy)" @click="clearPreviews">取消</button>
        </div>
      </div>
    </GsapPresence>

    <!-- 完成迁移后的重启引导 -->
    <div v-if="hasPendingRestart" class="restart-banner" role="status" aria-live="polite">
      <div class="restart-copy">
        <strong>{{ restartInfo?.message || '已完成数据目录变更或导入，请重启程序以加载新数据。' }}</strong>
        <p v-if="restartInfo?.backupDir" class="hint">原数据备份位置：<code>{{ restartInfo.backupDir }}</code></p>
        <p class="hint">为防止旧内存覆盖新数据，重启前已暂停后续写入保存。</p>
      </div>
      <button
        class="btn btn-primary"
        type="button"
        :disabled="busy === 'restart'"
        @click="restartNow"
      >
        {{ busy === 'restart' ? '正在重启…' : '立即重启以应用新数据' }}
      </button>
    </div>

    <div v-if="status && status.candidates.length" class="candidate-block">
      <p class="field-label">在本机发现的历史数据目录（点击预览后会显示在上方确认区）</p>
      <ul class="candidate-list">
        <li v-for="item in status.candidates" :key="item.path" class="candidate-item">
          <div class="candidate-meta">
            <div class="candidate-title">
              <strong>{{ item.label }}</strong>
              <span class="tag">{{ item.summary.totalFiles }} 个文件 · {{ formatBytes(item.summary.totalBytes) }}</span>
            </div>
            <code class="candidate-path">{{ item.path }}</code>
            <p class="hint candidate-counts">
              应用 {{ item.summary.appCount }} · 剪贴板 {{ item.summary.clipCount }}（图 {{ item.summary.clipImageCount }}）· 短语 {{ item.summary.snippetCount }} · 邮箱 {{ item.summary.mailAccountCount }}
            </p>
          </div>
          <div class="actions candidate-actions">
            <button
              class="btn"
              type="button"
              :disabled="Boolean(busy) || hasPendingRestart"
              @click="previewImportFrom(item.path)"
            >
              预览并导入
            </button>
            <button
              class="btn"
              type="button"
              :disabled="Boolean(busy) || hasPendingRestart || status.mode === 'env' || status.mode === 'isolated'"
              @click="previewSwitchTo(item.path)"
            >
              切换到此目录
            </button>
            <button
              class="btn btn-danger"
              type="button"
              :disabled="Boolean(busy) || hasPendingRestart"
              @click="confirmDeleteOldPath = confirmDeleteOldPath === item.path ? null : item.path"
            >
              删除旧配置
            </button>
          </div>
          <div v-if="confirmDeleteOldPath === item.path" class="candidate-confirm" role="alert">
            <span>将直接从磁盘删除该目录下的 DesktopHub 旧配置文件与缓存并移除此记录，操作不可恢复，是否继续？</span>
            <div class="actions">
              <button
                class="btn btn-danger is-confirming"
                type="button"
                :disabled="Boolean(busy)"
                @click="confirmDeleteOldDir(item.path)"
              >
                {{ busy === 'delete' ? '正在清理…' : '确认删除' }}
              </button>
              <button class="btn" type="button" :disabled="Boolean(busy)" @click="confirmDeleteOldPath = null">
                取消
              </button>
            </div>
          </div>
        </li>
      </ul>
    </div>

    <div v-if="status && status.backups.length" class="candidate-block">
      <div class="backup-head">
        <p class="field-label">历史配置与迁移备份（{{ status.backups.length }}）</p>
        <button
          v-if="!confirmClearAllBackups"
          class="btn btn-quiet-danger"
          type="button"
          :disabled="Boolean(busy)"
          @click="confirmClearAllBackups = true; confirmDeleteBackupPath = null"
        >
          清空全部备份
        </button>
        <div v-else class="actions" role="alert">
          <button
            class="btn btn-danger is-confirming"
            type="button"
            :disabled="Boolean(busy)"
            @click="confirmClearBackups"
          >
            {{ busy === 'delete' ? '清空中…' : '确认清空全部' }}
          </button>
          <button class="btn" type="button" :disabled="Boolean(busy)" @click="confirmClearAllBackups = false">
            取消
          </button>
        </div>
      </div>
      <ul class="candidate-list">
        <li v-for="bak in status.backups" :key="bak.path" class="candidate-item">
          <div class="candidate-meta">
            <div class="candidate-title">
              <strong>{{ bak.name }}</strong>
              <span class="tag">{{ bak.kind === 'migration' ? '迁移备份' : '配置备份' }}</span>
              <span class="tag">{{ bak.totalFiles }} 个文件 · {{ formatBytes(bak.totalBytes) }}</span>
            </div>
            <p class="hint candidate-counts">
              备份时间：{{ formatBackupTime(bak.modifiedMs) }} · <code>{{ bak.path }}</code>
            </p>
          </div>
          <div class="actions candidate-actions">
            <button
              v-if="confirmDeleteBackupPath !== bak.path"
              class="btn btn-danger"
              type="button"
              :disabled="Boolean(busy)"
              @click="confirmDeleteBackupPath = bak.path; confirmClearAllBackups = false"
            >
              删除
            </button>
            <template v-else>
              <button
                class="btn btn-danger is-confirming"
                type="button"
                :disabled="Boolean(busy)"
                @click="confirmDeleteBackup(bak.path)"
              >
                {{ busy === 'delete' ? '删除中…' : '确认删除' }}
              </button>
              <button class="btn" type="button" :disabled="Boolean(busy)" @click="confirmDeleteBackupPath = null">
                取消
              </button>
            </template>
          </div>
        </li>
      </ul>
    </div>

    <p v-if="feedback" class="hint" :class="{ error: failed }" role="status" aria-live="polite">{{ feedback }}</p>
  </section>
</template>

<style scoped>
.data-storage { margin-bottom: 12px; }
.storage-card {
  padding: 14px;
  border: 1px solid var(--line-strong);
  border-radius: 10px;
  background: var(--surface-2);
}
.storage-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}
.storage-path-group {
  flex: 1;
  min-width: 180px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.data-path-box,
.candidate-path {
  display: block;
  padding: 7px 10px;
  border-radius: 7px;
  border: 1px solid var(--line);
  background: var(--surface);
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text);
  overflow-wrap: anywhere;
}
.summary-line {
  margin: 10px 0 0;
  padding: 6px 10px;
  border-radius: 7px;
  background: var(--surface);
  border: 1px solid var(--line);
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-2);
  font-variant-numeric: tabular-nums;
}
.summary-line b {
  color: var(--text);
  font-weight: 600;
}
.upgrade-banner,
.auto-migrated-banner {
  margin-top: 12px;
  padding: 12px;
  border-radius: 9px;
  border: 1px solid var(--line-strong);
  background: var(--surface);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}
.auto-migrated-banner {
  display: block;
}
.auto-migrated-banner strong { font-size: 13px; }
.auto-migrated-banner .hint { margin: 4px 0 0; }
.upgrade-text { flex: 1; min-width: 180px; }
.upgrade-text strong { font-size: 13px; }
.upgrade-text .hint { margin: 4px 0 0; }
.storage-actions { margin-top: 12px; }
.candidate-block { margin-top: 14px; }
.candidate-list {
  list-style: none;
  margin: 8px 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.candidate-item {
  padding: 12px;
  border: 1px solid var(--line);
  border-radius: 9px;
  background: var(--surface-2);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}
.candidate-meta { flex: 1; min-width: 200px; }
.candidate-title { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin-bottom: 6px; font-size: 13px; }
.candidate-counts { margin: 6px 0 0; }
.candidate-confirm {
  flex: 1 0 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 7px;
  background: var(--danger-weak);
  border: 1px solid var(--danger);
  font-size: 12px;
  color: var(--danger);
}
.backup-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  flex-wrap: wrap;
}
.backup-head .field-label { margin: 0; }
.migration-preview {
  margin-top: 14px;
  padding: 16px;
  border: 1px solid var(--line-strong);
  border-radius: 10px;
  background: var(--surface);
  min-width: 0;
  overflow-wrap: anywhere;
}
.migration-preview h5 { margin: 0 0 8px; font-size: 14px; }
.migration-preview h5:focus-visible { outline: 2px solid var(--text); outline-offset: 4px; }
.restart-banner {
  margin-top: 14px;
  padding: 14px;
  border-radius: 10px;
  border: 1px solid var(--text);
  background: var(--surface-3);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
}
.restart-copy { flex: 1; min-width: 200px; }
.restart-copy strong { font-size: 13px; }
.restart-copy .hint { margin: 4px 0 0; }
</style>
