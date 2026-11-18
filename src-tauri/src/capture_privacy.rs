//! 仅排除灵动岛窗口的捕获，不采集桌面，也不改变 WebGL。
use tauri::{AppHandle, Manager};
use crate::{broadcast_ui, windows, AppState};

pub fn apply(app: &AppHandle, hidden: bool) -> Result<(), String> {
    use ::windows::Win32::Foundation::HWND;
    use ::windows::Win32::UI::WindowsAndMessaging::{
        GetWindowDisplayAffinity, SetWindowDisplayAffinity, WINDOW_DISPLAY_AFFINITY,
        WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    };
    let window = app.get_webview_window(windows::BAR).ok_or("找不到灵动岛窗口")?;
    let native = window.hwnd().map_err(|error| format!("无法读取灵动岛窗口：{error}"))?;
    let hwnd = HWND(native.0);
    let target = if hidden { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
    unsafe {
        let mut previous = 0;
        GetWindowDisplayAffinity(hwnd, &mut previous)
            .map_err(|error| format!("无法读取屏幕共享隐藏状态：{error}"))?;
        if previous == target.0 { return Ok(()); }
        SetWindowDisplayAffinity(hwnd, target)
            .map_err(|error| format!("无法修改屏幕共享隐藏状态：{error}"))?;
        let mut actual = 0;
        if GetWindowDisplayAffinity(hwnd, &mut actual).is_err() || actual != target.0 {
            // 不把系统未接受的设置当作成功；调用方会暂时隐藏窗口并显示错误。
            let _ = SetWindowDisplayAffinity(hwnd, WINDOW_DISPLAY_AFFINITY(previous));
            return Err("系统未确认屏幕共享隐藏状态，请检查 Windows 版本后重试".into());
        }
    }
    Ok(())
}

pub fn hold_on_error(app: &AppHandle, error: String) {
    crate::hide_window(app, windows::BAR);
    app.state::<AppState>().ui.update(|ui| ui.island_capture_error = Some(error.clone()));
    crate::log_warn!("灵动岛屏幕共享隐藏设置失败：{error}");
    broadcast_ui(app);
}

/// 启动和再次显示前确认保护已应用，失败时保持隐藏，由设置窗口提供恢复入口。
pub fn prepare_to_show(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let ui = state.ui.snapshot();
    if !ui.island_appearance.hide_from_capture && ui.island_capture_error.is_none() { return true; }
    match apply(app, ui.island_appearance.hide_from_capture) {
        Ok(()) => {
            if ui.island_capture_error.is_some() {
                state.ui.update(|ui| ui.island_capture_error = None);
                broadcast_ui(app);
            }
            true
        }
        Err(error) => { hold_on_error(app, error); false }
    }
}
