<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import StyleLight from './StyleLight.vue'
import Icon from '@/components/Icon.vue'
import AudioMixer from '@/features/audio/AudioMixer.vue'
import ClipboardPopover from '@/features/clipboard/ClipboardPopover.vue'
import TranslationPanel from '@/features/translation/TranslationPanel.vue'
import FileShelf from '@/features/files/FileShelf.vue'
import { iconFor, launchApp, state, visibleApps } from '@/store'
import type { StyleId } from './styles'
const props = defineProps<{ variant: StyleId; expanded: boolean; line: boolean; light: boolean; webgl: boolean; reduced: boolean; pulse: number }>()
const emit = defineEmits<{ expand: [value: boolean]; status: [text: string] }>()
const page = ref('audio')
const orbit = ref(false)
const playing = ref(false)
const feedback = ref(false)
const available = ref(true)
const pointer = ref({ x: 0, y: 0 })
const shell = ref<HTMLElement | null>(null)
const collapse = ref<HTMLButtonElement | null>(null)
const root = ref<HTMLElement | null>(null)
let leaveTimer: ReturnType<typeof setTimeout> | undefined
let feedbackTimer: ReturnType<typeof setTimeout> | undefined
const tucked = computed(() => props.line && !props.expanded)
const pages = [
  { id: 'applications', name: '应用', icon: 'grid' }, { id: 'clipboard', name: '剪贴板', icon: 'clipboard' },
  { id: 'translation', name: '翻译', icon: 'translate' }, { id: 'files', name: '文件中转', icon: 'folder' },
  { id: 'audio', name: '调音台', icon: 'volume' }
] as const
watch(page, value => { const selected = pages.find(item => item.id === value); if (selected) state.ui.islandPage = selected.id })
async function toggle(event: MouseEvent): Promise<void> {
  clearTimeout(leaveTimer)
  if (props.line) { orbit.value = false; emit('expand', true); if (event.detail === 0) { await nextTick(); collapse.value?.focus({ preventScroll: true }) }; return }
  if (props.variant === 'orbit' && !props.expanded && !orbit.value) {
    orbit.value = true; emit('status', '卫星已散开，选择一个入口展开。')
    if (event.detail === 0) { await nextTick(); root.value?.querySelector<HTMLButtonElement>('.satellites button')?.focus({ preventScroll: true }) }
    return
  }
  orbit.value = false; emit('expand', !props.expanded)
  if (event.detail === 0) { await nextTick(); collapse.value?.focus({ preventScroll: true }) }
}
async function select(id: string): Promise<void> {
  const fromSatellite = orbit.value
  page.value = id; orbit.value = false; emit('expand', true)
  if (fromSatellite) { await nextTick(); collapse.value?.focus({ preventScroll: true }) }
}
function action(): void {
  feedback.value = false
  clearTimeout(feedbackTimer)
  if (props.variant === 'orbit') { orbit.value = true; emit('expand', false) }
  else if (props.variant === 'vinyl') { playing.value = !playing.value; emit('expand', true) }
  else {
    emit('expand', true)
    requestAnimationFrame(() => { feedback.value = true })
    if (props.variant === 'shutter') page.value = page.value === 'applications' ? 'audio' : 'applications'
  }
  feedbackTimer = setTimeout(() => { feedback.value = false }, 1800)
}
watch(() => props.pulse, action)
watch(() => props.variant, () => { orbit.value = false; feedback.value = false; pointer.value = { x: 0, y: 0 }; clearTimeout(leaveTimer) })
watch(() => props.expanded, value => {
  if (!value && root.value?.contains(document.activeElement)) requestAnimationFrame(() => shell.value?.focus({ preventScroll: true }))
})
function hover(): void { clearTimeout(leaveTimer); if (tucked.value) emit('expand', true) }
function leave(): void { if (props.line && !root.value?.contains(document.activeElement)) leaveTimer = setTimeout(() => emit('expand', false), 700) }
function move(event: PointerEvent): void {
  if (props.variant !== 'stage' || props.reduced) return
  const bounds = event.currentTarget as HTMLElement
  const rect = bounds.getBoundingClientRect()
  pointer.value = { x: (event.clientX - rect.left) / rect.width - .5, y: (event.clientY - rect.top) / rect.height - .5 }
}
function escape(event: KeyboardEvent): void { if (event.isComposing) return; orbit.value = false; emit('expand', false); void nextTick(() => shell.value?.focus({ preventScroll: true })) }
onBeforeUnmount(() => { clearTimeout(leaveTimer); clearTimeout(feedbackTimer) })
</script>

<template>
  <div ref="root" class="specimen" :class="[`variant-${variant}`, { expanded, tucked, 'orbit-open': orbit, feedback, playing, reduced }]" :style="{ '--px': pointer.x, '--py': pointer.y }" @pointerenter="hover" @pointerleave="leave" @pointermove="move" @keydown.esc.stop="escape">
    <div v-if="variant === 'orbit'" class="satellites" :inert="!orbit" :aria-hidden="!orbit"><button v-for="(item, i) in pages.slice(4)" :key="item.id" :style="{ '--i': i }" :aria-label="`卫星入口：${item.name}`" @click="select(item.id)"><Icon :name="item.icon" :size="19" /><span>{{ item.name }}</span></button></div>
    <div class="island-shell">
      <StyleLight v-if="webgl && available && !reduced" :light-surface="light" :orbiting="variant === 'light'" :signal="`${variant}:${expanded}:${pulse}:${page}`" @unavailable="available = false" />
      <div class="shell-decoration" aria-hidden="true"><i class="deco-one" /><i class="deco-two" /><i class="deco-three" /><i class="deco-four" /></div>
      <button v-show="!expanded" ref="shell" class="capsule-trigger" :aria-expanded="expanded || orbit" :aria-label="tucked ? '展开细线灵动岛' : '展开灵动岛'" @click="toggle">
        <span class="capsule-symbol" aria-hidden="true"><span class="record-disc" /><span class="status-dot" /></span><span class="capsule-time">09:41</span><span class="capsule-divider" /><span class="capsule-status">{{ feedback ? '已完成' : '专注中' }}</span><span class="capsule-tail" aria-hidden="true">⌁</span>
      </button>
      <div class="island-body" :inert="!expanded" :aria-hidden="!expanded">
        <header class="island-header"><div class="header-brand"><span class="header-record" aria-hidden="true" /><b>DesktopHub</b><span class="header-state">{{ feedback ? '准备就绪' : '桌面工具随手可达' }}</span></div><button ref="collapse" aria-label="收起灵动岛" @click="emit('expand', false)"><Icon name="close" :size="16" /></button></header>
        <nav class="island-tabs" aria-label="演示功能"><button v-for="item in pages" :key="item.id" :aria-pressed="page === item.id" @click="page = item.id"><Icon :name="item.icon" :size="14" />{{ item.name }}</button></nav>
        <div class="island-page">
          <div v-if="page === 'applications'" class="lab-apps"><button v-for="app in visibleApps" :key="app.id" :aria-label="app.name" @click="launchApp(app)"><img v-if="iconFor(app.target)" :src="iconFor(app.target)" alt="" /><span v-else>{{ app.initials }}</span><small>{{ app.name }}</small></button></div>
          <div v-if="['clipboard', 'translation', 'files'].includes(page)" class="lab-feature"><ClipboardPopover v-if="page === 'clipboard'" /><TranslationPanel v-else-if="page === 'translation'" /><FileShelf v-else :drag-over="false" /></div>
          <AudioMixer v-show="page === 'audio'" :active="page === 'audio' && expanded" />
        </div>
        <footer class="island-footer"><span class="mini-indicator" />{{ feedback ? '交互反馈已触发' : '本地网页演示' }}<span>⌘ / 工作随手可达</span></footer>
      </div>
    </div>
    <span v-if="!available && webgl" class="gl-fallback" role="status">WebGL 不可用，已使用实底样式</span>
  </div>
</template>

<style src="./island.css" scoped></style>
