//! DesktopHub Lite 桌面外壳。
//!
//! 阶段 1 接通了「界面 ↔ Rust ↔ 隔离数据目录」的链路；阶段 2 在此之上接通：
//! - 应用配置的真实增删改（手动添加文件 / 文件夹、导入桌面快捷方式、编辑、删除、排序）；
//! - 真实启动（EXE / LNK / 目录 / 网址一律交给 `ShellExecuteW`，保留 LNK 自身语义）；
//! - 真实 Windows 图标（Shell 图标抽取 + 本地 PNG 缓存）。
//!
//! 剪贴板与翻译只登记契约，不读写真实数据、不发真实请求（阶段 3 / 4）。

mod applications;
mod clipboard;
mod clipboard_service;
mod desktop_support;
mod desktop_features;
mod file_shelf;
mod icons;
mod image_preview;
mod launcher;
mod logging;
mod state;
mod storage;
mod translation;
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
    PANEL_TRANSLATION, THEME_DARK, THEME_LIGHT, THEME_SYSTEM,
};
use storage::Store;
use translation::TranslationSettings;
use windows::{WindowsHandle, WindowState};

pub const UI_STATE_EVENT: &str = "ui-state";
/// 应用配置变化事件：图标条、溢出网格、设置窗口各自持有一份镜像，靠它刷新。
pub const APPLICATIONS_EVENT: &str = "applications-changed";
/// 轻提示事件：toast 窗口收到后渲染并回报尺寸。
pub const TOAST_EVENT: &str = "toast";
/// 需要跨重启保留的界面偏好。
pub const UI_PREFS_FILE: &str = "ui-preferences.json";

/// 应用运行期共享状态。
pub struct AppState {
    store: Store,
    apps: Mutex<AppConfig>,
    ui: UiStateHandle,
    windows: WindowsHandle,
    clipboard: Mutex<ClipboardIndex>,
    file_shelf: Mutex<file_shelf::ShelfIndex>,
    translation: Mutex<TranslationSettings>,
    running: AtomicBool,
    translation_cancel: Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>>,
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

fn hide_window(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.hide();
    }
}

fn show_window(app: &AppHandle, label: &str) {
    if label == windows::BAR && app.try_state::<AppState>().is_some_and(|state| state.ui.snapshot().fullscreen_suppressed) {
        return;
    }
    if let Some(window) = app.get_webview_window(label) {
        if let Err(error) = window.show() {
            log_warn!("显示窗口 {label} 失败：{error}");
            return;
        }
        let _ = window.set_focus();
    } else {
        log_warn!("找不到窗口 {label}");
    }
}

/// 分类与主题要跨重启保留：写在隔离目录，失败只记录不打断操作。
fn persist_ui_prefs(app: &AppHandle, snapshot: &UiState) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let prefs = state::UiPreferences::from_state(snapshot);
    if let Err(error) = state.store.write(UI_PREFS_FILE, &prefs) {
        crate::log_warn!("保存界面偏好失败：{error}");
    }
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
fn set_island_expanded(app: AppHandle, expanded: bool, focus: Option<bool>) -> UiState {
    let state = app.state::<AppState>();
    if expanded && state.ui.snapshot().fullscreen_suppressed { return state.ui.snapshot(); }
    let next = state.ui.update(|ui| {
        ui.island_expanded = expanded;
        ui.panel = if expanded && ui.island_page != state::PANEL_APPLICATIONS {
            Some(ui.island_page.clone())
        } else { None };
    });
    if expanded { state.windows.mark_pop_shown(); }
    else {
        state.windows.clear_pop_shown();
        translation::cancel_for_window(&state, windows::BAR);
    }
    if focus.unwrap_or(false) { show_window(&app, windows::BAR); }
    broadcast_ui(&app);
    next
}

#[tauri::command]
fn set_island_anchor(app: AppHandle, anchor: String) -> Result<UiState, String> {
    if !["left", "center", "right"].contains(&anchor.as_str()) { return Err("未知停靠位置".into()); }
    let state = app.state::<AppState>();
    let next = state.ui.update(|ui| ui.island_anchor = anchor);
    persist_ui_prefs(&app, &next);
    windows::place_bar(&app, &state.windows);
    broadcast_ui(&app);
    Ok(next)
}

#[tauri::command]
fn select_island_page(app: AppHandle, page: String, focus: Option<bool>) -> Result<UiState, String> {
    if !state::valid_island_page(&page) { return Err("未知灵动岛页面".into()); }
    let state = app.state::<AppState>();
    let previous = state.ui.snapshot();
    if previous.fullscreen_suppressed { return Ok(previous); }
    let next = state.ui.update(|ui| {
        ui.island_page = page.clone();
        ui.island_expanded = true;
        ui.panel = if page == state::PANEL_APPLICATIONS { None } else { Some(page.clone()) };
    });
    if previous.island_page == PANEL_TRANSLATION && page != PANEL_TRANSLATION {
        translation::cancel_for_window(&state, windows::BAR);
    }
    if previous.island_page != page { persist_ui_prefs(&app, &next); }
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
    translation::cancel_for_window(&app.state::<AppState>(), windows::TRANSLATION);
    let _ = app.emit_to(windows::TRANSLATION, "translation-window-hidden", ());
    hide_window(&app, windows::TRANSLATION);
}

// ---------------------------------------------------------------- 窗口命令

#[tauri::command]
fn toggle_panel(app: AppHandle, panel: String) -> Result<UiState, String> {
    if app.state::<AppState>().ui.snapshot().fullscreen_suppressed {
        return Ok(app.state::<AppState>().ui.snapshot());
    }
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
fn close_panel(app: AppHandle) -> UiState {
    let state = app.state::<AppState>();
    let next = state.ui.update(|ui| {
        if matches!(ui.panel.as_deref(), Some(PANEL_TRANSLATION | PANEL_CLIPBOARD)) {
            ui.island_page = state::PANEL_APPLICATIONS.into();
        }
        ui.panel = None;
    });
    persist_ui_prefs(&app, &next);
    hide_window(&app, windows::POP);
    broadcast_ui(&app);
    next
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<UiState, String> {
    windows::ensure_settings(&app)?;
    let state = app.state::<AppState>();
    let next = state.ui.update(|ui| {
        ui.panel = None;
        ui.settings_open = true;
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
fn start_bar_drag(window: WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

/// 切换图标条分类。分类是跨窗口状态：弹层窗口也要跟着变。
#[tauri::command]
fn set_category(app: AppHandle, category_id: String) -> UiState {
    let state = app.state::<AppState>();
    let known = {
        let config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
        config.categories.iter().any(|c| c.id == category_id)
    };
    let next = state.ui.update(|ui| {
        if known || category_id == CATEGORY_ALL {
            ui.active_category_id = category_id.clone();
        }
    });
    persist_ui_prefs(&app, &next);
    broadcast_ui(&app);
    next
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
    if ![THEME_DARK, THEME_LIGHT, THEME_SYSTEM].contains(&theme.as_str()) {
        return Err(format!("未知主题：{theme}"));
    }
    let state = app.state::<AppState>();
    let next = state.ui.update(|ui| ui.theme = theme.clone());
    persist_ui_prefs(&app, &next);
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
            if let Some(existing) = config.apps.iter_mut().find(|a| a.id == item.id) {
                existing.broken = Some(true);
            }
            let snapshot = config.clone();
            drop(config);
            let _ = state.store.write(applications::CONFIG_FILE, &snapshot);
            broadcast_apps(&app, &snapshot);
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
                    if let Some(existing) = config.apps.iter_mut().find(|a| a.id == item.id) {
                        existing.broken = None;
                    }
                    let snapshot = config.clone();
                    drop(config);
                    let _ = state.store.write(applications::CONFIG_FILE, &snapshot);
                    broadcast_apps(&app, &snapshot);
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
    let snapshot = {
        let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
        let mut incoming = application;
        // 失效状态由目标是否存在决定，不接受界面瞎传。
        incoming.broken = if launcher::exists(&incoming.target) {
            None
        } else {
            Some(true)
        };
        match config.apps.iter_mut().find(|a| a.id == incoming.id) {
            Some(existing) => *existing = incoming,
            None => config.apps.push(incoming),
        }
        config.clone()
    };
    state.store.write(applications::CONFIG_FILE, &snapshot)?;
    broadcast_apps(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn save_application_category(app: AppHandle, id: String, name: Option<String>) -> Result<AppConfig, String> {
    if id == "all" || id.trim().is_empty() { return Err("默认分类不能修改".into()); }
    let state = app.state::<AppState>();
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
    state.store.write(applications::CONFIG_FILE, &next)?;
    *config = next.clone(); drop(config); broadcast_apps(&app, &next); Ok(next)
}

#[tauri::command]
fn remove_application(app: AppHandle, id: String) -> Result<AppConfig, String> {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
        config.apps.retain(|a| a.id != id);
        config.clone()
    };
    state.store.write(applications::CONFIG_FILE, &snapshot)?;
    broadcast_apps(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
fn reorder_applications(app: AppHandle, ids: Vec<String>) -> Result<AppConfig, String> {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
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
        config.apps = ordered;
        config.clone()
    };
    state.store.write(applications::CONFIG_FILE, &snapshot)?;
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
fn import_desktop_shortcuts(app: AppHandle, category_id: Option<String>) -> ImportResult {
    let state = app.state::<AppState>();
    let found = applications::desktop_shortcuts();
    let total = found.len();
    let fallback_category = category_id.unwrap_or_else(|| "c-common".into());

    let (snapshot, added) = {
        let mut config = state.apps.lock().unwrap_or_else(|e| e.into_inner());
        let mut added = 0usize;
        for (target, name) in &found {
            if config.find_by_target(target).is_some() {
                continue;
            }
            let app_entry = applications::app_from_target(target, &fallback_category, Some(name));
            config.apps.push(app_entry);
            added += 1;
        }
        (config.clone(), added)
    };

    if added > 0 {
        if let Err(error) = state.store.write(applications::CONFIG_FILE, &snapshot) {
            log_warn!("导入桌面快捷方式后保存失败：{error}");
        }
        broadcast_apps(&app, &snapshot);
    }
    log_info!("导入桌面快捷方式：发现 {total}，新增 {added}");
    ImportResult {
        added,
        skipped: total.saturating_sub(added),
        total,
        config: snapshot,
    }
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
        if let Err(error) = open_settings(app.clone()) {
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
                if let Err(error) = open_settings(app.clone()) {
                    crate::log_warn!("打开设置失败：{error}");
                }
            }
            "quit" => {
                if let Some(state) = app.try_state::<AppState>() {
                    state.running.store(false, Ordering::Relaxed);
                    for (_, sender) in state.translation_cancel.lock().unwrap_or_else(|e| e.into_inner()).drain() { let _ = sender.send(()); }
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
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| { show_window(app, windows::BAR); }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // 隔离数据目录：开发验证一律落在 DesktopHub-work 下，不碰真实用户数据。
            let root = std::env::var("DESKTOPHUB_LITE_DATA")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    if let Some(directory) = std::env::current_exe().ok().and_then(|path| path.parent().map(|parent| parent.to_path_buf())) {
                        if directory.join("portable.flag").exists() { return directory.join("data"); }
                    }
                    handle
                        .path()
                        .app_local_data_dir()
                        .unwrap_or_else(|_| std::path::PathBuf::from("."))
                });
            let store = match Store::new(root) {
                Ok(store) => store,
                Err(error) => {
                    // 数据目录都建不起来时直接失败，而不是退化成内存态假装成功。
                    return Err(format!("无法创建数据目录：{error}").into());
                }
            };
            // 桌面程序没有控制台，诊断信息只能写文件。
            logging::init(store.root().join("desktophub-lite.log"));
            log_info!("启动，数据目录 {}", store.root().display());
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
            let translation: TranslationSettings = store
                .read(translation::SETTINGS_FILE)
                .unwrap_or_default();
            let prefs: state::UiPreferences = store.read(UI_PREFS_FILE).unwrap_or_default();
            let file_shelf = store.read(file_shelf::INDEX_FILE).unwrap_or_default();
            let paused = clipboard.paused;
            let shortcuts = store.read(desktop_support::SHORTCUTS_FILE).unwrap_or_default();

            app.manage(image_preview::Previews::default());
            app.manage(AppState {
                store,
                apps: Mutex::new(config),
                ui: UiStateHandle::new(UiState {
                    panel: None,
                    settings_open: false,
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
                    island_appearance: prefs.island_appearance,
                    fullscreen_suppressed: false,
                    paused,
                    active_category_id: prefs.active_category_id,
                    theme: prefs.theme,
                }),
                windows: WindowsHandle::new(window_state),
                clipboard: Mutex::new(clipboard),
                file_shelf: Mutex::new(file_shelf),
                translation: Mutex::new(translation),
                running: AtomicBool::new(true),
                translation_cancel: Mutex::new(std::collections::HashMap::new()),
                shortcuts: Mutex::new(shortcuts),
                update_status: Mutex::new(None),
                update_check_gate: tokio::sync::Mutex::new(()),
            });

            if let Err(error) = build_tray(&handle) {
                crate::log_warn!("创建托盘失败：{error}");
            }
            desktop_support::register(&handle);
            clipboard_service::start(handle.clone());
            wechat::start(handle.clone());
            desktop_features::start_fullscreen_watch(handle.clone());
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
                    // 关闭只隐藏，退出走托盘菜单。
                    api.prevent_close();
                    if window.label() == windows::TRANSLATION { close_translation_window(app.clone()); }
                    else { let _ = window.hide(); }
                }
                WindowEvent::Destroyed => { image_preview::forget(app, window.label()); }
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
            file_shelf::list_shelf_files,
            file_shelf::add_shelf_files,
            file_shelf::remove_shelf_files,
            file_shelf::pick_shelf_files,
            file_shelf::reveal_shelf_file,
            file_shelf::drag_shelf_files,
            image_preview::open_image_preview,
            image_preview::image_preview_content,
            image_preview::pin_image_preview,
            current_window_label,
            app_info,
            ui_state,
            set_island_expanded,
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
            get_translation_settings,
            save_translation_settings,
            translation::translate_text,
            translation::save_translation_secret,
            translation::delete_translation_secret,
            translation::cancel_translation,
            desktop_support::get_shortcuts,
            desktop_support::save_shortcuts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running DesktopHub Lite");
}
