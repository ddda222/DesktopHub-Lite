import type {
  Category,
  ClipItem,
  HubApp,
  HubSettings,
  TranslationProvider,
  TranslationSettings
} from '@/lib/desktop/types'

/** 合成图标：样稿不使用真实应用图标，避免依赖本机程序与文件路径。 */
function svgThumb(from: string, to: string, label: string): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="480" height="300" viewBox="0 0 480 300">
    <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="${from}"/><stop offset="1" stop-color="${to}"/>
    </linearGradient></defs>
    <rect width="480" height="300" fill="url(#g)"/>
    <circle cx="120" cy="96" r="46" fill="rgba(255,255,255,0.22)"/>
    <rect x="200" y="70" width="200" height="16" rx="8" fill="rgba(255,255,255,0.34)"/>
    <rect x="200" y="104" width="140" height="12" rx="6" fill="rgba(255,255,255,0.22)"/>
    <text x="32" y="250" fill="rgba(255,255,255,0.9)" font-family="Segoe UI, sans-serif" font-size="28">${label}</text>
  </svg>`
  return `data:image/svg+xml;utf8,${encodeURIComponent(svg)}`
}

export const categories: Category[] = [
  { id: 'all', name: '全部' },
  { id: 'c-common', name: '常用' },
  { id: 'c-dev', name: '开发' },
  { id: 'c-media', name: '影音' },
  { id: 'c-work', name: '办公' }
]

export const apps: HubApp[] = [
  { id: 'a-chrome', name: 'Chrome', target: 'C:\\Program Files\\Google\\Chrome\\chrome.exe', categoryId: 'c-common', color: '#3C8DFF', initials: 'C', hidden: false },
  { id: 'a-explorer', name: '文件资源管理器', target: 'C:\\Windows\\explorer.exe', categoryId: 'c-common', color: '#E0A33E', initials: '文', hidden: false },
  { id: 'a-wechat', name: '微信', target: 'C:\\Program Files\\Tencent\\WeChat\\WeChat.exe', categoryId: 'c-common', color: '#22C55E', initials: '微', hidden: false },
  { id: 'a-terminal', name: '终端', target: 'C:\\Windows\\wt.exe', categoryId: 'c-common', color: '#33383F', initials: '>_', hidden: false },
  { id: 'a-vscode', name: 'VS Code', target: 'C:\\Users\\Public\\Code.exe', categoryId: 'c-dev', color: '#3B82F6', initials: 'VS', hidden: false },
  { id: 'a-notepad', name: '记事本', target: 'C:\\Windows\\notepad.exe', categoryId: 'c-common', color: '#8B93A1', initials: '记', hidden: false },
  { id: 'a-vs', name: 'Visual Studio', target: 'C:\\Program Files\\Microsoft Visual Studio\\devenv.exe', categoryId: 'c-dev', color: '#8B5CF6', initials: 'VS', hidden: false },
  { id: 'a-docker', name: 'Docker Desktop', target: 'C:\\Program Files\\Docker\\Docker Desktop.exe', categoryId: 'c-dev', color: '#2E90C8', initials: 'D', hidden: false },
  { id: 'a-postman', name: 'Postman', target: 'C:\\Users\\Public\\Postman.exe', categoryId: 'c-dev', color: '#F0673B', initials: 'P', hidden: false },
  { id: 'a-github', name: 'GitHub Desktop', target: 'C:\\Users\\Public\\GitHubDesktop.exe', categoryId: 'c-dev', color: '#57606A', initials: 'GH', hidden: false },
  { id: 'a-dbeaver', name: 'DBeaver', target: 'C:\\Program Files\\DBeaver\\dbeaver.exe', categoryId: 'c-dev', color: '#A2543B', initials: 'DB', hidden: false },
  { id: 'a-winscp', name: 'WinSCP', target: 'C:\\Program Files\\WinSCP\\WinSCP.exe', categoryId: 'c-dev', color: '#3A7D44', initials: 'SC', hidden: false },
  { id: 'a-netease', name: '网易云音乐', target: 'C:\\Program Files\\Netease\\CloudMusic\\cloudmusic.exe', categoryId: 'c-media', color: '#E23B3B', initials: '网', hidden: false },
  { id: 'a-potplayer', name: 'PotPlayer', target: 'C:\\Program Files\\PotPlayer\\PotPlayerMini64.exe', categoryId: 'c-media', color: '#5B6AD0', initials: 'P', hidden: false },
  { id: 'a-bilibili', name: '哔哩哔哩', target: 'https://www.bilibili.com', categoryId: 'c-media', color: '#E879A6', initials: 'B', hidden: false },
  { id: 'a-obs', name: 'OBS Studio', target: 'C:\\Program Files\\obs-studio\\bin\\64bit\\obs64.exe', categoryId: 'c-media', color: '#4A4F58', initials: 'O', hidden: false },
  { id: 'a-word', name: 'Word', target: 'C:\\Program Files\\Microsoft Office\\WINWORD.EXE', categoryId: 'c-work', color: '#2B579A', initials: 'W', hidden: false },
  { id: 'a-excel', name: 'Excel', target: 'C:\\Program Files\\Microsoft Office\\EXCEL.EXE', categoryId: 'c-work', color: '#217346', initials: 'X', hidden: false },
  { id: 'a-ppt', name: 'PowerPoint', target: 'C:\\Program Files\\Microsoft Office\\POWERPNT.EXE', categoryId: 'c-work', color: '#C43E1C', initials: 'P', hidden: false },
  { id: 'a-obsidian', name: 'Obsidian', target: 'C:\\Users\\Public\\Obsidian.exe', categoryId: 'c-work', color: '#6D5AE0', initials: 'O', hidden: false },
  { id: 'a-feishu', name: '飞书', target: 'C:\\Program Files\\Feishu\\Feishu.exe', categoryId: 'c-work', color: '#3D7BFF', initials: '飞', hidden: false },
  { id: 'a-dingtalk', name: '钉钉', target: 'C:\\Program Files\\DingDing\\DingTalk.exe', categoryId: 'c-work', color: '#2F80ED', initials: '钉', hidden: true }
]

const now = Date.now()
const minute = 60_000

export const clips: ClipItem[] = [
  {
    id: 'clip-01',
    kind: 'text',
    text: '重构说明确认点：图标条常驻、剪贴板弹层展开、单击记录只复制到系统剪贴板，不再注入 Ctrl+V。',
    group: '工作',
    copiedAt: now - minute * 2
  },
  {
    id: 'clip-02',
    kind: 'text',
    text: `export class MockBridge implements HostBridge {
  async invoke(action, payload, requestId) {
    await sleep(this.latencyMs)
    return { requestId, ok: true }
  }
}`,
    group: '代码',
    copiedAt: now - minute * 6
  },
  {
    id: 'clip-03',
    kind: 'image',
    imageUrl: svgThumb('#3C6FD6', '#121A2E', '设计截图'),
    width: 480,
    height: 300,
    group: '图片',
    copiedAt: now - minute * 11
  },
  {
    id: 'clip-04',
    kind: 'text',
    text: 'https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/performance',
    group: '未分组',
    copiedAt: now - minute * 24
  },
  {
    id: 'clip-05',
    kind: 'text',
    text: 'feat: 同步当前主题、灵动岛与设置界面代码及配套文档',
    group: '工作',
    copiedAt: now - minute * 38
  },
  {
    id: 'clip-06',
    kind: 'image',
    imageUrl: svgThumb('#7A4FD6', '#221436', '剪贴板弹层'),
    width: 480,
    height: 300,
    group: '图片',
    copiedAt: now - minute * 52
  },
  {
    id: 'clip-07',
    kind: 'text',
    text: '第一，先确认界面与交互；第二，再接真实系统功能；第三，最后迁移历史数据。每一步都要有可观察的结果，不能把写完说明当成已经做完。',
    group: '工作',
    copiedAt: now - minute * 74
  },
  {
    id: 'clip-08',
    kind: 'text',
    text: 'dotnet build DesktopHub.slnx -c Release --no-restore',
    group: '代码',
    copiedAt: now - minute * 96
  },
  {
    id: 'clip-09',
    kind: 'image',
    imageUrl: '',
    imageMissing: true,
    group: '图片',
    copiedAt: now - minute * 130
  },
  {
    id: 'clip-10',
    kind: 'text',
    text: '键盘交互：方向键定位、Enter 复制、Esc 关闭；中文输入法候选确认期间 Enter 不能触发复制。',
    group: '未分组',
    copiedAt: now - minute * 180
  },
  {
    id: 'clip-11',
    kind: 'text',
    text: '会议结论：分类保留、分组保留，管理入口收进设置；日常切换入口等样稿确认。',
    group: '工作',
    copiedAt: now - minute * 260
  },
  {
    id: 'clip-12',
    kind: 'text',
    text: 'const clamp = (v: number, min: number, max: number) => Math.min(Math.max(v, min), max)',
    group: '代码',
    copiedAt: now - minute * 320
  }
]

export const groups = ['未分组', '工作', '代码', '图片']

export const defaultSettings: HubSettings = {
  theme: 'dark',
  recordClipboard: true,
  launchAtLogin: false,
  clipboardHotkey: 'Ctrl + Alt + V'
}

/** 预设服务商（样稿只做展示，真实地址与模型名在编码阶段按官方文档核对） */
export const translationProviders: TranslationProvider[] = [
  { id: 'deepseek', name: 'DeepSeek', baseUrl: 'https://api.deepseek.com/v1', model: 'deepseek-chat' },
  { id: 'ark', name: '火山方舟', baseUrl: 'https://ark.cn-beijing.volces.com/api/v3', model: '（按控制台接入点填写）' },
  { id: 'qwen', name: '通义千问', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', model: 'qwen-plus' },
  { id: 'kimi', name: 'Kimi', baseUrl: 'https://api.moonshot.cn/v1', model: 'moonshot-v1-8k' },
  { id: 'siliconflow', name: '硅基流动', baseUrl: 'https://api.siliconflow.cn/v1', model: 'Qwen/Qwen2.5-7B-Instruct' },
  { id: 'openai', name: 'OpenAI', baseUrl: 'https://api.openai.com/v1', model: 'gpt-4o-mini' },
  { id: 'custom', name: '自定义兼容接口', baseUrl: 'http://127.0.0.1:11434/v1', model: '本机模型' }
]

export const defaultTranslationSettings: TranslationSettings = {
  providerId: 'deepseek',
  model: 'deepseek-chat',
  baseUrl: 'https://api.deepseek.com/v1',
  direction: 'auto',
  hasSecret: true
}

