use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use crate::{AppState, windows, toggle_panel, show_window, set_island_expanded};

pub const SHORTCUTS_FILE: &str = "shortcuts.json";
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcuts {
    pub bar: String,
    pub clipboard: String,
    pub translation: String,
    #[serde(default)]
    pub errors: Vec<String>,
}
impl Default for Shortcuts {
    fn default() -> Self { Self { bar: "Ctrl+Alt+Space".into(), clipboard: "Ctrl+Shift+V".into(), translation: "Ctrl+Shift+F".into(), errors: vec![] } }
}
pub fn register(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut settings = state.shortcuts.lock().unwrap_or_else(|e| e.into_inner());
    settings.errors.clear();
    for (key, target) in [(settings.bar.clone(), "bar"), (settings.clipboard.clone(), "clipboard"), (settings.translation.clone(), "translation")] {
        if key.trim().is_empty() { continue; }
        if app.global_shortcut().on_shortcut(key.as_str(), move |app, _, event| {
            if event.state() != ShortcutState::Pressed { return; }
            show_window(app, windows::BAR);
            if target != "bar" { let _ = toggle_panel(app.clone(), target.into()); }
            else { set_island_expanded(app.clone(), true, Some(true)); }
        }).is_err() { settings.errors.push(format!("{key} 注册失败，可能已被占用，请更换组合键")); }
    }
}
#[tauri::command]
pub fn get_shortcuts(state: State<'_, AppState>) -> Shortcuts { state.shortcuts.lock().unwrap_or_else(|e| e.into_inner()).clone() }
#[tauri::command]
pub fn save_shortcuts(app: AppHandle, mut settings: Shortcuts) -> Result<Shortcuts, String> {
    let keys = [&settings.bar, &settings.clipboard, &settings.translation];
    let mut seen = std::collections::HashSet::new();
    for key in keys { if !key.is_empty() && (key.len() > 80 || !seen.insert(key.to_lowercase())) { return Err("快捷键不能重复，且长度不能超过 80".into()); } }
    let state = app.state::<AppState>();
    state.store.write(SHORTCUTS_FILE, &settings)?;
    app.global_shortcut().unregister_all().map_err(|_| "无法释放旧快捷键")?;
    settings.errors.clear();
    *state.shortcuts.lock().unwrap_or_else(|e| e.into_inner()) = settings;
    register(&app);
    Ok(get_shortcuts(app.state()))
}
