<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import type { AudioDevice, AudioSnapshot, MixerVisibility } from '@/lib/desktop/types'
import { state } from '@/store'
import { mockSessions } from '@/features/audio/mock'

type Group = keyof MixerVisibility
const snapshot = ref<AudioSnapshot | null>(null)
const reading = ref(false)
const saving = ref(false)
const readError = ref('')
const saveError = ref('')
let disposed = false
let timer: ReturnType<typeof setInterval> | undefined

async function refresh(): Promise<void> {
  if (reading.value) return
  reading.value = true
  try {
    const result: AudioSnapshot = isDesktop
      ? await invokeHost<AudioSnapshot>('list_audio_sessions')
      : { devices: mockSessions.map(row => ({ id: row.deviceId, name: row.deviceName })), sessions: mockSessions, warnings: [] }
    if (!disposed) {
      snapshot.value = result
      readError.value = ''
    }
  } catch (e) {
    if (!disposed) { snapshot.value = null; readError.value = String(e) }
  } finally {
    reading.value = false
  }
}

function items(group: Group, detected: AudioDevice[]) {
  const hidden = state.ui.mixerVisibility[group]
  const available = new Map(detected.map(item => [item.id, item.name]))
  const names = new Map([...Object.entries(hidden), ...available])
  const counts = new Map<string, number>()
  for (const name of names.values()) counts.set(name, (counts.get(name) ?? 0) + 1)
  return Array.from(names, ([id, name]) => ({
    id, name,
    visible: !Object.hasOwn(hidden, id),
    missing: snapshot.value !== null && !available.has(id),
    sameName: counts.get(name)! > 1,
  })).sort((a, b) => a.name.localeCompare(b.name, 'zh-CN') || a.id.localeCompare(b.id))
}

const groups = computed(() => {
  const apps = (snapshot.value?.sessions ?? []).flatMap(row => row.appKey ? [{ id: row.appKey, name: row.name }] : [])
  return [
    { key: 'devices' as const, label: '输出设备', empty: '暂无可用的输出设备。', rows: items('devices', snapshot.value?.devices ?? []) },
    { key: 'apps' as const, label: '应用', empty: '暂无可识别的应用。让应用播放一次声音后，会出现在这里。', rows: items('apps', apps) },
  ]
})
const unidentified = computed(() => snapshot.value?.sessions.some(row => !row.appKey))

async function setVisible(group: Group, item: AudioDevice, visible: boolean): Promise<void> {
  if (saving.value) return
  saveError.value = ''
  saving.value = true
  try {
    if (isDesktop) {
      state.ui.mixerVisibility = await invokeHost<MixerVisibility>('set_mixer_item_visible', {
        kind: group === 'devices' ? 'device' : 'app', id: item.id, name: item.name, visible,
      })
    } else {
      const current = state.ui.mixerVisibility
      const hidden = { ...current[group] }
      if (visible) delete hidden[item.id]
      else hidden[item.id] = item.name
      state.ui.mixerVisibility = { ...current, [group]: hidden }
    }
  } catch (e) {
    saveError.value = `保存失败：${String(e)}`
  } finally {
    saving.value = false
  }
}

onMounted(() => {
  void refresh()
  if (isDesktop) timer = setInterval(() => { if (!document.hidden) void refresh() }, 2500)
})
onBeforeUnmount(() => { disposed = true; clearInterval(timer) })
</script>

<template>
  <div class="panel">
    <h3 class="panel-title">调音台</h3>
    <p class="hint">选择在灵动岛显示的声音条目。新设备和新应用默认显示，开关即时生效。</p>
    <p class="hint">{{ isDesktop ? '应用播放声音后会出现在调音台中。声音配置按应用和输出设备匹配（包含隐藏条目，跳过未运行的应用），仅在点击预设时应用，不会调整系统总音量。' : '当前为网页合成会话演示，可体验独立音量、静音与配置切换，不会改变 Windows 系统音量。' }}</p>
    <p v-if="readError" class="inline-feedback error" role="alert">读取失败：{{ readError }} <button class="btn" type="button" @click="refresh">重试</button></p>
    <p v-else-if="!snapshot" class="hint" role="status">正在识别声音设备和应用…</p>
    <p v-if="saveError" class="inline-feedback error" role="alert">{{ saveError }}</p>
    <p v-for="warning in snapshot?.warnings" :key="warning" class="inline-feedback">{{ warning }}</p>
    <section v-for="group in groups" :key="group.key" :aria-label="group.label">
      <h4 class="sub-title">{{ group.label }}</h4>
      <p class="hint">{{ group.key === 'devices' ? '关闭后，隐藏通过该设备播放的所有应用。' : '关闭后，该应用在所有输出设备上都会隐藏。' }}</p>
      <div class="card">
        <div v-for="row in group.rows" :key="row.id" class="set-row">
          <span class="set-text">
            <span class="set-label item-name">{{ row.name }}</span>
            <span v-if="row.sameName" class="set-desc">{{ row.id }}</span>
            <span v-if="row.missing" class="set-desc">{{ group.key === 'devices' ? '当前未检测到' : '当前没有声音会话' }}</span>
          </span>
          <button class="switch" type="button" role="switch" :class="{ 'is-on': row.visible }"
            :aria-label="`显示${group.label}：${row.name}${row.sameName ? ' ' + row.id : ''}`" :aria-checked="row.visible"
            :disabled="saving" @click="setVisible(group.key, row, !row.visible)"><span class="switch-knob" aria-hidden="true" /></button>
        </div>
        <p v-if="snapshot && !group.rows.length" class="hint">{{ group.empty }}</p>
      </div>
    </section>
    <p v-if="unidentified" class="hint">部分会话无法识别所属应用，可通过输出设备开关隐藏。</p>
  </div>
</template>

<style scoped>
.item-name { overflow-wrap: anywhere; }
</style>
