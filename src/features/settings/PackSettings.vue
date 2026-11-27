<script setup lang="ts">
/**
 * 「设置 → 打包与联调」分区。
 *
 * 信息层级：
 * 1. 核心配置（项目目录、包名、输出目录、自动装机、默认转发端口）默认直出；
 * 2. 低频工具路径与证书参数收纳进「高级与工具路径」折叠区，按需展开。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import MotionSaveButton from '@/components/MotionSaveButton.vue'
import MotionSwitch from '@/components/MotionSwitch.vue'
import { vGsapPulse } from '@/lib/gsapMotion'
import { invokeHost, isDesktop, listenHost, pickApplicationTarget } from '@/lib/desktop/desktop'
import { DEFAULT_PACK_SETTINGS, DEFAULT_REVERSE_PORT, type PackSettings } from '@/features/pack/types'
import { pushToast, state } from '@/store'

const draft = ref<PackSettings>({ ...DEFAULT_PACK_SETTINGS })
const ignoreFilesText = ref('')
const savedSignature = ref(JSON.stringify(DEFAULT_PACK_SETTINGS))
const loading = ref(false)
const saving = ref(false)
const justSaved = ref(false)
let savedResetTimer: number | null = null
const feedback = ref('')
const isError = ref(false)

let unlisten: (() => void) | null = null

function cleanIgnorePath(raw: string, projectDir: string): string {
  const cleaned = raw.trim().replace(/^['"]+|['"]+$/g, '').trim().replace(/\\/g, '/')
  if (!cleaned) return ''
  const proj = projectDir.trim().replace(/^['"]+|['"]+$/g, '').trim().replace(/\\/g, '/').replace(/\/+$/, '')
  if (proj && cleaned.toLowerCase().startsWith(`${proj.toLowerCase()}/`)) {
    return cleaned.slice(proj.length + 1)
  }
  return cleaned.replace(/^(\.\/|\/)+/, '')
}

function parseIgnoreFiles(text: string, projectDir: string): string[] {
  const seen = new Set<string>()
  const out: string[] = []
  for (const line of text.split(/\r?\n/)) {
    const item = cleanIgnorePath(line, projectDir)
    if (!item) continue
    const key = item.toLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    out.push(item)
  }
  return out
}

function onIgnoreTextInput(value: string): void {
  ignoreFilesText.value = value
  draft.value.remoteIgnoreFiles = parseIgnoreFiles(value, draft.value.project)
}

function signatureOf(value: PackSettings): string {
  return JSON.stringify({
    cli: value.cli.trim(),
    hbuilderRoot: value.hbuilderRoot.trim(),
    project: value.project.trim(),
    packagename: value.packagename.trim(),
    packType: value.packType.trim() || '3',
    outputDir: value.outputDir.trim(),
    adb: value.adb.trim(),
    device: value.device.trim(),
    reversePort: Number(value.reversePort) || 0,
    autoInstall: !!value.autoInstall,
    remoteIgnoreFiles: parseIgnoreFiles(
      (value.remoteIgnoreFiles ?? []).join('\n'),
      value.project
    )
  })
}

const dirty = computed(() => signatureOf(draft.value) !== savedSignature.value)
const hasCustomAdvanced = computed(() =>
  Boolean(
    draft.value.hbuilderRoot.trim() ||
    draft.value.cli.trim() ||
    draft.value.adb.trim() ||
    draft.value.device.trim() ||
    (draft.value.packType && draft.value.packType !== '3')
  )
)

function applyLoaded(next: PackSettings): void {
  const ignoreList = Array.isArray(next.remoteIgnoreFiles) ? next.remoteIgnoreFiles : []
  const normalized: PackSettings = {
    ...DEFAULT_PACK_SETTINGS,
    ...next,
    packType: next.packType || '3',
    reversePort: next.reversePort > 0 ? next.reversePort : DEFAULT_REVERSE_PORT,
    remoteIgnoreFiles: ignoreList
  }
  draft.value = normalized
  ignoreFilesText.value = ignoreList.join('\n')
  savedSignature.value = signatureOf(normalized)
}

async function load(): Promise<void> {
  if (!isDesktop) {
    applyLoaded(DEFAULT_PACK_SETTINGS)
    return
  }
  loading.value = true
  try {
    const loaded = await invokeHost<PackSettings>('pack_settings')
    applyLoaded(loaded)
  } catch (error) {
    isError.value = true
    feedback.value = `读取失败：${String(error)}`
  } finally {
    loading.value = false
  }
}

async function pickFolder(field: 'project' | 'outputDir' | 'hbuilderRoot'): Promise<void> {
  if (!isDesktop) return
  try {
    const picked = await pickApplicationTarget('folder')
    if (picked) draft.value[field] = picked
  } catch (error) {
    pushToast(`选择目录失败：${String(error)}`, 'error')
  }
}

async function pickFile(field: 'cli' | 'adb'): Promise<void> {
  if (!isDesktop) return
  try {
    const picked = await pickApplicationTarget('file')
    if (picked) draft.value[field] = picked
  } catch (error) {
    pushToast(`选择文件失败：${String(error)}`, 'error')
  }
}

async function pickIgnoreFile(): Promise<void> {
  if (!isDesktop) return
  try {
    const picked = await pickApplicationTarget('file')
    if (!picked) return
    const cleaned = cleanIgnorePath(picked, draft.value.project)
    if (!cleaned) return
    const next = parseIgnoreFiles(
      ignoreFilesText.value ? `${ignoreFilesText.value}\n${cleaned}` : cleaned,
      draft.value.project
    )
    ignoreFilesText.value = next.join('\n')
    draft.value.remoteIgnoreFiles = next
  } catch (error) {
    pushToast(`选择文件失败：${String(error)}`, 'error')
  }
}

async function save(): Promise<void> {
  if (!isDesktop || saving.value) return
  const port = Math.trunc(Number(draft.value.reversePort))
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    isError.value = true
    feedback.value = 'ADB 转发端口需为 1～65535 之间的整数'
    return
  }
  saving.value = true
  isError.value = false
  feedback.value = ''
  try {
    const remoteIgnoreFiles = parseIgnoreFiles(ignoreFilesText.value, draft.value.project)
    const payload: PackSettings = {
      ...draft.value,
      cli: draft.value.cli.trim(),
      hbuilderRoot: draft.value.hbuilderRoot.trim(),
      project: draft.value.project.trim(),
      packagename: draft.value.packagename.trim(),
      packType: draft.value.packType.trim() || '3',
      outputDir: draft.value.outputDir.trim(),
      adb: draft.value.adb.trim(),
      device: draft.value.device.trim(),
      reversePort: port,
      autoInstall: !!draft.value.autoInstall,
      remoteIgnoreFiles
    }
    const saved = await invokeHost<PackSettings>('save_pack_settings', { settings: payload })
    applyLoaded(saved)
    feedback.value = '打包与联调设置已保存'
    justSaved.value = true
    if (savedResetTimer !== null) window.clearTimeout(savedResetTimer)
    savedResetTimer = window.setTimeout(() => { justSaved.value = false; savedResetTimer = null }, 1200)
  } catch (error) {
    isError.value = true
    feedback.value = String(error)
  } finally {
    saving.value = false
  }
}

async function openOutputDir(): Promise<void> {
  if (!isDesktop) return
  try {
    await invokeHost('open_pack_dir', { apkPath: null })
  } catch (error) {
    pushToast(String(error), 'error')
  }
}

onMounted(async () => {
  await load()
  if (isDesktop) {
    unlisten = await listenHost<PackSettings>('pack-settings-changed', next => {
      if (!dirty.value && !saving.value) {
        applyLoaded(next)
      }
    })
  }
})

onBeforeUnmount(() => {
  unlisten?.()
  unlisten = null
  if (savedResetTimer !== null) window.clearTimeout(savedResetTimer)
})
</script>

<template>
  <div class="panel">
    <h3 class="panel-title">打包与联调</h3>
    <p class="hint">设置云打包路径与默认 ADB 转发端口；仅做真机联调时可不填打包路径。</p>
    <p v-if="loading" class="hint loading-inline" role="status">
      <span class="bars-loader" aria-hidden="true">
        <i
          v-for="idx in 3"
          :key="idx"
          v-gsap-pulse="state.ui.islandReducedMotion ? null : { opacity: [0.22, 0.95], scaleY: [0.6, 1], duration: 0.9, delay: (idx - 1) * 0.2 }"
        />
      </span>
      正在读取当前配置…
    </p>

    <h4 class="sub-title">云打包配置</h4>
    <div class="field-grid">
      <label class="field field-wide">
        <span class="field-label">项目目录</span>
        <span class="path-row">
          <input
            v-model="draft.project"
            class="control"
            type="text"
            spellcheck="false"
            placeholder="例如：D:\work\my-uniapp-project"
          />
          <button class="btn" type="button" :disabled="!isDesktop" @click="pickFolder('project')">选择目录</button>
        </span>
      </label>

      <label class="field field-wide">
        <span class="field-label">Android 包名</span>
        <input
          v-model="draft.packagename"
          class="control"
          type="text"
          spellcheck="false"
          placeholder="例如：com.example.app"
        />
      </label>

      <label class="field field-wide">
        <span class="field-label">APK 输出目录</span>
        <span class="path-row">
          <input
            v-model="draft.outputDir"
            class="control"
            type="text"
            spellcheck="false"
            placeholder="例如：D:\work\my-uniapp-project\dist\pack"
          />
          <button class="btn" type="button" :disabled="!isDesktop" @click="pickFolder('outputDir')">选择目录</button>
        </span>
      </label>

      <label class="field field-wide">
        <span class="field-label-row">
          <span class="field-label">远程打包忽略更改的文件（每行一个路径，用于联调切换）</span>
          <button class="btn btn-sm" type="button" :disabled="!isDesktop" @click="pickIgnoreFile">添加文件</button>
        </span>
        <textarea
          :value="ignoreFilesText"
          class="control ignore-textarea"
          rows="3"
          spellcheck="false"
          placeholder="例如：&#10;src/api/gov-auth.js&#10;src/config/area/areaConfig.json"
          @input="onIgnoreTextInput(($event.target as HTMLTextAreaElement).value)"
        />
        <span class="hint ignore-hint">
          仅「远程联调」云打包生效：上传前临时恢复为 Git HEAD（不改暂存区），进入云端排队后自动还原你的本地修改；若 HEAD 仍含 localhost 会拦截提醒。
        </span>
      </label>
    </div>

    <div class="card auto-install-card">
      <div class="set-row">
        <span class="set-text">
          <span class="set-label">打包完成后自动装机</span>
          <span class="set-desc">关闭时只保存 APK 到输出目录，不自动覆盖手机上已安装的应用。</span>
        </span>
        <MotionSwitch
          :checked="draft.autoInstall"
          :disabled="saving"
          label="打包完成后自动装机"
          @toggle="draft.autoInstall = !draft.autoInstall"
        />
      </div>
    </div>

    <h4 class="sub-title">ADB 联调</h4>
    <div class="field-grid">
      <label class="field field-wide">
        <span class="field-label">默认反向转发端口</span>
        <span class="port-row">
          <input
            v-model.number="draft.reversePort"
            class="control port-control"
            type="number"
            min="1"
            max="65535"
            placeholder="11034"
          />
          <span class="hint">用于一键 <code>adb reverse</code>，默认 11034</span>
        </span>
      </label>
    </div>

    <details class="advanced-group">
      <summary class="advanced-summary">
        <span>高级与工具路径（HBuilderX、CLI、ADB、证书与指定设备）</span>
        <span v-if="hasCustomAdvanced" class="tag">已自定义</span>
      </summary>
      <div class="field-grid advanced-body">
        <label class="field field-wide">
          <span class="field-label">HBuilderX 安装根目录（可选）</span>
          <span class="path-row">
            <input
              v-model="draft.hbuilderRoot"
              class="control"
              type="text"
              spellcheck="false"
              placeholder="留空则直接使用下方 cli.exe；填写后自动匹配最高版本号"
            />
            <button class="btn" type="button" :disabled="!isDesktop" @click="pickFolder('hbuilderRoot')">选择目录</button>
          </span>
        </label>

        <label class="field field-wide">
          <span class="field-label">HBuilderX cli.exe 路径（可选）</span>
          <span class="path-row">
            <input
              v-model="draft.cli"
              class="control"
              type="text"
              spellcheck="false"
              placeholder="留空时按 HBuilderX 安装根目录查找"
            />
            <button class="btn" type="button" :disabled="!isDesktop" @click="pickFile('cli')">选择文件</button>
          </span>
        </label>

        <label class="field">
          <span class="field-label">证书类型</span>
          <span class="select-wrap">
            <select v-model="draft.packType" class="control select">
              <option value="3">3 · 云端证书</option>
              <option value="0">0 · 自有证书</option>
              <option value="1">1 · 公共测试（旧版）</option>
            </select>
          </span>
        </label>

        <label class="field">
          <span class="field-label">默认调试设备序列号（可选）</span>
          <input
            v-model="draft.device"
            class="control"
            type="text"
            spellcheck="false"
            placeholder="留空自动使用唯一已授权设备"
          />
        </label>

        <label class="field field-wide">
          <span class="field-label">adb.exe 路径（可选）</span>
          <span class="path-row">
            <input
              v-model="draft.adb"
              class="control"
              type="text"
              spellcheck="false"
              placeholder="留空时使用系统 PATH 中的 adb"
            />
            <button class="btn" type="button" :disabled="!isDesktop" @click="pickFile('adb')">选择文件</button>
          </span>
        </label>
      </div>
    </details>

    <div class="save-row">
      <p role="status" :class="{ error: isError }">
        {{ dirty ? '有未保存的修改' : feedback || '更改后点击保存' }}
        <span v-if="dirty && isError"> · {{ feedback }}</span>
      </p>
      <div class="actions">
        <button
          v-if="draft.outputDir.trim()"
          class="btn"
          type="button"
          :disabled="!isDesktop"
          @click="openOutputDir"
        >
          打开输出目录
        </button>
        <MotionSaveButton
          label="保存打包设置"
          :busy="saving"
          :done="justSaved"
          :disabled="saving || !isDesktop"
          @click="save"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.path-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.path-row .control {
  flex: 1;
  min-width: 0;
}
.field-label-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.btn-sm {
  min-height: 28px !important;
  padding: 2px 10px !important;
  font-size: 12px !important;
}
.ignore-textarea {
  min-height: 76px !important;
  padding: 8px 12px !important;
  font-family: var(--font-mono, ui-monospace, monospace) !important;
  font-size: 12px !important;
  line-height: 1.55;
  resize: vertical;
}
.ignore-hint {
  margin: 0 !important;
}
.auto-install-card {
  margin-top: 16px;
}
.port-row {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.port-control {
  width: 120px;
  font-variant-numeric: tabular-nums;
}
.port-row .hint {
  margin: 0;
}
.advanced-group {
  margin-top: 20px;
}
.advanced-summary {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  list-style: none;
}
.advanced-body {
  padding-top: 12px;
}
</style>
