<script setup lang="ts">
import { ref, watch } from 'vue'
import Icon from '@/components/Icon.vue'
import { invokeHost } from '@/lib/desktop/desktop'
import { providerName, type MailNotice } from './mail'

defineProps<{ notices: MailNotice[] }>()
const emit = defineEmits<{ error: [message: string] }>()
const busy = ref(false)
const copied = ref<{ id: string; index?: number } | null>(null)
watch(copied, (value, _, onCleanup) => {
  if (!value) return
  const timer = setTimeout(() => { copied.value = null }, 2200)
  onCleanup(() => clearTimeout(timer))
})

function isCopied(id: string, index?: number) {
  return copied.value?.id === id && copied.value.index === index
}

async function act(id: string, action: 'copy-code' | 'copy-link' | 'open-link' | 'dismiss', index?: number) {
  if (busy.value) return
  const copying = action === 'copy-code' || action === 'copy-link'
  busy.value = true
  try {
    if (copying) copied.value = null
    await invokeHost('mail_action', { id, action, index })
    if (copying) copied.value = { id, index }
  }
  catch (error) { emit('error', String(error)) }
  finally { busy.value = false }
}
</script>

<template>
  <section class="mail-panel" aria-label="最近邮件">
    <div class="mail-heading"><h2>最近邮件</h2><span>{{ notices.length }} 封</span></div>
    <div class="mail-list scroll-y">
      <article v-for="item in notices" :key="item.id" class="mail-item">
        <header>
          <h3>{{ item.subject || '无主题邮件' }}</h3>
          <button class="dismiss" type="button" :disabled="busy" :aria-label="'移除提醒：' + (item.subject || '无主题邮件')" @click="act(item.id, 'dismiss')">移除</button>
        </header>
        <p class="mail-meta">
          <span>{{ item.sender || '未知发件人' }}</span>
          <span aria-hidden="true">·</span>
          <span>{{ item.address || providerName[item.provider] }}</span>
        </p>
        <button v-if="item.code" class="code" :class="{ copied: isCopied(item.id) }" type="button" :disabled="busy" @click="act(item.id, 'copy-code')">
          <strong>{{ item.code }}</strong>
          <Icon :name="isCopied(item.id) ? 'check' : 'copy'" :size="14" />
          {{ isCopied(item.id) ? '已复制' : '复制验证码' }}
        </button>
        <div v-for="(link, index) in item.links" :key="index" class="mail-link">
          <div class="link-target"><strong>{{ link.label }}</strong><small>{{ link.domain }}</small></div>
          <div class="link-actions">
            <button type="button" :class="{ copied: isCopied(item.id, index) }" :disabled="busy" :aria-label="'复制链接：' + link.label + '，' + link.domain" @click="act(item.id, 'copy-link', index)">
              {{ isCopied(item.id, index) ? '已复制' : '复制链接' }}
            </button>
            <button type="button" :disabled="busy" :aria-label="'打开网页：' + link.label + '，' + link.domain" @click="act(item.id, 'open-link', index)">打开网页<Icon name="external" :size="12" /></button>
          </div>
        </div>
      </article>
    </div>
    <span class="sr-only" role="status">{{ copied ? (copied.index === undefined ? '验证码已复制' : '链接已复制') : '' }}</span>
  </section>
</template>

<style scoped>
.mail-panel { min-width: 0; color: var(--text); }
.mail-heading, header, .mail-link, .link-actions { display: flex; align-items: center; gap: 8px; }
.mail-heading { justify-content: space-between; padding-bottom: 6px; border-bottom: 1px solid var(--line); }
h2, h3, .mail-meta { margin: 0; }
h2 { font-size: 13.5px; font-weight: 600; }
.mail-heading span, small { color: var(--text-3); font-size: 11px; font-variant-numeric: tabular-nums; }
.mail-list { max-height: 340px; }
.mail-item { display: grid; min-width: 0; gap: 6px; padding: 9px 2px; overflow-wrap: anywhere; }
.mail-item + .mail-item, .mail-link + .mail-link { border-top: 1px solid var(--line); }
header { justify-content: space-between; align-items: baseline; }
h3 { font-size: 13px; line-height: 1.4; font-weight: 600; }
.mail-meta { display: flex; flex-wrap: wrap; gap: 4px; font-size: 11.5px; color: var(--text-3); user-select: text; }
h3, .link-target { user-select: text; }
button { display: inline-flex; align-items: center; justify-content: center; gap: 5px; min-height: 28px; padding: 3px 9px; color: var(--text-2); font-size: 11.5px; background: var(--surface-2); border: 1px solid var(--line); border-radius: var(--r-chip); transition: background 120ms ease, border-color 120ms ease, color 120ms ease; }
button:hover:enabled { background: var(--hover); border-color: var(--line-strong); color: var(--text); }
button:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
button:disabled { cursor: wait; }
button.dismiss { flex: none; min-height: 22px; padding: 2px 6px; color: var(--text-3); font-size: 11px; background: transparent; border-color: transparent; }
button.dismiss:hover:enabled { color: var(--text); }
.code { justify-self: start; min-height: 32px; gap: 7px; color: var(--accent); background: var(--accent-weak); border-color: color-mix(in srgb, var(--accent) 45%, var(--line)); font-weight: 550; }
.code:hover:enabled { background: var(--accent); color: var(--accent-text); border-color: var(--accent); }
.code strong { font: 650 15px/1.2 var(--font-mono); letter-spacing: .08em; font-variant-numeric: tabular-nums; }
button.copied { color: var(--accent-text); border-color: var(--accent); background: var(--accent); }
.mail-link { flex-wrap: wrap; padding-top: 5px; }
.link-target { flex: 1 1 10em; min-width: 0; font-size: 12.5px; }
.link-target strong { font-weight: 550; }
small { display: block; }
.link-actions { gap: 4px; }
</style>
