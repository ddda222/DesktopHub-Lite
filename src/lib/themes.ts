/** 主题清单的唯一来源：设置项、配置恢复、预览页与图片窗都从这里读取，避免各处列表漏改。 */
export const THEME_MODES = [
  'system', 'light', 'dark', 'liquid', 'midnight', 'mecha', 'scroll', 'pixel', 'blueprint', 'neon', 'clay'
] as const

export type ThemeMode = typeof THEME_MODES[number]

export const THEME_LABELS: Record<ThemeMode, string> = {
  system: '跟随系统',
  light: '浅色',
  dark: '深色',
  liquid: '液态琉璃',
  midnight: '星舰仪表',
  mecha: '琥珀重工',
  scroll: '海棠书卷',
  pixel: '像素掌机',
  blueprint: '蓝晒图纸',
  neon: '霓虹夜行',
  clay: '软糖黏土'
}

/** 深色底的具体主题；「跟随系统」由调用方按系统偏好判断。 */
const DARK_THEMES: ReadonlySet<ThemeMode> = new Set(['dark', 'liquid', 'midnight', 'mecha', 'pixel', 'blueprint', 'neon'])

export function isThemeMode(value: string): value is ThemeMode {
  return (THEME_MODES as readonly string[]).includes(value)
}

export function isDarkThemeMode(theme: ThemeMode): boolean {
  return DARK_THEMES.has(theme)
}
