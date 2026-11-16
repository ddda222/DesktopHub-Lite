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
import BarWindow from './windows/BarWindow.vue'
import PopWindow from './windows/PopWindow.vue'
import SettingsWindowRoot from './windows/SettingsWindowRoot.vue'
import ToastWindow from './windows/ToastWindow.vue'
import { isDesktop, windowLabel } from '@/lib/desktop/desktop'
import * as store from './store'
import './styles/tokens.css'
import './styles/base.css'

function pickRoot(): { root: Component; shell: string } {
  if (!isDesktop) return { root: BrowserPreview, shell: 'browser' }
  switch (windowLabel) {
    case 'bar':
      return { root: BarWindow, shell: 'bar' }
    case 'pop':
      return { root: PopWindow, shell: 'pop' }
    case 'settings':
      return { root: SettingsWindowRoot, shell: 'settings' }
    case 'toast':
      return { root: ToastWindow, shell: 'toast' }
    default:
      return { root: BrowserPreview, shell: 'browser' }
  }
}

const { root, shell } = pickRoot()

// 桌面窗口必须透明：窗口的透明区域要露出真实桌面，而不是一块不透明底色。
document.documentElement.dataset.shell = shell
if (isDesktop) {
  document.documentElement.dataset.theme = 'dark'
}

const app = createApp(root)

// 先加载数据再挂载，避免界面先闪一下空态。
void store.bootstrap().finally(() => {
  app.mount('#app')
})

// 仅开发环境：把 store 暴露到 window，方便用控制台核对各状态
if (import.meta.env.DEV) {
  ;(window as unknown as { __hub: typeof store }).__hub = store
}
