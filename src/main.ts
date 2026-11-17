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
import BrowserPreview from './windows/BrowserPreview.vue'
import StyleLab from './windows/StyleLab.vue'
import BarWindow from './windows/BarWindow.vue'
import PopWindow from './windows/PopWindow.vue'
import SettingsWindowRoot from './windows/SettingsWindowRoot.vue'
import TranslationWindow from './windows/TranslationWindow.vue'
import ImagePreviewWindow from './windows/ImagePreviewWindow.vue'
import CompareWindow from './windows/CompareWindow.vue'
import ToastWindow from './windows/ToastWindow.vue'
import { isDesktop, windowLabel, resolveWindowLabel } from '@/lib/desktop/desktop'
import * as store from './store'
import './styles/tokens.css'
import './styles/base.css'

function pickRoot(): { root: Component; shell: string } {
  if (!isDesktop) return { root: new URLSearchParams(location.search).get('preview') === 'styles' ? StyleLab : BrowserPreview, shell: 'browser' }
  if (windowLabel.startsWith('image-preview-')) return { root: ImagePreviewWindow, shell: 'image-preview' }
  if (windowLabel.startsWith('compare-')) return { root: CompareWindow, shell: 'compare' }
  switch (windowLabel) {
    case 'bar':
      return { root: BarWindow, shell: 'bar' }
    case 'pop':
      return { root: PopWindow, shell: 'pop' }
    case 'settings':
      return { root: SettingsWindowRoot, shell: 'settings' }
    case 'translation':
      return { root: TranslationWindow, shell: 'translation' }
    case 'toast':
      return { root: ToastWindow, shell: 'toast' }
    default:
      return { root: BrowserPreview, shell: 'browser' }
  }
}

async function mountWindow(): Promise<void> {
  await resolveWindowLabel()
  const { root, shell } = pickRoot()
  document.documentElement.dataset.shell = shell
  if (isDesktop) document.documentElement.dataset.theme = 'dark'
  try { if (!['image-preview', 'compare'].includes(shell)) await store.bootstrap() } finally { createApp(root).mount('#app') }
}
void mountWindow().catch(error => {
  document.getElementById('app')!.textContent = `窗口加载失败，请关闭后重新打开：${String(error)}`
})

// 仅开发环境：把 store 暴露到 window，方便用控制台核对各状态
if (import.meta.env.DEV) {
  ;(window as unknown as { __hub: typeof store }).__hub = store
}
