//! DesktopHub 自己的启动项、全屏避让与版本检查。
use crate::{broadcast_ui, state::UiPreferences, windows, AppState, UI_PREFS_FILE};
use ::windows::core::{w, HSTRING, PCWSTR};
use ::windows::Win32::{
    Foundation::RECT,
    Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL},
    System::Registry::*,
    UI::WindowsAndMessaging::{
        GetForegroundWindow, GetShellWindow, GetWindowRect, IsIconic, IsWindowVisible,
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
    pub avoid_fullscreen: bool,
    pub auto_check_updates: bool,
}
impl Default for IslandAppearance {
    fn default() -> Self {
        Self {
            scale: 100,
            opacity: 100,
            tint: String::new(),
            preset: "glass".into(),
            avoid_fullscreen: true,
            auto_check_updates: false,
        }
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
    if !(100..=2000).contains(&hover_ms) || !(100..=4000).contains(&leave_ms) {
        return Err("展开延时为 100–2000 毫秒，收起延时为 100–4000 毫秒".into());
    }
    let valid_tint = appearance.tint.is_empty()
        || (appearance.tint.len() == 7
            && appearance.tint.starts_with('#')
            && appearance.tint[1..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()));
    if !(75..=125).contains(&appearance.scale)
        || !(75..=100).contains(&appearance.opacity)
        || !valid_tint
        || !matches!(appearance.preset.as_str(), "classic" | "glass")
        || !matches!(anchor.as_str(), "left" | "center" | "right")
    {
        return Err("无效的外观参数".into());
    }
    let state = app.state::<AppState>();
    let mut next = state.ui.snapshot();
    next.island_appearance = appearance.clone();
    next.island_anchor = anchor.clone();
    next.island_hover_ms = hover_ms;
    next.island_leave_ms = leave_ms;
    next.island_reduced_motion = reduced_motion;
    state
        .store
        .write(UI_PREFS_FILE, &UiPreferences::from_state(&next))?;
    state.ui.update(|ui| {
        ui.island_appearance = appearance;
        ui.island_anchor = anchor;
        ui.island_hover_ms = hover_ms;
        ui.island_leave_ms = leave_ms;
        ui.island_reduced_motion = reduced_motion;
    });
    windows::place_bar(&app, &state.windows);
    broadcast_ui(&app);
    Ok(())
}

fn foreground_is_fullscreen() -> bool {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.0.is_null()
            || foreground == GetShellWindow()
            || !IsWindowVisible(foreground).as_bool()
            || IsIconic(foreground).as_bool()
        {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(foreground, &mut rect).is_err() {
            return false;
        }
        let monitor = MonitorFromWindow(foreground, MONITOR_DEFAULTTONULL);
        if monitor.0.is_null() {
            return false;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }
        let screen = info.rcMonitor;
        rect.left <= screen.left + 2
            && rect.top <= screen.top + 2
            && rect.right >= screen.right - 2
            && rect.bottom >= screen.bottom - 2
    }
}

pub fn start_fullscreen_watch(app: AppHandle) {
    std::thread::spawn(move || {
        let mut suppressed = false;
        while app.state::<AppState>().running.load(Ordering::Relaxed) {
            let next = app
                .state::<AppState>()
                .ui
                .snapshot()
                .island_appearance
                .avoid_fullscreen
                && foreground_is_fullscreen();
            if next != suppressed {
                suppressed = next;
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || {
                    let state = handle.state::<AppState>();
                    state.ui.update(|ui| ui.fullscreen_suppressed = next);
                    if let Some(bar) = handle.get_webview_window(windows::BAR) {
                        let _ = bar.set_ignore_cursor_events(next);
                    }
                    broadcast_ui(&handle);
                });
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("DesktopHubLite");

fn current_command() -> Result<String, String> {
    let path = std::env::current_exe().map_err(|_| "未找到当前程序")?;
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatus {
    pub enabled: bool,
    pub conflict: bool,
}

#[tauri::command]
pub fn startup_status() -> Result<StartupStatus, String> {
    let expected = current_command()?;
    let actual = read_startup_command()?;
    Ok(StartupStatus {
        enabled: actual.as_ref() == Some(&expected),
        conflict: actual.is_some_and(|value| value != expected),
    })
}

#[tauri::command]
pub fn set_startup_enabled(enabled: bool) -> Result<StartupStatus, String> {
    let status = startup_status()?;
    if status.conflict {
        return Err("已存在其他路径的 DesktopHub Lite 自启项；未覆盖或删除".into());
    }
    if enabled == status.enabled {
        return Ok(status);
    }

    let command = HSTRING::from(current_command()?);
    let bytes = (command.len() as u32 + 1) * 2;
    let result = unsafe {
        if enabled {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                bytes,
            )
        } else {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE)
        }
    };
    if result.0 != 0 {
        return Err(format!("无法修改自启项（Windows 错误 {}）", result.0));
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
