/** 打包 / 联调台的数据模型，字段与 Rust 侧 pack_scripts / app_pack / adb 保持一致。 */

export type ScriptEnv = 'local' | 'remote'
export type ScriptKind = 'command' | 'pack'

export interface PackScript {
  id: string
  name: string
  env: ScriptEnv
  kind: ScriptKind
  command: string
  scriptPath: string
  workDir: string
  preCommand: string
  /** 预估时长（秒） */
  estimateSec: number
  /** 成功完成的耗时（秒），最近 5 次 */
  history: number[]
}

export const DEFAULT_REVERSE_PORT = 11034

export interface PackSettings {
  cli: string
  hbuilderRoot: string
  project: string
  packagename: string
  packType: string
  outputDir: string
  adb: string
  device: string
  reversePort: number
  autoInstall: boolean
  /** 远程联调打包时临时忽略本地更改（恢复为 Git HEAD）的文件列表 */
  remoteIgnoreFiles: string[]
}

export const DEFAULT_PACK_SETTINGS: PackSettings = {
  cli: '',
  hbuilderRoot: '',
  project: '',
  packagename: '',
  packType: '3',
  outputDir: '',
  adb: '',
  device: '',
  reversePort: DEFAULT_REVERSE_PORT,
  autoInstall: false,
  remoteIgnoreFiles: []
}

export interface InstallApkResult {
  device: string
  reversedPort: number | null
  adb: AdbStatus
}

export interface AdbDevice {
  serial: string
  model: string
}

export interface AdbStatus {
  adb: string
  devices: AdbDevice[]
  serial: string
  reverse: string[]
  error: string
}

export const EMPTY_SCRIPT: PackScript = {
  id: '',
  name: '',
  env: 'local',
  kind: 'command',
  command: '',
  scriptPath: '',
  workDir: '',
  preCommand: '',
  estimateSec: 0,
  history: []
}

/** 环境标签与强调色：正式远程必须一眼看出来。 */
export const ENV_LABEL: Record<ScriptEnv, string> = {
  local: '本地联调',
  remote: '正式远程'
}

export function kindLabel(kind: ScriptKind): string {
  return kind === 'pack' ? '云打包' : '短命令'
}

/** 估算时长：有历史取平均，否则用预估；都没有时返回 null（界面显示「首次运行」）。 */
export function estimateSeconds(script: PackScript): number | null {
  if (script.history.length) {
    const sum = script.history.reduce((total, value) => total + value, 0)
    return sum / script.history.length
  }
  return script.estimateSec > 0 ? script.estimateSec : null
}