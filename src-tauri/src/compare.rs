//! 独立对照窗口仅保存一次性内存快照，关闭即释放，不落盘、不读取任意路径。
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::{Mutex, atomic::{AtomicU64, Ordering}}};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
#[derive(Clone, Deserialize, Serialize)]
pub struct Snapshot { kind: String, mode: String, texts: Vec<String>, images: Vec<String>, names: Vec<String> }
#[derive(Default)]
pub struct Comparisons(Mutex<HashMap<String, Snapshot>>);
static NEXT: AtomicU64 = AtomicU64::new(1);
fn validate(snapshot: &Snapshot) -> Result<(), String> {
    if !matches!(snapshot.kind.as_str(), "image" | "text") || !matches!(snapshot.mode.as_str(), "side" | "wipe" | "diff") || snapshot.texts.len() != 2 || snapshot.images.len() != 2 || snapshot.names.len() != 2 { return Err("对照内容格式无效".into()); }
    if snapshot.texts.iter().any(|s| s.len() > 64 * 1024) || snapshot.names.iter().any(|s| s.len() > 2048) { return Err("对照文字过长".into()); }
    for image in &snapshot.images {
        if image.len() > (crate::file_preview::IMAGE_LIMIT as usize * 4 / 3 + 128) { return Err("对照图片超过 8 MiB".into()); }
        if !image.is_empty() && !["data:image/png;base64,", "data:image/jpeg;base64,", "data:image/webp;base64,", "data:image/gif;base64,", "data:image/bmp;base64,", "data:image/x-icon;base64,"].iter().any(|prefix| image.starts_with(prefix)) { return Err("不支持这种对照图片来源".into()); }
    }
    Ok(())
}
#[tauri::command]
pub async fn open_compare_window(app: AppHandle, snapshot: Snapshot) -> Result<(), String> {
    validate(&snapshot)?;
    let label = format!("compare-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    {
        let state = app.state::<Comparisons>();
        let mut windows = state.0.lock().map_err(|_| "对照窗口暂时不可用")?;
        if windows.len() >= 4 { return Err("最多同时打开 4 个对照窗口，请先关闭一个".into()); }
        windows.insert(label.clone(), snapshot);
    }
    if let Err(error) = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title("浮动对照台 · DesktopHub Lite").inner_size(920.0, 680.0).min_inner_size(420.0, 360.0)
        .decorations(false).resizable(true).focused(true).visible(true).build() {
        forget(&app, &label); return Err(format!("无法打开对照窗口：{error}"));
    }
    Ok(())
}
#[tauri::command]
pub fn compare_window_content(state: State<'_, Comparisons>, window: WebviewWindow) -> Result<Snapshot, String> {
    state.0.lock().map_err(|_| "对照内容暂时不可用")?.get(window.label()).cloned().ok_or("对照内容已释放，请重新打开".into())
}
#[tauri::command]
pub fn pin_compare_window(window: WebviewWindow, pinned: bool) -> Result<bool, String> {
    if !window.label().starts_with("compare-") { return Err("当前窗口不是对照台".into()); }
    window.set_always_on_top(pinned).map_err(|_| "无法设置对照窗口置顶")?;
    window.is_always_on_top().map_err(|_| "无法读回窗口置顶状态".into())
}
pub fn forget(app: &AppHandle, label: &str) { if let Some(state) = app.try_state::<Comparisons>() { state.0.lock().unwrap_or_else(|e| e.into_inner()).remove(label); } }
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn snapshot_rejects_remote_and_oversize_content() {
        let mut s = Snapshot { kind: "text".into(), mode: "side".into(), texts: vec!["a".into(), "b".into()], images: vec![String::new(), String::new()], names: vec!["A".into(), "B".into()] };
        assert!(validate(&s).is_ok());
        s.images[0] = "https://example.org/image.png".into(); assert!(validate(&s).is_err());
        s.images[0].clear(); s.texts[0] = "a".repeat(65537); assert!(validate(&s).is_err());
        s.texts[0].clear(); s.images.pop(); assert!(validate(&s).is_err());
    }
}
