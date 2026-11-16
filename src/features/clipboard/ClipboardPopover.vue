<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import { clipSummary, relativeTime } from '@/lib/format'
import type { ClipItem } from '@/lib/desktop/types'
import {
  activeClip,
  assignClipGroup,
  closeDetail,
  closePanel,
  copyClip,
  deleteClip,
  filteredClips,
  groupFilters,
  moveActive,
  openDetail,
  openTranslation,
  setGroup,
  setPaused,
  setQuery,
  state
} from '@/store'

const searchEl = ref<HTMLInputElement | null>(null)
const listEl = ref<HTMLElement | null>(null)

const detailClip = computed(() => activeClip.value)

/** 文件缺失或没有可用地址时都不渲染 <img>，避免出现破图 */
function isImageBroken(clip: ClipItem | null): boolean {
  if (!clip || clip.kind !== 'image') return false
  return clip.imageMissing === true || !clip.imageUrl
}

function onInput(event: Event): void {
  const target = event.target as HTMLInputElement | null
  if (target) setQuery(target.value)
}

function onClear(): void {
  setQuery('')
  searchEl.value?.focus()
}

function onTogglePause(): void {
  void setPaused(!state.ui.paused)
}

function setActive(index: number): void {
  state.ui.activeIndex = index
}

function clampActive(): void {
  const total = filteredClips.value.length
  if (state.ui.activeIndex > total - 1) {
    state.ui.activeIndex = Math.max(0, total - 1)
  }
}

function onCopy(clip: ClipItem | null): void {
  if (!clip) return
  void copyClip(clip)
  // 行不可聚焦，点击后把焦点交回搜索框，键盘操作才能继续
  searchEl.value?.focus()
}

function onDelete(clip: ClipItem | null): void {
  if (!clip) return
  void deleteClip(clip).then(clampActive)
}

function onTranslate(clip: ClipItem | null): void {
  if (!clip || clip.kind !== 'text') return
  openTranslation(clip.text ?? '', true)
}

async function moveAndReveal(delta: number): Promise<void> {
  moveActive(delta)
  await nextTick()
  const row = listEl.value?.querySelector<HTMLElement>(`[data-index="${state.ui.activeIndex}"]`)
  row?.scrollIntoView({ block: 'nearest' })
}

function onKeydown(event: KeyboardEvent): void {
  // 中文输入法候选确认期间（isComposing / 229）不能把 Enter 当成复制
  if (event.isComposing || event.keyCode === 229) return

  if (event.key === 'Escape') {
    event.stopPropagation()
    closePanel()
    return
  }

  // 详情视图内由详情自己的按钮负责操作，方向键与 Enter 不再作用于列表
  if (state.ui.detailId) return

  if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
    event.preventDefault()
    void moveAndReveal(event.key === 'ArrowDown' ? 1 : -1)
    return
  }

  if (event.key === 'Enter') {
    // 焦点在按钮上时 Enter 属于该按钮，不再触发列表复制
    const target = event.target as HTMLElement | null
    if (target?.closest('button')) return
    const clip = filteredClips.value[state.ui.activeIndex]
    if (!clip) return
    event.preventDefault()
    onCopy(clip)
  }
}

onMounted(() => {
  searchEl.value?.focus()
})

// 从详情返回列表后焦点回到搜索框
watch(
  () => state.ui.detailId,
  (id) => {
    if (id) return
    void nextTick(() => {
      searchEl.value?.focus()
    })
  }
)
</script>

<template>
  <div class="pop" role="dialog" aria-label="剪贴板历史" @keydown="onKeydown">
    <header class="head">
      <div class="search">
        <Icon class="search-icon" name="search" :size="14" />
        <input
          ref="searchEl"
          type="text"
          spellcheck="false"
          autocomplete="off"
          placeholder="搜索文本、网址、图片…"
          aria-label="搜索剪贴板历史"
          :value="state.ui.query"
          @input="onInput"
        />
        <button v-if="state.ui.query" class="clear" type="button" aria-label="清除搜索" @click="onClear">
          <Icon name="close" :size="13" />
        </button>
      </div>

      <button
        class="pause"
        type="button"
        :class="{ 'is-paused': state.ui.paused }"
        :aria-pressed="state.ui.paused"
        :aria-label="state.ui.paused ? '已暂停记录，点击恢复' : '暂停记录剪贴板'"
        @click="onTogglePause"
      >
        <Icon :name="state.ui.paused ? 'play' : 'pause'" :size="14" />
        <span v-if="state.ui.paused">已暂停</span>
      </button>
    </header>

    <div class="chips" role="group" aria-label="按分组筛选">
      <button
        v-for="group in groupFilters"
        :key="group"
        class="chip"
        type="button"
        :class="{ 'is-on': group === state.ui.group }"
        :aria-pressed="group === state.ui.group"
        @click="setGroup(group)"
      >
        {{ group }}
      </button>
    </div>

    <section v-if="detailClip" class="detail" aria-label="记录详情">
      <div class="detail-head">
        <button class="back" type="button" @click="closeDetail()">
          <Icon name="back" :size="14" />
          <span>返回列表</span>
        </button>
        <span class="detail-meta">{{ detailClip.group }} · {{ relativeTime(detailClip.copiedAt) }}</span>
      </div>

      <div class="detail-body scroll-y">
        <template v-if="detailClip.kind === 'image'">
          <div v-if="isImageBroken(detailClip)" class="detail-missing">
            <Icon name="image" :size="22" />
            <p class="detail-missing-title">图片文件已缺失</p>
            <p class="detail-missing-sub">原文件可能被移动或删除，这条记录无法预览，也无法复制。</p>
          </div>
          <img
            v-else
            class="detail-img"
            :src="detailClip.imageUrl"
            :alt="`剪贴板图片 ${detailClip.width ?? 0} × ${detailClip.height ?? 0}`"
          />
        </template>
        <pre v-else class="detail-text">{{ detailClip.text || '（空白文本）' }}</pre>
      </div>

      <div class="detail-actions">
        <select :value="detailClip.group" aria-label="记录分组" @change="assignClipGroup(detailClip.id, ($event.target as HTMLSelectElement).value)">
          <option v-for="group in state.groups" :key="group" :value="group">{{ group }}</option>
        </select>
        <button
          class="btn btn-ghost is-danger"
          type="button"
          @click="onDelete(detailClip)"
        >
          删除
        </button>
        <span class="spacer" />
        <button
          v-if="detailClip.kind === 'text'"
          class="btn btn-ghost"
          type="button"
          @click="onTranslate(detailClip)"
        >
          翻译这条
        </button>
        <button
          class="btn btn-primary"
          type="button"
          :disabled="isImageBroken(detailClip)"
          @click="onCopy(detailClip)"
        >
          复制
        </button>
      </div>
    </section>

    <template v-else>
      <div v-if="state.ui.paused" class="banner banner-paused" role="status">
        <Icon name="pause" :size="13" />
        <span>已暂停记录，新内容不会写入历史</span>
        <button class="banner-btn" type="button" @click="onTogglePause">恢复记录</button>
      </div>

      <div v-if="filteredClips.length" ref="listEl" class="list scroll-y" role="group" aria-label="剪贴板记录">
        <div
          v-for="(clip, index) in filteredClips"
          :key="clip.id"
          class="row"
          role="button"
          tabindex="-1"
          :data-index="index"
          :class="{ 'is-active': index === state.ui.activeIndex }"
          :aria-label="`${clipSummary(clip)}，${clip.group}，${relativeTime(clip.copiedAt)}`"
          @click="onCopy(clip)"
          @mouseenter="setActive(index)"
        >
          <div class="thumb" :class="{ 'is-broken': isImageBroken(clip) }">
            <img v-if="clip.kind === 'image' && !isImageBroken(clip)" :src="clip.imageUrl" alt="" />
            <Icon v-else-if="clip.kind === 'image'" name="image" :size="16" />
            <Icon v-else name="text" :size="16" />
          </div>

          <div class="row-main">
            <div class="row-title">{{ clipSummary(clip) }}</div>
            <div class="row-meta">
              <span class="row-meta-text">{{ clip.group }} · {{ relativeTime(clip.copiedAt) }}</span>
              <span v-if="clip.imageMissing" class="tag-missing">图片缺失</span>
            </div>
          </div>

          <div class="row-actions">
            <button
              class="icon-btn"
              type="button"
              :aria-label="`查看详情：${clipSummary(clip)}`"
              @click.stop="openDetail(clip.id)"
            >
              <Icon name="info" :size="14" />
            </button>
            <button
              class="icon-btn is-danger"
              type="button"
              :aria-label="`删除这条记录：${clipSummary(clip)}`"
              @click.stop="onDelete(clip)"
            >
              <Icon name="trash" :size="14" />
            </button>
          </div>
        </div>
      </div>

      <div v-else class="empty">
        <Icon :name="state.clips.length ? 'search' : 'clipboard'" :size="26" />
        <p class="empty-title">{{ state.clips.length ? '没有匹配的记录' : '还没有剪贴板记录' }}</p>
        <p class="empty-sub">
          {{
            state.clips.length
              ? '换个关键词，或切到「全部」分组再试。'
              : '复制任意文本、网址或图片，历史会按时间倒序出现在这里。'
          }}
        </p>
      </div>
    </template>

    <footer class="foot">方向键选择 · Enter 复制 · Esc 关闭</footer>
  </div>
</template>

<style scoped>
.pop {
  display: flex;
  flex-direction: column;
  width: 384px;
  max-height: 480px;
  background: var(--pop-bg);
  border: 1px solid var(--pop-border);
  border-radius: var(--r-pop);
  box-shadow: var(--pop-shadow);
  color: var(--text);
  overflow: hidden;
  animation: pop-in var(--dur) var(--ease);
}

@keyframes pop-in {
  from {
    opacity: 0;
    transform: translateY(4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.pop button:focus-visible,
.pop .row:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 1px;
}

/* 顶部：搜索与暂停 */
.head {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 10px 10px 8px;
}

.search {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 8px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  transition: border-color var(--dur-fast) var(--ease), background var(--dur-fast) var(--ease);
}

.search:hover {
  border-color: var(--line-strong);
}

.search:focus-within {
  border-color: var(--accent);
}

.search-icon {
  flex: none;
  color: var(--text-3);
}

.search input {
  flex: 1 1 auto;
  min-width: 0;
  height: 100%;
  padding: 0;
  border: 0;
  background: none;
  font-size: var(--text-base);
  color: var(--text);
}

.search input::placeholder {
  color: var(--text-3);
}

/* 焦点落在输入框上时由外层描边提示，避免框内出现第二道轮廓 */
.search input:focus-visible {
  outline: none;
}

.clear {
  flex: none;
  width: 20px;
  height: 20px;
  display: grid;
  place-items: center;
  border-radius: 6px;
  color: var(--text-3);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.clear:hover {
  background: var(--hover);
  color: var(--text);
}

.clear:active {
  background: var(--active);
}

.pause {
  flex: none;
  display: flex;
  align-items: center;
  gap: 5px;
  height: 32px;
  padding: 0 8px;
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  font-size: var(--text-sm);
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.pause:hover {
  background: var(--hover);
  border-color: var(--line-strong);
  color: var(--text);
}

.pause:active {
  background: var(--active);
}

.pause.is-paused {
  background: var(--accent-weak);
  border-color: var(--accent);
  color: var(--accent);
}

.pause.is-paused:hover {
  border-color: var(--accent-hover);
  color: var(--accent-hover);
}

/* 提示条 */
.banner {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0 10px 8px;
  padding: 6px 8px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  font-size: var(--text-xs);
  color: var(--text-2);
}

.banner-readonly {
  border-color: var(--line-strong);
}

.banner-btn {
  margin-left: auto;
  flex: none;
  height: 22px;
  padding: 0 8px;
  border: 1px solid var(--line-strong);
  border-radius: 6px;
  font-size: var(--text-xs);
  color: var(--text);
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.banner-btn:hover {
  background: var(--hover);
  border-color: var(--accent);
  color: var(--accent);
}

.banner-btn:active {
  background: var(--active);
}

/* 分组 chips：不换行，横向滚动 */
.chips {
  flex: none;
  display: flex;
  gap: 4px;
  padding: 0 10px 8px;
  overflow-x: auto;
  overflow-y: hidden;
  scrollbar-width: none;
}

.chips::-webkit-scrollbar {
  display: none;
}

.chip {
  flex: none;
  height: 24px;
  padding: 0 9px;
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  font-size: var(--text-xs);
  color: var(--text-2);
  white-space: nowrap;
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.chip:hover {
  background: var(--hover);
  border-color: var(--line-strong);
  color: var(--text);
}

.chip:active {
  background: var(--active);
}

.chip.is-on {
  background: var(--accent-weak);
  border-color: var(--accent-weak);
  color: var(--accent);
}

.chip.is-on:hover {
  border-color: var(--accent);
}

/* 列表 */
.list {
  flex: 1 1 auto;
  min-height: 0;
  padding: 0 8px 8px;
}

.row {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 7px 8px;
  border-radius: var(--r-tile);
  cursor: pointer;
  user-select: none;
  transition: background var(--dur-fast) var(--ease);
}

.row + .row {
  margin-top: 2px;
}

.row:hover {
  background: var(--hover);
}

.row:active {
  background: var(--active);
}

.row.is-active {
  background: var(--accent-weak);
}

.row.is-active:active {
  background: var(--active);
}

.thumb {
  flex: none;
  width: 36px;
  height: 36px;
  display: grid;
  place-items: center;
  overflow: hidden;
  background: var(--thumb-bg);
  border: 1px solid var(--line);
  border-radius: 8px;
  color: var(--text-2);
}

.thumb img {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: cover;
  border-radius: 8px;
}

.thumb.is-broken {
  background: var(--surface-2);
  color: var(--text-3);
}

.row-main {
  flex: 1 1 auto;
  min-width: 0;
}

.row-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-base);
  color: var(--text);
}

.row-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  margin-top: 2px;
}

.row-meta-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-xs);
  color: var(--text-3);
}

.tag-missing {
  flex: none;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--danger-weak);
  color: var(--danger);
  font-size: var(--text-xs);
  line-height: 1.35;
}

.row-actions {
  flex: none;
  display: flex;
  gap: 2px;
  opacity: 0;
  visibility: hidden;
  transition: opacity var(--dur-fast) var(--ease);
}

.row:hover .row-actions,
.row.is-active .row-actions {
  opacity: 1;
  visibility: visible;
}

.icon-btn {
  width: 26px;
  height: 26px;
  display: grid;
  place-items: center;
  border-radius: 7px;
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.icon-btn:hover {
  background: var(--active);
  color: var(--text);
}

.icon-btn:active {
  background: var(--line-strong);
}

.icon-btn.is-danger:hover {
  background: var(--danger-weak);
  color: var(--danger);
}

/* 空态 */
.empty {
  flex: 1 1 auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 34px 24px 38px;
  text-align: center;
  color: var(--text-3);
}

.empty-title {
  margin: 8px 0 0;
  font-size: var(--text-base);
  color: var(--text-2);
}

.empty-sub {
  margin: 0;
  max-width: 260px;
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--text-3);
}

/* 详情 */
.detail {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 0 10px 10px;
}

.detail-head {
  flex: none;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding-bottom: 8px;
}

.back {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 26px;
  padding: 0 8px 0 5px;
  border-radius: 7px;
  font-size: var(--text-sm);
  color: var(--text-2);
  transition: background var(--dur-fast) var(--ease), color var(--dur-fast) var(--ease);
}

.back:hover {
  background: var(--hover);
  color: var(--text);
}

.back:active {
  background: var(--active);
}

.detail-meta {
  margin-left: auto;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-xs);
  color: var(--text-3);
}

.detail-body {
  flex: 1 1 auto;
  min-height: 0;
  padding: 10px;
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: var(--r-card);
}

.detail-text {
  margin: 0;
  font-family: var(--font-sans);
  font-size: var(--text-base);
  line-height: 1.65;
  color: var(--text);
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  user-select: text;
}

.detail-img {
  display: block;
  width: auto;
  height: auto;
  max-width: 100%;
  max-height: 300px;
  margin: 0 auto;
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
}

.detail-missing {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 26px 12px;
  text-align: center;
  color: var(--text-3);
}

.detail-missing p {
  margin: 0;
}

.detail-missing-title {
  font-size: var(--text-base);
  color: var(--danger);
}

.detail-missing-sub {
  max-width: 250px;
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--text-3);
}

.detail-actions {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  padding-top: 10px;
}

.spacer {
  flex: 1 1 auto;
}

.btn {
  height: 28px;
  padding: 0 12px;
  border: 1px solid var(--line);
  border-radius: var(--r-chip);
  font-size: var(--text-sm);
  transition: background var(--dur-fast) var(--ease), border-color var(--dur-fast) var(--ease),
    color var(--dur-fast) var(--ease);
}

.btn-ghost {
  color: var(--text-2);
}

.btn-ghost:hover {
  background: var(--hover);
  border-color: var(--line-strong);
  color: var(--text);
}

.btn-ghost:active {
  background: var(--active);
}

.btn-ghost.is-danger {
  border-color: var(--danger-weak);
  color: var(--danger);
}

.btn-ghost.is-danger:hover {
  background: var(--danger-weak);
  border-color: var(--danger);
  color: var(--danger);
}

.btn-primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-text);
}

.btn-primary:hover {
  background: var(--accent-hover);
  border-color: var(--accent-hover);
}

.btn-primary:active {
  background: var(--accent);
  border-color: var(--accent);
}

.btn:disabled {
  background: var(--surface-2);
  border-color: var(--line);
  color: var(--text-3);
  cursor: not-allowed;
}

/* 底部提示 */
.foot {
  flex: none;
  padding: 7px 12px;
  border-top: 1px solid var(--line);
  font-size: var(--text-xs);
  color: var(--text-3);
}
</style>
