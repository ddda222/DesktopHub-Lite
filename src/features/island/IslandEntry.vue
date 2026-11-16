<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import ClipboardPopover from '@/features/clipboard/ClipboardPopover.vue'
import TranslationPanel from '@/features/translation/TranslationPanel.vue'
import { invokeHost, resizeBar } from '@/lib/desktop/desktop'
import { copyClip, iconFor, launchApp, openSettings, setCategory, state, togglePanel, visibleApps } from '@/store'

const element = ref<HTMLElement | null>(null)
const expanded = computed(() => state.ui.islandExpanded)
const peeking = ref(false)
const revealed = computed(() => expanded.value || peeking.value)
const page = computed(() => state.ui.panel === 'clipboard' || state.ui.panel === 'translation' ? state.ui.panel : 'applications')
const recentClips = computed(() => state.clips.slice(0, 5))
const expandedWidth = Math.min(720, window.screen.availWidth - 32)
const anchors = [
  { value: 'left', label: '顶部左侧', icon: 'align-left' },
  { value: 'center', label: '顶部居中', icon: 'align-center' },
  { value: 'right', label: '顶部右侧', icon: 'align-right' }
] as const
let hoverTimer: ReturnType<typeof setTimeout> | undefined
let leaveTimer: ReturnType<typeof setTimeout> | undefined
let observer: ResizeObserver | undefined

function expand(value: boolean, focus = false): void {
  clearTimeout(hoverTimer)
  clearTimeout(leaveTimer)
  peeking.value = value
  void invokeHost('set_island_expanded', { expanded: value, focus })
}

function enter(): void {
  clearTimeout(hoverTimer)
  clearTimeout(leaveTimer)
  if (!expanded.value) {
    peeking.value = true
    hoverTimer = setTimeout(() => expand(true), 320)
  }
}

function leave(): void {
  clearTimeout(hoverTimer)
  clearTimeout(leaveTimer)
  leaveTimer = setTimeout(() => {
    const active = document.activeElement
    const editing = active instanceof HTMLTextAreaElement ||
      (active instanceof HTMLInputElement && !['button', 'checkbox', 'radio', 'range', 'submit'].includes(active.type)) ||
      (active instanceof HTMLElement && active.isContentEditable) || active instanceof HTMLSelectElement
    if (!editing || !element.value?.contains(active)) expand(false)
  }, 450)
}

function focusout(): void {
  if (!element.value?.matches(':hover')) leave()
}

watch(expanded, value => {
  if (!value) {
    clearTimeout(hoverTimer)
    clearTimeout(leaveTimer)
    peeking.value = false
  }
})

async function applications(): Promise<void> {
  await invokeHost('close_panel')
  expand(true)
}

async function launch(id: string): Promise<void> {
  const application = state.apps.find(item => item.id === id)
  if (!application) return
  await launchApp(application)
  expand(false)
}

function keydown(event: KeyboardEvent): void {
  if (event.isComposing || event.keyCode === 229) return
  if (event.key === 'Escape') { event.stopPropagation(); expand(false) }
}

onMounted(() => {
  observer = new ResizeObserver(entries => {
    const rect = entries[0].target.getBoundingClientRect()
    void resizeBar(Math.ceil(rect.width), Math.ceil(rect.height))
  })
  if (element.value) observer.observe(element.value)
})

onBeforeUnmount(() => {
  observer?.disconnect()
  clearTimeout(hoverTimer)
  clearTimeout(leaveTimer)
})
</script>

<template>
  <section ref="element" class="island-hit-area" :class="{ expanded, tucked: !revealed }"
    :style="{ width: `${expanded ? expandedWidth : revealed ? 184 : 140}px`, height: `${expanded ? 440 : revealed ? 36 : 14}px` }"
    aria-label="顶部灵动岛" @mouseenter="enter" @mouseleave="leave" @focusout="focusout" @keydown="keydown">
    <button v-if="!revealed" class="tuck-trigger" type="button" aria-label="展开灵动岛" @click="expand(true, true)" />
    <div class="island" :class="{ expanded, tucked: !revealed }"
      :style="{ width: `${expanded ? expandedWidth : revealed ? 184 : 72}px`, height: `${expanded ? 440 : revealed ? 36 : 5}px` }">
    <button class="capsule" type="button" :aria-expanded="expanded" aria-label="展开或收起应用面板"
      :inert="!revealed" @pointerdown="clearTimeout(hoverTimer)" @click="expand(!expanded, true)">
      <Icon name="grid" :size="16" />
      <Icon name="chevron-down" :size="12" class="arrow" />
    </button>

    <div class="expanded-content" :inert="!expanded">
      <header class="toolbar">
        <nav aria-label="灵动岛功能">
          <button type="button" title="应用" aria-label="应用" :aria-pressed="page === 'applications'" :class="{ selected: page === 'applications' }" @click="applications"><Icon name="grid" :size="18" /></button>
          <button type="button" title="剪贴板" aria-label="剪贴板" :aria-pressed="page === 'clipboard'" :class="{ selected: page === 'clipboard' }" @click="togglePanel('clipboard')"><Icon name="clipboard" :size="18" /></button>
          <button type="button" title="翻译" aria-label="翻译" :aria-pressed="page === 'translation'" :class="{ selected: page === 'translation' }" @click="togglePanel('translation')"><Icon name="translate" :size="18" /></button>
        </nav>
        <div class="anchor-actions" role="group" aria-label="顶部停靠位置">
          <button v-for="anchor in anchors" :key="anchor.value" class="icon-action" type="button"
            :title="anchor.label" :aria-label="anchor.label" :aria-pressed="state.ui.islandAnchor === anchor.value"
            :class="{ selected: state.ui.islandAnchor === anchor.value }"
            @click="invokeHost('set_island_anchor', { anchor: anchor.value })"><Icon :name="anchor.icon" :size="16" /></button>
        </div>
        <button class="icon-action" type="button" title="设置" aria-label="设置" @click="openSettings()"><Icon name="settings" :size="16" /></button>
        <button class="icon-action" type="button" title="收起" aria-label="收起灵动岛" @click="expand(false)"><Icon name="chevron-down" :size="16" /></button>
      </header>

      <div v-if="page === 'applications'" class="applications">
        <nav class="categories" aria-label="应用分类">
          <button v-for="category in state.categories" :key="category.id" type="button"
            :class="{ selected: state.ui.activeCategoryId === category.id }" @click="setCategory(category.id)">{{ category.name }}</button>
        </nav>
        <div class="columns">
          <div class="app-grid scroll-y">
            <button v-for="app in visibleApps" :key="app.id" class="app-tile" type="button" :title="app.name" :aria-label="app.name" @click="launch(app.id)">
              <img v-if="iconFor(app.target)" :src="iconFor(app.target)" alt="" />
              <span v-else class="fallback" :style="{ background: app.color }">{{ app.initials }}</span>
            </button>
            <p v-if="!visibleApps.length" class="empty">这个分类还没有应用</p>
          </div>
          <aside class="recent">
            <div class="recent-heading"><span>最近复制</span><button type="button" title="全部剪贴板记录" aria-label="全部剪贴板记录" @click="togglePanel('clipboard')"><Icon name="chevron-right" :size="16" /></button></div>
            <button v-for="clip in recentClips" :key="clip.id" type="button" class="clip" @click="copyClip(clip)">
              <Icon :name="clip.kind === 'image' ? 'image' : 'clipboard'" :size="14" />
              <span>{{ clip.kind === 'image' ? `图片 ${clip.width} × ${clip.height}` : clip.text }}</span>
            </button>
            <p v-if="!recentClips.length" class="empty">新的复制内容会出现在这里</p>
            <span v-if="state.ui.paused" class="paused-note">已暂停记录</span>
          </aside>
        </div>
      </div>
      <div v-else class="feature-pane scroll-y">
        <ClipboardPopover v-if="page === 'clipboard'" />
        <TranslationPanel v-else />
      </div>
    </div>
    </div>
  </section>
</template>

<style scoped>
.island-hit-area { position: relative; display: flex; justify-content: center; align-items: flex-start; }
.tuck-trigger { position: absolute; inset: 0; z-index: 1; width: 100%; height: 100%; padding: 0; background: transparent; border: 0; }
.island {
  position: relative;
  overflow: hidden;
  border-radius: 0 0 20px 20px;
  background: var(--surface);
  color: var(--text);
  box-shadow: inset 0 -1px 0 var(--line-strong);
  transition: border-radius 180ms ease;
}
.island.tucked { border-radius: 3px; box-shadow: inset 0 0 0 1px var(--line-strong); }
.island.tucked .capsule { visibility: hidden; }
.island.expanded { border-radius: 0 0 32px 32px; }
button { color: inherit; cursor: pointer; }
.capsule { display: flex; align-items: center; justify-content: center; gap: 9px; height: 36px; width: 184px; margin: 0 auto; border: 0; background: transparent; white-space: nowrap; }
.arrow { color: var(--text-3); transition: transform 240ms; }
.expanded .arrow { transform: rotate(180deg); }
.expanded-content { width: 100%; padding: 0 20px 18px; height: 404px; opacity: 0; transform: translateY(-8px); transition: opacity 160ms ease, transform 220ms ease; }
.expanded .expanded-content { opacity: 1; transform: translateY(0); transition-delay: 80ms; }
.toolbar { display: flex; height: 44px; align-items: center; gap: 8px; border-bottom: 1px solid var(--line); margin-bottom: 12px; }
.toolbar nav { display: flex; gap: 4px; flex: 1; }
.toolbar nav button { display: grid; place-items: center; width: 36px; height: 36px; padding: 0; }
.toolbar button, .categories button { border: 0; background: transparent; padding: 7px 11px; border-radius: 8px; font-size: 12px; }
.toolbar button:hover, .categories button:hover { background: var(--hover); }
.toolbar button.selected, .categories button.selected { background: var(--accent-weak); color: var(--accent); }
.anchor-actions { display: flex; gap: 2px; }
.toolbar .icon-action { display: grid; place-items: center; width: 32px; height: 32px; padding: 0; }
.categories { display: flex; flex-wrap: wrap; gap: 4px; max-height: 70px; overflow-y: auto; margin-bottom: 12px; }
.columns { display: grid; grid-template-columns: minmax(0,1fr) 216px; gap: 16px; height: 265px; }
.app-grid { display: grid; grid-template-columns: repeat(4,minmax(0,1fr)); align-content: start; overflow-y: auto; gap: 6px; }
.app-tile { display: flex; align-items: center; justify-content: center; min-width: 0; min-height: 64px; border: 0; background: transparent; border-radius: 12px; padding: 12px 5px; }
.app-tile:hover { background: var(--hover); }
.app-tile:active { background: var(--active); }
.app-tile img, .fallback { width: 34px; height: 34px; object-fit: contain; }
.fallback { display: grid; place-items: center; border-radius: 9px; color: white; }
.recent { border-left: 1px solid var(--line); padding-left: 16px; min-width: 0; }
.recent-heading { display: flex; align-items: center; justify-content: space-between; margin: 0 0 10px; font-size: 12px; color: var(--text-2); }
.recent-heading button { border: 0; background: transparent; font-size: 11px; color: var(--accent); }
.clip { display: flex; width: 100%; align-items: center; gap: 8px; margin-bottom: 5px; padding: 10px; background: var(--surface-2); border: 1px solid var(--line); border-radius: 9px; text-align: left; }
.clip:hover { background: var(--surface-3); }
.clip span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
.empty { grid-column: 1/-1; color: var(--text-3); font-size: 12px; line-height: 1.8; padding: 12px 4px; }
.paused-note { color: var(--text-3); font-size: 11px; }
.feature-pane { height: 330px; overflow: auto; }
.feature-pane :deep(.pop), .feature-pane :deep(.panel) { width: 100%; max-height: none; border: 0; border-radius: 0; box-shadow: none; }
button:focus-visible, select:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
@media (max-width: 560px) { .columns { grid-template-columns: minmax(0,1fr) 160px; } .app-grid { grid-template-columns: repeat(3,minmax(0,1fr)); } .expanded-content { padding-right: 12px; padding-left: 12px; } }
@media (prefers-reduced-motion: reduce) { .island, .expanded-content, .arrow { transition: none; } }
</style>
