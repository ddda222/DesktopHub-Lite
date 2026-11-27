/** 主题清单的唯一来源：设置项、配置恢复、预览页与图片窗都从这里读取，避免各处列表漏改。 */
export const THEME_MODES = [
  'system', 'light', 'dark', 'midnight', 'mecha', 'pixel', 'blueprint', 'neon'
] as const

export type ThemeMode = typeof THEME_MODES[number]

export const THEME_LABELS: Record<ThemeMode, string> = {
  system: '跟随系统',
  light: '浅色',
  dark: '深色',
  midnight: '星舰仪表',
  mecha: '琥珀重工',
  pixel: '像素掌机',
  blueprint: '蓝晒图纸',
  neon: '霓虹夜行'
}

/** 深色底的具体主题；「跟随系统」由调用方按系统偏好判断。 */
const DARK_THEMES: ReadonlySet<ThemeMode> = new Set(['dark', 'midnight', 'mecha', 'pixel', 'blueprint', 'neon'])

export function isThemeMode(value: string): value is ThemeMode {
  return (THEME_MODES as readonly string[]).includes(value)
}

/** 将历史已移除主题平滑映射到相近的明/暗主题，确保旧配置与旧备份无痛降级。 */
export function normalizeThemeMode(value: string | null | undefined): ThemeMode {
  if (value === 'liquid') return 'dark'
  if (value === 'scroll' || value === 'clay') return 'light'
  if (value && isThemeMode(value)) return value
  return 'dark'
}

export function isDarkThemeMode(theme: ThemeMode): boolean {
  return DARK_THEMES.has(theme)
}
