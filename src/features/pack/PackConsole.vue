<script setup lang="ts">
/**
 * 打包 / 联调台。
 *
 * 信息优先级：
 * P0 活动任务（仅在运行或刚完成未收起时置顶为唯一焦点）
 * P1 脚本清单（扁平高密度列表，每行唯一主按钮「运行」）
 * P2 ADB 环境条（轻量单行状态 + 端口映射）
 * P3 底部配置页脚（自动装机开关 + 摘要直达「设置 → 打包与联调」）
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import { invokeHost, isDesktop, listenHost, pickApplicationTarget } from '@/lib/desktop/desktop'
import { openSettings, pushToast } from '@/store'
import { clock, useTaskRunner } from './useTaskRunner'
import {
  DEFAULT_PACK_SETTINGS,
  DEFAULT_REVERSE_PORT,
  EMPTY_SCRIPT,
  estimateSeconds,
  type AdbStatus,
  type InstallApkResult,
  type PackScript,
  type PackSettings
} from './types'

const runner = useTaskRunner(message => pushToast(message, 'error'), { withLog: true })
const { task, log, elapsedMs } = runner

const scripts = ref<PackScript[]>([])
const settings = ref<PackSettings | null>(null)
const reversePort = ref<number>(DEFAULT_REVERSE_PORT)
const adb = ref<AdbStatus | null>(null)
const adbBusy = ref(false)
const loading = ref(false)
const editing = ref<PackScript | null>(null)
const confirmRun = ref<PackScript | null>(null)
const confirmDelete = ref<string | null>(null)
const savingSettings = ref(false)
const installingApk = ref(false)
const dismissedRunKey = ref('')
const logExpanded = ref(false)
let stopSettingsListener: (() => void) | null = null

const running = computed(() => task.value.stage === 'running')
const logLines = computed(() => log.value.slice(-60))
const latestLogLine = computed(() => logLines.value[logLines.value.length - 1] ?? '')
const runKey = computed(() => `${task.value.startedAt}:${task.value.stage}`)
const showRunCard = computed(
  () => task.value.stage !== 'idle' && (running.value || dismissedRunKey.value !== runKey.value)
)

watch(
  () => task.value.stage,
  stage => {
    if (stage === 'error') logExpanded.value = true
    else if (stage === 'running') logExpanded.value = false
  }
)

const elapsedText = computed(() => clock(elapsedMs.value))
const remainText = computed(() => {
  const estimate = task.value.estimateSec
  if (!estimate) return ''
  const remain = estimate * 1000 - elapsedMs.value
  return remain > 0 ? `约剩 ${clock(remain)}` : `收尾中 · ${clock(elapsedMs.value)}`
})
const progress = computed(() => {
  const estimate = task.value.estimateSec
  if (running.value) {
    if (!estimate || estimate <= 0) return null
    return Math.min(1, elapsedMs.value / (estimate * 1000))
  }
  if (task.value.stage === 'done') return 1
  if (estimate && estimate > 0) return Math.min(1, task.value.elapsedMs / (estimate * 1000))
  return 0
})

const adbReady = computed(() => !!adb.value?.serial)
const adbStateTone = computed(() => {
  if (adbReady.value) return 'is-on'
  if (adb.value?.error && !adb.value.devices.length) return 'is-err'
  return ''
})
const autoInstall = computed(() => settings.value?.autoInstall ?? false)
const displayPort = computed(() => {
  const port = Math.trunc(Number(reversePort.value))
  return Number.isInteger(port) && port >= 1 && port <= 65535 ? port : DEFAULT_REVERSE_PORT
})

const packConfigured = computed(
  () => !!(settings.value?.project && settings.value?.packagename && settings.value?.outputDir)
)
const remoteIgnoreFiles = computed(() => settings.value?.remoteIgnoreFiles ?? [])
const remoteIgnorePreview = computed(() => {
  const files = remoteIgnoreFiles.value
  if (!files.length) return ''
  const head = files.slice(0, 4).join(' · ')
  return files.length > 4 ? `${head} 等共 ${files.length} 项` : head
})
const projectShortName = computed(() => {
  const raw = settings.value?.project?.trim() ?? ''
  if (!raw) return ''
  const parts = raw.split(/[\\/]+/).filter(Boolean)
  return parts[parts.length - 1] ?? raw
})
const configTooltip = computed(() => {
  const s = settings.value
  return [
    `项目：${s?.project || '未配置'}`,
    `包名：${s?.packagename || '未配置'}`,
    `输出：${s?.outputDir || '未配置'}`,
    `端口：${displayPort.value}`,
    `远程忽略：${s?.remoteIgnoreFiles?.length ?? 0} 个文件`,
    '点击前往「设置 → 打包与联调」'
  ].join('\n')
})

function applyPackSettings(next: PackSettings): void {
  const normalized: PackSettings = {
    ...DEFAULT_PACK_SETTINGS,
    ...next,
    reversePort: next.reversePort > 0 ? next.reversePort : DEFAULT_REVERSE_PORT,
    remoteIgnoreFiles: Array.isArray(next.remoteIgnoreFiles) ? next.remoteIgnoreFiles : []
  }
  settings.value = normalized
  reversePort.value = normalized.reversePort
}

function estimateText(script: PackScript): string {
  const seconds = estimateSeconds(script)
  if (seconds === null) return '首次'
  const prefix = script.history.length ? '均 ' : '约 '
  if (seconds < 10) return `${prefix}${seconds.toFixed(1)}s`
  return `${prefix}${clock(seconds * 1000)}`
}

function scriptMetaTitle(script: PackScript): string {
  if (script.kind !== 'command') return `${script.name}（云打包流水线）`
  const parts: string[] = []
  if (script.command) parts.push(`命令：${script.command}`)
  else if (script.scriptPath) parts.push(`脚本：${script.scriptPath}`)
  if (script.preCommand) parts.push(`前置：${script.preCommand}`)
  return parts.join('\n') || script.name
}

async function loadScripts(): Promise<void> {
  scripts.value = await invokeHost<PackScript[]>('list_pack_scripts')
}

async function loadSettings(): Promise<void> {
  applyPackSettings(await invokeHost<PackSettings>('pack_settings'))
}

async function loadAdb(): Promise<void> {
  if (!isDesktop) return
  adbBusy.value = true
  try {
    adb.value = await invokeHost<AdbStatus>('adb_status')
  } catch (error) {
    pushToast(`ADB 状态读取失败：${String(error)}`, 'error')
  } finally {
    adbBusy.value = false
  }
}

async function loadAll(): Promise<void> {
  if (!isDesktop) {
    applyPackSettings(DEFAULT_PACK_SETTINGS)
    return
  }
  loading.value = true
  try {
    await Promise.all([loadScripts(), loadSettings(), loadAdb()])
  } catch (error) {
    pushToast(`联调台读取失败：${String(error)}`, 'error')
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  await loadAll()
  if (isDesktop) {
    stopSettingsListener = await listenHost<PackSettings>('pack-settings-changed', applyPackSettings)
  }
})

onBeforeUnmount(() => {
  stopSettingsListener?.()
  stopSettingsListener = null
})

/* ---------- ADB ---------- */

async function reversePortNow(): Promise<void> {
  if (!isDesktop || adbBusy.value) return
  const port = Math.trunc(Number(reversePort.value))
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    pushToast('转发端口需在 1～65535 之间', 'error')
    return
  }
  adbBusy.value = true
  try {
    adb.value = await invokeHost<AdbStatus>('adb_reverse', { port })
    pushToast(`已建立 tcp:${port} 端口映射`, 'success')
  } catch (error) {
    pushToast(String(error), 'error')
  } finally {
    adbBusy.value = false
  }
}

async function chooseDevice(serial: string): Promise<void> {
  if (!isDesktop) return
  try {
    adb.value = await invokeHost<AdbStatus>('adb_set_device', { serial })
  } catch (error) {
    pushToast(String(error), 'error')
  }
}

/* ---------- 脚本 ---------- */

function startScript(script: PackScript): void {
  if (running.value) {
    pushToast('已有任务在运行，请先等它结束或取消', 'info')
    return
  }
  if (script.env === 'remote' && script.kind === 'pack') {
    confirmRun.value = script
    return
  }
  void doStart(script)
}

async function doStart(script: PackScript): Promise<void> {
  confirmRun.value = null
  dismissedRunKey.value = ''
  await runner.start(script.id)
}

async function stopTask(): Promise<void> {
  await runner.cancel()
}

function dismissRunCard(): void {
  if (!running.value) dismissedRunKey.value = runKey.value
}

function newScript(): void {
  editing.value = { ...EMPTY_SCRIPT, name: '', estimateSec: 60 }
}

function editScript(script: PackScript): void {
  editing.value = { ...script }
}

function cancelEdit(): void {
  editing.value = null
}

async function saveScript(): Promise<void> {
  const draft = editing.value
  if (!draft || !isDesktop) return
  const rawEstimate = Number(draft.estimateSec)
  const estimateSec = Number.isFinite(rawEstimate)
    ? Math.max(0, Math.min(7200, Math.trunc(rawEstimate)))
    : 0
  const isPack = draft.kind === 'pack'
  try {
    scripts.value = await invokeHost<PackScript[]>('save_pack_script', {
      script: {
        ...draft,
        name: draft.name.trim(),
        command: isPack ? '' : draft.command.trim(),
        scriptPath: isPack ? '' : draft.scriptPath.trim(),
        workDir: isPack ? '' : draft.workDir.trim(),
        preCommand: isPack ? '' : draft.preCommand.trim(),
        estimateSec
      }
    })
    editing.value = null
    pushToast('脚本已保存', 'success')
  } catch (error) {
    pushToast(String(error), 'error')
  }
}

async function removeScript(id: string): Promise<void> {
  confirmDelete.value = null
  if (!isDesktop) return
  try {
    scripts.value = await invokeHost<PackScript[]>('delete_pack_script', { id })
  } catch (error) {
    pushToast(String(error), 'error')
  }
}

async function pickScriptFile(): Promise<void> {
  if (!editing.value || !isDesktop) return
  const picked = await pickApplicationTarget('file')
  if (picked) editing.value.scriptPath = picked
}

async function pickWorkDir(): Promise<void> {
  if (!editing.value || !isDesktop) return
  const picked = await pickApplicationTarget('folder')
  if (picked) editing.value.workDir = picked
}

/* ---------- 打包设置 ---------- */

async function toggleAutoInstall(): Promise<void> {
  if (!isDesktop || !settings.value || savingSettings.value) return
  savingSettings.value = true
  try {
    applyPackSettings(
      await invokeHost<PackSettings>('save_pack_settings', {
        settings: { ...settings.value, autoInstall: !settings.value.autoInstall }
      })
    )
    pushToast(settings.value.autoInstall ? '打包完成后会自动装机' : '打包完成后只保存 APK，不装手机', 'info')
  } catch (error) {
    pushToast(String(error), 'error')
  } finally {
    savingSettings.value = false
  }
}

async function openApkDir(): Promise<void> {
  if (!isDesktop) return
  try {
    await invokeHost('open_pack_dir', { apkPath: task.value.apkPath || null })
  } catch (error) {
    pushToast(String(error), 'error')
  }
}

async function installCurrentApk(): Promise<void> {
  if (!isDesktop || !task.value.apkPath || installingApk.value) return
  installingApk.value = true
  try {
    const result = await invokeHost<InstallApkResult>('install_pack_apk', {
      apkPath: task.value.apkPath,
      env: task.value.env
    })
    adb.value = result.adb
    pushToast(
      result.reversedPort
        ? `已安装到 ${result.device} 并建立 :${result.reversedPort} 端口映射`
        : `已安装到 ${result.device}`,
      'success'
    )
  } catch (error) {
    pushToast(String(error), 'error')
  } finally {
    installingApk.value = false
  }
}

function openPackSettings(): void {
  openSettings('pack')
}
</script>

<template>
  <div class="pack-console">
    <!-- P0 活动任务焦点区：空闲时不占位 -->
    <section
      v-if="showRunCard"
      class="run-focus"
      :class="`is-${task.stage}`"
      aria-label="当前任务状态"
    >
      <div class="run-top">
        <div class="run-title">
          <span class="run-dot" :class="`is-${task.stage}`" aria-hidden="true" />
          <strong>{{ task.name || '任务' }}</strong>
          <span v-if="task.env === 'remote'" class="risk-tag">正式</span>
        </div>
        <div class="run-right">
          <span class="run-time">
            <template v-if="running">
              {{ remainText || `已用 ${elapsedText}` }}
            </template>
            <template v-else>
              {{ task.stage === 'done' ? '完成' : task.stage === 'cancelled' ? '已取消' : '失败' }} · {{ elapsedText }}
            </template>
          </span>
          <button v-if="running" class="btn-inline danger" type="button" @click="stopTask">
            <Icon name="stop" :size="11" />取消
          </button>
          <template v-else>
            <button
              v-if="task.apkPath"
              class="btn-inline"
              type="button"
              :disabled="installingApk || !isDesktop"
              :title="task.env === 'local' ? '覆盖安装到手机并自动转发端口' : '覆盖安装到手机（不转发端口）'"
              @click="installCurrentApk"
            >
              <Icon name="download" :size="11" />{{ installingApk ? '安装中…' : '装到手机' }}
            </button>
            <button
              v-if="task.kind === 'pack' || task.apkPath"
              class="btn-inline"
              type="button"
              @click="openApkDir"
            >
              <Icon name="folder" :size="11" />打开目录
            </button>
          </template>
          <button
            v-if="!running"
            class="icon-action"
            type="button"
            title="收起任务状态"
            aria-label="收起任务状态"
            @click="dismissRunCard"
          >
            <Icon name="close" :size="12" />
          </button>
        </div>
      </div>

      <div class="run-sub">
        <span class="run-msg">{{ task.message }}</span>
        <div
          class="progress"
          :class="{
            'is-indeterminate': running && progress === null,
            'is-done': task.stage === 'done',
            'is-error': task.stage === 'error',
            'is-cancelled': task.stage === 'cancelled'
          }"
        >
          <span :style="progress !== null ? { width: `${Math.round(progress * 100)}%` } : undefined" />
        </div>
      </div>

      <p v-if="task.detail" class="run-error">{{ task.detail }}</p>

      <div v-if="logLines.length" class="log-box">
        <button
          class="log-toggle"
          type="button"
          :aria-expanded="logExpanded"
          @click="logExpanded = !logExpanded"
        >
          <span class="log-preview">{{ latestLogLine }}</span>
          <span class="log-count">
            日志 ({{ logLines.length }})
            <Icon name="chevron-down" :size="11" :class="{ rotated: logExpanded }" />
          </span>
        </button>
        <pre v-if="logExpanded" class="log scroll-y">{{ logLines.join('\n') }}</pre>
      </div>
    </section>

    <!-- P2 紧凑 ADB 环境条 -->
    <section class="adb-strip" aria-label="ADB 联调">
      <div class="adb-main">
        <div class="adb-device">
          <span class="status-dot" :class="adbStateTone" aria-hidden="true" />
          <select
            v-if="(adb?.devices.length ?? 0) > 1"
            class="device-select"
            :value="adb?.serial ?? ''"
            aria-label="选择调试设备"
            @change="chooseDevice(($event.target as HTMLSelectElement).value)"
          >
            <option value="">选择设备…</option>
            <option v-for="device in adb?.devices" :key="device.serial" :value="device.serial">
              {{ device.model || device.serial }} · {{ device.serial }}
            </option>
          </select>
          <span v-else-if="adb?.serial" class="device-text" :title="adb.serial">
            {{ adb.devices[0]?.model || '已连接' }} · <code>{{ adb.serial }}</code>
          </span>
          <span v-else class="device-text is-off">未连接设备</span>

          <button
            class="icon-action"
            type="button"
            :disabled="adbBusy || !isDesktop"
            title="刷新 ADB 设备与端口映射"
            aria-label="刷新 ADB 状态"
            @click="loadAdb"
          >
            <Icon name="refresh" :size="12" :class="{ spin: adbBusy }" />
          </button>
        </div>

        <div class="adb-port">
          <label class="port-box" title="本地反向转发端口（1～65535）">
            <span class="port-prefix">:</span>
            <input
              v-model.number="reversePort"
              type="number"
              min="1"
              max="65535"
              aria-label="ADB 转发端口"
            />
          </label>
          <button
            class="btn-inline"
            type="button"
            :disabled="!adbReady || adbBusy || !isDesktop"
            @click="reversePortNow"
          >
            <Icon name="download" :size="11" />转发
          </button>
        </div>
      </div>

      <p v-if="adb?.error" class="adb-note" :class="{ 'is-err': !adb.devices.length }">
        {{ adb.error }}
      </p>
      <p v-else-if="adb?.reverse.length" class="adb-note is-mono" :title="adb.reverse.join('\n')">
        已映射：{{ adb.reverse.join(' · ') }}
      </p>
    </section>

    <!-- P1 脚本主列表 -->
    <section class="scripts-section" aria-label="脚本">
      <header class="section-head">
        <h3>脚本</h3>
        <button class="text-action" type="button" :disabled="!isDesktop" @click="newScript">
          <Icon name="plus" :size="12" />新增
        </button>
      </header>

      <!-- 内联新增 / 编辑表单 -->
      <div v-if="editing" class="editor" role="region" aria-label="编辑脚本">
        <div class="editor-head">
          <strong>{{ editing.id ? '编辑脚本' : '新增脚本' }}</strong>
          <button class="icon-action" type="button" aria-label="关闭编辑" @click="cancelEdit">
            <Icon name="close" :size="12" />
          </button>
        </div>

        <label class="field">
          <span>名称</span>
          <input v-model="editing.name" type="text" maxlength="40" placeholder="例如：本地联调" />
        </label>

        <div class="field-row">
          <label class="field">
            <span>环境</span>
            <select v-model="editing.env">
              <option value="local">本地联调</option>
              <option value="remote">正式远程</option>
            </select>
          </label>
          <label class="field">
            <span>类型</span>
            <select v-model="editing.kind">
              <option value="command">短命令</option>
              <option value="pack">云打包长任务</option>
            </select>
          </label>
          <label class="field">
            <span>预估（秒）</span>
            <input v-model.number="editing.estimateSec" type="number" min="0" max="7200" />
          </label>
        </div>

        <template v-if="editing.kind === 'command'">
          <label class="field">
            <span>命令</span>
            <input v-model="editing.command" type="text" :placeholder="`adb reverse tcp:${displayPort} tcp:${displayPort}`" />
          </label>
          <label class="field">
            <span>或脚本（.ps1 / .bat）</span>
            <span class="with-button">
              <input v-model="editing.scriptPath" type="text" placeholder="留空则用上面的命令" />
              <button class="btn-inline" type="button" @click="pickScriptFile">选择</button>
            </span>
          </label>
          <label class="field">
            <span>工作目录</span>
            <span class="with-button">
              <input v-model="editing.workDir" type="text" placeholder="留空：脚本用自身目录" />
              <button class="btn-inline" type="button" @click="pickWorkDir">选择</button>
            </span>
          </label>
          <label class="field">
            <span>前置命令（可选，失败则不继续）</span>
            <input v-model="editing.preCommand" type="text" :placeholder="`例如：adb reverse tcp:${displayPort} tcp:${displayPort}`" />
          </label>
        </template>
        <p v-else class="editor-note">
          云打包走内置 HBuilderX CLI 流水线；项目与输出目录可在「设置 → 打包与联调」修改。
        </p>

        <div class="editor-actions">
          <button class="btn-primary" type="button" @click="saveScript">保存</button>
          <button class="btn-inline" type="button" @click="cancelEdit">取消</button>
        </div>
      </div>

      <p v-if="loading && !scripts.length" class="empty-hint">读取中…</p>
      <p v-else-if="!scripts.length" class="empty-hint">
        {{ isDesktop ? '还没有脚本，点右上角「新增」。' : '网页预览模式下不读取本机脚本。' }}
      </p>

      <ul v-else class="script-list">
        <li
          v-for="script in scripts"
          :key="script.id"
          class="script-row"
          :class="{ 'is-remote': script.env === 'remote' }"
        >
          <div class="script-line">
            <div class="script-info" :title="scriptMetaTitle(script)">
              <div class="script-name-row">
                <strong>{{ script.name }}</strong>
                <span v-if="script.env === 'remote'" class="risk-tag">正式</span>
                <span v-if="script.kind === 'pack'" class="kind-tag">云打包</span>
              </div>
              <p class="script-sub">
                <span class="tnum">{{ estimateText(script) }}</span>
                <template v-if="script.kind === 'pack'">
                  <span v-if="script.env === 'local'" class="pre-frag">· 按当前工作区</span>
                  <span v-else class="pre-frag">
                    · {{ remoteIgnoreFiles.length ? `忽略 ${remoteIgnoreFiles.length} 项更改` : '未配忽略文件' }}
                  </span>
                </template>
                <template v-else>
                  <span v-if="script.command || script.scriptPath" class="cmd-frag">
                    · {{ script.command || script.scriptPath }}
                  </span>
                  <span v-if="script.preCommand" class="pre-frag">· 含前置</span>
                </template>
              </p>
            </div>

            <div class="script-ops">
              <div class="script-secondary">
                <button
                  class="icon-action"
                  type="button"
                  title="编辑"
                  :aria-label="`编辑 ${script.name}`"
                  :disabled="!isDesktop"
                  @click="editScript(script)"
                >
                  <Icon name="edit" :size="12" />
                </button>
                <button
                  class="icon-action danger"
                  type="button"
                  title="删除"
                  :aria-label="`删除 ${script.name}`"
                  :disabled="!isDesktop"
                  @click="confirmDelete = script.id"
                >
                  <Icon name="trash" :size="12" />
                </button>
              </div>
              <button
                class="btn-run"
                :class="{ 'is-remote': script.env === 'remote' }"
                type="button"
                :disabled="running || !isDesktop"
                @click="startScript(script)"
              >
                <Icon name="play" :size="11" />运行
              </button>
            </div>
          </div>

          <div v-if="confirmDelete === script.id" class="confirm-bar">
            <span>删除「{{ script.name }}」及其历史耗时？</span>
            <div class="confirm-actions">
              <button class="btn-inline danger" type="button" @click="removeScript(script.id)">删除</button>
              <button class="btn-inline" type="button" @click="confirmDelete = null">取消</button>
            </div>
          </div>
          <div v-else-if="confirmRun?.id === script.id" class="confirm-bar is-remote">
            <div class="confirm-text">
              <template v-if="remoteIgnoreFiles.length">
                <span>将临时忽略 <b>{{ remoteIgnoreFiles.length }}</b> 个文件的本地更改（云端排队后自动还原）：</span>
                <span class="confirm-files" :title="remoteIgnoreFiles.join('\n')">{{ remoteIgnorePreview }}</span>
              </template>
              <span v-else>未配置「远程忽略文件」，将按当前工作区直接提交<b>正式远程</b>云打包。</span>
            </div>
            <div class="confirm-actions">
              <button
                v-if="!remoteIgnoreFiles.length"
                class="btn-inline"
                type="button"
                @click="confirmRun = null; openPackSettings()"
              >
                去配置
              </button>
              <button class="btn-run is-remote" type="button" @click="doStart(script)">继续</button>
              <button class="btn-inline" type="button" @click="confirmRun = null">取消</button>
            </div>
          </div>
        </li>
      </ul>
    </section>

    <!-- P3 单行配置页脚 -->
    <footer class="pack-footer" aria-label="打包配置摘要">
      <button
        class="toggle"
        type="button"
        :aria-pressed="autoInstall"
        :disabled="savingSettings || !isDesktop"
        title="关闭时只保存 APK 到输出目录，不覆盖手机上已安装的应用"
        @click="toggleAutoInstall"
      >
        <span class="switch" :class="{ 'is-on': autoInstall }" />
        <span>自动装机</span>
      </button>

      <button
        class="config-link"
        :class="{ 'is-missing': !packConfigured }"
        type="button"
        :title="configTooltip"
        @click="openPackSettings"
      >
        <span class="config-summary">
          <template v-if="packConfigured">{{ projectShortName }} · :{{ displayPort }}</template>
          <template v-else>未配置打包路径 · 配置</template>
        </span>
        <Icon name="settings" :size="12" />
      </button>
    </footer>
  </div>
</template>

<style scoped>
.pack-console {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 2px 0 4px;
}

/* ---------- 通用微按钮 ---------- */
.btn-inline,
.btn-primary,
.btn-run {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 8px;
  border-radius: 7px;
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--text-2);
  font-size: 11px;
  font-weight: 550;
  white-space: nowrap;
  cursor: pointer;
  transition: background-color 120ms ease, border-color 120ms ease, color 120ms ease, transform 80ms ease;
}
.btn-inline:hover:not(:disabled) {
  background: var(--hover);
  color: var(--text);
  border-color: var(--line-strong);
}
.btn-inline.danger {
  color: var(--danger);
  border-color: color-mix(in srgb, var(--danger) 35%, var(--line));
}
.btn-inline.danger:hover:not(:disabled) {
  background: color-mix(in srgb, var(--danger) 10%, var(--surface));
  border-color: var(--danger);
}
.btn-primary,
.btn-run {
  border-color: var(--accent);
  color: var(--accent);
  background: var(--accent-weak);
  font-weight: 600;
}
.btn-primary:hover:not(:disabled),
.btn-run:hover:not(:disabled) {
  background: var(--accent);
  color: var(--accent-text);
}
.btn-run.is-remote {
  border-color: color-mix(in srgb, var(--danger) 55%, var(--line));
  color: var(--danger);
  background: color-mix(in srgb, var(--danger) 10%, transparent);
}
.btn-run.is-remote:hover:not(:disabled) {
  background: var(--danger);
  color: #fff;
  border-color: var(--danger);
}
.btn-inline:disabled,
.btn-primary:disabled,
.btn-run:disabled,
.icon-action:disabled,
.text-action:disabled {
  opacity: .42;
  cursor: not-allowed;
}
.btn-inline:active:not(:disabled),
.btn-primary:active:not(:disabled),
.btn-run:active:not(:disabled) {
  transform: scale(0.97);
}

.icon-action {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-2);
  cursor: pointer;
  transition: background-color 120ms ease, color 120ms ease, opacity 120ms ease;
}
.icon-action:hover:not(:disabled) {
  background: var(--hover);
  color: var(--text);
}
.icon-action.danger:hover:not(:disabled) {
  color: var(--danger);
  background: color-mix(in srgb, var(--danger) 10%, transparent);
}
.icon-action:focus-visible,
.btn-inline:focus-visible,
.btn-primary:focus-visible,
.btn-run:focus-visible,
.text-action:focus-visible,
.config-link:focus-visible,
.log-toggle:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

.risk-tag {
  padding: 0 5px;
  height: 16px;
  display: inline-flex;
  align-items: center;
  border-radius: 999px;
  font-size: 9.5px;
  font-weight: 650;
  color: var(--danger);
  background: color-mix(in srgb, var(--danger) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--danger) 36%, transparent);
}
.kind-tag {
  font-size: 10px;
  font-weight: 500;
  color: var(--text-3);
}
.tnum {
  font-variant-numeric: tabular-nums;
}

/* ---------- P0 活动任务焦点区 ---------- */
.run-focus {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 9px 10px;
  border-radius: 10px;
  background: var(--surface-2);
  border: 1px solid var(--line);
}
.run-focus.is-error {
  border-color: color-mix(in srgb, var(--danger) 42%, var(--line));
}
.run-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.run-title {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.run-title strong {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.run-dot {
  flex: none;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-3);
}
.run-dot.is-running {
  background: var(--accent);
  box-shadow: 0 0 6px var(--accent);
}
.run-dot.is-done {
  background: var(--success, #3fb950);
}
.run-dot.is-error {
  background: var(--danger);
}
.run-right {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: none;
}
.run-time {
  font-size: 10.5px;
  color: var(--text-2);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.run-sub {
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.run-msg {
  font-size: 11px;
  color: var(--text-2);
}
.progress {
  position: relative;
  height: 3px;
  border-radius: 999px;
  background: var(--line);
  overflow: hidden;
}
.progress > span {
  position: absolute;
  inset: 0 auto 0 0;
  background: var(--accent);
  border-radius: 999px;
  transition: width 360ms ease;
}
.progress.is-done > span {
  background: var(--success, #3fb950);
}
.progress.is-error > span {
  background: var(--danger);
}
.progress.is-cancelled > span {
  background: var(--text-3);
}
.progress.is-indeterminate > span {
  width: 35%;
  animation: slide 1.4s ease-in-out infinite;
}
@keyframes slide {
  0% { left: -35%; }
  100% { left: 100%; }
}
.run-error {
  margin: 0;
  font-size: 11px;
  line-height: 1.45;
  color: var(--danger);
  word-break: break-word;
}
.log-box {
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.log-toggle {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  padding: 4px 6px;
  border: 0;
  border-radius: 6px;
  background: color-mix(in srgb, var(--text) 5%, transparent);
  color: var(--text-3);
  font-size: 10px;
  cursor: pointer;
  text-align: left;
}
.log-toggle:hover {
  color: var(--text-2);
  background: color-mix(in srgb, var(--text) 8%, transparent);
}
.log-preview {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono, ui-monospace, monospace);
}
.log-count {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-variant-numeric: tabular-nums;
}
.log-count svg {
  transition: transform 150ms ease;
}
.log-count svg.rotated {
  transform: rotate(180deg);
}
.log {
  margin: 0;
  padding: 6px 7px;
  max-height: 120px;
  border-radius: 7px;
  background: color-mix(in srgb, var(--text) 6%, transparent);
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 10px;
  line-height: 1.5;
  color: var(--text-2);
  white-space: pre-wrap;
  word-break: break-all;
}

/* ---------- P2 紧凑 ADB 环境条 ---------- */
.adb-strip {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 4px 0 10px;
  border-bottom: 1px solid var(--line);
}
.adb-main {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.adb-device {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  min-width: 0;
}
.status-dot {
  flex: none;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-3);
}
.status-dot.is-on {
  background: var(--success, #3fb950);
}
.status-dot.is-err {
  background: var(--danger);
}
.device-text {
  font-size: 11.5px;
  color: var(--text-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.device-text.is-off {
  color: var(--text-3);
}
.device-text code {
  font-size: 10.5px;
}
.device-select {
  flex: 1;
  min-width: 0;
  max-width: 210px;
  height: 24px;
  padding: 0 6px;
  border-radius: 6px;
  border: 1px solid var(--line);
  background: var(--surface-2);
  color: var(--text);
  font-size: 11px;
}
.adb-port {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex: none;
}
.port-box {
  display: inline-flex;
  align-items: center;
  height: 24px;
  padding: 0 6px;
  border-radius: 6px;
  border: 1px solid var(--line);
  background: var(--surface-2);
  color: var(--text-3);
  font-size: 11px;
}
.port-box:focus-within {
  border-color: var(--accent);
}
.port-prefix {
  user-select: none;
  margin-right: 1px;
}
.port-box input {
  width: 48px;
  border: 0;
  padding: 0;
  background: transparent;
  color: var(--text);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  outline: none;
}
.adb-note {
  margin: 0;
  padding-left: 12px;
  font-size: 10.5px;
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.adb-note.is-err {
  color: var(--danger);
}
.adb-note.is-mono {
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 10px;
  color: var(--text-2);
}
.spin {
  animation: spin 0.9s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}

/* ---------- P1 脚本扁平列表 ---------- */
.scripts-section {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 2px 2px 4px;
}
.section-head h3 {
  margin: 0;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-2);
  letter-spacing: 0.02em;
}
.text-action {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  height: 22px;
  padding: 0 6px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-2);
  font-size: 11px;
  font-weight: 550;
  cursor: pointer;
}
.text-action:hover:not(:disabled) {
  background: var(--hover);
  color: var(--text);
}

.empty-hint {
  margin: 6px 2px;
  font-size: 11px;
  color: var(--text-3);
}

.script-list {
  margin: 0;
  padding: 0;
  list-style: none;
}
.script-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 7px 4px;
  border-top: 1px solid var(--line);
  border-radius: 6px;
  transition: background-color 120ms ease;
}
.script-row:first-child {
  border-top-color: transparent;
}
.script-row:hover {
  background: color-mix(in srgb, var(--hover) 70%, transparent);
}
.script-line {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-width: 0;
}
.script-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.script-name-row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.script-name-row strong {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.script-sub {
  margin: 0;
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10.5px;
  color: var(--text-3);
  min-width: 0;
  overflow: hidden;
}
.cmd-frag {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 10px;
}
.pre-frag {
  flex: none;
  white-space: nowrap;
}
.script-ops {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: none;
}
.script-secondary {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  opacity: 0.42;
  transition: opacity 120ms ease;
}
.script-row:hover .script-secondary,
.script-row:focus-within .script-secondary {
  opacity: 1;
}

.confirm-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
  padding: 5px 8px;
  border-radius: 7px;
  background: var(--surface-2);
  font-size: 11px;
  color: var(--text-2);
}
.confirm-bar.is-remote {
  background: color-mix(in srgb, var(--danger) 10%, var(--surface-2));
}
.confirm-text {
  flex: 1 1 200px;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.confirm-files {
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 10px;
  color: var(--text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.confirm-actions {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
}

/* ---------- 内联脚本编辑器 ---------- */
.editor {
  display: flex;
  flex-direction: column;
  gap: 7px;
  padding: 9px 10px;
  margin-bottom: 6px;
  border-radius: 9px;
  background: var(--surface-2);
  border: 1px solid var(--line);
}
.editor-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.editor-head strong {
  font-size: 12px;
  font-weight: 600;
  color: var(--text);
}
.editor .field {
  display: flex;
  flex-direction: column;
  gap: 3px;
  font-size: 10.5px;
  color: var(--text-2);
}
.field-row {
  display: flex;
  gap: 6px;
}
.field-row .field {
  flex: 1;
  min-width: 0;
}
.editor input,
.editor select {
  height: 26px;
  padding: 0 7px;
  border-radius: 6px;
  border: 1px solid var(--line);
  background: var(--surface);
  color: var(--text);
  font-size: 11px;
  width: 100%;
}
.editor input:focus-visible,
.editor select:focus-visible {
  outline: none;
  border-color: var(--accent);
}
.with-button {
  display: flex;
  gap: 4px;
}
.with-button input {
  flex: 1;
  min-width: 0;
}
.editor-note {
  margin: 0;
  font-size: 10.5px;
  line-height: 1.45;
  color: var(--text-3);
}
.editor-actions {
  display: flex;
  gap: 6px;
  padding-top: 2px;
}

/* ---------- P3 底部单行页脚 ---------- */
.pack-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-top: 4px;
  padding: 8px 2px 0;
  border-top: 1px solid var(--line);
}
.toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 0;
  padding: 0;
  background: transparent;
  color: var(--text-2);
  font-size: 11px;
  cursor: pointer;
  white-space: nowrap;
}
.toggle:hover:not(:disabled) {
  color: var(--text);
}
.toggle:disabled {
  opacity: .5;
  cursor: not-allowed;
}
.switch {
  position: relative;
  width: 26px;
  height: 14px;
  border-radius: 999px;
  background: var(--line-strong, var(--line));
  transition: background 150ms ease;
}
.switch::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: var(--surface);
  transition: transform 150ms ease;
}
.switch.is-on {
  background: var(--accent);
}
.switch.is-on::after {
  transform: translateX(12px);
}
.config-link {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-width: 0;
  max-width: 65%;
  padding: 3px 6px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-3);
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  cursor: pointer;
}
.config-link:hover {
  background: var(--hover);
  color: var(--text);
}
.config-link.is-missing {
  color: var(--accent);
}
.config-summary {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
