<script setup lang="ts">
/**
 * 常驻图标条。
 *
 * 桌面版里这个组件独占 `bar` 窗口：窗口尺寸等于图标条本身，
 * 所以分类菜单与溢出网格都不在这里渲染，而是请 Rust 打开独立的 `pop` 窗口。
 * 浏览器样稿里没有独立窗口，退回内联菜单。
 */
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import Icon from '@/components/Icon.vue'
import CategoryMenu from '@/features/applications/CategoryMenu.vue'
import OverflowGrid from '@/features/applications/OverflowGrid.vue'
import { isDesktop, requestPanel, resizeBar, startBarDrag } from '@/lib/desktop/desktop'
import {
  categoryName,
  iconFor,
  launchApp,
  openSettings,
  openTranslation,
  reorderApps,
  setCategory,
  state,
  togglePanel,
  visibleApps
} from '@/store'
import type { HubApp } from '@/lib/desktop/types'

const barEl = ref<HTMLElement | null>(null)
const appsEl = ref<HTMLElement | null>(null)

/** 浏览器样稿里内联展开的菜单；桌面版由独立窗口承载。 */
const catOpen = ref(false)
const overflowOpen = ref(false)

const TILE = 36
const GAP = 4
/** 图标条内容宽度上限；窗口宽度按实测内容回报，不做硬编码。 */
const BAR_WIDTH_MAX = 720

const width = ref(380)
let observer: ResizeObserver | null = null

const capacity = computed(() => Math.max(1, Math.floor((width.value + GAP) / (TILE + GAP))))
const needsOverflow = computed(() => visibleApps.value.length > capacity.value)
const shownApps = computed(() =>
  needsOverflow.value
    ? visibleApps.value.slice(0, Math.max(1, capacity.value - 1))
    : visibleApps.value
)
const hiddenCount = computed(() => visibleApps.value.length - shownApps.value.length)

const catMenuOpen = computed(() =>
  isDesktop ? state.ui.panel === 'category' : catOpen.value
)
const overflowMenuOpen = computed(() =>
  isDesktop ? state.ui.panel === 'overflow' : overflowOpen.value
)

/* 浏览器样稿里的悬停名称提示；桌面版窗口只有图标条大小，改用原生 title。 */
const tip = reactive({ visible: false, text: '', x: 0, y: 0 })

function showTip(event: Event, text: string): void {
  if (isDesktop) return
  const target = event.currentTarget as HTMLElement | null
  if (!target) return
  const rect = target.getBoundingClientRect()
  tip.text = text
  tip.x = rect.left + rect.width / 2
  tip.y = rect.bottom + 10
  tip.visible = true
}

function hideTip(): void {
  tip.visible = false
}

/* 拖动排序：拖动应用只改顺序，拖动整条走左侧握把 */
const draggingId = ref<string | null>(null)
const dragOverId = ref<string | null>(null)

function onDragStart(id: string): void {
  draggingId.value = id
  tip.visible = false
}

function onDrop(id: string): void {
  if (draggingId.value) reorderApps(draggingId.value, id)
  draggingId.value = null
  dragOverId.value = null
}

function onDragEnd(): void {
  draggingId.value = null
  dragOverId.value = null
}

/* 键盘：左右方向键在图标之间移动焦点，Enter 启动 */
function onAppsKeydown(event: KeyboardEvent): void {
  if (event.key !== 'ArrowRight' && event.key !== 'ArrowLeft') return
  const items = Array.from(appsEl.value?.querySelectorAll<HTMLElement>('.tile') ?? [])
  if (!items.length) return
  const current = items.indexOf(document.activeElement as HTMLElement)
  if (current < 0) return
  event.preventDefault()
  const delta = event.key === 'ArrowRight' ? 1 : -1
  items[(current + delta + items.length) % items.length]?.focus()
}

/* 整条拖动：桌面版交给系统原生拖动窗口，浏览器样稿用 CSS 位移演示 */
const grip = reactive({ active: false, startX: 0, startY: 0, baseX: 0, baseY: 0 })

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max)
}

function onGripDown(event: PointerEvent): void {
  if (isDesktop) {
    void startBarDrag()
    return
  }
  grip.active = true
  grip.startX = event.clientX
  grip.startY = event.clientY
  grip.baseX = state.ui.dockOffset.x
  grip.baseY = state.ui.dockOffset.y
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
}

function onGripMove(event: PointerEvent): void {
  if (isDesktop || !grip.active) return
  state.ui.dockOffset.x = clamp(grip.baseX + event.clientX - grip.startX, -520, 520)
  state.ui.dockOffset.y = clamp(grip.baseY + event.clientY - grip.startY, -560, 40)
}

function onGripUp(): void {
  grip.active = false
}

function openClipboard(): void {
  closeLocalMenus()
  togglePanel('clipboard')
}

function openTranslate(): void {
  closeLocalMenus()
  if (state.ui.panel === 'translation') {
    togglePanel('translation')
    return
  }
  openTranslation()
}

function closeLocalMenus(): void {
  catOpen.value = false
  overflowOpen.value = false
}

function toggleCategoryMenu(): void {
  if (isDesktop) {
    void requestPanel('category')
    return
  }
  overflowOpen.value = false
  catOpen.value = !catOpen.value
}

function toggleOverflow(): void {
  if (isDesktop) {
    void requestPanel('overflow')
    return
  }
  // 溢出网格与剪贴板/翻译弹层互斥，避免两块浮层叠在一起
  if (!overflowOpen.value) {
    state.ui.panel = null
    catOpen.value = false
  }
  overflowOpen.value = !overflowOpen.value
}

function pickCategory(id: string): void {
  setCategory(id)
  catOpen.value = false
}

function openAppsSettings(): void {
  closeLocalMenus()
  openSettings('apps')
}

function onDocPointerDown(event: PointerEvent): void {
  if (isDesktop) return
  if (!barEl.value?.contains(event.target as Node)) {
    closeLocalMenus()
  }
}

/**
 * 图标条窗口必须等于图标条本身，透明区域才不会挡住桌面点击。
 * 量的是整个 .bar 的宽高，而不是内层应用区，否则窗口会偏窄把内容裁掉。
 */
function syncWindowSize(): void {
  if (!isDesktop) return
  const rect = barEl.value?.getBoundingClientRect()
  if (!rect) return
  void resizeBar(Math.min(Math.ceil(rect.width), BAR_WIDTH_MAX), Math.ceil(rect.height))
}

onMounted(() => {
  const el = appsEl.value
  if (el) {
    width.value = el.clientWidth
    observer = new ResizeObserver((entries) => {
      width.value = entries[0].contentRect.width
    })
    observer.observe(el)
  }
  document.addEventListener('pointerdown', onDocPointerDown, true)
  syncWindowSize()
})

onBeforeUnmount(() => {
  observer?.disconnect()
  document.removeEventListener('pointerdown', onDocPointerDown, true)
})
</script>

<template>
  <div ref="barEl" class="bar-wrap">
    <div class="bar" role="toolbar" aria-label="应用图标条">
      <button
        class="grip"
        type="button"
        aria-label="拖动图标条位置"
        title="拖动图标条位置"
        @pointerdown="onGripDown"
        @pointermove="onGripMove"
        @pointerup="onGripUp"
        @pointercancel="onGripUp"
      >
        <Icon name="grip" :size="14" />
      </button>

      <button
        class="cat"
        type="button"
        :aria-expanded="catMenuOpen"
        aria-haspopup="menu"
        title="切换分类"
        @click="toggleCategoryMenu"
      >
        <Icon name="grid" :size="14" />
        <span>{{ categoryName }}</span>
        <Icon name="chevron-down" :size="12" />
      </button>

      <span class="sep" aria-hidden="true" />

      <div ref="appsEl" class="apps" role="group" aria-label="应用" @keydown="onAppsKeydown">
        <button
          v-for="app in shownApps"
          :key="app.id"
          class="tile"
          :class="{ 'is-drop': dragOverId === app.id, 'is-dragging': draggingId === app.id }"
          type="button"
          draggable="true"
          :aria-label="app.name"
          :title="isDesktop ? app.name : undefined"
          @click="launchApp(app as HubApp)"
          @mouseenter="showTip($event, app.name)"
          @mouseleave="hideTip"
          @focus="showTip($event, app.name)"
          @blur="hideTip"
          @dragstart="onDragStart(app.id)"
          @dragover.prevent="dragOverId = app.id"
          @dragleave="dragOverId === app.id && (dragOverId = null)"
          @drop.prevent="onDrop(app.id)"
          @dragend="onDragEnd"
        >
          <img
            v-if="iconFor(app.target)"
            class="tile-img"
            :src="iconFor(app.target)"
            :alt="app.name"
            draggable="false"
          />
          <span v-else class="tile-icon" :style="{ background: app.color }">{{ app.initials }}</span>
          <span v-if="app.broken" class="tile-broken" aria-hidden="true" />
        </button>

        <button
          v-if="needsOverflow"
          class="tile more"
          type="button"
          :aria-expanded="overflowMenuOpen"
          :aria-label="`还有 ${hiddenCount} 个应用，展开查看`"
          :title="`还有 ${hiddenCount} 个应用`"
          @click="toggleOverflow"
        >
          <Icon name="more" :size="16" />
        </button>
      </div>

      <span class="sep" aria-hidden="true" />

      <button
        class="clip-btn"
        :class="{ 'is-open': state.ui.panel === 'clipboard', 'is-paused': state.ui.paused }"
        type="button"
        :aria-expanded="state.ui.panel === 'clipboard'"
        aria-label="剪贴板历史"
        title="剪贴板历史"
        @click="openClipboard"
      >
        <Icon name="clipboard" :size="16" />
        <span v-if="!state.ui.paused" class="badge">{{ state.clips.length }}</span>
        <span v-else class="badge badge-paused">暂停</span>
      </button>

      <button
        class="clip-btn"
        :class="{ 'is-open': state.ui.panel === 'translation' }"
        type="button"
        :aria-expanded="state.ui.panel === 'translation'"
        aria-label="翻译"
        title="翻译"
        @click="openTranslate"
      >
        <Icon name="text" :size="16" />
      </button>
    </div>

    <!-- 浏览器样稿：没有独立窗口，菜单内联在图标条上方 -->
    <template v-if="!isDesktop">
      <div v-if="catOpen" class="menu menu-cat">
        <CategoryMenu />
      </div>
      <div v-if="overflowOpen" class="menu menu-overflow">
        <OverflowGrid />
      </div>
    </template>

    <Teleport to="body">
      <div
        v-if="tip.visible"
        class="tip"
        :style="{ left: `${tip.x}px`, top: `${tip.y}px` }"
        role="presentation"
      >
        {{ tip.text }}
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.bar-wrap {
  position: relative;
}

.bar {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 56px;
  padding: 0 10px 0 6px;
  background: var(--bar-bg);
  border: 1px solid var(--bar-border);
  border-radius: var(--r-bar);
  box-shadow: var(--bar-shadow);
}

.grip {
  width: 16px;
  height: 30px;
  display: grid;
  place-items: center;
  color: var(--text-3);
  opacity: 0.35;
  border-radius: 8px;
  cursor: grab;
  transition: opacity var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
}

.bar:hover .grip {
  opacity: 0.8;
}

.grip:hover {
  background: var(--hover);
}

.grip:active {
  cursor: grabbing;
}

.cat {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 8px;
  border-radius: 9px;
  font-size: var(--text-sm);
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.cat:hover {
  background: var(--hover);
  color: var(--text);
}

.sep {
  width: 1px;
  height: 20px;
  background: var(--line);
  flex: none;
}

.apps {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
}

.tile {
  position: relative;
  width: 36px;
  height: 36px;
  flex: none;
  display: grid;
  place-items: center;
  border-radius: var(--r-tile);
  transition: background var(--dur-fast) var(--ease), transform var(--dur-fast) var(--ease);
}

.tile:hover {
  background: var(--hover);
}

.tile:active {
  transform: scale(0.94);
}

.tile.is-dragging {
  opacity: 0.4;
}

.tile.is-drop::before {
  content: '';
  position: absolute;
  left: -3px;
  top: 5px;
  bottom: 5px;
  width: 2px;
  border-radius: 2px;
  background: var(--accent);
}

.tile-icon {
  width: 26px;
  height: 26px;
  display: grid;
  place-items: center;
  border-radius: 7px;
  color: #fff;
  font-size: 10.5px;
  font-weight: 600;
  letter-spacing: 0.1px;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.14), 0 1px 2px rgba(0, 0, 0, 0.24);
  user-select: none;
}

/* 真实 Windows 图标：按 26px 显示，保留原图比例 */
.tile-img {
  width: 26px;
  height: 26px;
  object-fit: contain;
  pointer-events: none;
  user-select: none;
}

/* 目标失效：右下角一个小红点，不改变图标本身 */
.tile-broken {
  position: absolute;
  right: 4px;
  bottom: 4px;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--danger, #e23b3b);
  box-shadow: 0 0 0 1.5px var(--bar-bg);
  pointer-events: none;
}

.more {
  color: var(--text-2);
}

.more:hover {
  color: var(--text);
}

.clip-btn {
  position: relative;
  display: flex;
  align-items: center;
  gap: 6px;
  height: 36px;
  padding: 0 8px;
  border-radius: var(--r-tile);
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.clip-btn:hover {
  background: var(--hover);
  color: var(--text);
}

.clip-btn.is-open {
  background: var(--accent-weak);
  color: var(--accent);
}

.clip-btn.is-paused {
  color: var(--text-3);
}

.badge {
  min-width: 17px;
  height: 16px;
  padding: 0 4px;
  display: grid;
  place-items: center;
  border-radius: 8px;
  background: var(--accent);
  color: #fff;
  font-size: 10px;
  font-weight: 600;
  line-height: 1;
}

.badge-paused {
  background: var(--surface-2);
  color: var(--text-3);
  font-size: 9.5px;
}

/* 浏览器样稿的内联菜单定位 */
.menu {
  position: absolute;
  bottom: calc(100% + 10px);
  z-index: var(--z-pop);
}

.menu-cat {
  left: 20px;
  width: 210px;
}

.menu-overflow {
  right: 8px;
  width: 322px;
}
</style>

<style>
/* 名称提示挂到 body，避免被图标条裁切（仅浏览器样稿） */
.tip {
  position: fixed;
  transform: translate(-50%, 0);
  padding: 4px 9px;
  border-radius: 7px;
  background: var(--surface);
  color: var(--text);
  font-size: var(--text-xs);
  line-height: 1.5;
  white-space: nowrap;
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.32);
  border: 1px solid var(--line);
  pointer-events: none;
  z-index: var(--z-toast);
  animation: tip-in 110ms var(--ease);
}

@keyframes tip-in {
  from {
    opacity: 0;
    transform: translate(-50%, -3px);
  }
  to {
    opacity: 1;
    transform: translate(-50%, 0);
  }
}
</style>
