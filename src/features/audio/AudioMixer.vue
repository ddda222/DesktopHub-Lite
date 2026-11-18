<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import { mockProfiles, mockSessions } from './mock'
const props = withDefaults(defineProps<{ active?: boolean }>(), { active: true })
interface Session { id: string; name: string; deviceId: string; deviceName: string; appKey: string | null; pid: number; volume: number; muted: boolean; active: boolean }
interface Setting { appKey: string; deviceId: string; volume: number; muted: boolean }
interface Profile { name: string; settings: Setting[] }
interface Snapshot { sessions: Session[]; warnings: string[] }
const sessions = ref<Session[]>(isDesktop ? [] : mockSessions)
const profiles = ref<Profile[]>(isDesktop ? [] : mockProfiles)
const profileName = ref('')
const notice = ref(isDesktop ? '应用播放声音后会出现在这里。配置只在点击时应用。' : '合成会话，可体验独立音量与静音。')
const error = ref('')
const readError = ref('')
const warnings = ref<string[]>([])
const busy = ref(false)
const exclusive = ref(false)
const drafts = ref<Record<string, number>>({})
const draftVersions: Record<string, number> = {}
const queuedUpdates = new Map<string, { volume?: number; muted?: boolean; revision: number }>()
let writingSession = false
let disposed = false, sequence = 0
let timer: ReturnType<typeof setTimeout> | undefined
const activeProfile = computed(() => profiles.value.find(p => p.settings.length && p.settings.every(s => { const rows = sessions.value.filter(row => row.appKey === s.appKey && row.deviceId === s.deviceId); return rows.length && rows.every(row => Math.abs(row.volume - s.volume) <= 1 && row.muted === s.muted) }))?.name)
function draftVolume(id: string, event: Event): void { sequence++; draftVersions[id] = (draftVersions[id] ?? 0) + 1; drafts.value[id] = Number((event.target as HTMLInputElement).value) }
function displayedVolume(session: Session): number { return Math.round(Math.max(0, Math.min(100, drafts.value[session.id] ?? session.volume))) }
async function refresh(): Promise<void> {
  if (!isDesktop || busy.value || Object.keys(drafts.value).length || !props.active) return
  const ticket = ++sequence
  try { const snapshot = await invokeHost<Snapshot>('list_audio_sessions'); if (!disposed && ticket === sequence) { sessions.value = snapshot.sessions; warnings.value = snapshot.warnings; readError.value = '' } }
  catch (e) { if (!disposed && ticket === sequence) { readError.value = String(e); sessions.value = [] } }
}
async function update(session: Session, volume?: number, muted?: boolean): Promise<void> {
  if (exclusive.value || disposed) return
  const previous = queuedUpdates.get(session.id)
  queuedUpdates.set(session.id, { volume: volume ?? previous?.volume, muted: muted ?? previous?.muted, revision: draftVersions[session.id] ?? 0 })
  if (writingSession) return
  writingSession = true; busy.value = true; sequence++; error.value = ''
  try {
    while (queuedUpdates.size && !disposed) {
      const [id, change] = queuedUpdates.entries().next().value!
      queuedUpdates.delete(id)
      try {
        const row = sessions.value.find(item => item.id === id)
        if (isDesktop) {
          const actual = await invokeHost<Session>('update_audio_session', { id, volume: change.volume ?? null, muted: change.muted ?? null })
          if (!disposed && row) Object.assign(row, actual)
        } else if (row) {
          if (change.volume !== undefined) row.volume = change.volume
          if (change.muted !== undefined) row.muted = change.muted
        }
        if (!disposed) notice.value = isDesktop ? `${row?.name ?? '应用'}：已读取系统实际状态` : '已更新网页模拟，不改变系统音量。'
      } catch (e) {
        if (!disposed) {
          error.value = String(e)
          if (isDesktop) { try { const snapshot = await invokeHost<Snapshot>('list_audio_sessions'); if (!disposed) { sessions.value = snapshot.sessions; warnings.value = snapshot.warnings } } catch { if (!disposed) sessions.value = [] } }
        }
      } finally {
        if (!queuedUpdates.has(id) && (draftVersions[id] ?? 0) === change.revision) delete drafts.value[id]
      }
    }
  }
  finally { writingSession = false; busy.value = false }
}
async function apply(index: number): Promise<void> {
  if (busy.value) return
  const profile = profiles.value[index]!
  busy.value = true; exclusive.value = true; sequence++; error.value = ''
  try {
    if (isDesktop) {
      const result = await invokeHost<{ applied: number; skipped: number; failed: string[]; snapshot: Snapshot }>('apply_audio_profile', { name: profile.name })
      sessions.value = result.snapshot.sessions; warnings.value = result.snapshot.warnings
      notice.value = `「${profile.name}」：成功 ${result.applied} 个会话，跳过 ${result.skipped} 个未运行应用/设备，失败 ${result.failed.length} 个。`
      error.value = result.failed.join('；')
    } else {
      for (const setting of profile.settings) for (const row of sessions.value.filter(s => s.appKey === setting.appKey && s.deviceId === setting.deviceId)) { row.volume = setting.volume; row.muted = setting.muted }
      notice.value = `已在网页中模拟「${profile.name}」，没有改变系统声音。`
    }
  } catch (e) { error.value = String(e) }
  finally { busy.value = false; exclusive.value = false }
}
async function save(): Promise<void> {
  if (busy.value) return
  const name = profileName.value.trim()
  if (!name) { error.value = '先给声音配置起个名字。'; return }
  const settings: Setting[] = []
  for (const row of sessions.value) {
    if (!row.appKey) continue
    const previous = settings.find(s => s.appKey === row.appKey && s.deviceId === row.deviceId)
    if (previous && (previous.volume !== row.volume || previous.muted !== row.muted)) { error.value = `${row.name} 在同一设备有多个不同音量的会话，请先统一再保存。`; return }
    if (!previous) settings.push({ appKey: row.appKey, deviceId: row.deviceId, volume: row.volume, muted: row.muted })
  }
  if (!settings.length) { error.value = '没有可保存的应用会话。'; return }
  const profile = { name, settings }
  busy.value = true; exclusive.value = true; error.value = ''
  try {
    if (isDesktop) profiles.value = await invokeHost<Profile[]>('save_audio_profile', { profile })
    else { const index = profiles.value.findIndex(p => p.name === name); if (index >= 0) profiles.value[index] = profile; else if (profiles.value.length < 16) profiles.value.push(profile); else throw new Error('最多保存 16 个配置') }
    profileName.value = ''; notice.value = `已保存「${name}」${isDesktop ? '到本机' : '到本次网页演示'}。${sessions.value.some(s => !s.appKey) ? '无法识别应用身份的会话未保存。' : ''}`
  } catch (e) { error.value = String(e) }
  finally { busy.value = false; exclusive.value = false }
}
function poll(): void { if (!document.hidden) void refresh(); timer = setTimeout(poll, 2500) }
watch(() => props.active, active => { if (active) void refresh() })
onMounted(async () => {
  if (isDesktop) { try { const rows = await invokeHost<Profile[]>('list_audio_profiles'); if (!disposed) profiles.value = rows } catch (e) { error.value = String(e) } }
  if (!disposed) poll()
})
onBeforeUnmount(() => { disposed = true; sequence++; queuedUpdates.clear(); clearTimeout(timer) })
</script>

<template>
  <section class="audio-mixer" aria-label="应用声音调音台">
    <div class="mixer-heading"><span>应用声音</span><button v-if="isDesktop" :disabled="busy" @click="refresh">刷新会话</button><span class="simulation">{{ isDesktop ? 'Windows 实际会话' : '网页模拟' }}</span></div>
    <div class="sound-profiles" aria-label="声音配置"><button v-for="(profile, i) in profiles" :key="profile.name" :disabled="busy" :aria-pressed="activeProfile === profile.name" @click="apply(i)">{{ profile.name }}</button></div>
    <p v-if="!sessions.length" class="mixer-notice">{{ readError ? '暂时无法取得会话，请重试。' : '暂时没有音频会话。先让应用播放声音，再刷新。' }}</p>
    <div v-for="session in sessions" :key="session.id" class="sound-channel" :class="{ muted: session.muted }">
      <span class="sound-icon" aria-hidden="true">♫</span>
      <div class="sound-body">
        <div class="channel-heading"><strong :title="session.name">{{ session.name }}</strong><span v-if="session.muted" class="mute-status">已静音</span></div>
        <small class="device-name">{{ session.deviceName }} · {{ session.active ? '播放中' : '空闲' }}</small>
        <div class="volume-control" :style="{ '--volume-progress': `${displayedVolume(session)}%` }">
          <input class="volume-slider" :value="displayedVolume(session)" :disabled="exclusive" type="range" min="0" max="100" step="1" :aria-label="`${session.name}音量`" :aria-valuetext="`${displayedVolume(session)}%${session.muted ? '，已静音' : ''}`" @input="draftVolume(session.id, $event)" @change="update(session, Number(($event.target as HTMLInputElement).value))" />
          <output class="volume-readout" aria-hidden="true"><b>{{ displayedVolume(session) }}</b><span>%</span></output>
        </div>
      </div>
      <button class="mute" :disabled="busy" :aria-pressed="session.muted" :aria-label="`${session.muted ? '取消' : ''}静音${session.name}`" @click="update(session, undefined, !session.muted)">{{ session.muted ? '恢复' : '静音' }}</button>
    </div>
    <form class="save-profile" @submit.prevent="save"><input v-model="profileName" maxlength="20" placeholder="如：开会、游戏（同名覆盖）" aria-label="声音配置名称" /><button type="submit" :disabled="busy || !sessions.length">保存配置</button></form>
    <p v-if="error" class="mixer-error" role="alert">{{ error }}</p><p v-if="readError" class="mixer-error" role="alert">{{ readError }}</p><p v-for="warning in warnings" :key="warning" class="mixer-notice">{{ warning }}</p>
    <p class="mixer-notice" role="status">{{ notice }}</p>
    <p class="mixer-boundary">{{ isDesktop ? '配置匹配同一应用与输出设备，未运行的应用会跳过。不会调整系统总音量。' : '合成会话 · 配置仅保留在当前页面 · 不控制 Windows 音量' }}</p>
  </section>
</template>

<style scoped>
.audio-mixer { color:var(--text); font-size:12px; }
.mixer-heading { display:flex; justify-content:space-between; align-items:center; margin-bottom:12px; font-weight:600; }
.simulation { font-size:10px; font-weight:400; padding:3px 6px; border:1px dashed var(--text-3); border-radius:4px; }
.sound-profiles { display:flex; flex-wrap:wrap; gap:6px; margin-bottom:12px; }
.sound-profiles button { padding:5px 12px; border:1px solid var(--line-strong); border-radius:8px; background:var(--surface-2); font-size:11px; transition:background 120ms ease,border-color 120ms ease,color 120ms ease; }
.sound-profiles button:hover:not([aria-pressed=true]):not(:disabled) { background:var(--hover); border-color:var(--text-3); }
.sound-profiles button[aria-pressed=true] { background:var(--text); color:var(--surface); border-color:var(--text); font-weight:600; }
.sound-channel { display:flex; align-items:center; gap:12px; padding:12px 0; border-top:1px solid var(--line); }
.sound-icon { display:grid; place-items:center; width:34px; height:34px; flex:none; font-size:22px; background:var(--surface-2); border:1px solid var(--line); border-radius:9px; }
.sound-body { flex:1; min-width:0; }
.channel-heading { display:flex; align-items:center; gap:8px; margin-bottom:5px; }
.channel-heading strong { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-weight:600; }
.mute-status { flex:none; color:var(--text-2); font-size:10px; padding:2px 5px; border:1px dashed var(--text-3); border-radius:4px; }
.device-name { display:block; margin:0 0 4px; color:var(--text-2); font-size:10px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }.mixer-error { color:var(--danger); font-size:11px; line-height:1.7; }
.volume-control { --volume-fill:var(--text); display:grid; grid-template-columns:minmax(0,1fr) 52px; align-items:center; gap:12px; }
.volume-slider { appearance:none; -webkit-appearance:none; display:block; min-width:0; width:100%; height:32px; margin:0; padding:0; border:0; background:transparent; cursor:pointer; accent-color:var(--text); }
.volume-slider::-webkit-slider-runnable-track { height:7px; border-radius:99px; box-shadow:inset 0 0 0 1px var(--line-strong); background:linear-gradient(to right,var(--volume-fill) 0 var(--volume-progress),var(--surface-3) var(--volume-progress) 100%); }
.volume-slider::-webkit-slider-thumb { -webkit-appearance:none; appearance:none; box-sizing:border-box; width:17px; height:17px; margin-top:-5px; border:2px solid var(--text); border-radius:50%; background:var(--surface); box-shadow:0 1px 3px rgb(0 0 0 / .22), 0 0 0 2px var(--surface); transition:transform 120ms ease; }
.volume-slider::-moz-range-track { height:7px; border-radius:99px; box-shadow:inset 0 0 0 1px var(--line-strong); background:linear-gradient(to right,var(--volume-fill) 0 var(--volume-progress),var(--surface-3) var(--volume-progress) 100%); }
.volume-slider::-moz-range-thumb { box-sizing:border-box; width:17px; height:17px; border:2px solid var(--text); border-radius:50%; background:var(--surface); box-shadow:0 1px 3px rgb(0 0 0 / .22), 0 0 0 2px var(--surface); }
.volume-slider:hover::-webkit-slider-thumb,.volume-slider:active::-webkit-slider-thumb { transform:scale(1.12); }
.volume-slider:focus-visible { outline:2px solid var(--text); outline-offset:3px; border-radius:8px; }
.volume-slider:disabled { cursor:wait; opacity:.55; }
.volume-readout { display:flex; align-items:baseline; justify-content:center; gap:2px; min-height:28px; padding:4px; border:1px solid var(--line-strong); border-radius:7px; background:var(--surface-2); color:var(--text); font-family:var(--font-mono); font-variant-numeric:tabular-nums; line-height:20px; }
.volume-readout b { font-size:15px; font-weight:600; }.volume-readout span { color:var(--text-2); font-size:10px; }
.muted .volume-control { --volume-fill:var(--text-3); }
.mute { border:1px solid var(--line-strong); border-radius:7px; background:var(--surface-2); padding:5px 8px; font-size:10px; transition:background 120ms ease,border-color 120ms ease,color 120ms ease; }
.mute:hover:not(:disabled) { background:var(--hover); border-color:var(--text-3); }
.muted .mute { background:var(--text); color:var(--surface); border-color:var(--text); }.muted .sound-icon { border-style:dashed; }
.save-profile { display:flex; gap:8px; margin-top:12px; }
.save-profile input { min-width:0; flex:1; padding:7px 9px; border:1px solid var(--line-strong); background:var(--surface-2); border-radius:8px; font:11px var(--font-sans); transition:border-color 120ms ease,box-shadow 120ms ease; }
.save-profile input:hover { border-color:var(--text-3); }
.save-profile input:focus-visible { border-color:var(--accent); outline:none; box-shadow:0 0 0 2px var(--accent-weak); }
.save-profile button { background:var(--text); color:var(--surface); border-radius:8px; padding:6px 12px; font-size:11px; font-weight:600; transition:background 120ms ease; }
.save-profile button:hover:not(:disabled) { background:var(--text-2); }
.mixer-notice,.mixer-boundary { font-size:10px; color:var(--text-2); line-height:1.7; margin:10px 0 0; }
.mixer-boundary { margin-top:4px; }
@media (prefers-reduced-motion:reduce) { .volume-slider::-webkit-slider-thumb { transition:none; } }
</style>
