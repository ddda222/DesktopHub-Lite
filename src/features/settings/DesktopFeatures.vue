<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { defaultIslandAppearance, invokeHost, isDesktop } from '@/lib/desktop/desktop'
import { pushToast, state } from '@/store'

const appearance = ref({ ...state.ui.islandAppearance })
const anchor = ref(state.ui.islandAnchor)
const hoverMs = ref(state.ui.islandHoverMs)
const leaveMs = ref(state.ui.islandLeaveMs)
const reducedMotion = ref(state.ui.islandReducedMotion)
const saving = ref(false)
const wechatBusy = ref(false)
const startupBusy = ref(false)
const startupError = ref('')
const startup = ref<{ enabled: boolean; conflict: boolean } | null>(null)
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
const validTiming = computed(() => Number.isInteger(hoverMs.value) && hoverMs.value >= 100 && hoverMs.value <= 2000 &&
  Number.isInteger(leaveMs.value) && leaveMs.value >= 100 && leaveMs.value <= 4000)

function restore(): void {
  appearance.value = defaultIslandAppearance()
  anchor.value = 'center'
  hoverMs.value = 320
  leaveMs.value = 450
  reducedMotion.value = false
}

async function save(): Promise<void> {
  if (saving.value || !isDesktop || !validTiming.value) return
  saving.value = true
  try {
    await invokeHost('save_island_appearance', {
      appearance: { ...appearance.value }, anchor: anchor.value,
      hoverMs: hoverMs.value, leaveMs: leaveMs.value, reducedMotion: reducedMotion.value
    })
    pushToast('灵动岛设置已保存', 'success')
  } catch (error) { pushToast(String(error), 'error') }
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
  appearance.value = { ...state.ui.islandAppearance }
  anchor.value = state.ui.islandAnchor
  hoverMs.value = state.ui.islandHoverMs
  leaveMs.value = state.ui.islandLeaveMs
  reducedMotion.value = state.ui.islandReducedMotion
  loadGeneralStatus()
})
</script>

<template>
  <section class="features" aria-label="灵动岛与系统">
    <h4>灵动岛</h4>
    <div class="box">
      <div class="fields">
        <label>位置<select v-model="anchor"><option value="left">左上</option><option value="center">顶部居中</option><option value="right">右上</option></select></label>
        <label>外观预设<select v-model="appearance.preset"><option value="classic">经典</option><option value="glass">玻璃质感（渐变）</option></select></label>
        <label>大小 {{ appearance.scale }}%<input v-model.number="appearance.scale" type="range" min="75" max="125" /></label>
        <label>背景透明度 {{ 100 - appearance.opacity }}%<input v-model.number="appearance.opacity" type="range" min="75" max="100" /></label>
        <label>点缀颜色<input type="color" :value="appearance.tint || '#5685ee'" @input="appearance.tint = ($event.target as HTMLInputElement).value" /></label>
        <label>展开延时（毫秒）<input v-model.number="hoverMs" type="number" min="100" max="2000" /></label>
        <label>收起延时（毫秒）<input v-model.number="leaveMs" type="number" min="100" max="4000" /></label>
      </div>
      <p>玻璃质感使用半透明渐变，不读取或模糊桌面内容。系统启用减少动态效果时也会关闭动画。</p>
      <div class="actions">
        <label><input v-model="reducedMotion" type="checkbox" /> 减少动画</label>
        <label><input v-model="appearance.avoidFullscreen" type="checkbox" /> 全屏时暂时隐藏</label>
        <button type="button" @click="restore">恢复默认值</button>
        <button type="button" :disabled="saving || !validTiming || !isDesktop" @click="save">{{ saving ? '保存中…' : '应用设置' }}</button>
      </div>
    </div>

    <h4>微信普通提醒</h4>
    <div class="box">
      <button type="button" role="switch" aria-label="接收微信普通提醒" :aria-checked="state.ui.wechatEnabled"
        :disabled="wechatBusy || !isDesktop" @click="toggleWechat">{{ state.ui.wechatEnabled ? '关闭提醒' : '开启提醒' }}</button>
      <p role="status">{{ wechatStatus }}。只接收请求关注，不读取聊天内容；不统计消息数。</p>
      <button v-if="state.ui.wechatAttention" type="button" @click="invokeHost('clear_wechat_attention')">清除待关注</button>
    </div>

    <h4>登录启动与更新</h4>
    <div class="box">
      <button type="button" role="switch" aria-label="Windows 登录时启动" :aria-checked="startup?.enabled ?? false"
        :disabled="!startup || startup.conflict || startupBusy" @click="toggleStartup">
        {{ startup?.enabled ? '关闭登录启动' : '开启登录启动' }}
      </button>
      <p v-if="startupError || startup?.conflict" role="status">{{ startupError || '发现其他路径的同名启动项，未覆盖或移除。' }}</p>
      <p v-else>只有点击开关才会修改当前用户的启动项。</p>
      <label><input v-model="appearance.autoCheckUpdates" type="checkbox" /> 下次启动时检查更新（改动后点「应用设置」）</label>
      <p>默认不自动检查；不自动下载或重启。便携版目前没有签名升级渠道。</p>
      <div class="actions">
        <button type="button" :disabled="updateBusy || !isDesktop" @click="checkUpdate">{{ updateBusy ? '检查中…' : update?.state === 'error' ? '重试检查' : '检查更新' }}</button>
        <button type="button" :disabled="!isDesktop" @click="openReleases">在 GitHub 查看发布</button>
        <button type="button" disabled title="未配置签名更新清单">安装更新（未配置）</button>
      </div>
      <p v-if="update" role="status">当前 {{ update.current }}<template v-if="update.version"> · 发现 {{ update.version }}</template>：{{ update.details }}</p>
      <pre v-if="update?.notes">{{ update.notes }}</pre>
    </div>
  </section>
</template>

<style scoped>
.features h4 { margin: 14px 0 7px; font-size: 13px; }
.box { background: var(--surface-2); border: 1px solid var(--line); border-radius: 10px; padding: 10px; }
.fields { display: grid; grid-template-columns: repeat(auto-fit, minmax(145px, 1fr)); gap: 10px; }
.fields label { display: grid; gap: 3px; }
label, button, p, pre { font-size: 12px; }
p { margin: 6px 0; color: var(--text-2); }
button, select, input:not([type="range"]):not([type="checkbox"]) { color: var(--text); background: var(--surface); border: 1px solid var(--line); border-radius: 5px; padding: 5px; }
button:disabled { opacity: .45; }
.actions { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
pre { max-height: 160px; white-space: pre-wrap; overflow: auto; }
</style>
