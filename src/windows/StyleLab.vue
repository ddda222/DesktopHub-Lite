<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import StyleIsland from '@/features/style-lab/StyleIsland.vue'
import { islandStyles } from '@/features/style-lab/styles'
import { setTheme } from '@/store'
import { loadBrowserShelfSamples } from '@/features/files/shelf'
import { makeShelfSamples } from '@/mock/shelf-samples'
const selected = ref(0)
const theme = ref('light')
const darkSystem = window.matchMedia('(prefers-color-scheme: dark)')
const motionSystem = window.matchMedia('(prefers-reduced-motion: reduce)')
const systemDark = ref(darkSystem.matches)
const systemReduced = ref(motionSystem.matches)
const reduced = ref(false)
const webgl = ref(true)
const expanded = ref(true)
const line = ref(false)
const pulse = ref(0)
const cycleRunning = ref(false)
const status = ref('点击功能入口切换内容，移动指针查看边缘光效。')
const current = computed(() => islandStyles[selected.value]!)
const isDark = computed(() => theme.value === 'dark' || (theme.value === 'system' && systemDark.value))
const reduceMotion = computed(() => reduced.value || systemReduced.value)
const mode = computed(() => line.value && !expanded.value ? '细线' : expanded.value ? '展开' : '胶囊')
let timers: ReturnType<typeof setTimeout>[] = []
function stopCycle(): void { timers.forEach(clearTimeout); timers = []; cycleRunning.value = false }
function selectStyle(index: number): void { stopCycle(); selected.value = index; status.value = islandStyles[index]!.motion }
function setState(state: 'capsule' | 'expanded' | 'line'): void {
  stopCycle(); line.value = state === 'line'; expanded.value = state === 'expanded'
  status.value = state === 'line' ? '移入细线立即展开，移开后收回；也可用 Tab 和 Enter 打开。' : state === 'capsule' ? '点击胶囊展开，轨道卫星可先选择一个入口。' : '现在可以切换对照台、调音台和原有功能。'
}
function replay(): void {
  stopCycle(); cycleRunning.value = true; line.value = false; expanded.value = false
  status.value = '正在演示：胶囊 → 展开 → 交互 → 收起 → 展开。'
  timers = [setTimeout(() => { expanded.value = true }, 800), setTimeout(() => { pulse.value++ }, 1700), setTimeout(() => { expanded.value = false }, 3400), setTimeout(() => { expanded.value = true; cycleRunning.value = false; status.value = '演示结束，可以自由操作。' }, 4300)]
}
function action(): void { stopCycle(); pulse.value++; status.value = current.value.motion }
function updateSystem(): void { systemDark.value = darkSystem.matches; systemReduced.value = motionSystem.matches }
watch(isDark, value => { void setTheme(value ? 'dark' : 'light') }, { immediate: true })
onMounted(() => { document.title = 'DesktopHub · 十款样式实验室'; loadBrowserShelfSamples(makeShelfSamples()); darkSystem.addEventListener('change', updateSystem); motionSystem.addEventListener('change', updateSystem) })
onBeforeUnmount(() => { stopCycle(); darkSystem.removeEventListener('change', updateSystem); motionSystem.removeEventListener('change', updateSystem) })
</script>

<template>
  <div class="style-lab" :class="{ 'lab-dark': isDark }">
    <header class="lab-header"><a class="lab-logo" href="?preview=styles" aria-label="DesktopHub 样式实验室首页"><span aria-hidden="true">d.</span><b>DesktopHub</b><small>样式实验室</small></a><div class="header-right"><a href="/">原版预览 ↗</a><span class="version">实验 001—010</span></div></header>
    <main class="lab-main">
      <div class="lab-intro"><div><p class="eyebrow">形态探索 / INTERACTION STUDIES</p><h1>同一个岛，<span>十种可能。</span></h1><p class="intro-copy">从收起到展开，找到最顺手的那一种。</p></div><div class="theme-picker" role="group" aria-label="演示主题"><button v-for="option in [{ id:'light', label:'浅色' },{ id:'dark', label:'深色' },{ id:'system', label:'跟随系统' }]" :key="option.id" :aria-pressed="theme === option.id" @click="theme = option.id">{{ option.label }}</button></div></div>
      <div class="lab-layout">
        <aside class="style-index" aria-label="十种候选样式"><div class="index-heading"><span>选择一种形态</span><small>10 款</small></div><div class="style-list"><button v-for="(style, i) in islandStyles" :key="style.id" :aria-pressed="selected === i" @click="selectStyle(i)"><span class="style-number">{{ String(i + 1).padStart(2, '0') }}</span><span class="style-glyph" :class="`glyph-${style.id}`" aria-hidden="true">{{ style.shape }}</span><span class="style-name"><strong>{{ style.name }}</strong><small>{{ style.tag }}</small></span><span class="selected-mark" aria-hidden="true">{{ selected === i ? '↗' : '·' }}</span></button></div><p class="index-note">共用相同示例内容<br />只比较形态与交互</p></aside>
        <section class="preview-column" aria-label="样式互动预览">
          <div class="stage-toolbar"><div><span class="live-dot" /><span>实时预览</span><span class="stage-mode">{{ mode }}</span></div><div><label><input v-model="webgl" type="checkbox" />WebGL 光效</label><label><input :checked="reduceMotion" type="checkbox" :disabled="systemReduced" @change="reduced = ($event.target as HTMLInputElement).checked" />{{ systemReduced ? '系统减少动画' : '减少动画' }}</label></div></div>
          <div class="lab-canvas" :class="{ 'is-collapsed': !expanded }">
            <span class="canvas-corner corner-tl" aria-hidden="true" /><span class="canvas-corner corner-tr" aria-hidden="true" /><span class="canvas-corner corner-bl" aria-hidden="true" /><span class="canvas-corner corner-br" aria-hidden="true" />
            <div class="canvas-caption"><span>{{ String(selected + 1).padStart(2,'0') }} / {{ current.name }}</span><span>黑白实底 · {{ isDark ? '深色' : '浅色' }}</span></div>
            <div class="specimen-space"><StyleIsland :variant="current.id" :expanded="expanded" :line="line" :light="!isDark" :webgl="webgl" :reduced="reduceMotion" :pulse="pulse" @expand="expanded = $event" @status="status = $event" /></div>
            <div class="canvas-bottom"><span>轮廓动起来，文字静下来。</span><span aria-hidden="true">＋</span></div>
          </div>
          <div class="playback"><div class="state-controls" role="group" aria-label="灵动岛状态"><button :aria-pressed="mode === '胶囊'" @click="setState('capsule')"><span aria-hidden="true">▬</span>胶囊</button><button :aria-pressed="mode === '细线'" @click="setState('line')"><span aria-hidden="true">—</span>细线</button><button :aria-pressed="mode === '展开'" @click="setState('expanded')"><span aria-hidden="true">▣</span>展开</button></div><button class="replay-button" @click="cycleRunning ? stopCycle() : replay()">{{ cycleRunning ? '■ 停止演示' : '▷ 播放完整动效' }}</button></div>
          <div class="style-description"><div><p class="eyebrow">{{ current.tag }}</p><h2>{{ current.name }}</h2><p>{{ current.description }}</p></div><button class="interaction-button" @click="action">{{ current.action }} <span aria-hidden="true">↗</span></button></div>
          <p class="interaction-status" role="status">{{ status }}</p>
        </section>
      </div>
      <footer class="lab-footer"><p>先体验，再定稿。<span>候选样式不会改变桌面版设置。</span></p><p>对照台支持本地内容操作 · 声音调节和置顶属于网页演示</p></footer>
    </main>
  </div>
</template>

<style src="../features/style-lab/lab.css" scoped></style>
