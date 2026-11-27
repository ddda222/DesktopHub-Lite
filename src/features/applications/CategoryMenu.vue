<script setup lang="ts">
/**
 * 分类菜单。桌面版渲染在独立的 pop 窗口里，浏览器样稿里内联渲染。
 * 两种场合共用同一份模板，避免两套代码走偏。
 */
import { onMounted, ref } from 'vue'
import Icon from '@/components/Icon.vue'
import { gsap, vGsapPress } from '@/lib/gsapMotion'
import { activeMotionPreset } from '@/lib/motionPresets'
import { categoryName, closePanel, openSettings, setCategory, state, visibleApps } from '@/store'

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

function pick(id: string): void {
  setCategory(id)
  closePanel()
}

function openAppsSettings(): void {
  openSettings('apps')
}
</script>

<template>
  <div
    ref="menuEl"
    class="menu menu-cat"
    role="menu"
    aria-label="应用分类"
  >
    <div class="menu-head">{{ categoryName }} · {{ visibleApps.length }} 个应用</div>
    <button
      v-for="cat in state.categories"
      :key="cat.id"
      v-gsap-press="state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap"
      class="menu-item"
      :class="{ 'is-active': cat.id === state.ui.activeCategoryId }"
      role="menuitemradio"
      :aria-checked="cat.id === state.ui.activeCategoryId"
      @click="pick(cat.id)"
    >
      <span class="menu-label">{{ cat.name }}</span>
      <Icon v-if="cat.id === state.ui.activeCategoryId" name="check" :size="14" />
    </button>
    <span class="menu-line" aria-hidden="true" />
    <button
      v-gsap-press="state.ui.islandReducedMotion ? undefined : activeMotionPreset.buttonTap"
      class="menu-item"
      role="menuitem"
      @click="openAppsSettings"
    >
      <span class="menu-label">应用管理与设置…</span>
      <Icon name="chevron-right" :size="13" />
    </button>
  </div>
</template>

<style scoped>
.menu {
  /* 弹层窗口按内容收缩，没有最小宽度会把「应用管理与设置…」裁掉 */
  min-width: 208px;
  padding: 6px;
  background: var(--pop-bg);
  border: 1px solid var(--pop-border);
  border-radius: var(--r-card);
  box-shadow: var(--pop-shadow);
}

.menu-head {
  padding: 2px 9px 6px;
  font-size: var(--text-xs);
  color: var(--text-3);
}

.menu-item {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  height: 32px;
  padding: 0 9px;
  border-radius: 8px;
  font-size: 12.5px;
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.menu-item:hover {
  background: var(--hover);
  color: var(--text);
}

.menu-item.is-active {
  color: var(--accent);
}

.menu-line {
  display: block;
  height: 1px;
  margin: 5px 4px;
  background: var(--line);
}
</style>
