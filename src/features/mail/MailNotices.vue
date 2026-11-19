<script setup lang="ts">
import { ref } from 'vue'
import { invokeHost } from '@/lib/desktop/desktop'
import { providerName, type MailNotice } from './mail'

defineProps<{ notices: MailNotice[] }>()
const emit = defineEmits<{ error: [message: string] }>()
const busy = ref(false)
async function act(id: string, action: 'copy-code' | 'copy-link' | 'open-link' | 'dismiss', index?: number) {
  if (busy.value) return
  busy.value = true
  try { await invokeHost('mail_action', { id, action, index }) }
  catch (error) { emit('error', String(error)) }
  finally { busy.value = false }
}
</script>

<template>
  <section class="mail-list scroll-y" aria-label="新邮件">
    <article v-for="item in notices" :key="item.id" class="mail-item">
      <header><span>{{ item.provider === 'gmail' ? item.address : providerName[item.provider] }} · {{ item.sender || '未知发件人' }}</span>
        <button type="button" :disabled="busy" aria-label="移除这条邮件提醒" @click="act(item.id, 'dismiss')">移除</button>
      </header>
      <h3>{{ item.subject || '无主题邮件' }}</h3>
      <button v-if="item.code" class="code" type="button" :disabled="busy" @click="act(item.id, 'copy-code')">复制验证码 <strong>{{ item.code }}</strong></button>
      <div v-for="(link, index) in item.links" :key="index" class="mail-link">
        <span>{{ link.label }}<small>{{ link.domain }}</small></span>
        <button type="button" :disabled="busy" :aria-label="'复制链接：' + link.domain" @click="act(item.id, 'copy-link', index)">复制链接</button>
        <button type="button" :disabled="busy" :aria-label="'打开网页：' + link.domain" @click="act(item.id, 'open-link', index)">打开网页</button>
      </div>
    </article>
  </section>
</template>

<style scoped>
.mail-list { max-height: min(48vh, 340px); overflow-y: auto; overscroll-behavior: contain; color: var(--text); }
.mail-item { display: grid; gap: 8px; padding: 12px 4px; }
.mail-item + .mail-item { border-top: 1px solid var(--line); }
.mail-item header, .mail-link { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; }
.mail-item header span, .mail-link span { flex: 1 1 9em; min-width: 0; overflow-wrap: anywhere; }
.mail-item header, small { color: var(--text-2); font-size: 12px; }
.mail-item h3 { font-size: 14px; margin: 0; overflow-wrap: anywhere; }
.mail-item button { color: var(--text); font: inherit; font-size: 12px; background: var(--surface-2); border: 1px solid var(--line); border-radius: 7px; padding: 5px 8px; }
.mail-item button:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.mail-link { font-size: 13px; }
small { display: block; }
.code { justify-self: start; }
.code strong { margin-inline-start: 6px; font-variant-numeric: tabular-nums; letter-spacing: .08em; }
</style>
