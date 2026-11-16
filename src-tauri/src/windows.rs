//! 窗口创建、尺寸与位置。
//!
//! 多窗口是点击穿透的前提：透明窗口的透明区域同样会拦截鼠标，
//! 所以图标条窗口必须刚好等于图标条本身的大小；
//! 剪贴板 / 翻译 / 分类 / 溢出这些弹层各放在独立的 `pop` 窗口里，
//! 设置放在独立的 `settings` 窗口里。
//! 任何窗口都不做「全屏透明覆盖」，否则桌面会被整块挡住。

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::storage::Store;

pub const BAR: &str = "bar";
pub const POP: &str = "pop";
pub const SETTINGS: &str = "settings";
pub const TRANSLATION: &str = "translation";
/// 轻提示窗口：图标条窗口刚好等于图标条本身，放不下提示，
/// 所以提示单独用一个按内容自适应的小窗口，浮在图标条上方。
pub const TOAST: &str = "toast";

pub const WINDOW_STATE_FILE: &str = "window-state.json";

/// 弹层与图标条之间的间距（逻辑像素）。
const POP_GAP: f64 = 10.0;
/// 图标条离工作区边缘的默认距离（逻辑像素）。
const SETTINGS_SIZE: (f64, f64) = (760.0, 560.0);


#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    pub bar_x: Option<i32>,
    pub bar_y: Option<i32>,
}

pub struct WindowsHandle {
    state: Mutex<WindowState>,
    bar_size: Mutex<(f64, f64)>,
    pop_size: Mutex<(f64, f64)>,
    /// 弹层相对图标条的水平对齐：center / left / right
    pop_align: Mutex<String>,
    toast_size: Mutex<(f64, f64)>,
    /// 弹层窗口显示的时刻。刚显示时系统会给它一次焦点，
    /// 之后紧接着的失焦并不代表用户点了别处，需要忽略这一小段。
    pop_shown_at: Mutex<Option<Instant>>,
    /// 最近一次「因失焦自动收起」的时刻与弹层名。
    /// 点图标条上同一个按钮想收起时，焦点先离开弹层触发自动收起，
    /// 紧接着这次点击又会被当成「打开」，需要用这个记录消掉重复打开。
    last_auto_close: Mutex<Option<(Instant, String)>>,
}

impl WindowsHandle {
    pub fn new(state: WindowState) -> Self {
        Self {
            state: Mutex::new(state),
            bar_size: Mutex::new((140.0, 14.0)),
            pop_size: Mutex::new((384.0, 480.0)),
            pop_align: Mutex::new("center".to_string()),
            toast_size: Mutex::new((360.0, 40.0)),
            pop_shown_at: Mutex::new(None),
            last_auto_close: Mutex::new(None),
        }
    }

    /// 记录「因失焦自动收起」，供随后的点击判断是否属于同一次收起动作。
    pub fn mark_auto_closed(&self, panel: &str) {
        *self.last_auto_close.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((Instant::now(), panel.to_string()));
    }

    /// 取走并清空自动收起记录；超过 600ms 视为过期。
    pub fn take_auto_closed(&self, panel: &str) -> bool {
        let mut guard = self.last_auto_close.lock().unwrap_or_else(|e| e.into_inner());
        match guard.as_ref() {
            Some((at, name)) if name == panel && at.elapsed() < Duration::from_millis(600) => {
                *guard = None;
                true
            }
            _ => {
                *guard = None;
                false
            }
        }
    }

    /// 记录弹层刚显示的时刻，用于忽略「显示瞬间」的失焦。
    pub fn mark_pop_shown(&self) {
        *self.pop_shown_at.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }

    pub fn clear_pop_shown(&self) {
        *self.pop_shown_at.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// 弹层是否刚显示不久（此时失焦不算点击外部）。
    pub fn pop_recently_shown(&self) -> bool {
        self.pop_shown_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map(|at| at.elapsed() < Duration::from_millis(800))
            .unwrap_or(false)
    }

    pub fn pop_align(&self) -> String {
        self.pop_align.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }


    pub fn bar_size(&self) -> (f64, f64) {
        *self.bar_size.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_bar_size(&self, width: f64, height: f64) {
        let mut size = self.bar_size.lock().unwrap_or_else(|e| e.into_inner());
        *size = (width.max(140.0), height.max(14.0));
    }

    pub fn pop_size(&self) -> (f64, f64) {
        *self.pop_size.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_pop_size(&self, width: f64, height: f64) {
        let mut size = self.pop_size.lock().unwrap_or_else(|e| e.into_inner());
        *size = (width.max(160.0), height.max(80.0));
    }

    pub fn toast_size(&self) -> (f64, f64) {
        *self.toast_size.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_toast_size(&self, width: f64, height: f64) {
        let mut size = self.toast_size.lock().unwrap_or_else(|e| e.into_inner());
        *size = (width.clamp(120.0, 640.0), height.clamp(28.0, 200.0));
    }

    pub fn remember(&self, x: i32, y: i32) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.bar_x = Some(x);
        state.bar_y = Some(y);
    }

    pub fn snapshot(&self) -> WindowState {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn persist_now(&self, store: &Store) {
        let state = self.snapshot();
        if let Err(error) = store.write(WINDOW_STATE_FILE, &state) {
            crate::log_warn!("保存窗口位置失败：{error}");
        }
    }
}

/// 弹层首次显示时的默认尺寸（逻辑像素）；随后由界面量出真实尺寸再修正。

/// 当前图标条所在显示器的工作区（物理像素，已排除任务栏）。
pub fn work_area(app: &AppHandle) -> Option<(i32, i32, u32, u32)> {
    let window = app.get_webview_window(BAR)?;
    let monitor = window
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten())?;
    let area = monitor.work_area();
    Some((
        area.position.x,
        area.position.y,
        area.size.width,
        area.size.height,
    ))
}

fn default_bar_position(app: &AppHandle, width: f64, height: f64) -> (i32, i32) {
    let scale = scale_of(app);
    let width = (width * scale).round() as i32;
    let _ = height;
    let anchor = app.try_state::<crate::AppState>()
        .map(|state| state.ui.snapshot().island_anchor)
        .unwrap_or_else(|| "center".into());
    match work_area(app) {
        Some((x, y, available, _)) => {
            let offset = match anchor.as_str() {
                "left" => 16,
                "right" => (available as i32 - width - 16).max(0),
                _ => (available as i32 - width).max(0) / 2,
            };
            (x + offset, y)
        }
        None => (200, 0),
    }
}

fn scale_of(app: &AppHandle) -> f64 {
    app.get_webview_window(BAR)
        .and_then(|w| w.scale_factor().ok())
        .unwrap_or(1.0)
}


/// 恢复或计算图标条位置，并让窗口尺寸等于图标条尺寸。
pub fn place_bar(app: &AppHandle, handle: &WindowsHandle) {
    let Some(window) = app.get_webview_window(BAR) else {
        return;
    };
    let (width, height) = handle.bar_size();
    let (x, y) = default_bar_position(app, width, height);
    let scale = window.scale_factor().unwrap_or(1.0);
    let target = PhysicalSize::new((width * scale).round() as u32, (height * scale).round() as u32);
    if window.inner_size().ok() == Some(target) &&
        window.outer_position().ok() == Some(PhysicalPosition::new(x, y)) { return; }

    // 位置和大小一次提交；两次系统调用会先移动旧尺寸，造成边缘跳动。
    #[cfg(windows)]
    if let Ok(native) = window.hwnd() {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};
        let hwnd = HWND(native.0);
        let moved = unsafe { SetWindowPos(hwnd, None, x, y, target.width as i32, target.height as i32,
            SWP_NOACTIVATE | SWP_NOZORDER) };
        if moved.is_ok() {
            // 收起仍保留完整热区。展开后让透明圆角外的像素不截获鼠标。
            unsafe {
                if height <= 15.0 { SetWindowRgn(hwnd, None, true); }
                else {
                    let curve = ((height * 0.36).clamp(10.0, 28.0) * scale).round() as i32;
                    let region = CreateRoundRectRgn(0, 0, target.width as i32 + 1, target.height as i32 + 1, curve, curve);
                    if !region.0.is_null() && SetWindowRgn(hwnd, Some(region), true) == 0 {
                        let _ = DeleteObject(region.into());
                    }
                }
            }
            return;
        }
    }

    let _ = window.set_size(tauri::Size::Physical(target));
    let _ = window.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)));
}

/// 把弹层放到图标条正上方（上方放不下时放到下方），并水平对齐图标条。
pub fn place_pop(app: &AppHandle, handle: &WindowsHandle) {
    let Some(pop) = app.get_webview_window(POP) else {
        return;
    };
    let Some(bar) = app.get_webview_window(BAR) else {
        return;
    };
    let Ok(bar_pos) = bar.outer_position() else {
        return;
    };
    let scale = bar.scale_factor().unwrap_or(1.0);
    let (bar_w, bar_h) = handle.bar_size();
    let (pop_w, pop_h) = handle.pop_size();
    let bar_w = (bar_w * scale).round() as i32;
    let bar_h = (bar_h * scale).round() as i32;
    let pop_w = (pop_w * scale).round() as i32;
    let pop_h = (pop_h * scale).round() as i32;
    let gap = (POP_GAP * scale).round() as i32;

    let mut x = match handle.pop_align().as_str() {
        // 分类菜单贴图标条左侧，溢出网格贴右侧，其余居中
        "left" => bar_pos.x + (POP_GAP * scale).round() as i32,
        "right" => bar_pos.x + bar_w - pop_w - (POP_GAP * scale).round() as i32,
        _ => bar_pos.x + (bar_w - pop_w) / 2,
    };
    let mut y = bar_pos.y - pop_h - gap;

    if let Some((ax, ay, aw, ah)) = work_area(app) {
        if y < ay {
            let below = bar_pos.y + bar_h + gap;
            y = if below + pop_h <= ay + ah as i32 {
                below
            } else {
                (ay + ah as i32 - pop_h).max(ay)
            };
        }
        x = x.clamp(ax, (ax + aw as i32 - pop_w).max(ax));
        y = y.clamp(ay, (ay + ah as i32 - pop_h).max(ay));
    }

    let _ = pop.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)));
}

pub fn ensure_pop(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(POP) {
        return Ok(window);
    }
    WebviewWindowBuilder::new(app, POP, WebviewUrl::App("index.html".into()))
        .title("DesktopHub Lite 弹层")
        .inner_size(384.0, 480.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|error| format!("创建弹层窗口失败：{error}"))
}

pub fn ensure_settings(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        return Ok(window);
    }
    let window = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("index.html".into()))
        .title("DesktopHub Lite 设置")
        .inner_size(SETTINGS_SIZE.0, SETTINGS_SIZE.1)
        .decorations(false)
        .transparent(true)
        .resizable(false)
        .shadow(true)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|error| format!("创建设置窗口失败：{error}"))?;
    center_window(&window, SETTINGS_SIZE);
    Ok(window)
}

pub fn ensure_translation(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(TRANSLATION) { return Ok(window); }
    WebviewWindowBuilder::new(app, TRANSLATION, WebviewUrl::App("index.html".into()))
        .title("DesktopHub Lite 翻译")
        .inner_size(520.0, 620.0)
        .min_inner_size(360.0, 420.0)
        .visible(false)
        .build().map_err(|error| format!("无法打开翻译窗口：{error}"))
}

/// 轻提示窗口：不抢焦点、不进任务栏、尺寸随内容变化。
pub fn ensure_toast(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(window) = app.get_webview_window(TOAST) {
        return Ok(window);
    }
    let (width, height) = (360.0, 40.0);
    let window = WebviewWindowBuilder::new(app, TOAST, WebviewUrl::App("index.html".into()))
        .title("DesktopHub Lite 提示")
        .inner_size(width, height)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|error| format!("创建提示窗口失败：{error}"))?;
    // 提示是纯展示，绝不能吃掉鼠标：否则它盖住的桌面区域会点不动。
    let _ = window.set_ignore_cursor_events(true);
    Ok(window)
}

/// 把提示窗口放到「弹层上方」或「图标条上方」，水平居中。
pub fn place_toast(app: &AppHandle, handle: &WindowsHandle) {
    let Some(toast) = app.get_webview_window(TOAST) else {
        return;
    };
    let Some(bar) = app.get_webview_window(BAR) else {
        return;
    };
    let Ok(bar_pos) = bar.outer_position() else {
        return;
    };
    let scale = bar.scale_factor().unwrap_or(1.0);
    let (bar_w, _bar_h) = handle.bar_size();
    let (toast_w, toast_h) = handle.toast_size();
    let bar_w = (bar_w * scale).round() as i32;
    let toast_w = (toast_w * scale).round() as i32;
    let toast_h = (toast_h * scale).round() as i32;
    let gap = (POP_GAP * scale).round() as i32;

    // 有弹层时叠在弹层上方，避免提示被弹层盖住。
    let anchor_top = app
        .get_webview_window(POP)
        .filter(|pop| pop.is_visible().unwrap_or(false))
        .and_then(|pop| pop.outer_position().ok())
        .map(|pos| pos.y)
        .unwrap_or(bar_pos.y);

    let mut x = bar_pos.x + (bar_w - toast_w) / 2;
    let mut y = anchor_top - toast_h - gap;

    if let Some((ax, ay, aw, ah)) = work_area(app) {
        if y < ay {
            y = anchor_top + gap;
        }
        x = x.clamp(ax, (ax + aw as i32 - toast_w).max(ax));
        y = y.clamp(ay, (ay + ah as i32 - toast_h).max(ay));
    }

    let _ = toast.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)));
}

fn center_window(window: &WebviewWindow, size: (f64, f64)) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let width = (size.0 * scale).round() as i32;
    let height = (size.1 * scale).round() as i32;
    let x = area.position.x + (area.size.width as i32 - width).max(0) / 2;
    let y = area.position.y + (area.size.height as i32 - height).max(0) / 2;
    let _ = window.set_position(tauri::Position::Physical(PhysicalPosition::new(x, y)));
}
