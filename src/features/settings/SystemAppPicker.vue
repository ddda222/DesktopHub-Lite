<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import Icon from '@/components/Icon.vue'
import { invokeHost } from '@/lib/desktop/desktop'
import { importSelectedSystemApps, state } from '@/store'

const props = defineProps<{ categoryId: string }>()
const emit = defineEmits<{ close: []; saved: [added: number, skipped: number]; saving: [value: boolean] }>()
interface SystemApp { name: string; target: string }
const apps = ref<SystemApp[]>([])
const query = ref('')
const searchInput = ref<HTMLInputElement | null>(null)
const selected = ref<string[]>([])
const loading = ref(false)
const saving = ref(false)
const error = ref('')
const existing = computed(() => new Set(state.apps.map(a => a.target.trim().toLowerCase())))
const visible = computed(() => apps.value.filter(app => app.name.toLowerCase().includes(query.value.trim().toLowerCase())))
const toImport = computed(() => selected.value.filter(target => !existing.value.has(target.toLowerCase())))

async function load() {
  loading.value = true
  error.value = ''
  try {
    const found = await invokeHost<SystemApp[]>('list_system_apps')
    apps.value = found.sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'))
    selected.value = []
  } catch (e) { error.value = String(e) }
  finally { loading.value = false }
}
async function save() {
  if (saving.value || !toImport.value.length) return
  saving.value = true
  emit('saving', true)
  error.value = ''
  try {
    const result = await importSelectedSystemApps(props.categoryId, [...toImport.value])
    emit('saved', result.added, result.skipped)
  } catch (e) { error.value = String(e) }
  finally { saving.value = false; emit('saving', false) }
}
onMounted(() => {
  searchInput.value?.focus({ preventScroll: true })
  void load()
})
</script>

<template>
  <section id="system-app-picker" class="system-picker" aria-label="选择系统应用">
    <div class="system-search">
      <Icon name="search" :size="15" />
      <input ref="searchInput" v-model="query" class="control" type="search" aria-label="搜索系统应用" placeholder="搜索 Windows 应用" />
      <span v-if="!loading && apps.length" class="result-count">{{ visible.length }} 项</span>
    </div>
    <p v-if="loading" class="hint" role="status">正在读取 Windows 应用列表…</p>
    <p v-else-if="error" class="inline-feedback error" role="alert">{{ error }} <button class="btn" @click="load">刷新列表</button></p>
    <p v-else-if="!visible.length" class="hint">{{ apps.length ? '没有匹配的应用' : 'Windows 没有返回可导入的应用' }}</p>
    <div v-if="visible.length" class="system-list scroll-y" role="group" aria-label="系统应用列表">
      <label v-for="app in visible" :key="app.target" class="system-row" :title="app.target">
        <input v-model="selected" type="checkbox" :value="app.target" :disabled="saving || existing.has(app.target.toLowerCase())" />
        <span>{{ app.name }}</span><small v-if="existing.has(app.target.toLowerCase())">已添加</small>
      </label>
    </div>
    <div class="actions picker-actions">
      <button type="button" class="btn btn-primary" :disabled="loading || saving || !toImport.length" @click="save">{{ saving ? '导入中…' : `导入所选（${toImport.length}）` }}</button>
      <button type="button" class="btn" :disabled="saving" @click="emit('close')">取消</button>
    </div>
  </section>
</template>

<style scoped>
.system-picker { display: grid; gap: var(--space-2); margin-block: var(--space-3); padding-block: var(--space-3); border-block: 1px solid var(--line); }
.system-search { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: var(--space-2); }
.system-search > svg { grid-column: 1; grid-row: 1; margin-left: 10px; z-index: 1; color: var(--text-3); pointer-events: none; }
.system-search .control { grid-column: 1 / 3; grid-row: 1; padding-inline-start: 32px; }
.result-count { grid-column: 3; grid-row: 1; color: var(--text-2); font-size: var(--text-sm); font-variant-numeric: tabular-nums; }
.system-list { max-height: 216px; padding: 3px; background: var(--surface); border: 1px solid var(--line); border-radius: var(--r-chip); }
.system-row { display: flex; align-items: center; gap: var(--space-2); min-height: 32px; padding: 4px 8px; border-radius: 5px; cursor: pointer; transition: background-color var(--dur-fast) var(--ease); }
.system-row:has(input:enabled:not(:checked)):hover { background: var(--hover); }
.system-row:has(input:checked) { background: var(--accent-weak); }
.system-row:has(input:focus-visible) { outline: 2px solid var(--accent); outline-offset: -2px; }
.system-row:has(input:disabled) { cursor: default; color: var(--text-2); }
.system-row input { flex: none; width: 16px; height: 16px; margin: 0; accent-color: var(--accent); }
.system-row input[type="checkbox"]:focus-visible { outline: none; }
.system-row span { min-width: 0; overflow-wrap: anywhere; }
.system-row small { margin-inline-start: auto; color: var(--text-2); white-space: nowrap; }
.picker-actions { justify-content: flex-end; }
</style>
