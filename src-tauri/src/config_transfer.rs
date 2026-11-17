//! Explicit settings-only export / preview / confirm. No imported target is launched.
use std::{io::Read, sync::{Mutex, atomic::{AtomicU64, Ordering}}, time::Instant};
use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;
use crate::{AppState, applications::AppConfig, config_format::{self, ConfigurationBackup, PendingConfiguration}, state::UiState};

#[derive(Default)]
pub struct ConfigurationTransfers(pub Mutex<Option<PendingConfiguration>>);
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview { token: String, file_name: String, changes: config_format::Changes, warnings: Vec<String> }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult { backup_path: String, config: AppConfig, ui: UiState }

fn settings_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == crate::windows::SETTINGS { Ok(()) } else { Err("请在设置窗口操作配置备份".into()) }
}

fn check_targets(backup: &mut ConfigurationBackup) -> Vec<String> {
    config_format::mark_targets(backup, crate::launcher::exists)
}

#[tauri::command]
pub async fn export_configuration(app: AppHandle, window: WebviewWindow) -> Result<Option<String>, String> {
    settings_only(&window)?;
    let dialog = app.dialog().file().add_filter("DesktopHub 配置", &["json"]).set_file_name("DesktopHub-config.json");
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = dialog.blocking_save_file() else { return Ok(None); };
        let path = file.into_path().map_err(|_| "只支持本地配置文件")?;
        let state = app.state::<AppState>();
        let parent = path.parent().ok_or("保存位置无效")?.canonicalize().map_err(|_| "无法读取保存目录")?;
        let root = state.store.root().canonicalize().map_err(|_| "无法读取配置目录")?;
        if parent.starts_with(root) { return Err("请选择应用 data 目录之外的备份位置".into()); }
        let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
        let apps = state.apps.lock().map_err(|_| "应用配置暂时不可用")?;
        let backup = ConfigurationBackup::capture(&apps, &state.ui.snapshot());
        config_format::validate(&backup)?;
        let bytes = serde_json::to_vec_pretty(&backup).map_err(|_| "无法导出配置")?;
        if bytes.len() > config_format::MAX_BYTES { return Err("配置超过 2 MiB，无法导出".into()); }
        crate::storage::write_file_atomic(&path, &bytes)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    }).await.map_err(|_| "导出任务未完成")?
}

#[tauri::command]
pub async fn preview_configuration_import(app: AppHandle, window: WebviewWindow) -> Result<Option<ImportPreview>, String> {
    settings_only(&window)?;
    *app.state::<ConfigurationTransfers>().0.lock().map_err(|_| "配置预览暂时不可用")? = None;
    let dialog = app.dialog().file().add_filter("DesktopHub 配置", &["json"]);
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = dialog.blocking_pick_file() else { return Ok(None); };
        let path = file.into_path().map_err(|_| "只支持本地配置文件")?;
        let mut bytes = Vec::new();
        std::fs::File::open(&path).map_err(|_| "无法打开所选文件")?.take(config_format::MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|_| "无法读取所选文件")?;
        let mut backup = config_format::parse(&bytes)?;
        for item in &mut backup.applications.apps { item.broken = None; }
        let state = app.state::<AppState>();
        let current = {
            let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
            let apps = state.apps.lock().map_err(|_| "应用配置暂时不可用")?;
            ConfigurationBackup::capture(&apps, &state.ui.snapshot())
        };
        let changes = config_format::describe(&current, &backup);
        let warnings = check_targets(&mut backup);
        let token = format!("{}-{}", std::process::id(), NEXT_TOKEN.fetch_add(1, Ordering::Relaxed));
        let preview = ImportPreview { token: token.clone(), file_name: path.file_name().unwrap_or_default().to_string_lossy().into_owned(), changes, warnings };
        *app.state::<ConfigurationTransfers>().0.lock().map_err(|_| "配置预览暂时不可用")? = Some(PendingConfiguration { token, backup, fingerprint: current.fingerprint()?, created: Instant::now() });
        Ok(Some(preview))
    }).await.map_err(|_| "配置预览任务未完成")?
}

#[tauri::command]
pub fn cancel_configuration_import(app: AppHandle, window: WebviewWindow, token: String) -> Result<(), String> {
    settings_only(&window)?;
    let service = app.state::<ConfigurationTransfers>();
    let mut pending = service.0.lock().map_err(|_| "配置预览暂时不可用")?;
    if pending.as_ref().is_some_and(|item| item.token == token) { *pending = None; }
    Ok(())
}

#[tauri::command]
pub async fn apply_configuration_import(app: AppHandle, window: WebviewWindow, token: String) -> Result<ImportResult, String> {
    settings_only(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let service = app.state::<ConfigurationTransfers>();
        let mut pending = service.0.lock().map_err(|_| "配置预览暂时不可用")?;
        let item = pending.as_mut().ok_or("导入已取消或预览已失效，请重新选择文件")?;
        let state = app.state::<AppState>();
        let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
        let mut apps = state.apps.lock().map_err(|_| "应用配置暂时不可用")?;
        let previous = state.ui.snapshot();
        let current = ConfigurationBackup::capture(&apps, &previous);
        item.validate_confirmation(&token, &current)?;
        check_targets(&mut item.backup);
        let prefs = item.backup.merged_preferences(&previous);
        let backup_path = state.store.write_configuration(&item.backup.applications, &prefs, &current)?;
        *apps = item.backup.applications.clone();
        let config = apps.clone();
        let ui = state.ui.update(|ui| item.backup.preferences.apply(ui));
        *pending = None;
        drop(apps); drop(pending);
        if previous.island_page == crate::state::PANEL_TRANSLATION && ui.island_page != previous.island_page { crate::translation::cancel_for_window(&app, crate::windows::BAR); }
        if previous.island_anchor != ui.island_anchor { state.windows.set_bar_free_position(false); }
        crate::windows::place_bar(&app, &state.windows);
        crate::broadcast_apps(&app, &config); crate::broadcast_ui(&app);
        Ok(ImportResult { backup_path: backup_path.to_string_lossy().into_owned(), config, ui: state.ui.snapshot() })
    }).await.map_err(|_| "恢复任务未完成，请重启以检查恢复记录")?
}
