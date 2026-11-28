//! 窗口创建、尺寸与位置。
//!
//! 多窗口是点击穿透的前提：透明窗口的透明区域同样会拦截鼠标，
//! 所以图标条窗口必须刚好等于图标条本身的大小；
//! 剪贴板 / 翻译 / 分类 / 溢出这些弹层各放在独立的 `pop` 窗口里，
//! 设置放在独立的 `settings` 窗口里。
//! 任何窗口都不做「全屏透明覆盖」，否则桌面会被整块挡住。

use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};
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

/// 灵动岛主面板和胶囊导航的可见轮廓（逻辑像素）。空列表沿用旧版热区。
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct BarRegion {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

pub struct WindowsHandle {
    state: Mutex<WindowState>,
    /// 用户本轮拖动后，开合沿用当前窗口中心，不再按顶部停靠重新定位。
    bar_free_position: AtomicBool,
    bar_size: Mutex<(f64, f64)>,
    bar_regions: Mutex<Vec<BarRegion>>,
    bar_regions_changed: Mutex<bool>,
    bar_regions_expanded: Mutex<bool>,
    pop_size: Mutex<(f64, f64)>,
    /// 弹层相对图标条的水平对齐：center / left / right
    pop_align: Mutex<String>,
    toast_size: Mutex<(f64, f64)>,
    /// 弹层窗口显示的时刻。刚显示时系统会给它一次焦点，
    /// 之后紧接着的失焦并不代表用户点了别处，需要忽略这一小段。
    pop_shown_at: Mutex<Option<Instant>>,
    /// 灵动岛最近一次展开的时刻，用于忽略展开瞬间系统偶发的焦点抖动。
    bar_expanded_at: Mutex<Option<Instant>>,
    /// 最近一次「因失焦自动收起」的时刻与弹层名。
    /// 点图标条上同一个按钮想收起时，焦点先离开弹层触发自动收起，
    /// 紧接着这次点击又会被当成「打开」，需要用这个记录消掉重复打开。
    last_auto_close: Mutex<Option<(Instant, String)>>,
}

impl WindowsHandle {
    pub fn new(state: WindowState) -> Self {
        Self {
            state: Mutex::new(state),
            bar_free_position: AtomicBool::new(false),
            bar_size: Mutex::new((140.0, 14.0)),
            bar_regions: Mutex::new(Vec::new()),
            bar_regions_changed: Mutex::new(false),
            bar_regions_expanded: Mutex::new(false),
            pop_size: Mutex::new((384.0, 480.0)),
            pop_align: Mutex::new("center".to_string()),
            toast_size: Mutex::new((360.0, 40.0)),
            pop_shown_at: Mutex::new(None),
            bar_expanded_at: Mutex::new(None),
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

    /// 记录灵动岛主窗刚展开的时刻。
    pub fn mark_bar_expanded(&self) {
        *self.bar_expanded_at.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }

    /// 清除灵动岛主窗展开时刻记录。
    pub fn clear_bar_expanded(&self) {
        *self.bar_expanded_at.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    /// 灵动岛是否刚展开不久（用于忽略展开瞬间系统焦点切换抖动）。
    pub fn bar_recently_expanded(&self) -> bool {
        self.bar_expanded_at
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map(|at| at.elapsed() < Duration::from_millis(320))
            .unwrap_or(false)
    }

    pub fn pop_align(&self) -> String {
        self.pop_align.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }


    pub fn bar_size(&self) -> (f64, f64) {
        *self.bar_size.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_bar_free_position(&self, free: bool) -> bool {
        self.bar_free_position.swap(free, Ordering::Relaxed)
    }

    pub fn set_bar_size(&self, width: f64, height: f64) {
        let mut size = self.bar_size.lock().unwrap_or_else(|e| e.into_inner());
        *size = (width.max(140.0), height.max(14.0));
    }

    pub fn set_bar_regions(&self, regions: Vec<BarRegion>) {
        let mut current = self.bar_regions.lock().unwrap_or_else(|e| e.into_inner());
        if *current != regions {
            let area = |list: &[BarRegion]| -> f64 {
                list.iter().map(|r| r.width * r.height).sum()
            };
            let expanded = regions.is_empty() || area(&regions) > area(&current) + 1.0;
            *current = regions;
            *self.bar_regions_changed.lock().unwrap_or_else(|e| e.into_inner()) = true;
            if expanded {
                *self.bar_regions_expanded.lock().unwrap_or_else(|e| e.into_inner()) = true;
            }
        }
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
    let (anchor, island_shape) = app.try_state::<crate::AppState>()
        .map(|state| {
            let snap = state.ui.snapshot();
            (snap.island_anchor, snap.island_shape)
        })
        .unwrap_or_else(|| ("center".into(), crate::state::ISLAND_SHAPE_FLOATING.into()));
    // 贴顶刘海（notch）与贴顶分层（split）靠左/靠右停靠时贴紧屏边（0 边距），悬浮胶囊保持 16px 边距。
    let edge_margin = if matches!(island_shape.as_str(), "notch" | "split") { 0 } else { 16 };
    match work_area(app) {
        Some((x, y, available, _)) => {
            let offset = match anchor.as_str() {
                "left" => edge_margin,
                "right" => (available as i32 - width - edge_margin).max(0),
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


#[cfg(windows)]
fn bar_style(style: u32) -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::{
        WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU, WS_THICKFRAME,
    };
    style & !(WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX).0
}

// 仅给灵动岛最外层 HWND 安装。在首次显示前处理 Tao 后续的样式重写，
// 避免系统先把渐变标题画进透明背景，等 Focused 或下一次布局才清样式。
#[cfg(windows)]
pub fn install_bar_frame_guard(app: &AppHandle) {
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        GWL_STYLE, STYLESTRUCT, WM_ERASEBKGND, WM_NCACTIVATE, WM_NCDESTROY, WM_NCPAINT,
        WM_STYLECHANGING,
    };
    unsafe extern "system" fn frame_proc(
        hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, id: usize, _: usize,
    ) -> LRESULT {
        match msg {
            WM_STYLECHANGING if wparam.0 as i32 == GWL_STYLE.0 && lparam.0 != 0 => {
                let styles = &mut *(lparam.0 as *mut STYLESTRUCT);
                styles.styleNew = bar_style(styles.styleNew);
            }
            // 阻止系统在透明窗口扩容或重设 HRGN 时用 GDI 背景画刷擦除客户区，消除扩容瞬间的闪黑块。
            WM_ERASEBKGND => return LRESULT(1),
            WM_NCPAINT => return LRESULT(0),
            // 让 Tao 正常更新激活与焦点状态，同时要求 DefWindowProc 跳过标题重绘。
            WM_NCACTIVATE => return DefSubclassProc(hwnd, msg, wparam, LPARAM(-1)),
            WM_NCDESTROY => { let _ = RemoveWindowSubclass(hwnd, Some(frame_proc), id); }
            _ => {}
        }
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }
    let Some(window) = app.get_webview_window(BAR) else { return; };
    let Ok(native) = window.hwnd() else { return; };
    if !unsafe { SetWindowSubclass(HWND(native.0), Some(frame_proc), 1, 0) }.as_bool() {
        crate::log_warn!("未能安装灵动岛无边框消息处理");
    }
    enforce_bar_frameless(app);
}

/// 清除 HWND 在应用安装消息处理之前已有的系统装饰。
#[cfg(windows)]
pub fn enforce_bar_frameless(app: &AppHandle) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_STYLE,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    };
    let Some(window) = app.get_webview_window(BAR) else { return; };
    let Ok(native) = window.hwnd() else { return; };
    let hwnd = HWND(native.0);
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let frameless = bar_style(style as u32) as i32 as isize;
        // 没有变化就不触发系统重绘；保留透明、置顶、拖放和窗口可见性等其他样式。
        if frameless as u32 == style as u32 { return; }
        SetWindowLongPtrW(hwnd, GWL_STYLE, frameless);
        if GetWindowLongPtrW(hwnd, GWL_STYLE) as u32 != frameless as u32 {
            crate::log_warn!("清除灵动岛原生标题栏样式失败");
            return;
        }
        if let Err(error) = SetWindowPos(hwnd, None, 0, 0, 0, 0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER) {
            crate::log_warn!("刷新灵动岛无边框样式失败：{error}");
        }
    }
}

#[cfg(windows)]
pub fn restore_bar_region(app: &AppHandle, handle: &WindowsHandle) {
    *handle.bar_regions_changed.lock().unwrap_or_else(|e| e.into_inner()) = true;
    place_bar_inner(app, handle, true);
}

/// 恢复或计算图标条位置，并让窗口尺寸等于图标条尺寸。
pub fn place_bar(app: &AppHandle, handle: &WindowsHandle) {
    place_bar_inner(app, handle, false);
}

fn place_bar_inner(app: &AppHandle, handle: &WindowsHandle, force_redraw: bool) {
    #[cfg(windows)]
    enforce_bar_frameless(app);
    let Some(window) = app.get_webview_window(BAR) else {
        return;
    };
    let (width, height) = handle.bar_size();
    let (mut x, mut y) = default_bar_position(app, width, height);
    let scale = window.scale_factor().unwrap_or(1.0);
    let target = PhysicalSize::new((width * scale).round() as u32, (height * scale).round() as u32);
    if handle.bar_free_position.load(Ordering::Relaxed) {
        if let (Ok(position), Ok(previous)) = (window.outer_position(), window.inner_size()) {
            x = position.x + (previous.width as i32 - target.width as i32) / 2;
            y = position.y;
            if let Some((ax, ay, aw, ah)) = work_area(app) {
                x = x.clamp(ax, ax + (aw as i32 - target.width as i32).max(0));
                y = y.clamp(ay, ay + (ah as i32 - target.height as i32).max(0));
            }
        }
    }
    let previous_size = window.inner_size().ok();
    let growing = previous_size
        .map(|prev| target.width > prev.width || target.height > prev.height)
        .unwrap_or(false);
    let regions_changed = std::mem::take(&mut *handle.bar_regions_changed.lock().unwrap_or_else(|e| e.into_inner()));
    let regions_expanded = std::mem::take(&mut *handle.bar_regions_expanded.lock().unwrap_or_else(|e| e.into_inner()));
    let bounds_changed = previous_size != Some(target) ||
        window.outer_position().ok() != Some(PhysicalPosition::new(x, y));
    if !regions_changed && !bounds_changed { return; }

    // 位置和大小一次提交；两次系统调用会先移动旧尺寸，造成边缘跳动。
    #[cfg(windows)]
    if let Ok(native) = window.hwnd() {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Gdi::{CombineRgn, CreateRectRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, RGN_OR};
        use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOREDRAW, SWP_NOZORDER};
        let hwnd = HWND(native.0);
        // 扩容时延迟至 SetWindowRgn 落地再重绘，避免新扩出区域在裁切更新前暴露未绘制底色；
        // 缩容时保留正常重绘，保证下方刚露出的桌面区域即时刷新。
        let swp_flags = if growing {
            SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOREDRAW
        } else {
            SWP_NOACTIVATE | SWP_NOZORDER
        };
        // CSS 动画期间只改变轮廓，不重复发送 WM_SIZE / WM_WINDOWPOSCHANGED。
        let moved = if bounds_changed {
            unsafe { SetWindowPos(hwnd, None, x, y, target.width as i32, target.height as i32, swp_flags) }
        } else { Ok(()) };
        if moved.is_ok() {
            // 窗口尺寸变化、隐藏恢复或 HRGN 扩大时同步刷新（兼顾无动画模式即时显示）；
            // 仅收缩 HRGN 释放透明穿透区时传 false，不触发 WM_WINDOWPOSCHANGED，消除收缩尾帧闪烁。
            let redraw_rgn = bounds_changed || force_redraw || regions_expanded;
            // 收起仍保留完整热区。展开后让透明圆角外的像素不截获鼠标。
            unsafe {
                let shapes = handle.bar_regions.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if height <= 15.0 { SetWindowRgn(hwnd, None, redraw_rgn); }
                else if !shapes.is_empty() {
                    let combined = CreateRectRgn(0, 0, 0, 0);
                    if !combined.0.is_null() {
                        let mut valid = true;
                        for shape in shapes {
                            let px = |value: f64| (value * scale).round() as i32;
                            // HRGN 是整数像素硬裁剪；留出两物理像素，让 WebView 的
                            // 半透明圆角边缘完整参与桌面合成，不用 HRGN 描绘可见轮廓。
                            let fringe = if shape.radius > 0.0 { 2 } else { 0 };
                            let part = CreateRoundRectRgn(px(shape.x) - fringe, px(shape.y) - fringe,
                                px(shape.x + shape.width) + 1 + fringe, px(shape.y + shape.height) + 1 + fringe,
                                px(shape.radius * 2.0) + fringe * 2, px(shape.radius * 2.0) + fringe * 2);
                            if part.0.is_null() { valid = false; break; }
                            let result = CombineRgn(Some(combined), Some(combined), Some(part), RGN_OR);
                            let _ = DeleteObject(part.into());
                            if result.0 == 0 { valid = false; break; }
                        }
                        // SetWindowRgn 成功后由系统接管 region，失败时仍由本程序释放。
                        if !valid || SetWindowRgn(hwnd, Some(combined), redraw_rgn) == 0 {
                            let _ = DeleteObject(combined.into());
                            crate::log_warn!("灵动岛可见区域更新失败");
                        }
                    }
                }
                else {
                    let curve = ((height * 0.36).clamp(10.0, 28.0) * scale).round() as i32;
                    let region = CreateRoundRectRgn(0, 0, target.width as i32 + 1, target.height as i32 + 1, curve, curve);
                    if !region.0.is_null() && SetWindowRgn(hwnd, Some(region), redraw_rgn) == 0 {
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
        .disable_drag_drop_handler()
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
        .focused(false)
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
