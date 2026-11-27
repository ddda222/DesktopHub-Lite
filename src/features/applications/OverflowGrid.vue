<script setup lang="ts">
/**
 * 溢出网格：分类里放不下的应用必须可发现，不能直接裁掉。
 * 桌面版渲染在独立的 pop 窗口里，浏览器样稿里内联渲染。
 */
import { onMounted, ref } from 'vue'
import Icon from '@/components/Icon.vue'
import { gsap, vGsapHover, vGsapPress } from '@/lib/gsapMotion'
import { activeMotionPreset } from '@/lib/motionPresets'
import { categoryName, closePanel, iconFor, launchApp, state, visibleApps } from '@/store'

const menuEl = ref<HTMLElement | null>(null)

onMounted(() => {
  if (!menuEl.value || state.ui.islandReducedMotion) return
  const spec = activeMotionPreset.value.popTransition
  gsap.fromTo(
    menuEl.value,
    { opacity: 0, y: 6, scale: 0.98 },
    { opacity: 1, y: 0, scale: 1, duration: spec.duration, ease: spec.ease, clearProps: 'transform' }
  )
})

async function launch(id: string): Promise<void> {
  const app = visibleApps.value.find((item) => item.id === id)
  if (!app) return
  if (await launchApp(app)) closePanel()
}
</script>

<template>
  <div
    ref="menuEl"
    class="menu menu-overflow"
    role="dialog"
    aria-label="全部应用"
  >
    <div class="menu-title">{{ categoryName }} · 全部 {{ visibleApps.length }} 个应用</div>
    <div class="ov-grid scroll-y">
      <button
        v-for="app in visibleApps"
        :key="app.id"
        v-gsap-hover="state.ui.islandReducedMotion ? undefined : activeMotionPreset.tileHover"
        v-gsap-press="state.ui.islandReducedMotion ? undefined : activeMotionPreset.tileTap"
        class="ov-item"
        type="button"
        :title="app.name"
        @click="launch(app.id)"
      >
        <img
          v-if="iconFor(app.target)"
          class="ov-img"
          :src="iconFor(app.target)"
          :alt="app.name"
          draggable="false"
        />
        <span v-else class="ov-icon" :style="{ background: app.color }">{{ app.initials }}</span>
        <span class="ov-name">{{ app.name }}</span>
        <span v-if="app.broken" class="ov-broken">失效</span>
      </button>
    </div>
    <div v-if="!visibleApps.length" class="ov-empty">
      <Icon name="grid" :size="18" />
      <span>这个分类还没有应用</span>
    </div>
  </div>
</template>

<style scoped>
.menu {
  /* 弹层窗口按内容收缩，应用多时必须有高度上限并滚动，否则会超出屏幕 */
  max-height: 460px;
  display: flex;
  flex-direction: column;
  padding: 10px;
  background: var(--pop-bg);
  border: 1px solid var(--pop-border);
  border-radius: var(--r-card);
  box-shadow: var(--pop-shadow);
}

.menu-title {
  flex: none;
  padding: 2px 4px 8px;
  font-size: var(--text-xs);
  color: var(--text-3);
}

.ov-grid {
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-width: thin;
  scrollbar-color: var(--line-strong) transparent;
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  align-content: start;
  gap: 4px;
}

.ov-grid::-webkit-scrollbar {
  width: 6px;
}

.ov-grid::-webkit-scrollbar-thumb {
  background: var(--line-strong);
  border-radius: 3px;
}

.ov-grid::-webkit-scrollbar-track {
  background: transparent;
}

.ov-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  min-width: 0;
  padding: 8px 4px;
  border-radius: 9px;
  transition: background var(--dur-fast) var(--ease);
}

.ov-item:hover {
  background: var(--hover);
}

.ov-icon {
  width: 28px;
  height: 28px;
  display: grid;
  place-items: center;
  border-radius: 8px;
  color: #fff;
  font-size: 11px;
  font-weight: 600;
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.14);
}

.ov-img {
  width: 28px;
  height: 28px;
  object-fit: contain;
  pointer-events: none;
}

.ov-broken {
  font-size: 9.5px;
  line-height: 1.2;
  color: var(--danger, #e23b3b);
}

.ov-name {
  max-width: 100%;
  font-size: 11px;
  line-height: 1.35;
  color: var(--text-2);
  text-align: center;
  overflow-wrap: anywhere;
  display: -webkit-box;
  -webkit-box-orient: vertical;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  overflow: hidden;
}

.ov-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 18px 0 12px;
  font-size: var(--text-sm);
  color: var(--text-3);
}
</style>
