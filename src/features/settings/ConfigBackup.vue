<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import { state } from '@/store'

const props = defineProps<{ blocked: boolean }>()
type Counts = { before: number; after: number; added: number; removed: number; changed: number; reordered: boolean }
type Preview = { token: string; fileName: string; changes: { apps: Counts; categories: Counts; preferences: { label: string; before: string; after: string }[]; appDetails: string[]; categoryDetails: string[] }; warnings: string[] }
const preview = ref<Preview | null>(null)
const busy = ref<'export' | 'preview' | 'apply' | null>(null)
const feedback = ref('')
const failed = ref(false)
const previewHeading = ref<HTMLElement | null>(null)
const importButton = ref<HTMLButtonElement | null>(null)
let sequence = 0
let demoSignature = ''
const signature = () => JSON.stringify([state.apps, state.categories, state.settings.theme, state.ui.islandAppearance])
const displayValue = (value: string) => ({ dark: '深色', light: '浅色', midnight: '星舰仪表', mecha: '琥珀重工', scroll: '海棠书卷', pixel: '像素掌机', system: '跟随系统', left: '左上', center: '顶部居中', right: '右上', applications: '应用', clipboard: '剪贴板', translation: '翻译', files: '文件中转', audio: '调音台', true: '开启', false: '关闭', all: '全部' })[value] ?? value

async function discard(token: string): Promise<void> {
  if (isDesktop) await invokeHost('cancel_configuration_import', { token })
}
async function cancel(): Promise<void> {
  if (busy.value === 'apply') return
  ++sequence
  const token = preview.value?.token
  preview.value = null
  if (token) { try { await discard(token) } catch { /* Expired tokens cannot be applied by this UI. */ } }
  feedback.value = '已取消，配置未更改'; failed.value = false
  await nextTick(() => importButton.value?.focus({ preventScroll: true }))
}
async function exportConfig(): Promise<void> {
  if (busy.value || !isDesktop) return
  busy.value = 'export'; feedback.value = ''; failed.value = false
  try {
    const path = await invokeHost<string | null>('export_configuration')
    feedback.value = path ? `配置已导出：${path}` : '已取消导出'
  } catch (error) { failed.value = true; feedback.value = String(error) }
  finally { busy.value = null }
}
async function choose(): Promise<void> {
  if (busy.value || props.blocked) return
  const ticket = ++sequence
  const old = preview.value?.token
  preview.value = null; busy.value = 'preview'; feedback.value = ''; failed.value = false
  try {
    if (old) await discard(old)
    let result: Preview | null
    if (isDesktop) result = await invokeHost<Preview | null>('preview_configuration_import')
    else {
      demoSignature = signature()
      result = { token: 'browser-demo', fileName: '示例显示偏好.json', changes: {
        apps: { before: state.apps.length, after: state.apps.length, added: 0, removed: 0, changed: 0, reordered: false },
        categories: { before: state.categories.length, after: state.categories.length, added: 0, removed: 0, changed: 0, reordered: false },
        preferences: [{ label: '显示主题', before: state.settings.theme, after: state.settings.theme === 'light' ? 'dark' : 'light' }], appDetails: [], categoryDetails: []
      }, warnings: ['示例应用：本机路径未验证，桌面版会检查并列出失效目标。'] }
    }
    if (ticket !== sequence) { if (result) await discard(result.token); return }
    preview.value = result
    if (result) await nextTick(() => previewHeading.value?.focus())
    else feedback.value = '已取消选择，配置未更改'
  } catch (error) { if (ticket === sequence) { failed.value = true; feedback.value = String(error) } }
  finally { busy.value = null }
}
async function apply(): Promise<void> {
  if (!preview.value || busy.value || props.blocked) return
  busy.value = 'apply'; feedback.value = ''; failed.value = false
  try {
    if (isDesktop) {
      const result = await invokeHost<{ backupPath: string }>('apply_configuration_import', { token: preview.value.token })
      // The native host broadcasts the committed configuration to every window.
      feedback.value = `配置已恢复。原配置备份：${result.backupPath}`
    } else {
      if (demoSignature !== signature()) throw new Error('预览后配置已变化，请重新预览')
      state.settings.theme = (['light', 'dark', 'midnight', 'mecha', 'scroll', 'pixel', 'system'].includes(preview.value.changes.preferences[0]?.after ?? '') ? preview.value.changes.preferences[0]!.after : 'light') as typeof state.settings.theme
      feedback.value = '演示完成：仅切换当前网页主题，未写入本机配置或创建备份。'
    }
    preview.value = null
  } catch (error) { failed.value = true; feedback.value = String(error) }
  finally {
    busy.value = null
    if (!preview.value && state.ui.settingsOpen) await nextTick(() => importButton.value?.focus({ preventScroll: true }))
  }
}
watch(() => state.ui.settingsOpen, open => { if (!open && busy.value !== 'apply') void cancel() })
onBeforeUnmount(() => { ++sequence; if (preview.value && busy.value !== 'apply') void discard(preview.value.token).catch(() => {}) })
</script>

<template>
  <section class="configuration-backup" aria-labelledby="configuration-title" :aria-busy="Boolean(busy)">
    <h4 id="configuration-title" class="sub-title">配置备份 <span class="save-mode">预览后确认恢复</span></h4>
    <p class="hint">保存应用清单、分类和显示偏好。路径与网址只保留引用，换电脑后可能需要重新选择；请勿在应用目标中保存凭据。API 密钥、翻译设置、剪贴板、文件内容、声音配置和系统联动开关均不包含在内。</p>
    <p v-if="!isDesktop" class="hint">网页演示仅使用合成预览，不读取或备份本机配置。</p>
    <div class="actions">
      <button class="btn" :disabled="Boolean(busy) || !isDesktop" @click="exportConfig">{{ busy === 'export' ? '正在导出…' : '导出配置' }}</button>
      <button ref="importButton" class="btn" :disabled="Boolean(busy) || blocked" @click="choose">{{ busy === 'preview' ? '正在读取…' : isDesktop ? '选择配置文件' : '演示恢复预览' }}</button>
    </div>
    <p v-if="blocked" class="hint">应用管理或灵动岛有未保存的修改，请先保存或取消，再恢复配置。</p>
    <div v-if="preview" class="restore-preview">
      <h5 ref="previewHeading" tabindex="-1">恢复预览 · {{ preview.fileName }}</h5>
      <p class="hint">确认后将替换应用清单、分类和列出的显示偏好。桌面版会先备份当前配置；移出清单不会删除原文件，也不会启动导入的应用。</p>
      <dl class="change-counts">
        <div v-for="entry in ([['应用', preview.changes.apps], ['分类', preview.changes.categories]] as const)" :key="entry[0]">
          <dt>{{ entry[0] }} <strong>{{ entry[1].before }} → {{ entry[1].after }}</strong></dt>
          <dd>新增 {{ entry[1].added }} · 修改 {{ entry[1].changed }} · 移出 {{ entry[1].removed }}<span v-if="entry[1].reordered"> · 顺序变化</span></dd>
        </div>
      </dl>
      <ul v-if="preview.changes.preferences.length" class="change-list" aria-label="显示偏好差异"><li v-for="item in preview.changes.preferences" :key="item.label">{{ item.label }}：{{ displayValue(item.before) }} → {{ displayValue(item.after) }}</li></ul>
      <p v-else class="hint">显示偏好没有变化。</p>
      <details v-if="preview.changes.appDetails.length"><summary>应用差异（{{ preview.changes.appDetails.length }}）</summary><ul class="change-list"><li v-for="(detail, index) in preview.changes.appDetails" :key="index">{{ detail }}</li></ul></details>
      <details v-if="preview.changes.categoryDetails.length"><summary>分类差异（{{ preview.changes.categoryDetails.length }}）</summary><ul class="change-list"><li v-for="(detail, index) in preview.changes.categoryDetails" :key="index">{{ detail }}</li></ul></details>
      <details v-if="preview.warnings.length" open><summary>需要检查的路径（{{ preview.warnings.length }}）</summary><ul class="change-list"><li v-for="(warning, index) in preview.warnings" :key="index">{{ warning }}</li></ul></details>
      <div class="actions restore-actions">
        <button class="btn btn-primary" :disabled="Boolean(busy) || blocked" @click="apply">{{ busy === 'apply' ? '正在备份并恢复…' : isDesktop ? '备份并恢复' : '确认演示恢复' }}</button>
        <button class="btn" :disabled="Boolean(busy)" @click="cancel">取消恢复</button>
      </div>
    </div>
    <p class="hint" :class="{ error: failed }" role="status" aria-live="polite">{{ feedback }}</p>
  </section>
</template>

<style scoped>
.restore-preview { margin-top: 16px; padding: 16px; border: 1px solid var(--line-strong); border-radius: 10px; background: var(--surface); min-width: 0; overflow-wrap: anywhere; }
h5 { margin: 0 0 8px; font-size: 14px; }
h5:focus-visible { outline: 2px solid var(--text); outline-offset: 4px; }
.change-counts { margin: 16px 0; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; font-size: 13px; }
.change-counts dt { display: flex; flex-wrap: wrap; gap: 8px; }
.change-counts dd { margin: 4px 0 0; color: var(--text-2); line-height: 1.6; }
.change-list { padding-left: 20px; font-size: 13px; line-height: 1.7; }
details { margin-top: 12px; font-size: 13px; }
summary { cursor: pointer; padding: 4px 0; }
.restore-actions { margin-top: 16px; }
@media (max-width: 440px) { .change-counts { grid-template-columns: 1fr; } }
</style>
