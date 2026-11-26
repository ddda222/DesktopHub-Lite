<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { defaultIslandAppearance, invokeHost, isDesktop } from '@/lib/desktop/desktop'
import { pushToast, state } from '@/store'

const appearance = ref({ ...defaultIslandAppearance(), ...state.ui.islandAppearance })
const anchor = ref(state.ui.islandAnchor)
const hoverMs = ref(state.ui.islandHoverMs)
const leaveMs = ref(state.ui.islandLeaveMs)
const reducedMotion = ref(state.ui.islandReducedMotion)
const saving = ref(false)
const feedback = ref('')
const failed = ref(false)
const signature = () => JSON.stringify([appearance.value, anchor.value, hoverMs.value, leaveMs.value, reducedMotion.value])
const applied = ref(signature())
const dirty = computed(() => signature() !== applied.value)
const emit = defineEmits<{ 'dirty-change': [value: boolean] }>()
watch(() => dirty.value || saving.value, value => emit('dirty-change', value), { immediate: true })
function discardDraft(): void {
  appearance.value = { ...defaultIslandAppearance(), ...state.ui.islandAppearance }
  anchor.value = state.ui.islandAnchor; hoverMs.value = state.ui.islandHoverMs; leaveMs.value = state.ui.islandLeaveMs
  reducedMotion.value = state.ui.islandReducedMotion; applied.value = signature(); feedback.value = ''; failed.value = false
}
watch(() => JSON.stringify([state.ui.islandAppearance, state.ui.islandAnchor, state.ui.islandHoverMs, state.ui.islandLeaveMs, state.ui.islandReducedMotion]), () => { if (!dirty.value && !saving.value) discardDraft() })
watch(() => state.ui.islandAppearance.motion, motion => {
  if (dirty.value) appearance.value.motion = motion
  else if (!saving.value) discardDraft()
})
const wechatBusy = ref(false)
const startupBusy = ref(false)
const startupError = ref('')
const startup = ref<{ enabled: boolean; conflict: boolean; stale?: boolean; systemDisabled?: boolean } | null>(null)
const updateBusy = ref(false)
const update = ref<{ state: string; current: string; version?: string; notes?: string; details: string } | null>(null)
const wechatStatus = computed(() => {
  switch (state.ui.wechatStatus) {
    case 'starting': return '正在注册监听'
    case 'listening': return '正在监听 Windows 任务栏提醒'
    case 'error': return '注册未成功，稍后重试'
    default: return '已关闭'
  }
})
const validTiming = computed(() => Number.isInteger(hoverMs.value) && appearance.value.scale >= 75 && appearance.value.scale <= 125 && hoverMs.value >= 100 && hoverMs.value <= 2000 &&
  Number.isInteger(leaveMs.value) && leaveMs.value >= 100 && leaveMs.value <= 4000)
const validTelemetry = computed(() =>
  ['off', 'smart', 'always'].includes(appearance.value.telemetryMode) &&
  (appearance.value.telemetryMode === 'off' || appearance.value.telemetryNet || appearance.value.telemetryCpu || appearance.value.telemetryMemory)
)
const validSettings = computed(() => validTiming.value && validTelemetry.value)

function restore(): void {
  appearance.value = defaultIslandAppearance()
  anchor.value = 'center'
  hoverMs.value = 320
  leaveMs.value = 450
  reducedMotion.value = false
}

async function save(): Promise<void> {
  if (saving.value || !isDesktop || !validSettings.value) return
  saving.value = true
  const submitted = signature()
  try {
    await invokeHost('save_island_appearance', {
      appearance: { ...appearance.value }, anchor: anchor.value,
      hoverMs: hoverMs.value, leaveMs: leaveMs.value, reducedMotion: reducedMotion.value
    })
    applied.value = submitted; failed.value = false; feedback.value = '设置已应用'
    pushToast('灵动岛设置已保存', 'success')
  } catch (error) { failed.value = true; feedback.value = `应用失败：${String(error)}`; pushToast(String(error), 'error') }
  finally { saving.value = false }
}

async function toggleWechat(): Promise<void> {
  if (wechatBusy.value) return
  wechatBusy.value = true
  try { await invokeHost('set_wechat_enabled', { enabled: !state.ui.wechatEnabled }) }
  catch (error) { pushToast(String(error), 'error') }
  finally { wechatBusy.value = false }
}

async function toggleStartup(): Promise<void> {
  if (startupBusy.value || !startup.value) return
  startupBusy.value = true
  try { startup.value = await invokeHost('set_startup_enabled', { enabled: !startup.value.enabled }); startupError.value = '' }
  catch (error) { startupError.value = String(error) }
  finally { startupBusy.value = false }
}

async function checkUpdate(): Promise<void> {
  if (updateBusy.value) return
  updateBusy.value = true
  try { update.value = await invokeHost('check_deskhub_updates') }
  catch (error) { pushToast(String(error), 'error') }
  finally { updateBusy.value = false }
}

function openReleases(): void {
  void invokeHost('open_deskhub_release_page').catch(error => pushToast(String(error), 'error'))
}

function loadGeneralStatus(): void {
  if (!isDesktop) return
  void invokeHost<typeof startup.value>('startup_status')
    .then(result => { startup.value = result; startupError.value = '' })
    .catch(error => { startupError.value = String(error) })
  void invokeHost<typeof update.value>('last_deskhub_update')
    .then(result => { if (!updateBusy.value && result) update.value = result })
    .catch(() => {})
}
onMounted(loadGeneralStatus)
watch(() => state.ui.settingsOpen, open => {
  if (!open) return
  appearance.value = { ...defaultIslandAppearance(), ...state.ui.islandAppearance }
  anchor.value = state.ui.islandAnchor
  hoverMs.value = state.ui.islandHoverMs
  leaveMs.value = state.ui.islandLeaveMs
  reducedMotion.value = state.ui.islandReducedMotion
  applied.value = signature(); feedback.value = ''; failed.value = false
  loadGeneralStatus()
})
</script>

<template>
  <div class="save-row general-actions" role="group" aria-label="灵动岛设置操作">
    <p role="status" :class="{ error: failed }">{{ dirty ? '灵动岛设置有尚未应用的修改' : feedback || '灵动岛设置' }}<span v-if="dirty && failed"> · {{ feedback }}</span><span v-if="!isDesktop"> · 桌面版可应用</span></p>
    <div class="actions">
      <button v-if="dirty" type="button" class="btn" :disabled="saving" @click="discardDraft">撤销修改</button>
      <button type="button" class="btn" :disabled="saving" @click="restore">恢复默认值</button>
      <button type="button" class="btn btn-primary" :disabled="saving || !validSettings || !isDesktop" @click="save">{{ saving ? '应用中…' : '应用设置' }}</button>
    </div>
  </div>
  <slot />
  <section class="features" aria-label="灵动岛与系统">
    <h4 class="sub-title">灵动岛 <span class="save-mode">手动应用</span></h4>
    <div class="field-grid">
      <label class="field"><span class="field-label">停靠位置</span><select v-model="anchor" class="control"><option value="left">左上</option><option value="center">顶部居中</option><option value="right">右上</option></select></label>
      <label class="field"><span class="field-label">显示大小 <output>{{ appearance.scale }}%</output></span><input v-model.number="appearance.scale" class="size-slider" type="range" min="75" max="125" step="5" aria-label="灵动岛显示大小" :aria-valuetext="`${appearance.scale}%`" /></label>
      <label class="field field-wide">
        <span class="field-label">胶囊微读数</span>
        <select v-model="appearance.telemetryMode" class="control" aria-label="胶囊微读数显示模式">
          <option value="smart">智能浮现（流量或高占用时显示）</option>
          <option value="always">常驻显示（替代时间）</option>
          <option value="off">关闭</option>
        </select>
      </label>
    </div>
    <div v-if="appearance.telemetryMode !== 'off'" class="telemetry-config">
      <div class="seg" role="group" aria-label="微读数指标">
        <label class="seg-item" :class="{ 'is-on': appearance.telemetryNet }"><input v-model="appearance.telemetryNet" class="sr-only" type="checkbox" />实时网速</label>
        <label class="seg-item" :class="{ 'is-on': appearance.telemetryCpu }"><input v-model="appearance.telemetryCpu" class="sr-only" type="checkbox" />处理器占用</label>
        <label class="seg-item" :class="{ 'is-on': appearance.telemetryMemory }"><input v-model="appearance.telemetryMemory" class="sr-only" type="checkbox" />内存占用</label>
      </div>
      <p class="hint telemetry-hint">{{ appearance.telemetryMode === 'smart'
        ? '平时保持简洁时间；当网速 ≥ 64 KB/s、处理器 ≥ 35% 或内存 ≥ 80% 时自动切为读数，回落后保持 3 秒平滑收回。仅统计活动物理以太网与 Wi-Fi。'
        : '胶囊收起时常驻显示所选指标；最大化或全屏收为细线时自动休眠停止采集。' }}</p>
    </div>
    <div class="feature-options">
      <div class="set-row">
        <span class="set-text">
          <span class="set-label">应用网格显示名称</span>
          <span class="set-desc">关闭后仅显示紧凑应用图标，悬停图标时查看完整名称。</span>
        </span>
        <button
          class="switch"
          type="button"
          role="switch"
          aria-label="应用网格显示名称"
          :class="{ 'is-on': appearance.showAppNames }"
          :aria-checked="appearance.showAppNames"
          @click="appearance.showAppNames = !appearance.showAppNames; if (!isDesktop) state.ui.islandAppearance.showAppNames = appearance.showAppNames"
        ><span class="switch-knob" aria-hidden="true" /></button>
      </div>
      <label class="set-row switch-row">
        <span class="set-text">
          <span class="set-label">光轨舱门</span>
          <span class="set-desc">沿边缘巡光，展开时向两侧分流，正文保持实底。</span>
        </span>
        <input v-model="appearance.webgl" class="sr-only" type="checkbox" />
        <span class="switch" :class="{ 'is-on': appearance.webgl }" aria-hidden="true"><span class="switch-knob" /></span>
      </label>
      <label class="set-row switch-row">
        <span class="set-text">
          <span class="set-label">减少动画</span>
          <span class="set-desc">停用动态光效，保留清晰边框。</span>
        </span>
        <input v-model="reducedMotion" class="sr-only" type="checkbox" />
        <span class="switch" :class="{ 'is-on': reducedMotion }" aria-hidden="true"><span class="switch-knob" /></span>
      </label>
      <label class="set-row switch-row">
        <span class="set-text">
          <span class="set-label">最大化或全屏时收为细线</span>
          <span class="set-desc">悬停驻留或点击展开，移开后自动收回。</span>
        </span>
        <input v-model="appearance.avoidFullscreen" class="sr-only" type="checkbox" />
        <span class="switch" :class="{ 'is-on': appearance.avoidFullscreen }" aria-hidden="true"><span class="switch-knob" /></span>
      </label>
      <label class="set-row switch-row">
        <span class="set-text">
          <span class="set-label">在截图和屏幕共享中隐藏</span>
          <span class="set-desc">仅隐藏灵动岛，本机屏幕仍可见，共享前请实测。</span>
        </span>
        <input v-model="appearance.hideFromCapture" class="sr-only" type="checkbox" :disabled="saving" aria-describedby="capture-scope" />
        <span class="switch" :class="{ 'is-on': appearance.hideFromCapture }" aria-hidden="true"><span class="switch-knob" /></span>
      </label>
    </div>
    <p v-if="appearance.hideFromCapture" class="inline-feedback" role="status">远程操作时，你自己也可能看不到灵动岛。可从托盘打开设置，取消勾选并应用以恢复。</p>
    <p v-if="state.ui.islandCaptureError" class="inline-feedback error" role="alert">{{ state.ui.islandCaptureError }}。灵动岛已暂时隐藏，请重试应用设置，或关闭隐藏选项后应用。</p>
    <details>
      <summary>更多行为设置与捕获说明</summary>
      <div class="field-grid advanced-fields">
        <label class="field"><span class="field-label">展开延时（毫秒）</span><input v-model.number="hoverMs" class="control" type="number" min="100" max="2000" /></label>
        <label class="field"><span class="field-label">收起延时（毫秒）</span><input v-model.number="leaveMs" class="control" type="number" min="100" max="4000" /></label>
      </div>
      <label class="set-row switch-row">
        <span class="set-text">
          <span class="set-label">下次启动时检查更新</span>
          <span class="set-desc">不自动下载或重启，点击「应用设置」后生效。</span>
        </span>
        <input v-model="appearance.autoCheckUpdates" class="sr-only" type="checkbox" />
        <span class="switch" :class="{ 'is-on': appearance.autoCheckUpdates }" aria-hidden="true"><span class="switch-knob" /></span>
      </label>
      <p id="capture-scope" class="hint">捕获隐藏仅作用于胶囊、细线和展开面板；设置、独立翻译和钉图窗口仍可见。依赖 Windows 10 2004 及以上的支持，不保证所有采集方式生效。</p>
      <p class="hint">边缘光效使用 WebGL；系统减少动画或高对比度时停用光效。浅深色主题均保持正文不透明。</p>
      <p v-if="isDesktop" class="hint">当前已应用：{{ state.ui.islandAppearance.hideFromCapture ? '隐藏灵动岛' : '允许捕获灵动岛' }}</p>
    </details>
    <p v-if="!validTiming" class="inline-feedback error" role="alert">展开延时需为 100–2000 毫秒，收起延时需为 100–4000 毫秒，大小需为 75%–125%。</p>
    <p v-if="!validTelemetry" class="inline-feedback error" role="alert">开启胶囊微读数时，请至少勾选一项系统指标。</p>
    <h4 class="sub-title">提醒与启动 <span class="save-mode">即时生效</span></h4>
    <div class="set-row">
      <span class="set-text"><span class="set-label">微信普通提醒</span><span class="set-desc" role="status">{{ isDesktop ? wechatStatus : '桌面版可用' }}。只接收关注提醒，不读取聊天内容。</span></span>
      <button class="switch" :class="{ 'is-on': state.ui.wechatEnabled }" role="switch" aria-label="接收微信普通提醒" :aria-checked="state.ui.wechatEnabled" :disabled="wechatBusy || !isDesktop" @click="toggleWechat"><span class="switch-knob" /></button>
    </div>
    <button v-if="state.ui.wechatAttention" class="btn" :disabled="!isDesktop" @click="invokeHost('clear_wechat_attention').catch(error => pushToast(String(error), 'error'))">清除待关注</button>
    <div class="set-row">
      <span class="set-text"><span class="set-label">Windows 登录时启动</span><span class="set-desc">{{ isDesktop ? '仅修改当前用户的启动项。' : '桌面版可用。' }}</span></span>
      <button class="switch" :class="{ 'is-on': startup?.enabled }" role="switch" aria-label="Windows 登录时启动" :aria-checked="startup?.enabled ?? false" :disabled="!startup || startup.conflict || startupBusy || !isDesktop" @click="toggleStartup"><span class="switch-knob" /></button>
    </div>
    <p v-if="startupError || startup?.conflict" class="inline-feedback error" role="status">{{ startupError || '发现其他有效路径的同名启动项，未覆盖或移除。' }}</p>
    <p v-else-if="startup?.stale" class="inline-feedback" role="status">原自启路径已失效，开启右侧开关即可更新为当前程序。</p>
    <p v-else-if="startup?.systemDisabled" class="inline-feedback" role="status">已在系统「启动应用 / 任务管理器」中被禁用，重新开启右侧开关即可恢复。</p>

    <h4 class="sub-title">版本更新</h4>
    <div class="actions"><button class="btn" :disabled="updateBusy || !isDesktop" @click="checkUpdate">{{ updateBusy ? '检查中…' : update?.state === 'error' ? '重试检查' : '检查更新' }}</button><button class="btn" :disabled="!isDesktop" @click="openReleases">查看 GitHub 发布</button></div>
    <p class="hint">便携版手动更新；不会自动安装或重启。</p>
    <p v-if="update" class="inline-feedback" :class="{ error: update.state === 'error' }" role="status">当前 {{ update.current }}<template v-if="update.version"> · 发现 {{ update.version }}</template>：{{ update.details }}</p>
    <details v-if="update?.notes"><summary>查看更新说明</summary><pre>{{ update.notes }}</pre></details>
  </section>
</template>

<style scoped>
.settings-ui .general-actions { position: sticky; top: 0; z-index: 1; margin: 8px 0 16px; padding: 12px 0; background: var(--surface); border-bottom: 1px solid var(--line); }
.telemetry-config { margin-top: 12px; display: flex; flex-direction: column; align-items: flex-start; gap: 6px; }
.telemetry-hint { margin: 2px 0 0; }
.sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
.feature-options { margin-top: 14px; }
.size-slider {
  appearance: none; -webkit-appearance: none;
  width: 100%; min-width: 0; height: 36px; margin: 0; background: transparent; accent-color: var(--text); cursor: pointer;
}
.size-slider::-webkit-slider-runnable-track {
  height: 7px; border-radius: 99px;
  background: var(--surface-3); box-shadow: inset 0 0 0 1px var(--line-strong);
}
.size-slider::-webkit-slider-thumb {
  -webkit-appearance: none; appearance: none; box-sizing: border-box;
  width: 18px; height: 18px; margin-top: -5.5px; border-radius: 50%;
  border: 2px solid var(--text); background: var(--surface);
  box-shadow: 0 1px 3px rgb(0 0 0 / .2), 0 0 0 2px var(--surface);
  transition: transform 120ms ease;
}
.size-slider:hover::-webkit-slider-thumb, .size-slider:active::-webkit-slider-thumb { transform: scale(1.1); }
.size-slider::-moz-range-track {
  height: 7px; border-radius: 99px;
  background: var(--surface-3); box-shadow: inset 0 0 0 1px var(--line-strong);
}
.size-slider::-moz-range-thumb {
  box-sizing: border-box; width: 18px; height: 18px; border-radius: 50%;
  border: 2px solid var(--text); background: var(--surface);
  box-shadow: 0 1px 3px rgb(0 0 0 / .2), 0 0 0 2px var(--surface);
}
.field-label output { float: right; color: var(--text); font-weight: 600; font-variant-numeric: tabular-nums; }
.advanced-fields { margin: 12px 0; }
pre { white-space: pre-wrap; overflow-wrap: anywhere; font-size: 12px; color: var(--text-2); }
</style>
