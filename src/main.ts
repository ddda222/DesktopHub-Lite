/**
 * 窗口路由：同一个 index.html 被多个 Tauri 窗口加载，按窗口标签决定挂哪个根组件。
 *
 * 为什么要按窗口拆分：
 * - 图标条窗口的尺寸必须等于图标条本身，透明区域才不会挡住桌面点击；
 *   如果把它做成全屏透明覆盖层，桌面就整块点不动了。
 * - 弹层、设置、提示各有自己的尺寸与焦点行为，放同一个窗口里只能互相迁就。
 *
 * 浏览器里没有这些窗口，`windowLabel` 是 'browser'，退回单页样稿。
 */
import { createApp, type Component } from 'vue'
import { isDesktop, windowLabel, resolveWindowLabel } from '@/lib/desktop/desktop'
import * as store from './store'

type RootLoader = () => Promise<{ default: Component }>

function pickRoot(): { load: RootLoader; shell: string } {
  if (!isDesktop) {
    const preview = new URLSearchParams(location.search).get('preview')
    if (preview === 'styles') return { load: () => import('./windows/StyleLab.vue'), shell: 'browser' }
    if (preview === 'settings-lab') return { load: () => import('./windows/InteractiveSettingsLab.vue'), shell: 'browser' }
    if (preview === 'capture') return { load: () => import('./windows/CaptureOverlay.vue'), shell: 'capture' }
    return { load: () => import('./windows/BrowserPreview.vue'), shell: 'browser' }
  }
  if (windowLabel.startsWith('image-preview-')) return { load: () => import('./windows/ImagePreviewWindow.vue'), shell: 'image-preview' }
  switch (windowLabel) {
    case 'bar':
      return { load: () => import('./windows/BarWindow.vue'), shell: 'bar' }
    case 'pop':
      return { load: () => import('./windows/PopWindow.vue'), shell: 'pop' }
    case 'settings':
      return { load: () => import('./windows/SettingsWindowRoot.vue'), shell: 'settings' }
    case 'translation':
      return { load: () => import('./windows/TranslationWindow.vue'), shell: 'translation' }
    case 'capture':
      return { load: () => import('./windows/CaptureOverlay.vue'), shell: 'capture' }
    default:
      return { load: () => import('./windows/BrowserPreview.vue'), shell: 'browser' }
  }
}

async function mountWindow(): Promise<void> {
  await resolveWindowLabel()
  const { load, shell } = pickRoot()
  document.documentElement.dataset.shell = shell
  if (isDesktop) document.documentElement.dataset.theme = 'dark'
  // 先拉取根组件再拉取全局主题样式：主题里的 `:root[data-theme] *` 等规则
  // 与组件 scoped 样式特异性相同，必须排在后面才能保持原有覆盖关系。
  const loading = load().then(async (module) => {
    await import('./styles/global')
    return module.default
  })
  try {
    if (shell !== 'image-preview' && shell !== 'capture') await store.bootstrap()
  } finally {
    createApp(await loading).mount('#app')
  }
}
void mountWindow().catch(error => {
  document.getElementById('app')!.textContent = `窗口加载失败，请关闭后重新打开：${String(error)}`
})

// 仅开发环境：把 store 暴露到 window，方便用控制台核对各状态
if (import.meta.env.DEV) {
  ;(window as unknown as { __hub: typeof store }).__hub = store
}
