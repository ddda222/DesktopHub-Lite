import type { HubApp } from '@/lib/desktop/types'
import chromium from './icons/chromium.png'
import explorer from './icons/explorer.png'
import wechatMark from './icons/wechat.svg?raw'
import terminal from './icons/terminal.png'
import vscode from './icons/vscode.png'
import notepad from './icons/notepad.png'
import githubMark from './icons/github.svg?raw'
import dockerMark from './icons/docker.svg?raw'
import powershell from './icons/powershell.png'
import taskmanager from './icons/taskmanager.png'
import calculator from './icons/calculator.png'
import controlpanel from './icons/controlpanel.png'

// 仅浏览器预览：Windows 图标来自已有缓存，Chromium / VS Code 来自各自源码仓库；
// 品牌标记来自 Simple Icons，以品牌底色装入预览图标。桌面仍由 Shell 读取原始图标。
// 来源：github.com/chromium/chromium（chrome/app/theme/chromium/product_logo_48.png）
// github.com/microsoft/vscode-docs（images/logo-stable.png）及 github.com/simple-icons/simple-icons。
function brandIcon(mark: string, color: string): string {
  const body = mark.replace(/<svg[^>]*>/, '').replace('</svg>', '')
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48"><rect width="48" height="48" rx="12" fill="${color}"/><g transform="translate(10 10) scale(1.1667)" fill="white">${body}</g></svg>`
  return `data:image/svg+xml,${encodeURIComponent(svg)}`
}

const samples = [
  ['chromium', 'Chromium', 'c-common', chromium],
  ['explorer', '文件资源管理器', 'c-common', explorer],
  ['wechat', '微信', 'c-common', brandIcon(wechatMark, '#10c45b')],
  ['terminal', '终端', 'c-dev', terminal],
  ['vscode', 'VS Code', 'c-dev', vscode],
  ['notepad', '记事本', 'c-work', notepad],
  ['github', 'GitHub', 'c-dev', brandIcon(githubMark, '#24292f')],
  ['docker', 'Docker', 'c-dev', brandIcon(dockerMark, '#168cf2')],
  ['powershell', 'PowerShell', 'c-dev', powershell],
  ['taskmanager', '任务管理器', 'c-common', taskmanager],
  ['calculator', '计算器', 'c-work', calculator],
  ['controlpanel', '控制面板', 'c-common', controlpanel]
] as const

export const previewApps: HubApp[] = samples.map(([id, name, categoryId]) => ({
  id: `preview-${id}`, name, categoryId, target: `preview:${id}`,
  color: '#59667a', initials: name.slice(0, 1), hidden: false
}))
export const previewIcons = Object.fromEntries(samples.map(([id, , , icon]) => [`preview:${id}`, icon]))
