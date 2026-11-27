//! DesktopHub 自己的启动项、全屏避让与版本检查。
use crate::{broadcast_ui, state::UiPreferences, windows, AppState, UI_PREFS_FILE};
use ::windows::core::{w, HSTRING, PCWSTR};
use ::windows::Win32::{
    Foundation::RECT,
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL},
    System::Registry::*,
    UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetShellWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed,
    },
};
use serde::{Deserialize, Serialize};
use std::{sync::atomic::Ordering, time::Duration};
use tauri::{AppHandle, Manager, State};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct IslandAppearance {
    pub scale: u32,
    pub opacity: u32,
    pub tint: String,
    pub preset: String,
    pub motion: String,
    pub avoid_fullscreen: bool,
    pub auto_check_updates: bool,
    pub webgl: bool,
    pub hide_from_capture: bool,
    pub telemetry_mode: String,
    pub telemetry_net: bool,
    pub telemetry_cpu: bool,
    pub telemetry_memory: bool,
    pub show_app_names: bool,
}
impl Default for IslandAppearance {
    fn default() -> Self {
        Self {
            scale: 100,
            opacity: 100,
            tint: String::new(),
            preset: "classic".into(),
            motion: "glide".into(),
            avoid_fullscreen: true,
            auto_check_updates: false,
            webgl: true,
            hide_from_capture: false,
            telemetry_mode: "smart".into(),
            telemetry_net: true,
            telemetry_cpu: true,
            telemetry_memory: true,
            show_app_names: true,
        }
    }
}

impl IslandAppearance {
    // 保留旧配置字段以兼容便携数据，显示统一采用主题实底。
    pub fn solid(mut self) -> Self {
        self.preset = "classic".into();
        self.opacity = 100;
        self.tint.clear();
        self
    }

    pub fn valid_telemetry_mode(mode: &str) -> bool {
        matches!(mode, "off" | "smart" | "always")
    }

    pub fn valid_motion_mode(mode: &str) -> bool {
        matches!(mode, "glide" | "sweep" | "bounce" | "focus" | "flicker" | "step" | "unroll")
    }

    pub fn has_any_telemetry_metric(&self) -> bool {
        self.telemetry_net || self.telemetry_cpu || self.telemetry_memory
    }

    pub fn normalized_on_load(mut self) -> Self {
        self = self.solid();
        if !Self::valid_motion_mode(&self.motion) {
            self.motion = "glide".into();
        }
        if !Self::valid_telemetry_mode(&self.telemetry_mode) {
            self.telemetry_mode = "smart".into();
        }
        if self.telemetry_mode != "off" && !self.has_any_telemetry_metric() {
            self.telemetry_mode = "off".into();
        }
        self
    }
}

#[tauri::command]
pub fn save_island_appearance(
    app: AppHandle,
    appearance: IslandAppearance,
    anchor: String,
    hover_ms: u32,
    leave_ms: u32,
    reduced_motion: bool,
) -> Result<(), String> {
    let appearance = appearance.solid();
    if !(100..=2000).contains(&hover_ms) || !(100..=4000).contains(&leave_ms) {
        return Err("展开延时为 100–2000 毫秒，收起延时为 100–4000 毫秒".into());
    }
    if !(75..=125).contains(&appearance.scale)
        || !matches!(anchor.as_str(), "left" | "center" | "right")
    {
        return Err("无效的外观参数".into());
    }
    if !IslandAppearance::valid_motion_mode(&appearance.motion) {
        return Err("无效的动态效果模式".into());
    }
    if !IslandAppearance::valid_telemetry_mode(&appearance.telemetry_mode) {
        return Err("无效的胶囊微读数模式".into());
    }
    if appearance.telemetry_mode != "off" && !appearance.has_any_telemetry_metric() {
        return Err("开启胶囊微读数时，请至少勾选一项系统指标".into());
    }
    let state = app.state::<AppState>();
    let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
    let previous = state.ui.snapshot();
    let mut next = previous.clone();
    let capture_changed = appearance.hide_from_capture != previous.island_appearance.hide_from_capture
        || appearance.hide_from_capture || previous.island_capture_error.is_some();
    if capture_changed {
        if let Err(error) = crate::capture_privacy::apply(&app, appearance.hide_from_capture) {
            crate::capture_privacy::hold_on_error(&app, error.clone());
            return Err(error);
        }
    }
    let anchor_changed = next.island_anchor != anchor;
    next.island_appearance = appearance.clone();
    next.island_anchor = anchor.clone();
    next.island_hover_ms = hover_ms;
    next.island_leave_ms = leave_ms;
    next.island_reduced_motion = reduced_motion;
    if let Err(error) = state.store.write(UI_PREFS_FILE, &UiPreferences::from_state(&next)) {
        if capture_changed {
            if let Err(rollback) = crate::capture_privacy::apply(&app, previous.island_appearance.hide_from_capture) {
                let detail = format!("保存设置失败：{error}；恢复原隐藏状态失败：{rollback}");
                crate::capture_privacy::hold_on_error(&app, detail.clone());
                return Err(detail);
            }
        }
        return Err(error);
    }
    state.ui.update(|ui| {
        ui.island_appearance = appearance;
        ui.island_anchor = anchor;
        ui.island_hover_ms = hover_ms;
        ui.island_leave_ms = leave_ms;
        ui.island_reduced_motion = reduced_motion;
        ui.island_capture_error = None;
    });
    if anchor_changed { state.windows.set_bar_free_position(false); }
    windows::place_bar(&app, &state.windows);
    if previous.island_capture_error.is_some() {
        crate::reveal_bar(&app);
    }
    broadcast_ui(&app);
    Ok(())
}

/// 桌面保持胶囊；普通最大化窗口和覆盖显示器的全屏窗口都让灵动岛退为细线。
fn foreground_occupies_screen() -> Option<bool> {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.0.is_null()
            || foreground == GetShellWindow()
            || !IsWindowVisible(foreground).as_bool()
            || IsIconic(foreground).as_bool()
        {
            return Some(false);
        }
        // 在全屏背景上点开自己的面板时，沿用背景状态，避免每 500ms 误收起。
        let mut foreground_pid = 0;
        GetWindowThreadProcessId(foreground, Some(&mut foreground_pid));
        if foreground_pid == std::process::id() { return None; }
        // 桌面可能由 WorkerW 承载，不能仅凭它覆盖屏幕就按游戏全屏处理。
        let mut class_name = [0u16; 256];
        let length = GetClassNameW(foreground, &mut class_name);
        if length > 0 && matches!(String::from_utf16_lossy(&class_name[..length as usize]).as_str(), "Progman" | "WorkerW") {
            return Some(false);
        }
        if IsZoomed(foreground).as_bool() {
            return Some(true);
        }
        let mut rect = RECT::default();
        if GetWindowRect(foreground, &mut rect).is_err() {
            return Some(false);
        }
        let monitor = MonitorFromWindow(foreground, MONITOR_DEFAULTTONULL);
        if monitor.0.is_null() {
            return Some(false);
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return Some(false);
        }
        let screen = info.rcMonitor;
        Some(rect.left <= screen.left + 2
            && rect.top <= screen.top + 2
            && rect.right >= screen.right - 2
            && rect.bottom >= screen.bottom - 2)
    }
}

pub fn start_fullscreen_watch(app: AppHandle) {
    std::thread::spawn(move || {
        let mut suppressed = false;
        while app.state::<AppState>().running.load(Ordering::Relaxed) {
            let avoid = app
                .state::<AppState>()
                .ui
                .snapshot()
                .island_appearance
                .avoid_fullscreen;
            let next = avoid && foreground_occupies_screen().unwrap_or(suppressed);
            if next != suppressed {
                suppressed = next;
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let state = handle.state::<AppState>();
                    state.ui.update(|ui| {
                        ui.fullscreen_suppressed = next;
                        let reading = ui.island_page == crate::state::PANEL_TRANSLATION ||
                            (ui.island_page == crate::state::PANEL_CLIPBOARD && state.island_reading_explanation.load(Ordering::Relaxed));
                        if !reading { ui.island_expanded = false; }
                    });
                    if let Some(bar) = handle.get_webview_window(windows::BAR) {
                        // 细线保留小范围鼠标热区，悬停展开不设置焦点。
                        if let Err(error) = bar.set_ignore_cursor_events(false) {
                            crate::log_warn!("灵动岛鼠标避让更新失败：{error}");
                        }
                    }
                    broadcast_ui(&handle);
                });
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APPROVED_RUN_KEY: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
const RUN_VALUE: PCWSTR = w!("DesktopHubLite");
const APPROVED_ENABLED_BYTES: [u8; 12] = [0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

fn current_exe_path() -> Result<std::path::PathBuf, String> {
    std::env::current_exe().map_err(|_| "未找到当前程序".into())
}

fn current_command() -> Result<String, String> {
    let path = current_exe_path()?;
    let path = path.to_str().ok_or("程序路径不是有效 Unicode")?;
    Ok(format!("\"{path}\""))
}

fn read_startup_command() -> Result<Option<String>, String> {
    let mut data = [0u16; 2048];
    let mut bytes = std::mem::size_of_val(&data) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    match status.0 {
        0 => {
            let length = data
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(data.len());
            Ok(Some(String::from_utf16_lossy(&data[..length])))
        }
        2 | 3 => Ok(None),
        _ => Err(format!("无法读取自启项（Windows 错误 {}）", status.0)),
    }
}

fn read_startup_approved_bytes() -> Result<Option<Vec<u8>>, String> {
    let mut data = [0u8; 64];
    let mut bytes = data.len() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            APPROVED_RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_BINARY,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    match status.0 {
        0 => Ok(Some(data[..(bytes as usize).min(data.len())].to_vec())),
        2 | 3 => Ok(None),
        _ => Err(format!("无法读取系统启动审批状态（Windows 错误 {}）", status.0)),
    }
}

// === STARTUP_CORE_START ===
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatus {
    pub enabled: bool,
    pub conflict: bool,
    pub stale: bool,
    pub system_disabled: bool,
}

/// 解析注册表启动命令中的可执行文件路径（支持带引号与不带引号格式）。
pub fn parse_startup_exe_path(raw: &str) -> Option<std::path::PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let exe_path = if let Some(rest) = trimmed.strip_prefix('"') {
        rest.split('"').next()?
    } else {
        trimmed.split_whitespace().next()?
    };
    let clean = exe_path.trim();
    if clean.is_empty() {
        None
    } else {
        Some(std::path::PathBuf::from(clean))
    }
}

fn normalize_win_path_key(path: &std::path::Path) -> String {
    let raw = path.to_string_lossy();
    let stripped = raw.strip_prefix(r"\\?\").unwrap_or(&raw);
    stripped
        .chars()
        .map(|ch| if ch == '/' { '\\' } else { ch.to_ascii_lowercase() })
        .collect()
}

pub fn paths_match_win(a: &std::path::Path, b: &std::path::Path) -> bool {
    normalize_win_path_key(a) == normalize_win_path_key(b)
}

/// 判断 `Explorer\StartupApproved\Run` 二进制值是否表示被系统/任务管理器禁用（首字节最低位为 1，如 0x01/0x03）。
pub fn is_startup_approved_disabled(bytes: Option<&[u8]>) -> bool {
    bytes.and_then(|b| b.first()).is_some_and(|flag| flag & 1 != 0)
}

/// 综合评估当前程序路径、注册表 Run 命令、目标文件存活状态与系统审批状态。
pub fn evaluate_startup_status(
    current_exe: &std::path::Path,
    actual_command: Option<&str>,
    actual_target_exists: bool,
    approved_bytes: Option<&[u8]>,
) -> StartupStatus {
    let Some(raw) = actual_command else {
        return StartupStatus {
            enabled: false,
            conflict: false,
            stale: false,
            system_disabled: false,
        };
    };
    let system_disabled = is_startup_approved_disabled(approved_bytes);
    let Some(actual_exe) = parse_startup_exe_path(raw) else {
        return StartupStatus {
            enabled: false,
            conflict: false,
            stale: true,
            system_disabled,
        };
    };
    if paths_match_win(&actual_exe, current_exe) {
        return StartupStatus {
            enabled: !system_disabled,
            conflict: false,
            stale: false,
            system_disabled,
        };
    }
    if actual_target_exists {
        StartupStatus {
            enabled: false,
            conflict: true,
            stale: false,
            system_disabled: false,
        }
    } else {
        StartupStatus {
            enabled: false,
            conflict: false,
            stale: true,
            system_disabled,
        }
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn missing_run_entry_reports_disabled_without_conflict() {
        let cur = Path::new(r"D:\apply\DesktopHub-Lite\DesktopHub-Lite.exe");
        assert_eq!(
            evaluate_startup_status(cur, None, false, None),
            StartupStatus {
                enabled: false,
                conflict: false,
                stale: false,
                system_disabled: false,
            }
        );
    }

    #[test]
    fn matching_exe_respects_startup_approved_flag() {
        let cur = Path::new(r"D:\apply\DesktopHub-Lite\DesktopHub-Lite.exe");
        let cmd = Some(r#""d:\Apply\DesktopHub-Lite\DesktopHub-Lite.exe""#);
        // 未设置 StartupApproved 时视为正常启用
        assert_eq!(
            evaluate_startup_status(cur, cmd, true, None),
            StartupStatus {
                enabled: true,
                conflict: false,
                stale: false,
                system_disabled: false,
            }
        );
        // 0x02 表示显式启用
        let enabled_bytes = [0x02u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            evaluate_startup_status(cur, cmd, true, Some(&enabled_bytes)),
            StartupStatus {
                enabled: true,
                conflict: false,
                stale: false,
                system_disabled: false,
            }
        );
        // 0x03 表示在任务管理器/系统设置中被禁用
        let disabled_bytes = [0x03u8, 0, 0, 0, 0x05, 0xF4, 0x08, 0x46, 0xBF, 0x7B, 0xDD, 0x01];
        assert_eq!(
            evaluate_startup_status(cur, cmd, true, Some(&disabled_bytes)),
            StartupStatus {
                enabled: false,
                conflict: false,
                stale: false,
                system_disabled: true,
            }
        );
    }

    #[test]
    fn deleted_old_package_path_is_stale_and_recoverable_not_conflict() {
        let cur = Path::new(r"D:\apply\DesktopHub-Lite\DesktopHub-Lite.exe");
        let old_deleted = Some(
            r#""D:\codex\DesktopHub-work\packages\DesktopHub-Lite-0.1.3-win-x64-20261121-101518\DesktopHub-Lite.exe""#,
        );
        let disabled_bytes = [0x03u8, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8];
        assert_eq!(
            evaluate_startup_status(cur, old_deleted, false, Some(&disabled_bytes)),
            StartupStatus {
                enabled: false,
                conflict: false,
                stale: true,
                system_disabled: true,
            }
        );
    }

    #[test]
    fn existing_other_path_keeps_conflict_protection() {
        let cur = Path::new(r"D:\apply\DesktopHub-Lite\DesktopHub-Lite.exe");
        let other_live = Some(r#""D:\other\DesktopHub-Lite\DesktopHub-Lite.exe""#);
        assert_eq!(
            evaluate_startup_status(cur, other_live, true, None),
            StartupStatus {
                enabled: false,
                conflict: true,
                stale: false,
                system_disabled: false,
            }
        );
    }
}
// === STARTUP_CORE_END ===

/// 只读解析当前 Windows 登录启动项指向的旧版程序所在目录，供数据目录自动发现使用。
pub fn startup_exe_dir() -> Option<std::path::PathBuf> {
    let raw = read_startup_command().ok()??;
    parse_startup_exe_path(&raw)?.parent().map(|p| p.to_path_buf())
}

#[tauri::command]
pub fn startup_status() -> Result<StartupStatus, String> {
    let current_exe = current_exe_path()?;
    let actual = read_startup_command()?;
    let target_exists = actual
        .as_deref()
        .and_then(parse_startup_exe_path)
        .is_some_and(|p| p.is_file());
    let approved = read_startup_approved_bytes()?;
    Ok(evaluate_startup_status(
        &current_exe,
        actual.as_deref(),
        target_exists,
        approved.as_deref(),
    ))
}

#[tauri::command]
pub fn set_startup_enabled(enabled: bool) -> Result<StartupStatus, String> {
    let status = startup_status()?;
    if status.conflict {
        return Err("已存在其他有效路径的 DesktopHub Lite 自启项；未覆盖或删除".into());
    }
    if enabled && status.enabled {
        return Ok(status);
    }
    if !enabled && !status.enabled && !status.stale && !status.system_disabled {
        return Ok(status);
    }

    if enabled {
        let command = HSTRING::from(current_command()?);
        let bytes = (command.len() as u32 + 1) * 2;
        let run_res = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                bytes,
            )
        };
        if run_res.0 != 0 {
            return Err(format!("无法写入自启项（Windows 错误 {}）", run_res.0));
        }
        // 同步将 StartupApproved\Run 置为启用态，解除任务管理器/系统设置残留的 0x03 禁用拦截。
        let approve_res = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                APPROVED_RUN_KEY,
                RUN_VALUE,
                REG_BINARY.0,
                Some(APPROVED_ENABLED_BYTES.as_ptr().cast()),
                APPROVED_ENABLED_BYTES.len() as u32,
            )
        };
        if approve_res.0 != 0 {
            return Err(format!(
                "无法恢复系统启动审批启用状态（Windows 错误 {}）",
                approve_res.0
            ));
        }
    } else {
        let del_run = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE) };
        if !matches!(del_run.0, 0 | 2 | 3) {
            return Err(format!("无法移除自启项（Windows 错误 {}）", del_run.0));
        }
        let del_approved =
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, APPROVED_RUN_KEY, RUN_VALUE) };
        if !matches!(del_approved.0, 0 | 2 | 3) {
            crate::log_warn!(
                "清理 StartupApproved 启动审批记录失败（Windows 错误 {}）",
                del_approved.0
            );
        }
    }
    startup_status()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    state: &'static str,
    current: &'static str,
    version: Option<String>,
    notes: Option<String>,
    details: String,
}
const CURRENT: &str = env!("CARGO_PKG_VERSION");
// Lite 单独认 lite-v*，决不把原版 WPF v1.0.0 当成可安装更新。
const RELEASES: &str = "https://api.github.com/repos/ddda222/DesktopHub/releases?per_page=30";
pub const RELEASE_PAGE: &str = "https://github.com/ddda222/DesktopHub/releases";

async fn check_update() -> UpdateStatus {
    let outcome = async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "无法创建更新请求".to_string())?;
        let response = client
            .get(RELEASES)
            .header("User-Agent", "DesktopHub-Lite")
            .send()
            .await
            .map_err(|_| "无法连接 GitHub，请检查网络后重试".to_string())?;
        if matches!(response.status().as_u16(), 401 | 403 | 404) {
            return Err(
                "当前仓库为私有仓库，应用没有更新读取权限；可在已登录的 GitHub 页面查看".into(),
            );
        }
        if !response.status().is_success() {
            return Err(format!(
                "GitHub 更新检查失败（HTTP {}）",
                response.status().as_u16()
            ));
        }
        let releases: Vec<serde_json::Value> = response
            .json()
            .await
            .map_err(|_| "发布数据无法解析".to_string())?;
        let current = parse_version(CURRENT).unwrap_or_default();
        let latest = releases
            .iter()
            .filter(|release| {
                release["draft"].as_bool() == Some(false)
                    && release["prerelease"].as_bool() == Some(false)
            })
            .filter_map(|release| {
                let version = release["tag_name"].as_str()?.strip_prefix("lite-v")?;
                Some((parse_version(version)?, version, release["body"].as_str()))
            })
            .max_by_key(|release| release.0);
        let Some((version, name, notes)) = latest else {
            return Ok(UpdateStatus {
                state: "unavailable",
                current: CURRENT,
                version: None,
                notes: None,
                details: "没有找到 DesktopHub Lite 正式发布；旧版发布未计入".into(),
            });
        };
        Ok(UpdateStatus {
            state: if version > current {
                "available"
            } else {
                "current"
            },
            current: CURRENT,
            version: Some(name.into()),
            notes: notes.map(|text| text.chars().take(4000).collect()),
            details: if version > current {
                "发现新版本。未配置签名更新清单，应用内安装不可用".into()
            } else {
                "当前已是最新正式版本".into()
            },
        })
    }
    .await;
    outcome.unwrap_or_else(|details| UpdateStatus {
        state: "error",
        current: CURRENT,
        version: None,
        notes: None,
        details,
    })
}

fn parse_version(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.');
    let result = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(result)
}

#[tauri::command]
pub async fn check_deskhub_updates(state: State<'_, AppState>) -> Result<UpdateStatus, String> {
    let _guard = state.update_check_gate.lock().await;
    let result = check_update().await;
    *state
        .update_status
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(result.clone());
    Ok(result)
}

#[tauri::command]
pub fn last_deskhub_update(state: State<'_, AppState>) -> Option<UpdateStatus> {
    state
        .update_status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[tauri::command]
pub fn open_deskhub_release_page() -> Result<(), String> {
    crate::launcher::launch(RELEASE_PAGE)
}
