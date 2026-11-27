<script lang="ts">
export interface SelectOption {
  value: string | number
  label: string
  disabled?: boolean
}

interface ActiveSelectEntry {
  close: () => void
  containsTrigger: (root?: Element | null) => boolean
}

// 跨组件共享当前展开下拉框的单例引用，便于灵动岛防误收起与全局 Esc 优先级控制。
let activeSelect: ActiveSelectEntry | null = null

export function hasOpenThemedSelect(within?: Element | null): boolean {
  if (!activeSelect) return false
  return within ? activeSelect.containsTrigger(within) : true
}

export function closeAnyOpenThemedSelect(within?: Element | null): boolean {
  if (!activeSelect) return false
  if (within && !activeSelect.containsTrigger(within)) return false
  activeSelect.close()
  return true
}
</script>

<script setup lang="ts">
/**
 * 跟随主题视觉令牌与器物轮廓风格的自定义下拉选择器。
 *
 * 使用 Teleport 挂载至 body 规避父容器 overflow 裁切，支持键盘上下键导航、
 * 上下空间自适应弹出以及全局全主题配色同步。
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import { gsap } from '@/lib/gsapMotion'

const props = withDefaults(defineProps<{
  modelValue: string | number
  options: readonly SelectOption[]
  ariaLabel?: string
  id?: string
  disabled?: boolean
  size?: 'default' | 'sm' | 'xs'
  placeholder?: string
}>(), {
  ariaLabel: undefined,
  id: undefined,
  disabled: false,
  size: 'default',
  placeholder: '请选择'
})

const emit = defineEmits<{
  'update:modelValue': [value: string | number]
  change: [value: string | number]
}>()

const triggerEl = ref<HTMLButtonElement | null>(null)
const menuEl = ref<HTMLDivElement | null>(null)
const open = ref(false)
const highlightedIndex = ref(-1)
const menuStyle = ref<Record<string, string>>({})
const placement = ref<'bottom' | 'top'>('bottom')
const listboxId = `themed-listbox-${Math.random().toString(36).slice(2, 9)}`

const selectedOption = computed(() =>
  props.options.find(opt => opt.value === props.modelValue) ?? null
)

const displayText = computed(() => selectedOption.value?.label ?? props.placeholder)

function isReducedMotion(): boolean {
  return document.documentElement.dataset.reducedMotion === '1' ||
    window.matchMedia('(prefers-reduced-motion: reduce)').matches
}

function enabledIndices(): number[] {
  const indices: number[] = []
  props.options.forEach((opt, idx) => {
    if (!opt.disabled) indices.push(idx)
  })
  return indices
}

function updatePosition(): void {
  const trigger = triggerEl.value
  if (!trigger) return
  const rect = trigger.getBoundingClientRect()
  const vw = window.innerWidth
  const vh = window.innerHeight
  const islandSurface = trigger.closest<HTMLElement>('.island')
  const clampToIsland = !!islandSurface && islandSurface.clientHeight >= 140
  const boundsRect = clampToIsland ? islandSurface.getBoundingClientRect() : null
  const gap = 5
  const pad = clampToIsland ? 6 : 8
  const minLeft = boundsRect ? boundsRect.left + pad : pad
  const maxRight = boundsRect ? boundsRect.right - pad : vw - pad
  const topBound = boundsRect ? boundsRect.top + pad : pad
  const bottomBound = boundsRect ? boundsRect.bottom - pad : vh - pad

  const spaceBelow = bottomBound - rect.bottom - gap
  const spaceAbove = rect.top - gap - topBound
  const flipThreshold = clampToIsland ? 132 : 160
  const showTop = spaceBelow < flipThreshold && spaceAbove > spaceBelow
  placement.value = showTop ? 'top' : 'bottom'

  const maxAvail = Math.max(84, Math.min(clampToIsland ? 210 : 240, showTop ? spaceAbove : spaceBelow))
  const availW = Math.max(96, maxRight - minLeft)
  const minW = props.size === 'xs' ? Math.max(rect.width, 96) : Math.max(rect.width, 148)
  const width = Math.min(availW, minW)
  const left = Math.max(minLeft, Math.min(rect.left, maxRight - width))

  if (showTop) {
    menuStyle.value = {
      left: `${Math.round(left)}px`,
      bottom: `${Math.round(vh - rect.top + gap)}px`,
      width: `${Math.round(width)}px`,
      maxHeight: `${Math.round(maxAvail)}px`
    }
  } else {
    menuStyle.value = {
      left: `${Math.round(left)}px`,
      top: `${Math.round(rect.bottom + gap)}px`,
      width: `${Math.round(width)}px`,
      maxHeight: `${Math.round(maxAvail)}px`
    }
  }
}

const activeEntry: ActiveSelectEntry = {
  close,
  containsTrigger: (root?: Element | null) => !!(root && triggerEl.value && root.contains(triggerEl.value))
}

function close(): void {
  if (!open.value) return
  open.value = false
  if (activeSelect === activeEntry) activeSelect = null
  detachGlobalListeners()
  window.dispatchEvent(new CustomEvent('hub:themed-select-closed', { detail: { trigger: triggerEl.value } }))
}

function openMenu(): void {
  if (props.disabled || open.value) return
  if (activeSelect && activeSelect !== activeEntry) activeSelect.close()
  activeSelect = activeEntry
  const curIdx = props.options.findIndex(opt => opt.value === props.modelValue && !opt.disabled)
  highlightedIndex.value = curIdx >= 0 ? curIdx : (enabledIndices()[0] ?? -1)
  updatePosition()
  open.value = true
  attachGlobalListeners()
  void nextTick(() => {
    const el = menuEl.value
    if (!el) return
    const activeItem = el.querySelector<HTMLElement>('.themed-option.is-highlighted')
    activeItem?.scrollIntoView({ block: 'nearest' })
    if (!isReducedMotion()) {
      gsap.fromTo(
        el,
        { opacity: 0, y: placement.value === 'bottom' ? -5 : 5, scale: 0.985 },
        { opacity: 1, y: 0, scale: 1, duration: 0.14, ease: 'power2.out', clearProps: 'transform' }
      )
    }
  })
}

function toggle(): void {
  if (open.value) close()
  else openMenu()
}

function selectOption(option: SelectOption): void {
  if (option.disabled) return
  const changed = option.value !== props.modelValue
  emit('update:modelValue', option.value)
  if (changed) emit('change', option.value)
  close()
  triggerEl.value?.focus({ preventScroll: true })
}

function moveHighlight(step: 1 | -1): void {
  const enabled = enabledIndices()
  if (!enabled.length) return
  const curPos = enabled.indexOf(highlightedIndex.value)
  const nextPos = curPos < 0 ? 0 : (curPos + step + enabled.length) % enabled.length
  highlightedIndex.value = enabled[nextPos]!
  void nextTick(() => {
    menuEl.value?.querySelector<HTMLElement>('.themed-option.is-highlighted')?.scrollIntoView({ block: 'nearest' })
  })
}

function onTriggerKeydown(event: KeyboardEvent): void {
  if (props.disabled) return
  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    if (!open.value) {
      openMenu()
    } else {
      moveHighlight(event.key === 'ArrowDown' ? 1 : -1)
    }
    return
  }
  if (event.key === 'Home' && open.value) {
    event.preventDefault()
    const first = enabledIndices()[0]
    if (first !== undefined) highlightedIndex.value = first
    return
  }
  if (event.key === 'End' && open.value) {
    event.preventDefault()
    const list = enabledIndices()
    if (list.length) highlightedIndex.value = list[list.length - 1]!
    return
  }
  if (event.key === 'Enter' || event.key === ' ') {
    if (open.value) {
      event.preventDefault()
      const current = props.options[highlightedIndex.value]
      if (current && !current.disabled) selectOption(current)
    } else if (event.key === ' ') {
      event.preventDefault()
      openMenu()
    }
    return
  }
  if (event.key === 'Escape' && open.value) {
    event.preventDefault()
    event.stopPropagation()
    event.stopImmediatePropagation()
    close()
    triggerEl.value?.focus({ preventScroll: true })
  }
}

function handleOutsidePointer(event: PointerEvent): void {
  const target = event.target as Node | null
  if (!target) return
  if (triggerEl.value?.contains(target) || menuEl.value?.contains(target)) return
  close()
}

function handleGlobalScroll(event: Event): void {
  const target = event.target as Node | null
  if (target && menuEl.value?.contains(target)) return
  close()
}

function handleWindowBlur(): void {
  close()
}

function handleGlobalEscape(event: KeyboardEvent): void {
  if (event.key === 'Escape' && open.value) {
    event.preventDefault()
    event.stopPropagation()
    event.stopImmediatePropagation()
    close()
    triggerEl.value?.focus({ preventScroll: true })
  }
}

function attachGlobalListeners(): void {
  document.addEventListener('pointerdown', handleOutsidePointer, true)
  window.addEventListener('keydown', handleGlobalEscape, true)
  window.addEventListener('scroll', handleGlobalScroll, true)
  window.addEventListener('resize', close)
  window.addEventListener('blur', handleWindowBlur)
}

function detachGlobalListeners(): void {
  document.removeEventListener('pointerdown', handleOutsidePointer, true)
  window.removeEventListener('keydown', handleGlobalEscape, true)
  window.removeEventListener('scroll', handleGlobalScroll, true)
  window.removeEventListener('resize', close)
  window.removeEventListener('blur', handleWindowBlur)
}

watch(() => props.disabled, isDisabled => {
  if (isDisabled) close()
})

onBeforeUnmount(() => {
  if (menuEl.value) gsap.killTweensOf(menuEl.value)
  close()
})
</script>

<template>
  <button
    :id="id"
    ref="triggerEl"
    type="button"
    class="themed-select-trigger"
    :class="[`size-${size}`, { 'is-open': open }]"
    role="combobox"
    :disabled="disabled"
    :aria-label="ariaLabel"
    :aria-expanded="open"
    aria-haspopup="listbox"
    :aria-controls="open ? listboxId : undefined"
    @click="toggle"
    @keydown="onTriggerKeydown"
  >
    <span class="themed-select-value" :class="{ 'is-placeholder': !selectedOption }">
      {{ displayText }}
    </span>
    <Icon class="themed-select-chev" name="chevron-down" :size="size === 'xs' ? 11 : 12" />
  </button>

  <Teleport to="body">
    <div
      v-if="open"
      :id="listboxId"
      ref="menuEl"
      class="themed-select-menu scroll-y"
      :class="[`size-${size}`, `place-${placement}`]"
      role="listbox"
      :aria-label="ariaLabel"
      :style="menuStyle"
      @pointerdown.stop
    >
      <button
        v-for="(option, index) in options"
        :key="String(option.value)"
        type="button"
        role="option"
        class="themed-option"
        :disabled="option.disabled"
        :aria-selected="option.value === modelValue"
        :class="{
          'is-selected': option.value === modelValue,
          'is-highlighted': index === highlightedIndex,
          'is-disabled': option.disabled
        }"
        @mouseenter="!option.disabled && (highlightedIndex = index)"
        @click="selectOption(option)"
      >
        <span class="themed-option-label">{{ option.label }}</span>
        <Icon v-if="option.value === modelValue" class="themed-option-check" name="check" :size="12" />
      </button>
    </div>
  </Teleport>
</template>

<style scoped>
.themed-select-trigger {
  width: 100%;
  min-width: 0;
  display: inline-flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-height: 36px;
  padding: 6px 10px 6px 12px;
  border: 1px solid var(--line-strong);
  border-radius: var(--r-chip, 9px);
  background: var(--well-bg, var(--surface-2));
  box-shadow: var(--well-shadow, none);
  color: var(--text);
  font: inherit;
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  transition: border-color 120ms ease, background 120ms ease, box-shadow 120ms ease;
}
.themed-select-trigger.size-sm {
  min-height: 30px;
  height: 30px;
  padding: 0 8px 0 10px;
  font-size: 12px;
  border-color: var(--line);
}
.themed-select-trigger.size-xs {
  width: auto;
  min-height: 24px;
  height: 26px;
  padding: 0 7px 0 9px;
  gap: 5px;
  font-size: 12px;
  border-color: var(--line);
}
.themed-select-trigger:hover:not(:disabled) {
  border-color: var(--text-3);
}
.themed-select-trigger:focus-visible,
.themed-select-trigger.is-open {
  outline: none;
  border-color: var(--accent, var(--text));
  background: var(--surface);
  box-shadow: 0 0 0 2px var(--accent-weak, var(--hover));
}
.themed-select-trigger:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}
.themed-select-value {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.themed-select-value.is-placeholder {
  color: var(--text-3);
}
.themed-select-chev {
  flex: none;
  color: var(--text-2);
  transition: transform 150ms var(--ease-out, ease);
}
.themed-select-trigger.is-open .themed-select-chev {
  transform: rotate(180deg);
  color: var(--accent, var(--text));
}

/* 弹层菜单通过 Teleport 渲染在 body 下，使用主题 CSS 变量自动适配 8 套主题 */
.themed-select-menu {
  position: fixed;
  z-index: 120;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 5px;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--surface);
  border: 1px solid var(--line-strong);
  border-radius: var(--r-card, 11px);
  box-shadow: 0 14px 36px rgb(0 0 0 / 0.34), 0 0 0 1px var(--line);
  color: var(--text);
  font-family: var(--font-sans);
}
.themed-select-menu.size-sm,
.themed-select-menu.size-xs {
  padding: 4px;
}
.themed-option {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 7px 9px;
  border-radius: calc(var(--r-chip, 8px) - 1px);
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-2);
  font: inherit;
  font-size: 13px;
  line-height: 1.35;
  text-align: left;
  cursor: pointer;
  transition: background 95ms ease, color 95ms ease, border-color 95ms ease;
}
.themed-select-menu.size-sm .themed-option,
.themed-select-menu.size-xs .themed-option {
  padding: 5px 8px;
  font-size: 12px;
}
.themed-option-label {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.themed-option-check {
  flex: none;
  color: var(--accent, var(--text));
}
.themed-option.is-highlighted:not(:disabled) {
  background: var(--hover);
  color: var(--text);
}
.themed-option.is-selected {
  background: var(--accent-weak);
  color: var(--accent);
  font-weight: 600;
}
.themed-option.is-selected.is-highlighted {
  border-color: var(--accent-weak);
}
.themed-option:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* 器物主题特征细化 */
:root[data-theme='midnight'] .themed-select-trigger {
  border-radius: 6px;
  border-color: rgba(109, 182, 255, 0.28);
}
:root[data-theme='midnight'] .themed-select-menu {
  border-radius: 8px;
  border-color: rgba(109, 182, 255, 0.42);
  box-shadow: 0 16px 40px rgba(2, 8, 22, 0.72), 0 0 16px rgba(109, 182, 255, 0.16);
}
:root[data-theme='mecha'] .themed-select-trigger,
:root[data-theme='mecha'] .themed-select-menu {
  border-radius: 3px;
  border-color: rgba(245, 158, 11, 0.36);
}
:root[data-theme='mecha'] .themed-option.is-selected {
  background: var(--accent);
  color: #111318;
  clip-path: polygon(4px 0, 100% 0, 100% calc(100% - 4px), calc(100% - 4px) 100%, 0 100%, 0 4px);
}
:root[data-theme='mecha'] .themed-option.is-selected .themed-option-check {
  color: #111318;
}
:root[data-theme='pixel'] .themed-select-trigger,
:root[data-theme='pixel'] .themed-select-menu,
:root[data-theme='pixel'] .themed-option {
  border-radius: 0;
  font-family: var(--font-mono);
}
:root[data-theme='pixel'] .themed-select-menu {
  border: 2px solid #0f0d1a;
  box-shadow: 4px 4px 0 #0f0d1a;
}
:root[data-theme='pixel'] .themed-option.is-selected {
  background: var(--accent);
  color: #0f0d1a;
}
:root[data-theme='pixel'] .themed-option.is-selected .themed-option-check {
  color: #0f0d1a;
}
:root[data-theme='blueprint'] .themed-select-trigger,
:root[data-theme='blueprint'] .themed-select-menu,
:root[data-theme='blueprint'] .themed-option {
  border-radius: 2px;
  font-family: var(--font-mono);
}
:root[data-theme='blueprint'] .themed-select-menu {
  border: 1px dashed rgba(224, 242, 254, 0.65);
}
:root[data-theme='blueprint'] .themed-option.is-selected {
  background: #fde047;
  color: #092241;
}
:root[data-theme='blueprint'] .themed-option.is-selected .themed-option-check {
  color: #092241;
}
:root[data-theme='neon'] .themed-select-trigger {
  border-color: rgba(255, 43, 214, 0.36);
}
:root[data-theme='neon'] .themed-select-menu {
  border-color: #00f0ff;
  box-shadow: 0 0 22px rgba(255, 43, 214, 0.25), 0 16px 38px rgba(4, 1, 12, 0.85);
}
:root[data-theme='neon'] .themed-option.is-selected {
  background: linear-gradient(90deg, rgba(255, 43, 214, 0.28), rgba(0, 240, 255, 0.22));
  color: #ffffff;
  border-color: rgba(0, 240, 255, 0.52);
}
</style>
