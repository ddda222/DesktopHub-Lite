<script setup lang="ts">
import { computed, ref } from 'vue'
import { invokeHost, isDesktop } from '@/lib/desktop/desktop'
import { providerName, useMailList, type MailProvider, type MailAccount } from '@/features/mail/mail'
import { pushToast } from '@/store'

const providers: MailProvider[] = ['qq', 'gmail']
const editing = ref<MailProvider | null>(null)
const originalAddress = ref('')
const address = ref('')
const secret = ref('')
const busy = ref(false)
const error = ref('')
const accounts = useMailList<MailAccount>('settings', 'mail_accounts', 'mail-accounts-changed', message => { error.value = message })
const byProvider = computed(() => ({
  qq: accounts.value.filter(item => item.provider === 'qq'),
  gmail: accounts.value.filter(item => item.provider === 'gmail'),
}))
const secretLabel = (p: MailProvider) => p === 'qq' ? '授权码' : '应用专用密码'

type SetupPage = 'qq-account' | 'qq-guide' | 'google-app-passwords' | 'google-two-step' | 'google-guide'
async function openSetupPage(page: SetupPage) {
  try { await invokeHost('open_mail_setup_page', { page }) }
  catch (e) { pushToast(String(e), 'error') }
}

function edit(provider: MailProvider | null, existingAddress = '') {
  editing.value = provider
  originalAddress.value = existingAddress
  address.value = existingAddress
  secret.value = ''
  error.value = ''
}
async function save(provider: MailProvider) {
  if (busy.value || !isDesktop) return
  if (provider === 'gmail' && !originalAddress.value && byProvider.value.gmail.some(item =>
    item.address.toLowerCase() === address.value.trim().toLowerCase())) {
    error.value = '这个邮箱已绑定，可以在对应账号旁点「重新绑定」。'
    return
  }
  busy.value = true
  error.value = ''
  try {
    await invokeHost('bind_mail_account', { provider, address: address.value.trim(), secret: secret.value })
    edit(null)
  } catch (e) { error.value = String(e) }
  finally { busy.value = false }
}
async function disconnect({ provider, address }: MailAccount) {
  if (busy.value || !isDesktop) return
  busy.value = true
  error.value = ''
  try { await invokeHost('unbind_mail_account', { provider, address }); edit(null) }
  catch (e) { error.value = String(e) }
  finally { busy.value = false }
}
</script>

<template>
  <div class="panel">
    <h3 class="panel-title">邮箱通知</h3>
    <p class="hint">Lite 运行时在灵动岛提醒新邮件。首次绑定从当前收件箱开始。</p>
    <p v-if="!isDesktop" class="hint">请在桌面版绑定邮箱。</p>
    <p v-if="error" class="inline-feedback error" role="alert">{{ error }}</p>
    <section v-for="p in providers" :key="p" class="card" :aria-label="providerName[p]">
      <div class="set-row mail-row">
        <div class="set-text">
          <strong class="set-label">{{ providerName[p] }}</strong>
          <span v-if="!byProvider[p].length" class="set-desc">未绑定</span>
        </div>
        <div class="mail-buttons">
          <button v-if="(editing !== p || originalAddress) && (p === 'gmail' || !byProvider[p].length)" class="btn" type="button"
            :disabled="busy || !isDesktop" @click="edit(p)">{{ p === 'gmail' ? '添加 Gmail 账号' : '绑定邮箱' }}</button>
          <button class="btn" type="button" :disabled="!isDesktop"
            @click="openSetupPage(p === 'qq' ? 'qq-account' : 'google-app-passwords')">{{ p === 'qq' ? '获取 QQ 授权码' : '创建应用专用密码' }}</button>
        </div>
      </div>
      <div v-for="item in byProvider[p]" :key="item.address" class="set-row mail-row">
        <div class="set-text">
          <strong class="set-label mail-address">{{ item.address }}</strong>
          <span class="set-desc" role="status">{{ item.status }}</span>
        </div>
        <div class="mail-buttons" role="group" :aria-label="item.address">
          <button class="btn" type="button" :disabled="busy || !isDesktop" @click="edit(p, item.address)">重新绑定</button>
          <button class="btn" type="button" :disabled="busy || !isDesktop" @click="disconnect(item)">解绑</button>
        </div>
      </div>
      <details class="mail-guide" :open="editing === p">
        <summary>{{ p === 'qq' ? 'QQ 邮箱绑定步骤' : 'Gmail 绑定步骤' }}</summary>
        <ol v-if="p === 'qq'">
          <li>点「获取 QQ 授权码」，登录要绑定的 QQ 邮箱。若进入收件箱，点右上角头像，依次打开「设置 → 账号与安全」。</li>
          <li>在「安全设置」中找到 IMAP 服务，点「开启服务」。开启后点「生成授权码」，按网页提示验证身份。</li>
          <li>复制生成的 16 位授权码。回到 Lite 的绑定表单，填入完整邮箱地址，把授权码粘贴到「授权码」一栏，点「验证并绑定」。</li>
        </ol>
        <ol v-else>
          <li>确认要绑定的 Google 账号已开启两步验证。可点下方的「两步验证设置」查看。</li>
          <li>点「创建应用专用密码」，选择同一账号，应用名称填「DesktopHub Lite」，点「创建」。</li>
          <li>复制显示的 16 位专用密码。回到 Lite 的绑定表单，填入完整 Gmail 地址，将密码粘贴到「应用专用密码」一栏，点「验证并绑定」。</li>
        </ol>
        <div class="mail-buttons">
          <button v-if="p === 'gmail'" class="btn" type="button" :disabled="!isDesktop" @click="openSetupPage('google-two-step')">两步验证设置</button>
          <button class="btn" type="button" :disabled="!isDesktop"
            @click="openSetupPage(p === 'qq' ? 'qq-guide' : 'google-guide')">{{ p === 'qq' ? 'QQ 官方图文说明' : 'Google 官方帮助' }}</button>
        </div>
      </details>
      <form v-if="editing === p" class="mail-form" @submit.prevent="save(p)">
        <label class="field"><span class="field-label">邮箱地址</span>
          <input v-model="address" class="control" type="email" required autocomplete="off" spellcheck="false"
            :readonly="p === 'gmail' && !!originalAddress" :disabled="busy" />
        </label>
        <label class="field"><span class="field-label">{{ secretLabel(p) }}</span>
          <input v-model="secret" class="control" type="password" required autocomplete="new-password" :disabled="busy" />
        </label>
        <div class="mail-buttons">
          <button class="btn btn-primary" type="submit" :disabled="busy || !address.trim() || !secret.trim()">{{ busy ? '正在验证收信…' : '验证并绑定' }}</button>
          <button class="btn" type="button" :disabled="busy" @click="edit(null)">取消</button>
        </div>
      </form>
    </section>
  </div>
</template>

<style scoped>
.mail-row { flex-wrap: wrap; }
.mail-address { overflow-wrap: anywhere; }
.mail-buttons { display: flex; flex-wrap: wrap; gap: 8px; }
.mail-guide ol { margin: 4px 0 12px; padding-inline-start: 22px; list-style: decimal; font-size: 13px; line-height: 1.6; }
.mail-guide li + li { margin-top: 4px; }
.mail-form { display: grid; gap: 12px; padding: 12px 0 16px; }
</style>
