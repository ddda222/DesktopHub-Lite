<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'

/**
 * 快捷键实体录制器：
 * 点选「录制」后直接按键盘组合键生成实体键帽，无需手敲 Ctrl+Shift+V 字符串；
 * 点击「试按」可在活体灵动岛中立即演练对应面板动作。
 */
export interface ShortcutModel {
  bar: string
  clipboard: string
  translation: string
}

const props = defineProps<{
  modelValue: ShortcutModel
}>()

const emit = defineEmits<{
  'update:modelValue': [value: ShortcutModel]
  'test-trigger': [key: keyof ShortcutModel, event: MouseEvent]
  'keys-pressed': [caps: string[]]
}>()

const items: { key: keyof ShortcutModel; title: string; desc: string; testLabel: string }[] = [
  { key: 'bar', title: '唤出图标条', desc: '全局展开或收起顶部灵动岛，快速启动常用应用。', testLabel: '演练唤出' },
  { key: 'clipboard', title: '打开剪贴板', desc: '直接跳到灵动岛的剪贴板历史与常用片段页。', testLabel: '演练剪贴板' },
  { key: 'translation', title: '快速翻译', desc: '唤出翻译面板并准备接收待译文本。', testLabel: '演练翻译' }
]

const recordingKey = ref<keyof ShortcutModel | null>(null)
const livePreview = ref<string[]>([])

const duplicateKeys = computed(() => {
  const counts = new Map<string, number>()
  for (const raw of Object.values(props.modelValue)) {
    const normalized = raw.trim().toLowerCase()
    if (!normalized) continue
    counts.set(normalized, (counts.get(normalized) ?? 0) + 1)
  }
  return new Set(
    Array.from(counts.entries())
      .filter(([, count]) => count > 1)
      .map(([value]) => value)
  )
})

function splitCaps(combo: string): string[] {
  return combo
    .split('+')
    .map(part => part.trim())
    .filter(Boolean)
}

function normalizeMainKey(event: KeyboardEvent): string | null {
  const { key, code } = event
  if (['Control', 'Shift', 'Alt', 'Meta'].includes(key)) return null
  if (code === 'Space' || key === ' ') return 'Space'
  if (key.length === 1) return key.toUpperCase()
  if (/^F\d{1,2}$/i.test(key)) return key.toUpperCase()
  const aliases: Record<string, string> = {
    ArrowUp: 'Up',
    ArrowDown: 'Down',
    ArrowLeft: 'Left',
    ArrowRight: 'Right',
    Enter: 'Enter',
    Tab: 'Tab',
    Backspace: 'Backspace',
    Delete: 'Delete'
  }
  return aliases[key] ?? key
}

function stopRecording(): void {
  recordingKey.value = null
  livePreview.value = []
  window.removeEventListener('keydown', onRecordKeydown, true)
  window.removeEventListener('keyup', onRecordKeyup, true)
}

function onRecordKeydown(event: KeyboardEvent): void {
  if (!recordingKey.value) return
  event.preventDefault()
  event.stopPropagation()

  if (event.key === 'Escape') {
    stopRecording()
    return
  }

  const modifiers: string[] = []
  if (event.ctrlKey) modifiers.push('Ctrl')
  if (event.altKey) modifiers.push('Alt')
  if (event.shiftKey) modifiers.push('Shift')
  if (event.metaKey) modifiers.push('Win')

  const mainKey = normalizeMainKey(event)
  livePreview.value = mainKey ? [...modifiers, mainKey] : modifiers
  if (livePreview.value.length) emit('keys-pressed', livePreview.value)

  // 要求至少包含一个修饰键，或功能键 F1-F12，避免误占普通打字键
  if (mainKey && (modifiers.length > 0 || /^F\d{1,2}$/.test(mainKey))) {
    const combo = [...modifiers, mainKey].join('+')
    emit('update:modelValue', { ...props.modelValue, [recordingKey.value]: combo })
    stopRecording()
  }
}

function onRecordKeyup(event: KeyboardEvent): void {
  if (!recordingKey.value) return
  const modifiers: string[] = []
  if (event.ctrlKey) modifiers.push('Ctrl')
  if (event.altKey) modifiers.push('Alt')
  if (event.shiftKey) modifiers.push('Shift')
  if (event.metaKey) modifiers.push('Win')
  livePreview.value = modifiers
}

function startRecording(key: keyof ShortcutModel): void {
  if (recordingKey.value === key) {
    stopRecording()
    return
  }
  stopRecording()
  recordingKey.value = key
  livePreview.value = []
  window.addEventListener('keydown', onRecordKeydown, true)
  window.addEventListener('keyup', onRecordKeyup, true)
}

function hasConflict(combo: string): boolean {
  return duplicateKeys.value.has(combo.trim().toLowerCase())
}

onBeforeUnmount(stopRecording)
</script>

<template>
  <div class="recorder-list">
    <div
      v-for="item in items"
      :key="item.key"
      class="recorder-row"
      :class="{
        'is-recording': recordingKey === item.key,
        'is-conflict': hasConflict(props.modelValue[item.key])
      }"
    >
      <div class="recorder-info">
        <div class="recorder-title">
          <span>{{ item.title }}</span>
          <span v-if="recordingKey === item.key" class="rec-tag">正在聆听按键… Esc 取消</span>
          <span v-else-if="hasConflict(props.modelValue[item.key])" class="conflict-tag">组合键冲突</span>
        </div>
        <p class="recorder-desc">{{ item.desc }}</p>
      </div>

      <div class="recorder-controls">
        <button
          type="button"
          class="keycap-stage"
          :aria-pressed="recordingKey === item.key"
          :title="recordingKey === item.key ? '按下组合键完成录制，Esc 取消' : '点击后直接在键盘上按组合键'"
          @click="startRecording(item.key)"
        >
          <template v-if="recordingKey === item.key">
            <template v-if="livePreview.length">
              <kbd v-for="(cap, idx) in livePreview" :key="`${cap}-${idx}`" class="keycap is-live">{{ cap }}</kbd>
            </template>
            <span v-else class="listen-placeholder">按下修饰键 + 主键…</span>
          </template>
          <template v-else>
            <kbd v-for="(cap, idx) in splitCaps(props.modelValue[item.key])" :key="`${cap}-${idx}`" class="keycap">
              {{ cap }}
            </kbd>
          </template>
        </button>

        <button
          type="button"
          class="deck-btn is-sm"
          :class="{ 'is-primary': recordingKey === item.key }"
          @click="startRecording(item.key)"
        >
          {{ recordingKey === item.key ? '取消' : '录制' }}
        </button>
        <button
          type="button"
          class="deck-btn is-sm"
          @click="emit('test-trigger', item.key, $event)"
        >
          {{ item.testLabel }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.recorder-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.recorder-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  flex-wrap: wrap;
  padding: 12px 14px;
  border-radius: 12px;
  background: var(--deck-well);
  border: 1px solid var(--deck-line-strong);
  transition: border-color 140ms ease, box-shadow 140ms ease;
}

.recorder-row.is-recording {
  border-color: var(--deck-accent);
  box-shadow: 0 0 0 2px var(--deck-accent-soft);
  background: var(--deck-panel);
}

.recorder-row.is-conflict {
  border-color: #dc2626;
}

.recorder-info {
  min-width: 190px;
  flex: 1;
}

.recorder-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 700;
}

.recorder-desc {
  margin: 3px 0 0;
  font-size: 11.5px;
  color: var(--deck-ink-2);
}

.rec-tag {
  font-size: 10.5px;
  font-weight: 650;
  color: var(--deck-accent);
}

.conflict-tag {
  font-size: 10.5px;
  font-weight: 650;
  color: #dc2626;
}

.recorder-controls {
  display: flex;
  align-items: center;
  gap: 7px;
  flex-wrap: wrap;
}

.keycap-stage {
  min-width: 156px;
  min-height: 36px;
  padding: 4px 10px;
  border-radius: 9px;
  border: 1px solid var(--deck-line-strong);
  background: var(--deck-panel);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  cursor: pointer;
  box-shadow: var(--deck-inset-shadow);
}

.keycap {
  min-width: 26px;
  padding: 3px 7px;
  border-radius: 6px;
  border: 1px solid var(--deck-line-strong);
  background: var(--deck-key-bg);
  color: var(--deck-ink);
  font-family: var(--deck-mono);
  font-size: 11.5px;
  font-weight: 700;
  box-shadow: 0 2px 0 var(--deck-line-strong);
}

.keycap.is-live {
  border-color: var(--deck-accent);
  color: var(--deck-accent);
}

.listen-placeholder {
  font-size: 11px;
  color: var(--deck-accent);
  font-weight: 600;
}
</style>
