<script setup lang="ts">
/** 当前灵动岛的浏览器预览，只使用既有合成数据与 MockHost。 */
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import IslandEntry from '@/features/island/IslandEntry.vue'
import SettingsWindow from '@/features/settings/SettingsWindow.vue'
import { closeSettings, setTheme, state } from '@/store'
import { previewApps, previewIcons } from '@/mock/preview-icons'

const plain = ref(false)
const scene = ref('normal')
state.apps = previewApps
state.icons = previewIcons
state.ui.islandExpanded = false
state.ui.fullscreenSuppressed = false
setTheme('light')

// 只模拟宿主通知，不改变真实 Windows 前台窗口或显示模式。
watch([scene, () => state.ui.islandAppearance.avoidFullscreen], () => {
  state.ui.fullscreenSuppressed = state.ui.islandAppearance.avoidFullscreen && scene.value !== 'normal'
  state.ui.islandExpanded = false
})

function keydown(event: KeyboardEvent): void {
  if (event.key === 'Escape' && !event.isComposing && state.ui.settingsOpen) closeSettings()
}
onMounted(() => window.addEventListener('keydown', keydown))
onBeforeUnmount(() => window.removeEventListener('keydown', keydown))
</script>

<template>
  <div class="preview" :class="{ plain, dark: state.settings.theme === 'dark' }">
    <header class="preview-tools">
      <span>DesktopHub <small>液态玻璃预览</small></span>
      <div class="preview-options">
        <button type="button" :aria-pressed="state.settings.theme === 'light'" @click="setTheme('light')">浅色</button>
        <button type="button" :aria-pressed="state.settings.theme === 'dark'" @click="setTheme('dark')">深色</button>
        <label>尺寸<select v-model.number="state.ui.islandAppearance.scale"><option :value="75">75%</option><option :value="100">100%</option><option :value="125">125%</option></select></label>
        <label><input v-model="plain" type="checkbox" />纯色背景</label>
        <label>场景<select v-model="scene" aria-label="模拟窗口状态"><option value="normal">桌面／普通窗口</option><option value="maximized">窗口最大化</option><option value="fullscreen">游戏／全屏</option></select></label>
        <button type="button" :disabled="state.ui.fullscreenSuppressed" @click="state.ui.islandExpanded = true">展开</button>
      </div>
    </header>
    <main class="preview-stage"><IslandEntry /></main>
    <p class="preview-note">真实 Vue 组件 · 合成内容 · 窗口状态为模拟<br />普通窗口显示胶囊，悬停展开；最大化／全屏只留细线。Windows 检测与材质需桌面版验证。</p>
    <SettingsWindow v-if="state.ui.settingsOpen" />
    <div class="toasts" role="status" aria-live="polite">
      <div v-for="toast in state.toasts" :key="toast.id" class="toast"><Icon :name="toast.kind === 'success' ? 'check' : 'info'" :size="16" /><span>{{ toast.text }}</span></div>
    </div>
  </div>
</template>

<style scoped>
.preview { --wallpaper: url('../mock/liquid-wallpaper.svg'); position: relative; min-height: 100%; height: 100%; overflow: auto; background: var(--wallpaper) center / cover fixed; color: #252b3c; }
.preview.dark { background-color: #101a31; background-image: linear-gradient(rgba(10,18,40,.7),rgba(10,18,40,.7)),var(--wallpaper); color: #e3e8f2; }
.preview.plain { background: #d9dfe9; }
.preview.dark.plain { background: #171e2d; }
.preview-tools { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 12px; padding: 20px 28px; }
.preview-tools > span { font-size: 14px; font-weight: 600; }
.preview-tools small { margin-left: 10px; font-size: 12px; font-weight: 400; opacity: .65; }
.preview-options { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; font-size: 12px; }
.preview-options button { padding: 5px 10px; border-radius: 16px; }
.preview-options button[aria-pressed='true'] { background: rgba(255,255,255,.3); }
.preview-options label { display: flex; align-items: center; gap: 6px; }
select { font: inherit; color: inherit; background: transparent; border: 0; }
select option { color: #252b3c; }
.preview-stage { display: flex; justify-content: center; align-items: flex-start; padding: 20px 0 60px; }
.preview-note { margin: 0 28px 24px; font-size: 11px; line-height: 1.8; opacity: .7; }
.toasts { position: fixed; bottom: 30px; left: 50%; transform: translateX(-50%); display: grid; gap: 8px; z-index: var(--z-toast); pointer-events: none; }
.toast { display: flex; align-items: center; gap: 8px; padding: 10px 16px; border-radius: 20px; background: var(--surface); color: var(--text); box-shadow: 0 4px 18px rgba(0,0,0,.1); font-size: 13px; }
</style>
