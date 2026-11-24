//! DesktopHub Lite 桌面外壳。
//!
//! 阶段 1 接通了「界面 ↔ Rust ↔ 隔离数据目录」的链路；阶段 2 在此之上接通：
//! - 应用配置的真实增删改（手动添加文件 / 文件夹、导入桌面快捷方式、编辑、删除、排序）；
//! - 真实启动（EXE / LNK / 目录 / 网址一律交给 `ShellExecuteW`，保留 LNK 自身语义）；
//! - 真实 Windows 图标（Shell 图标抽取 + 本地 PNG 缓存）。
//!
//! 剪贴板与翻译只登记契约，不读写真实数据、不发真实请求（阶段 3 / 4）。

mod applications;
mod approvals;
mod clipboard;
mod clipboard_service;
mod capture_privacy;
mod desktop_support;
mod desktop_features;
mod file_shelf;
mod icons;
mod image_preview;
mod audio;
mod config_format;
mod config_transfer;
mod data_location;
mod launcher;
mod logging;
mod mail;
mod ocr;
mod snippets;
mod state;
mod storage;
mod system_apps;
mod telemetry;
mod translation;
mod file_preview;
mod windows;
mod wechat;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, State, WebviewWindow, WindowEvent,
};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;

use applications::{AppConfig, HubApp, OpenResult};
use clipboard::ClipboardIndex;
use state::{
    UiState, UiStateHandle, CATEGORY_ALL, PANEL_CATEGORY, PANEL_CLIPBOARD, PANEL_OVERFLOW,
    PANEL_TRANSLATION, valid_theme,
};
use storage::Store;
use translation::TranslationSettings;
use windows::{WindowsHandle, WindowState};

pub const UI_STATE_EVENT: &str = "ui-state";
/// 应用配置变化事件：图标条、溢出网格、设置窗口各自持有一份镜像，靠它刷新。
pub const APPLICATIONS_EVENT: &str = "applications-changed";
/// 轻提示事件：toast 窗口收到后渲染并回报尺寸。
pub const TOAST_EVENT: &str = "toast";
/// 灵动岛主窗失焦通知：由前端结合交互保护态决定是否自动收起。
pub const ISLAND_WINDOW_BLUR_EVENT: &str = "island-window-blur";
/// 需要跨重启保留的界面偏好。
pub const UI_PREFS_FILE: &str = "ui-preferences.json";

/// 应用运行期共享状态。
pub struct AppState {
    island_reading_explanation: AtomicBool,
    store: Store,
    configuration_gate: Mutex<()>,
    apps: Mutex<AppConfig>,
    ui: UiStateHandle,
    windows: WindowsHandle,
    clipboard: Mutex<ClipboardIndex>,
    snippets: Mutex<snippets::SnippetIndex>,
    file_shelf: Mutex<file_shelf::ShelfIndex>,
    translation: Mutex<TranslationSettings>,
    running: AtomicBool,
    translation_cancel: Mutex<std::collections::HashMap<String, translation::PendingTranslation>>,
    shortcuts: Mutex<desktop_support::Shortcuts>,
    update_status: Mutex<Option<desktop_features::UpdateStatus>>,
    update_check_gate: tokio::sync::Mutex<()>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: &'static str,
    stage: &'static str,
    reads_real_clipboard: bool,
    launches_external_apps: bool,
    data_dir: String,
}

fn broadcast_ui(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let snapshot = state.ui.snapshot();
    let _ = app.emit(UI_STATE_EVENT, &snapshot);
}

/// 应用配置变化后广播给所有窗口：图标条、溢出网格、设置窗口各有一份镜像。
fn broadcast_apps(app: &AppHandle, config: &AppConfig) {
    let _ = app.emit(APPLICATIONS_EVENT, config);
}

pub(crate) fn hide_window(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        if window.hide().is_ok() && label == windows::BAR {
            approvals::visibility(app, false);
            let _ = window.emit_to(label, "island-visibility", false);
        }
    }
}

fn display_window(window: &WebviewWindow) -> tauri::Result<()> {
    let bar = window.label() == windows::BAR;
    #[cfg(windows)]
    let restoring = bar && !window.is_visible().unwrap_or(false);
    window.show()?;
    #[cfg(windows)]
    if restoring {
        // Tao 在显示 HWND 时可能重写窗口样式并清空 HRGN。显隐完成后重设
        // 原来的命中区，包括不获取焦点的显示路径，即使 Vue 尺寸没有变化。
        let app = window.app_handle().clone();
        window.run_on_main_thread(move || {
            if let Some(state) = app.try_state::<AppState>() {
                windows::restore_bar_region(&app, &state.windows);
            }
        })?;
    }
    if bar {
        approvals::visibility(window.app_handle(), true);
        let _ = window.emit_to(windows::BAR, "island-visibility", true);
    }
    Ok(())
}

pub(crate) fn reveal_bar(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(windows::BAR) {
        if let Err(error) = display_window(&window) {
            log_warn!("显示灵动岛失败：{error}");
        }
    }
}

pub(crate) fn show_window(app: &AppHandle, label: &str) {
    if label == windows::BAR && !capture_privacy::prepare_to_show(app) {
        let _ = open_settings(app.clone(), None);
        return;
    }
    if let Some(window) = app.get_webview_window(label) {
        if let Err(error) = display_window(&window) {
            log_warn!("显示窗口 {label} 失败：{error}");
            return;
        }
        let _ = window.set_focus();
    } else {
        log_warn!("找不到窗口 {label}");
    }
}

/// 保存成功后才发布内存状态；与配置恢复共用锁，避免旧快照覆盖刚恢复的数据。
fn update_ui_preferences(app: &AppHandle, change: impl FnOnce(&mut UiState)) -> Result<UiState, String> {
    let state = app.state::<AppState>();
    let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
    let mut current = state.ui.0.lock().map_err(|_| "界面状态暂时不可用")?;
    let mut next = current.clone();
    change(&mut next);
    state.store.write(UI_PREFS_FILE, &state::UiPreferences::from_state(&next))?;
    *current = next.clone();
    Ok(next)
}

// ---------------------------------------------------------------- 基础命令

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        stage: "clipboard-translation-portable",
        reads_real_clipboard: true,
        launches_external_apps: true,
        data_dir: state.store.root().to_string_lossy().into_owned(),
    }
}

#[tauri::command]
fn ui_state(state: State<'_, AppState>) -> UiState {
    state.ui.snapshot()
}

#[tauri::command]
fn set_island_reading_explanation(window: WebviewWindow, state: State<'_, AppState>, active: bool) {
    if window.label() == windows::BAR { state.island_reading_explanation.store(active, Ordering::Relaxed); }
}

#[tauri::command]
fn set_island_expanded(app: AppHandle, expanded: bool, focus: Option<bool>) -> UiState {
    let state = app.state::<AppState>();
    let next = state.ui.update(|ui| {
        ui.island_expanded = expanded;
        ui.panel = if expanded && ui.island_page != state::PANEL_APPLICATIONS {
            Some(ui.island_page.clone())
        } else { None };
    });
    if expanded {
        state.windows.mark_pop_shown();
        state.windows.mark_bar_expanded();
    } else {
        state.windows.clear_pop_shown();
        state.windows.clear_bar_expanded();
        translation::cancel_for_window(&app, windows::BAR);
    }
    if focus.unwrap_or(false) { show_window(&app, windows::BAR); }
    broadcast_ui(&app);
    next
}

#[tauri::command]
fn set_island_anchor(app: AppHandle, anchor: String) -> Result<UiState, String> {
    if !["left", "center", "right"].contains(&anchor.as_str()) { return Err("未知停靠位置".into()); }
    let state = app.state::<AppState>();
    let next = update_ui_preferences(&app, |ui| ui.island_anchor = anchor)?;
    state.windows.set_bar_free_position(false);
    windows::place_bar(&app, &state.windows);
    broadcast_ui(&app);
    Ok(next)
}

#[tauri::command]
fn select_island_page(app: AppHandle, page: String, focus: Option<bool>) -> Result<UiState, String> {
    if !state::valid_island_page(&page) { return Err("未知灵动岛页面".into()); }
    let state = app.state::<AppState>();
    let previous = state.ui.snapshot();
    let next = update_ui_preferences(&app, |ui| {
        ui.island_page = page.clone();
        ui.island_expanded = true;
        ui.panel = if page == state::PANEL_APPLICATIONS { None } else { Some(page.clone()) };
    })?;
    if previous.island_page == PANEL_TRANSLATION && page != PANEL_TRANSLATION {
        translation::cancel_for_window(&app, windows::BAR);
    }
    state.windows.mark_bar_expanded();
    if focus.unwrap_or(true) { show_window(&app, windows::BAR); }
    hide_window(&app, windows::POP);
    broadcast_ui(&app);
    Ok(next)
}

#[tauri::command]
fn current_window_label(window: WebviewWindow) -> String { window.label().to_string() }

#[tauri::command]
async fn open_translation_window(app: AppHandle, focus: Option<bool>) -> Result<(), String> {
    let window = windows::ensure_translation(&app)?;
    window.show().map_err(|_| "无法显示翻译窗口")?;
    if focus.unwrap_or(true) { window.set_focus().map_err(|_| "无法激活翻译窗口")?; }
    Ok(())
}

#[tauri::command]
fn close_translation_window(app: AppHandle) {
    translation::cancel_for_window(&app, windows::TRANSLATION);
    let _ = app.emit_to(windows::TRANSLATION, "translation-window-hidden", ());
    hide_window(&app, windows::TRANSLATION);
}

// ---------------------------------------------------------------- 窗口命令

#[tauri::command]
fn toggle_panel(app: AppHandle, panel: String) -> Result<UiState, String> {
    let allowed = [PANEL_CLIPBOARD, PANEL_TRANSLATION, PANEL_CATEGORY, PANEL_OVERFLOW];
    if !allowed.contains(&panel.as_str()) {
        return Err(format!("未知弹层：{panel}"));
    }
    if state::valid_island_page(&panel) {
        return select_island_page(app, panel, Some(true));
    }
    log_info!("toggle_panel {panel}");
    let state = app.state::<AppState>();
    // 点图标条上同一个按钮想收起时：焦点先离开弹层已触发自动收起，
    // 这次点击不该再把弹层重新打开。
    if state.windows.take_auto_closed(&panel) {
        log_info!("[焦点] 点击紧跟在自动收起之后，按收起处理");
        broadcast_ui(&app);
        return Ok(state.ui.snapshot());
    }
    let next = state.ui.update(|ui| {
        ui.panel = if ui.panel.as_deref() == Some(panel.as_str()) {
            None
        } else {
            Some(panel.clone())
        };
    });

    if next.panel.is_some() {
        state.ui.update(|ui| ui.island_expanded = true);
        state.windows.mark_pop_shown();
        show_window(&app, windows::BAR);
    }
    hide_window(&app, windows::POP);
    broadcast_ui(&app);
    Ok(next)
}

#[tauri::command]
fn close_panel(app: AppHandle) -> Result<UiState, String> {
    let next = update_ui_preferences(&app, |ui| {
        if matches!(ui.panel.as_deref(), Some(PANEL_TRANSLATION | PANEL_CLIPBOARD)) {
            ui.island_page = state::PANEL_APPLICATIONS.into();
        }
        ui.panel = None;
    })?;
    hide_window(&app, windows::POP);
    broadcast_ui(&app);
    Ok(next)
}

#[tauri::command]
fn open_settings(app: AppHandle, section: Option<String>) -> Result<UiState, String> {
    windows::ensure_settings(&app)?;
    let state = app.state::<AppState>();
    let next = state.ui.update(|ui| {
        ui.panel = None;
        ui.settings_open = true;
        let next_request = ui.settings_request.as_ref().map_or(1, |(id, _)| id + 1);
        ui.settings_request = Some((next_request, section.unwrap_or_else(|| "general".into())));
        ui.island_expanded = false;
    });
    hide_window(&app, windows::POP);
    show_window(&app, windows::SETTINGS);
    broadcast_ui(&app);
    Ok(next)
}

#[tauri::command]
fn close_settings(app: AppHandle) -> UiState {
    let state = app.state::<AppState>();
    let next = state.ui.update(|ui| ui.settings_open = false);
    hide_window(&app, windows::SETTINGS);
    broadcast_ui(&app);
    next
}

/// 界面量出真实尺寸后回调，让图标条窗口始终等于图标条本身。
#[tauri::command]
fn set_bar_size(app: AppHandle, width: f64, height: f64, regions: Option<Vec<windows::BarRegion>>) -> Result<(), String> {
    if !width.is_finite() || !height.is_finite() || !(140.0..=1600.0).contains(&width) || !(14.0..=1200.0).contains(&height) {
        return Err("灵动岛尺寸无效".into());
    }
    let regions = regions.unwrap_or_default();
    if regions.len() > 2 || regions.iter().any(|r| {
        ![r.x, r.y, r.width, r.height, r.radius].iter().all(|n| n.is_finite()) ||
        r.x < 0.0 || r.y < 0.0 || r.width <= 0.0 || r.height <= 0.0 ||
        r.x + r.width > width + 1.0 || r.y + r.height > height + 1.0 ||
        r.radius < 0.0 || r.radius > r.width.min(r.height) / 2.0
    }) { return Err("灵动岛可见区域无效".into()); }
    let state = app.state::<AppState>();
    state.windows.set_bar_size(width, height);
    state.windows.set_bar_regions(regions);
    windows::place_bar(&app, &state.windows);
    Ok(())
}

/// 弹层量出真实高度后回调，位置随之重算。
#[tauri::command]
fn set_pop_size(app: AppHandle, width: f64, height: f64) {
    let state = app.state::<AppState>();
    state.windows.set_pop_size(width, height);
    if let Some(window) = app.get_webview_window(windows::POP) {
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize::new(width, height)));
    }
    windows::place_pop(&app, &state.windows);
}

#[tauri::command]
fn start_bar_drag(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    let state = app.state::<AppState>();
    let is_bar = window.label() == windows::BAR;
    let was_free = if is_bar { state.windows.set_bar_free_position(true) } else { false };
    if let Err(error) = window.start_dragging() {
        if is_bar { state.windows.set_bar_free_position(was_free); }
        return Err(error.to_string());
    }
    Ok(())
}

/// 切换图标条分类。分类是跨窗口状态：弹层窗口也要跟着变。
#[tauri::command]
fn set_category(app: AppHandle, category_id: String) -> Result<UiState, String> {
    let state = app.state::<AppState>();
    let next = update_ui_preferences(&app, |ui| {
        let known = state.apps.lock().unwrap_or_else(|e| e.into_inner()).categories.iter().any(|c| c.id == category_id);
        if known || category_id == CATEGORY_ALL {
            ui.active_category_id = category_id.clone();
        }
    })?;
    broadcast_ui(&app);
    Ok(next)
}

/// 自检：直接按标签显示窗口并回报真实可见状态，用于无人工介入地验证窗口链路。
#[tauri::command]
fn selftest_windows(app: AppHandle) -> Vec<String> {
    let mut report = Vec::new();
    for label in [windows::BAR, windows::POP, windows::SETTINGS, windows::TOAST] {
        let Some(window) = app.get_webview_window(label) else {
            report.push(format!("{label}: 未创建"));
            continue;
        };
        let visible = window.is_visible().map(|v| v.to_string()).unwrap_or_else(|e| e.to_string());
        let size = window
            .outer_size()
            .map(|s| format!("{}x{}", s.width, s.height))
            .unwrap_or_else(|e| e.to_string());
        let pos = window
            .outer_position()
            .map(|p| format!("{},{}", p.x, p.y))
            .unwrap_or_else(|e| e.to_string());
        report.push(format!("{label}: visible={visible} size={size} pos={pos}"));
    }
    report
}

/// 切换主题。system 交给前端按系统偏好解析，Rust 只记录用户选择。
#[tauri::command]
fn set_theme(app: AppHandle, theme: String) -> Result<UiState, String> {
    if !valid_theme(&theme) {
        return Err(format!("未知主题：{theme}"));
    }
    let next = update_ui_preferences(&app, |ui| ui.theme = theme)?;
    broadcast_ui(&app);
    Ok(next)
}

/// 切换独立动态效果，即时生效并广播到所有窗口。
#[tauri::command]
fn set_motion(app: AppHandle, motion: String) -> Result<UiState, String> {
    if !desktop_features::IslandAppearance::valid_motion_mode(&motion) {
        return Err(format!("未知动态效果：{motion}"));
    }
    let next = update_ui_preferences(&app, |ui| ui.island_appearance.motion = motion)?;
    broadcast_ui(&app);
    Ok(next)
}

/// 弹一条轻提示。提示由独立的 toast 窗口渲染，避免被图标条窗口裁掉。
#[tauri::command]
fn show_toast(app: AppHandle, message: String, kind: Option<String>) -> Result<(), String> {
    if message.trim().is_empty() {
        return Err("提示内容为空".into());
    }
    let window = windows::ensure_toast(&app)?;
    let payload = serde_json::json!({
        "message": message,
        "kind": kind.unwrap_or_else(|| "info".to_string()),
    });
    window
        .emit(TOAST_EVENT, payload)
        .map_err(|error| format!("发送提示失败：{error}"))?;

    let state = app.state::<AppState>();
    // 先按上一次的尺寸摆放，界面量出真实尺寸后再修正一次。
    windows::place_toast(&app, &state.windows);
    let _ = window.show();
    Ok(())
}

/// 提示窗口量出真实尺寸后回调。
#[tauri::command]
fn set_toast_size(app: AppHandle, width: f64, height: f64) {
    let state = app.state::<AppState>();
    state.windows.set_toast_size(width, height);
    let Some(window) = app.get_webview_window(windows::TOAST) else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let (width, height) = state.windows.toast_size();
    let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(
        (width * scale).round() as u32,
        (height * scale).round() as u32,
    )));
    windows::place_toast(&app, &state.windows);
}

#[tauri::command]
fn hide_toast(app: AppHandle) {
    hide_window(&app, windows::TOAST);
}

// ---------------------------------------------------------------- 应用命令

#[tauri::command]
fn list_applications(state: State<'_, AppState>) -> AppConfig {
    state.apps.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// 真正启动外部目标。
///
/// 返回 `launched=true` 只表示「系统接受了启动请求」，不代表目标窗口已经出现；
/// 目标不存在、系统拒绝时返回 `launched=false` 并把原因原样交给界面。
#[tauri::command]
fn open_application(app: AppHandle, id: String) -> Result<OpenResult, String> {
    let state = app.state::<AppState>();
    let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
    let found = {
        let config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
        config.find(&id).cloned()
    };

    let Some(item) = found else {
        return Err("找不到这个应用".into());
    };

    if !launcher::exists(&item.target) {
        // 目标已经不在：把「失效」写进配置，图标条据此显示告警态。
        {
            let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
            if let Ok(snapshot) = state.store.update(applications::CONFIG_FILE, &mut *config, |next| {
                if let Some(existing) = next.apps.iter_mut().find(|a| a.id == item.id) { existing.broken = Some(true); }
            }) { drop(config); broadcast_apps(&app, &snapshot); }
        }
        return Ok(OpenResult {
            launched: false,
            reason: format!("目标不存在，无法启动 {}（{}）", item.name, item.target),
        });
    }

    match launcher::launch(&item.target) {
        Ok(()) => {
            // 启动成功后清掉可能残留的失效标记。
            {
                let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
                if config.apps.iter().any(|a| a.id == item.id && a.broken.is_some()) {
                    if let Ok(snapshot) = state.store.update(applications::CONFIG_FILE, &mut *config, |next| {
                        if let Some(existing) = next.apps.iter_mut().find(|a| a.id == item.id) { existing.broken = None; }
                    }) { drop(config); broadcast_apps(&app, &snapshot); }
                }
            }
            Ok(OpenResult {
                launched: true,
                reason: format!("已交给系统启动 {}（{}）", item.name, item.target),
            })
        }
        Err(error) => Ok(OpenResult {
            launched: false,
            reason: format!("启动 {} 失败：{error}", item.name),
        }),
    }
}

#[tauri::command]
fn save_application(app: AppHandle, application: HubApp) -> Result<AppConfig, String> {
    let state = app.state::<AppState>();
    let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
    let snapshot = {
        let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
        let mut incoming = application;
        if incoming.id.trim().is_empty() || incoming.name.trim().is_empty() || incoming.target.trim().is_empty() { return Err("应用名称和目标不能为空".into()); }
        if !config.categories.iter().any(|c| c.id == incoming.category_id) { incoming.category_id = CATEGORY_ALL.into(); }
        // 失效状态由目标是否存在决定，不接受界面瞎传。
        incoming.broken = if launcher::exists(&incoming.target) {
            None
        } else {
            Some(true)
        };
        state.store.update(applications::CONFIG_FILE, &mut *config, |next| match next.apps.iter_mut().find(|a| a.id == incoming.id) {
            Some(existing) => *existing = incoming,
            None => next.apps.push(incoming),
        })?
    };
    broadcast_apps(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn save_application_category(app: AppHandle, id: String, name: Option<String>) -> Result<AppConfig, String> {
    if id == "all" || id.trim().is_empty() { return Err("默认分类不能修改".into()); }
    let state = app.state::<AppState>();
    let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
    let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
    let mut next = config.clone();
    if let Some(name) = name {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 40 { return Err("分类名称需为 1–40 个字符".into()); }
        if let Some(category) = next.categories.iter_mut().find(|category| category.id == id) { category.name = name.into(); }
        else { next.categories.push(applications::Category { id, name: name.into() }); }
    } else {
        next.categories.retain(|category| category.id != id);
        for item in &mut next.apps { if item.category_id == id { item.category_id = "all".into(); } }
    }
    let previous = state.ui.snapshot();
    let reset_category = !next.categories.iter().any(|category| category.id == previous.active_category_id);
    if reset_category {
        let mut ui = previous.clone(); ui.active_category_id = CATEGORY_ALL.into();
        let backup = config_format::ConfigurationBackup::capture(&config, &previous);
        state.store.write_configuration(&next, &state::UiPreferences::from_state(&ui), &backup)?;
        state.ui.update(|ui| ui.active_category_id = CATEGORY_ALL.into());
    } else { state.store.write(applications::CONFIG_FILE, &next)?; }
    *config = next.clone(); drop(config); broadcast_apps(&app, &next);
    if reset_category { broadcast_ui(&app); }
    Ok(next)
}

#[tauri::command]
fn remove_application(app: AppHandle, id: String) -> Result<AppConfig, String> {
    let state = app.state::<AppState>();
    let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
    let snapshot = {
        let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
        state.store.update(applications::CONFIG_FILE, &mut *config, |next| next.apps.retain(|a| a.id != id))?
    };
    broadcast_apps(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn reorder_applications(app: AppHandle, ids: Vec<String>) -> Result<AppConfig, String> {
    let state = app.state::<AppState>();
    let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
    let snapshot = {
        let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
        let mut unique = std::collections::HashSet::new();
        if ids.iter().any(|id| !unique.insert(id) || !config.apps.iter().any(|a| &a.id == id)) { return Err("排序包含重复或已失效的应用，请刷新后重试".into()); }
        let mut ordered = Vec::with_capacity(config.apps.len());
        for id in &ids {
            if let Some(found) = config.apps.iter().find(|a| &a.id == id) {
                ordered.push(found.clone());
            }
        }
        for app in config.apps.iter() {
            if !ids.contains(&app.id) {
                ordered.push(app.clone());
            }
        }
        state.store.update(applications::CONFIG_FILE, &mut *config, |next| next.apps = ordered)?
    };
    broadcast_apps(&app, &snapshot);
    Ok(snapshot)
}

/// 文件 / 文件夹选择框。走插件的主线程调度，命令本身是异步的，不卡界面。
#[tauri::command]
async fn pick_application_target(
    app: AppHandle,
    kind: Option<String>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let want_folder = kind.as_deref() == Some("folder");
    // 先把 builder 取出来（只借用一次 app），再整体移进阻塞任务。
    let builder = app.dialog().file();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let picked = if want_folder {
            builder.blocking_pick_folder()
        } else {
            builder.blocking_pick_file()
        };
        picked.and_then(|file| file.into_path().ok())
    })
    .await
    .map_err(|e| format!("打开选择框失败：{e}"))?;

    Ok(picked.map(|path| path.to_string_lossy().into_owned()))
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportResult {
    added: usize,
    skipped: usize,
    total: usize,
    config: AppConfig,
}

/// 导入当前用户桌面与公共桌面的快捷方式。
///
/// 只读这两个目录，不做全盘扫描；同一个目标已经存在就跳过，不重复添加。
#[tauri::command]
fn import_desktop_shortcuts(app: AppHandle, category_id: Option<String>) -> Result<ImportResult, String> {
    import_application_targets(&app, category_id, applications::desktop_shortcuts())
}

#[tauri::command]
async fn list_system_apps() -> Result<Vec<system_apps::SystemApp>, String> {
    tauri::async_runtime::spawn_blocking(system_apps::list).await.map_err(|_| "系统应用检测中断")?
}

#[tauri::command]
async fn import_system_apps(app: AppHandle, category_id: String, targets: Vec<String>) -> Result<ImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut selected: std::collections::HashSet<_> = targets.into_iter().collect();
        let found: Vec<_> = system_apps::list()?.into_iter().filter(|a| selected.remove(&a.target))
            .map(|a| (a.target, a.name)).collect();
        if !selected.is_empty() {
            return Err("系统应用列表已变化，请重新打开列表后选择".into());
        }
        import_application_targets(&app, Some(category_id), found)
    }).await.map_err(|_| "系统应用导入中断")?
}

fn import_application_targets(app: &AppHandle, category_id: Option<String>, found: Vec<(String, String)>) -> Result<ImportResult, String> {
    let state = app.state::<AppState>();
    let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
    let total = found.len();
    let fallback_category = category_id.unwrap_or_else(|| "c-common".into());

    let (snapshot, added) = {
        let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = config.clone();
        let fallback_category = if next.categories.iter().any(|c| c.id == fallback_category) { fallback_category } else { CATEGORY_ALL.into() };
        let mut added = 0usize;
        for (target, name) in &found {
            if next.find_by_target(target).is_some() {
                continue;
            }
            let app_entry = applications::app_from_target(target, &fallback_category, Some(name));
            next.apps.push(app_entry);
            added += 1;
        }
        if added > 0 { state.store.write(applications::CONFIG_FILE, &next)?; *config = next.clone(); }
        (next, added)
    };

    if added > 0 {
        broadcast_apps(app, &snapshot);
    }
    log_info!("导入应用：发现 {total}，新增 {added}");
    Ok(ImportResult {
        added,
        skipped: total.saturating_sub(added),
        total,
        config: snapshot,
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IconPayload {
    target: String,
    data_uri: Option<String>,
    error: Option<String>,
}

/// 批量读取图标（EXE / LNK / 目录）。读不到就返回错误说明，界面回退到首字母色块。
#[tauri::command]
async fn read_app_icons(app: AppHandle, targets: Vec<String>) -> Result<Vec<IconPayload>, String> {
    let root = {
        let state = app.state::<AppState>();
        state.store.root().to_path_buf()
    };
    let size = icons::ICON_SIZE;
    tauri::async_runtime::spawn_blocking(move || {
        targets
            .into_iter()
            .map(|target| match icons::icon_data_uri(&root, &target, size) {
                Ok(uri) => IconPayload {
                    target,
                    data_uri: Some(uri),
                    error: None,
                },
                Err(error) => IconPayload {
                    target,
                    data_uri: None,
                    error: Some(error),
                },
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| format!("读取图标任务失败：{e}"))
}

/// 在资源管理器中定位目标，方便用户确认路径是否还有效。
#[tauri::command]
fn reveal_application_target(target: String) -> Result<(), String> {
    launcher::reveal(&target)
}

/// 目标类型（界面据此显示「文件夹 / 网址 / 快捷方式」等提示）。
#[tauri::command]
fn describe_target(target: String) -> serde_json::Value {
    serde_json::json!({
        "kind": launcher::kind_of(&target).as_str(),
        "exists": launcher::exists(&target),
        "suggestedName": launcher::suggested_name(&target)
    })
}


// ---------------------------------------------------------------- 剪贴板命令

#[tauri::command]
fn list_clipboard_entries(state: State<'_, AppState>) -> Vec<clipboard::ClipboardEntry> {
    state
        .clipboard
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entries
        .clone()
}

#[tauri::command]
fn set_clipboard_paused(app: AppHandle, paused: bool) -> Result<UiState, String> {
    let state = app.state::<AppState>();
    {
        let mut index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner());
        let mut next = index.clone();
        next.paused = paused;
        state.store.write(clipboard::INDEX_FILE, &next)?;
        *index = next;
    }
    let next = state.ui.update(|ui| ui.paused = paused);
    broadcast_ui(&app);
    clipboard_service::changed(&app);
    Ok(next)
}

// ---------------------------------------------------------------- 翻译命令

#[tauri::command]
fn get_translation_settings(state: State<'_, AppState>) -> TranslationSettings {
    translation::with_secret(state
        .translation
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone())
}

#[tauri::command]
fn save_translation_settings(
    app: AppHandle,
    mut settings: TranslationSettings,
) -> Result<TranslationSettings, String> {
    translation::validate(&settings)?;
    settings.has_secret = false;
    settings.base_url = settings.base_url.trim().trim_end_matches('/').to_string();
    settings.model = settings.model.trim().to_string();
    let state = app.state::<AppState>();
    let mut current = state.translation.lock().unwrap_or_else(|e| e.into_inner());
    state.store.write(translation::SETTINGS_FILE, &settings)?;
    *current = settings;
    let result = translation::with_secret(current.clone());
    drop(current);
    translation::settings_changed(&app);
    Ok(result)
}

// ---------------------------------------------------------------- 托盘

/// 阶段 2 自检：只读图标，不启动任何外部程序、不动鼠标键盘。
///
/// 走的是界面按钮背后的同一条命令路径（`icons::icon_data_uri`），
/// 所以日志里的成功/失败就是界面上会看到的结果。
fn selftest_icons(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let root = state.store.root().to_path_buf();
    let targets: Vec<(String, String)> = {
        let config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
        config
            .apps
            .iter()
            .filter(|item| !item.hidden)
            .map(|item| (item.name.clone(), item.target.clone()))
            .collect()
    };

    let mut ok = 0usize;
    for (name, target) in &targets {
        match icons::icon_data_uri(&root, target, icons::ICON_SIZE) {
            Ok(uri) => {
                ok += 1;
                log_info!("[自检] 图标成功 {name}，data URI {} 字节", uri.len());
            }
            Err(error) => log_info!("[自检] 图标失败 {name}：{error}"),
        }
    }
    log_info!("[自检] 图标检查完成：共 {} 个，成功 {ok} 个", targets.len());
}

/// 阶段 2 自检：只启动 `DESKTOPHUB_LITE_SELFTEST_LAUNCH` 指定的一个目标。
///
/// 默认不启用。要真的验证启动链路时必须显式给出一个目标，
/// 不会遍历配置批量启动（配置里可能有用户不想被打开的条目）。
fn selftest_launch(app: &AppHandle) {
    let Ok(target) = std::env::var("DESKTOPHUB_LITE_SELFTEST_LAUNCH") else {
        return;
    };
    if target.trim().is_empty() {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let config = state.apps.lock().unwrap_or_else(|e| e.into_inner()).clone();
    // 只接受配置里已经存在的目标，避免自检变成任意命令执行入口。
    let Some(item) = config
        .apps
        .iter()
        .find(|entry| applications::normalize_target(&entry.target) == applications::normalize_target(&target))
        .cloned()
    else {
        log_warn!("[自检] 启动目标不在应用配置里，已跳过");
        return;
    };
    match launcher::launch(&item.target) {
        Ok(()) => log_info!("[自检] 启动请求已交给系统：{}（{}）", item.name, item.target),
        Err(error) => log_warn!("[自检] 启动失败 {}：{error}", item.name),
    }
}

/// 无人值守自检：不需要任何人操作鼠标，程序自己开弹层、按时间采样窗口状态。
///
/// 目的是把「弹层显示后立刻消失」这类问题变成日志里可读的事实，
/// 而不是靠人去点、去猜。
fn spawn_selftest(app: AppHandle) {
    std::thread::spawn(move || {
        let sleep = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms));
        // 每个弹层的停留时长：默认够界面量完尺寸回报即可，
        // 需要外部截图时用 DESKTOPHUB_LITE_SELFTEST_HOLD 调大，方便对准时机。
        let hold: u64 = std::env::var("DESKTOPHUB_LITE_SELFTEST_HOLD")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1800);

        // 采样一个窗口：可见性、尺寸、位置。尺寸是最容易出错的一项，
        // 窗口比内容大会露出桌面，比内容小会裁掉内容。
        let probe = |app: &AppHandle, label: &str, note: &str| {
            match app.get_webview_window(label) {
                Some(window) => {
                    let visible = window.is_visible().unwrap_or(false);
                    let size = window
                        .outer_size()
                        .map(|s| format!("{}x{}", s.width, s.height))
                        .unwrap_or_else(|_| "?".into());
                    let pos = window
                        .outer_position()
                        .map(|p| format!("{},{}", p.x, p.y))
                        .unwrap_or_else(|_| "?".into());
                    log_info!("[自检] {note} {label} visible={visible} size={size} pos={pos}");
                }
                None => log_info!("[自检] {note} {label} 未创建"),
            }
        };

        sleep(2500);
        log_info!("[自检] 开始");
        probe(&app, windows::BAR, "初始");

        for panel in [PANEL_CLIPBOARD, PANEL_CATEGORY, PANEL_OVERFLOW, PANEL_TRANSLATION] {
            log_info!("[自检] 打开 {panel}");
            if let Err(error) = toggle_panel(app.clone(), panel.to_string()) {
                log_warn!("[自检] 打开 {panel} 失败：{error}");
                continue;
            }
            // 等界面量出内容尺寸并回报；停留久一点，方便外部截图对齐
            sleep(hold);
            probe(&app, windows::POP, panel);
        }

        log_info!("[自检] 收起弹层");
        let _ = close_panel(app.clone());
        sleep(300);

        log_info!("[自检] 打开设置");
        if let Err(error) = open_settings(app.clone(), None) {
            log_warn!("[自检] 打开设置失败：{error}");
        } else {
            // 先等界面量完尺寸再采样：日志出现时窗口仍然可见，
            // 外部截图才能按这条记录的 pos/size 裁剪。
            sleep(700);
            probe(&app, windows::SETTINGS, "设置");
            sleep(hold);
            let _ = close_settings(app.clone());
            sleep(300);
        }

        log_info!("[自检] 弹提示");
        if let Err(error) = show_toast(app.clone(), "自检提示".to_string(), Some("info".into())) {
            log_warn!("[自检] 提示失败：{error}");
        } else {
            sleep(400);
            probe(&app, windows::TOAST, "提示");
            sleep(hold);
            let _ = hide_toast(app.clone());
        }

        sleep(300);
        log_info!("[自检] 结束");
    });
}

fn build_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "显示图标条", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "隐藏图标条", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &hide, &settings, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("DesktopHub Lite")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app, windows::BAR),
            "hide" => hide_window(app, windows::BAR),
            "settings" => {
                if let Err(error) = open_settings(app.clone(), None) {
                    crate::log_warn!("打开设置失败：{error}");
                }
            }
            "quit" => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.running.store(false, Ordering::Relaxed);
                    for (_, pending) in state.translation_cancel.lock().unwrap_or_else(|e| e.into_inner()).drain() { let _ = pending.sender.send(()); }
                    drop(state.clipboard.lock().unwrap_or_else(|e| e.into_inner()));
                    state.windows.persist_now(&state.store);
                }
                app.exit(0);
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}

// ---------------------------------------------------------------- 启动

pub fn run() {
    if approvals::run_adapter() { return; }
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| { show_window(app, windows::BAR); }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // 解析数据目录：环境变量 > 便携隔离标记 > 跨版本共享目录 > 已有数据的旧便携目录。
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(|parent| parent.to_path_buf()));
            let app_local_dir = handle
                .path()
                .app_local_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let env_override = std::env::var("DESKTOPHUB_LITE_DATA")
                .ok()
                .map(std::path::PathBuf::from);
            let resolved_root = data_location::resolve_data_root(
                env_override,
                exe_dir.as_deref(),
                &app_local_dir,
            );
            let startup_dir = desktop_features::startup_exe_dir();
            let extra_hint_dirs = data_location::collect_extra_hint_dirs(startup_dir.as_deref());
            let mut auto_migrated_from: Option<String> = None;
            let mut auto_migrated_files: usize = 0;
            let mut auto_migrate_error: Option<String> = None;
            if resolved_root.mode == "shared"
                && !data_location::has_meaningful_data(&resolved_root.path)
            {
                let current_ptr = data_location::load_pointer(&resolved_root.pointer_file);
                match data_location::auto_migrate_empty_shared_dir(
                    &resolved_root.path,
                    exe_dir.as_deref(),
                    &app_local_dir,
                    &resolved_root.default_shared_dir,
                    &current_ptr,
                    &extra_hint_dirs,
                ) {
                    Ok(Some(outcome)) => {
                        data_location::record_recent_dir(
                            &resolved_root.pointer_file,
                            &outcome.source_dir,
                        );
                        auto_migrated_from = Some(outcome.source_dir.to_string_lossy().into_owned());
                        auto_migrated_files = outcome.copied_files;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        auto_migrate_error = Some(error);
                    }
                }
            }
            if resolved_root.should_persist_pointer {
                let mut ptr = data_location::load_pointer(&resolved_root.pointer_file);
                ptr.shared_data_dir = Some(resolved_root.path.to_string_lossy().into_owned());
                let _ = data_location::save_pointer(&resolved_root.pointer_file, &ptr);
            }
            let store = match Store::new(resolved_root.path.clone()) {
                Ok(store) => store,
                Err(error) => {
                    // 数据目录都建不起来时直接失败，而不是退化成内存态假装成功。
                    return Err(format!("无法创建数据目录：{error}").into());
                }
            };
            // 桌面程序没有控制台，诊断信息只能写文件。
            logging::init(store.root().join("desktophub-lite.log"));
            log_info!("启动，数据目录（{}）{}", resolved_root.mode, store.root().display());
            if let Some(ref src_dir) = auto_migrated_from {
                log_info!(
                    "首次启用共享目录，已自动从旧版目录迁移 {} 个文件：{} -> {}",
                    auto_migrated_files,
                    src_dir,
                    store.root().display()
                );
            } else if let Some(ref error) = auto_migrate_error {
                log_warn!("首次启用共享目录尝试自动迁移旧版数据时跳过：{error}");
            }
            if resolved_root.mode != "env" {
                data_location::record_recent_dir(&resolved_root.pointer_file, store.root());
            }
            let window_state: WindowState = store.read(windows::WINDOW_STATE_FILE).unwrap_or_default();
            let config: AppConfig = match store.read(applications::CONFIG_FILE) {
                Some(existing) => existing,
                None => {
                    // 首次运行：把初始配置落盘，用户下次打开能看到这个文件。
                    let seeded = applications::seed();
                    if let Err(error) = store.write(applications::CONFIG_FILE, &seeded) {
                        log_warn!("写入初始应用配置失败：{error}");
                    }
                    seeded
                }
            };
            if config.schema_version != applications::SCHEMA_VERSION {
                return Err("应用配置版本不兼容，原文件已保留".into());
            }
            let clipboard: ClipboardIndex = store
                .read(clipboard::INDEX_FILE)
                .unwrap_or_default();
            if clipboard.schema_version != clipboard::SCHEMA_VERSION { return Err("剪贴板索引版本不兼容，原文件已保留".into()); }
            let mut snippets: snippets::SnippetIndex = store
                .read(snippets::SNIPPETS_FILE)
                .unwrap_or_default();
            if snippets.schema_version != snippets::SCHEMA_VERSION { return Err("常用短语索引版本不兼容，原文件已保留".into()); }
            snippets.ensure_invariants();
            let mut translation: TranslationSettings = store
                .read(translation::SETTINGS_FILE)
                .unwrap_or_default();
            // 旧版初始值是自动判断。保留服务商与密钥绑定，面板默认从中译英开始。
            if translation.direction == "auto" { translation.direction = "zh-en".into(); }
            let prefs: state::UiPreferences = store.read(UI_PREFS_FILE).unwrap_or_default();
            let file_shelf = store.read(file_shelf::INDEX_FILE).unwrap_or_default();
            let paused = clipboard.paused;
            let shortcuts = store.read(desktop_support::SHORTCUTS_FILE).unwrap_or_default();

            app.manage(image_preview::Previews::default());
            app.manage(approvals::Approvals::default());
            app.manage(mail::Mail::load(&store));
            app.manage(audio::AudioState::default());
            app.manage(ocr::OcrState::default());
            app.manage(config_transfer::ConfigurationTransfers::default());
            app.manage(data_location::DataLocationState::new(
                resolved_root,
                exe_dir,
                app_local_dir,
                extra_hint_dirs,
                auto_migrated_from,
                auto_migrated_files,
            ));
            app.manage(file_shelf::ShelfHistory::default());
            app.manage(AppState {
                island_reading_explanation: AtomicBool::new(false),
                store,
                apps: Mutex::new(config),
                ui: UiStateHandle::new(UiState {
                    panel: None,
                    settings_open: false,
                    settings_request: None,
                    island_expanded: false,
                    island_anchor: prefs.island_anchor,
                    island_page: if state::valid_island_page(&prefs.island_page) { prefs.island_page } else { state::PANEL_APPLICATIONS.into() },
                    island_hover_ms: prefs.island_hover_ms.clamp(100, 2000),
                    island_leave_ms: prefs.island_leave_ms.clamp(100, 4000),
                    island_reduced_motion: prefs.island_reduced_motion,
                    wechat_enabled: prefs.wechat_enabled,
                    wechat_status: if prefs.wechat_enabled { "starting" } else { "off" }.into(),
                    wechat_attention: false,
                    wechat_event: 0,
                    island_appearance: prefs.island_appearance.normalized_on_load(),
                    mixer_visibility: prefs.mixer_visibility,
                    island_capture_error: None,
                    translation_pending: 0,
                    fullscreen_suppressed: false,
                    paused,
                    active_category_id: prefs.active_category_id,
                    theme: prefs.theme,
                }),
                configuration_gate: Mutex::new(()),
                windows: WindowsHandle::new(window_state),
                clipboard: Mutex::new(clipboard),
                snippets: Mutex::new(snippets),
                file_shelf: Mutex::new(file_shelf),
                translation: Mutex::new(translation),
                running: AtomicBool::new(true),
                translation_cancel: Mutex::new(std::collections::HashMap::new()),
                shortcuts: Mutex::new(shortcuts),
                update_status: Mutex::new(None),
                update_check_gate: tokio::sync::Mutex::new(()),
            });

            #[cfg(windows)]
            windows::install_bar_frame_guard(&handle);
            if let Err(error) = build_tray(&handle) {
                crate::log_warn!("创建托盘失败：{error}");
            }
            desktop_support::register(&handle);
            clipboard_service::start(handle.clone());
            wechat::start(handle.clone());
            desktop_features::start_fullscreen_watch(handle.clone());
            telemetry::start(handle.clone());
            approvals::start(handle.clone());
            mail::start(handle.clone());
            if app.state::<AppState>().ui.snapshot().island_appearance.auto_check_updates {
                let update_app = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = desktop_features::check_deskhub_updates(update_app.state()).await;
                });
            }

            // 图标条窗口：先按内容尺寸摆放，再显示，避免启动时闪一下大窗口。
            {
                let state = app.state::<AppState>();
                windows::place_bar(&handle, &state.windows);
                let _ = windows::ensure_pop(&handle);
                let _ = windows::ensure_settings(&handle);
                let _ = windows::ensure_toast(&handle);
            }
            // 初始窗口不可见，先确认捕获排除再显示，避免重启时闪出未受保护的内容。
            if capture_privacy::prepare_to_show(&handle) {
                reveal_bar(&handle);
            } else {
                let _ = open_settings(handle.clone(), None);
            }

            // 无人值守自检：设置 DESKTOPHUB_LITE_SELFTEST=1 时，
            // 程序自己打开弹层并按时间采样窗口可见性，不需要任何人操作鼠标。
            if std::env::var("DESKTOPHUB_LITE_SELFTEST").is_ok() {
                spawn_selftest(handle.clone());
            }
            // 阶段 2 的图标链路自检：默认关闭，只在设置环境变量时跑一次。
            if std::env::var("DESKTOPHUB_LITE_SELFTEST_ICONS").is_ok() {
                selftest_icons(&handle);
            }
            // 启动链路自检：必须显式给出一个已在配置里的目标，不批量启动。
            selftest_launch(&handle);

            Ok(())
        })
        .on_window_event(|window, event| {
            let app = window.app_handle();
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    if window.label().starts_with("image-preview-") { return; }
                    if window.label() == ocr::CAPTURE_WINDOW {
                        ocr::cleanup_capture_on_destroy(app);
                        return;
                    }
                    // 关闭只隐藏，退出走托盘菜单。
                    api.prevent_close();
                    if window.label() == windows::TRANSLATION { close_translation_window(app.clone()); }
                    else if window.label() == windows::BAR { hide_window(app, windows::BAR); }
                    else { let _ = window.hide(); }
                }
                WindowEvent::Destroyed => {
                    image_preview::forget(app, window.label());
                    if window.label() == ocr::CAPTURE_WINDOW {
                        ocr::cleanup_capture_on_destroy(app);
                    }
                }
                WindowEvent::Moved(position) => {
                    if window.label() == windows::BAR {
                        // 程序布局也会发 Moved；动画过程中不能把每帧写入数据文件。
                        if let Some(state) = app.try_state::<AppState>() {
                            state.windows.remember(position.x, position.y);
                        }
                        if let Some(state) = app.try_state::<AppState>() {
                            if state.ui.snapshot().panel.is_some() {
                                windows::place_pop(app, &state.windows);
                            }
                        }
                    }
                }
                WindowEvent::Focused(focused) => {
                    #[cfg(windows)]
                    if window.label() == windows::BAR {
                        windows::enforce_bar_frameless(app);
                    }
                    if !*focused && window.label() == windows::BAR {
                        if let Some(state) = app.try_state::<AppState>() {
                            let is_expanded = state.ui.snapshot().island_expanded;
                            let in_selftest = std::env::var("DESKTOPHUB_LITE_SELFTEST").is_ok();
                            if is_expanded && !in_selftest && !state.windows.bar_recently_expanded() {
                                let _ = window.emit(ISLAND_WINDOW_BLUR_EVENT, ());
                            }
                        }
                    }
                    // 岛内交互由 WebView 管理，系统短暂失焦不关闭输入中的界面。
                    // 「点击弹层外面收起弹层」必须由系统焦点事件判断：
                    // 界面自己监听 blur 会把「窗口刚显示还没拿到焦点」也当成点了外面。
                    if !*focused && window.label() == windows::POP {
                        let Some(state) = app.try_state::<AppState>() else {
                            return;
                        };
                        if state.ui.snapshot().panel.is_none() {
                            return;
                        }
                        // 刚显示时系统会给一次焦点再收回，这一小段失焦要忽略
                        if state.windows.pop_recently_shown() {
                            log_info!("[焦点] 弹层刚显示，忽略本次失焦");
                            return;
                        }
                        // 无人值守自检时没有前台应用，系统会主动收走焦点，
                        // 这与「用户点了别处」无法区分，所以自检期间不做自动收起。
                        if std::env::var("DESKTOPHUB_LITE_SELFTEST").is_ok() {
                            log_info!("[焦点] 自检模式，忽略失焦");
                            return;
                        }
                        log_info!("[焦点] 弹层失焦，收起弹层");
                        let closed = state
                            .ui
                            .snapshot()
                            .panel
                            .unwrap_or_default();
                        state.windows.mark_auto_closed(&closed);
                        state.ui.update(|ui| ui.panel = None);
                        hide_window(app, windows::POP);
                        broadcast_ui(app);
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            config_transfer::export_configuration,
            config_transfer::preview_configuration_import,
            config_transfer::cancel_configuration_import,
            config_transfer::apply_configuration_import,
            data_location::get_data_storage_status,
            data_location::open_current_data_dir,
            data_location::pick_data_dir_for_switch,
            data_location::preview_switch_data_dir,
            data_location::apply_switch_data_dir,
            data_location::pick_old_data_dir_for_import,
            data_location::preview_old_data_import,
            data_location::apply_old_data_import,
            data_location::restart_for_data_change,
            audio::list_audio_sessions,
            audio::set_mixer_item_visible,
            audio::update_audio_session,
            audio::list_audio_profiles,
            audio::save_audio_profile,
            audio::apply_audio_profile,
            file_shelf::list_shelf_files,
            file_shelf::add_shelf_files,
            file_shelf::remove_shelf_files,
            file_shelf::undo_shelf_remove,
            file_preview::preview_shelf_file,
            file_shelf::pick_shelf_files,
            file_shelf::reveal_shelf_file,
            file_shelf::drag_shelf_files,
            image_preview::open_image_preview,
            image_preview::image_preview_content,
            image_preview::pin_image_preview,
            current_window_label,
            approvals::approval_snapshot,
            approvals::answer_approval,
            approvals::setup::list_approval_agents,
            approvals::setup::connected_approval_agents,
            approvals::setup::connect_approval_agent,
            mail::mail_accounts,
            mail::open_mail_setup_page,
            mail::bind_mail_account,
            mail::unbind_mail_account,
            mail::mail_notices,
            mail::mail_action,
            app_info,
            ui_state,
            set_island_expanded,
            set_island_reading_explanation,
            set_island_anchor,
            select_island_page,
            open_translation_window,
            close_translation_window,
            wechat::set_wechat_enabled,
            wechat::clear_wechat_attention,
            wechat::open_existing_wechat,
            desktop_features::save_island_appearance,
            desktop_features::startup_status,
            desktop_features::set_startup_enabled,
            desktop_features::check_deskhub_updates,
            desktop_features::last_deskhub_update,
            desktop_features::open_deskhub_release_page,
            toggle_panel,
            close_panel,
            open_settings,
            close_settings,
            set_bar_size,
            set_pop_size,
            set_toast_size,
            start_bar_drag,
            set_category,
            set_theme,
            set_motion,
            selftest_windows,
            show_toast,
            hide_toast,
            list_applications,
            open_application,
            save_application,
            save_application_category,
            remove_application,
            reorder_applications,
            pick_application_target,
            import_desktop_shortcuts,
            list_system_apps,
            import_system_apps,
            read_app_icons,
            reveal_application_target,
            describe_target,
            list_clipboard_entries,
            set_clipboard_paused,
            clipboard_service::clipboard_status,
            clipboard_service::set_clipboard_capture_images,
            clipboard_service::set_clipboard_history_limit,
            clipboard_service::edit_clipboard_entry,
            clipboard_service::delete_clipboard_entries,
            clipboard_service::copy_clipboard_entry,
            clipboard_service::copy_translation_result,
            clipboard_service::read_clipboard_thumbnail,
            clipboard_service::delete_clipboard_entry,
            clipboard_service::clear_clipboard_entries,
            clipboard_service::create_clipboard_group,
            clipboard_service::remove_clipboard_group,
            clipboard_service::assign_clipboard_group,
            snippets::list_snippets,
            snippets::save_snippet,
            snippets::delete_snippet,
            snippets::reorder_snippets,
            snippets::create_snippet_group,
            snippets::remove_snippet_group,
            snippets::save_clip_as_snippet,
            snippets::copy_snippet,
            snippets::reveal_snippet_password,
            get_translation_settings,
            save_translation_settings,
            translation::translate_text,
            translation::explain_clipboard_error,
            translation::save_translation_secret,
            translation::delete_translation_secret,
            translation::cancel_translation,
            desktop_support::get_shortcuts,
            desktop_support::save_shortcuts,
            ocr::start_screen_ocr,
            ocr::get_capture_session,
            ocr::cancel_screen_ocr,
            ocr::run_screen_ocr,
            ocr::ocr_clipboard_image,
            ocr::copy_ocr_text,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DesktopHub Lite");
}
