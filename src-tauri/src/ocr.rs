//! 屏幕截图捕获、剪贴板图片识别与系统离线 OCR 文字识别。
//!
//! 设计约束：
//! - 截屏与 OCR 全程在本地内存中完成，绝不向磁盘写入原始截图，也绝不把识别正文写入日志。
//! - 纯图像预处理与结果清洗逻辑位于模块上半部分，便于无桌面依赖的核心测试验证。

use serde::{Deserialize, Serialize};

pub const MAX_OCR_IMAGE_DIM: u32 = 2500;
pub const MIN_CROP_SIZE: u32 = 4;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSessionInfo {
    pub session_id: u64,
    pub data_uri: String,
    pub width: u32,
    pub height: u32,
    pub screen_x: i32,
    pub screen_y: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OcrResultPayload {
    pub text: String,
    pub line_count: usize,
    pub char_count: usize,
    pub language: String,
    pub elapsed_ms: u64,
    pub source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CropSelection {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// 将前端坐标夹紧到图像边界内，并拒绝过小的选区。
pub fn normalize_crop_rect(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    img_w: u32,
    img_h: u32,
) -> Result<(u32, u32, u32, u32), String> {
    if img_w == 0 || img_h == 0 {
        return Err("截图尺寸无效，请重试".into());
    }
    if width <= 0 || height <= 0 {
        return Err("选区过小，请重新框选文字区域".into());
    }
    let max_w = img_w as i64;
    let max_h = img_h as i64;
    let x0 = (x as i64).clamp(0, max_w);
    let y0 = (y as i64).clamp(0, max_h);
    let x1 = ((x as i64).saturating_add(width as i64)).clamp(0, max_w);
    let y1 = ((y as i64).saturating_add(height as i64)).clamp(0, max_h);
    let cw = (x1 - x0).max(0) as u32;
    let ch = (y1 - y0).max(0) as u32;
    if cw < MIN_CROP_SIZE || ch < MIN_CROP_SIZE {
        return Err("选区过小，请重新框选文字区域".into());
    }
    Ok((x0 as u32, y0 as u32, cw, ch))
}

/// 将归一化比例框选 [0, 1] 转换为物理像素裁剪区域，免疫混合 DPI 缩放误差。
pub fn selection_to_pixel_rect(
    selection: &CropSelection,
    img_w: u32,
    img_h: u32,
) -> Result<(u32, u32, u32, u32), String> {
    if !selection.x.is_finite()
        || !selection.y.is_finite()
        || !selection.width.is_finite()
        || !selection.height.is_finite()
    {
        return Err("选区坐标无效".into());
    }
    let rx = selection.x.clamp(0.0, 1.0);
    let ry = selection.y.clamp(0.0, 1.0);
    let rw = selection.width.max(0.0).min(1.0 - rx);
    let rh = selection.height.max(0.0).min(1.0 - ry);
    let px = (rx * img_w as f64).floor() as i32;
    let py = (ry * img_h as f64).floor() as i32;
    let pw = (rw * img_w as f64).round() as i32;
    let ph = (rh * img_h as f64).round() as i32;
    normalize_crop_rect(px, py, pw, ph, img_w, img_h)
}

/// 从整屏 BGRA8 缓冲中安全裁出指定矩形区域的像素字节。
pub fn crop_bgra(
    src: &[u8],
    img_w: u32,
    img_h: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Result<Vec<u8>, String> {
    let expected = (img_w as usize)
        .checked_mul(img_h as usize)
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| "图像尺寸超出处理范围".to_string())?;
    if src.len() < expected {
        return Err("图像像素数据不完整".into());
    }
    if x.saturating_add(w) > img_w || y.saturating_add(h) > img_h || w == 0 || h == 0 {
        return Err("裁剪区域超出屏幕图像范围".into());
    }
    let src_stride = (img_w as usize) * 4;
    let row_bytes = (w as usize) * 4;
    let mut out = vec![0u8; row_bytes * (h as usize)];
    for row in 0..(h as usize) {
        let src_start = ((y as usize) + row) * src_stride + (x as usize) * 4;
        let dst_start = row * row_bytes;
        out[dst_start..dst_start + row_bytes]
            .copy_from_slice(&src[src_start..src_start + row_bytes]);
    }
    Ok(out)
}

fn resize_bgra_bilinear(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Vec<u8> {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return Vec::new();
    }
    let mut out = vec![0u8; (dst_w as usize) * (dst_h as usize) * 4];
    for dy in 0..dst_h {
        let sy = if dst_h == 1 {
            0.0
        } else {
            (dy as f64 + 0.5) * (src_h as f64) / (dst_h as f64) - 0.5
        }
        .clamp(0.0, (src_h - 1) as f64);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(src_h - 1);
        let fy = sy - (y0 as f64);

        for dx in 0..dst_w {
            let sx = if dst_w == 1 {
                0.0
            } else {
                (dx as f64 + 0.5) * (src_w as f64) / (dst_w as f64) - 0.5
            }
            .clamp(0.0, (src_w - 1) as f64);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(src_w - 1);
            let fx = sx - (x0 as f64);

            let idx = |x: u32, y: u32| -> usize { ((y as usize) * (src_w as usize) + (x as usize)) * 4 };
            let i00 = idx(x0, y0);
            let i10 = idx(x1, y0);
            let i01 = idx(x0, y1);
            let i11 = idx(x1, y1);
            let o = ((dy as usize) * (dst_w as usize) + (dx as usize)) * 4;

            for c in 0..4 {
                let top = (src[i00 + c] as f64) * (1.0 - fx) + (src[i10 + c] as f64) * fx;
                let bot = (src[i01 + c] as f64) * (1.0 - fx) + (src[i11 + c] as f64) * fx;
                let val = (top * (1.0 - fy) + bot * fy).round().clamp(0.0, 255.0) as u8;
                out[o + c] = val;
            }
        }
    }
    out
}

/// 预处理待识别位图：过小选区适度放大提升小字识别率，超大图片等比缩至 WinRT 限制内。
pub fn prepare_bgra_for_ocr(bgra: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    if w == 0 || h == 0 {
        return (Vec::new(), 0, 0);
    }
    if w > MAX_OCR_IMAGE_DIM || h > MAX_OCR_IMAGE_DIM {
        let ratio = (MAX_OCR_IMAGE_DIM as f64 / w as f64).min(MAX_OCR_IMAGE_DIM as f64 / h as f64);
        let nw = ((w as f64 * ratio).floor() as u32).clamp(1, MAX_OCR_IMAGE_DIM);
        let nh = ((h as f64 * ratio).floor() as u32).clamp(1, MAX_OCR_IMAGE_DIM);
        return (resize_bgra_bilinear(bgra, w, h, nw, nh), nw, nh);
    }
    if (w < 240 || h < 72)
        && w.saturating_mul(2) <= MAX_OCR_IMAGE_DIM
        && h.saturating_mul(2) <= MAX_OCR_IMAGE_DIM
    {
        let nw = w * 2;
        let nh = h * 2;
        return (resize_bgra_bilinear(bgra, w, h, nw, nh), nw, nh);
    }
    (bgra.to_vec(), w, h)
}

fn is_cjk_ideograph(ch: char) -> bool {
    matches!(
        ch as u32,
        0x4E00..=0x9FFF
            | 0x3400..=0x4DBF
            | 0xF900..=0xFAFF
            | 0x3040..=0x30FF
            | 0xAC00..=0xD7AF
    )
}

fn is_cjk_or_fullwidth_punct(ch: char) -> bool {
    matches!(
        ch as u32,
        0x3000..=0x303F | 0xFF01..=0xFF0F | 0xFF1A..=0xFF20 | 0xFF3B..=0xFF40 | 0xFF5B..=0xFF65
    ) || matches!(ch, '·' | '—' | '…' | '‘' | '’' | '“' | '”')
}

/// 清理单行 Windows OCR 输出：去掉中文字符间多余空格，保留英文单词与数字间距。
pub fn clean_ocr_line(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut prev: Option<char> = None;
    let mut pending_space = false;

    for ch in raw.chars() {
        if ch.is_whitespace() {
            if prev.is_some() {
                pending_space = true;
            }
            continue;
        }
        if pending_space {
            if let Some(p) = prev {
                let drop_space = (is_cjk_ideograph(p) && is_cjk_ideograph(ch))
                    || is_cjk_or_fullwidth_punct(p)
                    || is_cjk_or_fullwidth_punct(ch);
                if !drop_space {
                    out.push(' ');
                }
            }
            pending_space = false;
        }
        out.push(ch);
        prev = Some(ch);
    }
    out
}

/// 整理全部 OCR 识别行文本，过滤空行并规范换行。
pub fn format_ocr_lines(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| clean_ocr_line(line))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

// === TAURI_COMMANDS ===

use crate::AppState;
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{
    codecs::png::{CompressionType, FilterType, PngEncoder},
    ExtendedColorType, ImageEncoder,
};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

pub const CAPTURE_WINDOW: &str = "capture";
pub const OCR_RESULT_EVENT: &str = "island-ocr-result";
pub const OCR_ERROR_EVENT: &str = "island-ocr-error";

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

struct ActiveCapture {
    session_id: u64,
    bgra: Vec<u8>,
    width: u32,
    height: u32,
    screen_x: i32,
    screen_y: i32,
    data_uri: String,
    bar_was_visible: bool,
    bar_was_expanded: bool,
}

#[derive(Default)]
pub struct OcrState {
    active: Mutex<Option<ActiveCapture>>,
}

#[cfg(windows)]
fn capture_virtual_screen() -> Result<(Vec<u8>, u32, u32, i32, i32, String), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
        DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };

    struct ScreenDc(HDC);
    impl Drop for ScreenDc {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                unsafe {
                    ReleaseDC(Some(HWND::default()), self.0);
                }
            }
        }
    }
    struct MemDc(HDC);
    impl Drop for MemDc {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                unsafe {
                    let _ = DeleteDC(self.0);
                }
            }
        }
    }
    struct OwnedBitmap(HBITMAP);
    impl Drop for OwnedBitmap {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                unsafe {
                    let _ = DeleteObject(HGDIOBJ(self.0.0));
                }
            }
        }
    }

    let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if vw <= 0 || vh <= 0 {
        return Err("无法获取屏幕尺寸".into());
    }
    let width = vw as u32;
    let height = vh as u32;

    let screen_dc = ScreenDc(unsafe { GetDC(None) });
    if screen_dc.0.is_invalid() {
        return Err("无法访问屏幕显示设备".into());
    }
    let mem_dc = MemDc(unsafe { CreateCompatibleDC(Some(screen_dc.0)) });
    if mem_dc.0.is_invalid() {
        return Err("无法创建内存绘图上下文".into());
    }
    let bitmap = OwnedBitmap(unsafe { CreateCompatibleBitmap(screen_dc.0, vw, vh) });
    if bitmap.0.is_invalid() {
        return Err("无法分配屏幕截图缓冲".into());
    }

    let old_obj = unsafe { SelectObject(mem_dc.0, HGDIOBJ(bitmap.0.0)) };
    let blt_res = unsafe {
        BitBlt(
            mem_dc.0,
            0,
            0,
            vw,
            vh,
            Some(screen_dc.0),
            vx,
            vy,
            SRCCOPY | CAPTUREBLT,
        )
    };
    unsafe {
        SelectObject(mem_dc.0, old_obj);
    }
    if blt_res.is_err() {
        return Err("捕获屏幕图像失败".into());
    }

    let mut bmi = BITMAPINFO::default();
    bmi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: vw,
        biHeight: -vh,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let pixel_bytes = (width as usize) * (height as usize) * 4;
    let mut bgra = vec![0u8; pixel_bytes];
    let lines = unsafe {
        GetDIBits(
            mem_dc.0,
            bitmap.0,
            0,
            height,
            Some(bgra.as_mut_ptr().cast()),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    if lines == 0 {
        return Err("无法读取屏幕截图像素".into());
    }

    let mut rgba = vec![0u8; pixel_bytes];
    for (src, dst) in bgra.chunks_exact_mut(4).zip(rgba.chunks_exact_mut(4)) {
        src[3] = 255;
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = 255;
    }

    let mut png_bytes = Vec::with_capacity(pixel_bytes / 8);
    let encoder =
        PngEncoder::new_with_quality(&mut png_bytes, CompressionType::Fast, FilterType::NoFilter);
    encoder
        .write_image(&rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|_| "屏幕截图编码失败")?;

    let data_uri = format!("data:image/png;base64,{}", STANDARD.encode(&png_bytes));
    Ok((bgra, width, height, vx, vy, data_uri))
}

#[cfg(not(windows))]
fn capture_virtual_screen() -> Result<(Vec<u8>, u32, u32, i32, i32, String), String> {
    Err("当前系统暂不支持屏幕截图识字".into())
}

#[cfg(windows)]
fn recognize_bgra_windows(bgra: &[u8], width: u32, height: u32) -> Result<(String, usize, String), String> {
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

    struct WinRtGuard(bool);
    impl Drop for WinRtGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { RoUninitialize() };
            }
        }
    }

    let init = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let need_uninit = match init {
        Ok(()) => true,
        Err(err) if err.code() == RPC_E_CHANGED_MODE => false,
        Err(err) => return Err(format!("无法初始化系统文字识别运行环境（{}）", err.code())),
    };
    let _guard = WinRtGuard(need_uninit);

    let engine = OcrEngine::TryCreateFromUserProfileLanguages().or_else(|_| {
        let langs = OcrEngine::AvailableRecognizerLanguages()
            .map_err(|_| "系统未提供可用的文字识别语言".to_string())?;
        let mut chosen = None;
        for lang in &langs {
            if let Ok(tag) = lang.LanguageTag() {
                if tag.to_string().to_ascii_lowercase().starts_with("zh") {
                    chosen = Some(lang);
                    break;
                }
            }
            if chosen.is_none() {
                chosen = Some(lang);
            }
        }
        let target = chosen.ok_or_else(|| {
            "系统未安装 Windows OCR 语言包，请在 Windows「设置 → 时间和语言 → 语言和区域」中添加中文语言包后重试".to_string()
        })?;
        OcrEngine::TryCreateFromLanguage(&target).map_err(|_| {
            "系统未安装 Windows OCR 语言包，请在 Windows「设置 → 时间和语言 → 语言和区域」中添加中文语言包后重试".to_string()
        })
    })?;

    let lang_tag = engine
        .RecognizerLanguage()
        .and_then(|l| l.LanguageTag())
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "unknown".into());

    let (prepared, pw, ph) = prepare_bgra_for_ocr(bgra, width, height);
    if pw == 0 || ph == 0 {
        return Err("图片尺寸无效，无法识别".into());
    }

    let buffer = CryptographicBuffer::CreateFromByteArray(&prepared)
        .map_err(|_| "无法装载待识别像素缓冲")?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        pw as i32,
        ph as i32,
    )
    .map_err(|_| "无法创建系统文字识别位图")?;

    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(|_| "无法发起系统文字识别任务")?
        .join()
        .map_err(|_| "系统文字识别失败")?;

    let ocr_lines = result
        .Lines()
        .map_err(|_| "无法读取文字识别行结果")?;
    let mut raw_lines = Vec::new();
    for line in &ocr_lines {
        if let Ok(text) = line.Text() {
            raw_lines.push(text.to_string());
        }
    }

    let formatted = format_ocr_lines(&raw_lines);
    let line_count = formatted
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count();
    Ok((formatted, line_count, lang_tag))
}

#[cfg(not(windows))]
fn recognize_bgra_windows(
    _bgra: &[u8],
    _width: u32,
    _height: u32,
) -> Result<(String, usize, String), String> {
    Err("当前系统暂不支持离线文字识别".into())
}

fn restore_bar_after_capture(app: &AppHandle, show_result: bool, was_visible: bool, was_expanded: bool) {
    if show_result {
        let _ = crate::set_island_expanded(app.clone(), true, Some(true));
        crate::show_window(app, crate::windows::BAR);
    } else if was_visible {
        if was_expanded {
            let _ = crate::set_island_expanded(app.clone(), true, Some(false));
        }
        crate::reveal_bar(app);
    }
}

fn close_capture_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(CAPTURE_WINDOW) {
        let _ = win.destroy();
    }
}

/// 窗口被用户或系统意外关闭时释放截屏内存并恢复灵动岛显示。
pub fn cleanup_capture_on_destroy(app: &AppHandle) {
    let Some(ocr_state) = app.try_state::<OcrState>() else {
        return;
    };
    let captured = ocr_state
        .active
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(session) = captured {
        restore_bar_after_capture(
            app,
            false,
            session.bar_was_visible,
            session.bar_was_expanded,
        );
    }
}

#[tauri::command]
pub async fn start_screen_ocr(app: AppHandle) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(CAPTURE_WINDOW) {
        let _ = existing.set_focus();
        return Ok(());
    }

    let bar_was_visible = app
        .get_webview_window(crate::windows::BAR)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(true);
    let bar_was_expanded = app
        .try_state::<AppState>()
        .map(|s| s.ui.snapshot().island_expanded)
        .unwrap_or(false);

    if bar_was_visible {
        crate::hide_window(&app, crate::windows::BAR);
        tokio::time::sleep(Duration::from_millis(90)).await;
    }

    let captured = tauri::async_runtime::spawn_blocking(capture_virtual_screen)
        .await
        .map_err(|_| "屏幕截图任务被中断".to_string())?;

    let (bgra, width, height, screen_x, screen_y, data_uri) = match captured {
        Ok(tuple) => tuple,
        Err(error) => {
            restore_bar_after_capture(&app, false, bar_was_visible, bar_was_expanded);
            crate::log_warn!("启动截图识字失败：{error}");
            return Err(error);
        }
    };

    let session_id = NEXT_SESSION.fetch_add(1, Ordering::Relaxed);
    {
        let state = app.state::<OcrState>();
        *state.active.lock().unwrap_or_else(|e| e.into_inner()) = Some(ActiveCapture {
            session_id,
            bgra,
            width,
            height,
            screen_x,
            screen_y,
            data_uri,
            bar_was_visible,
            bar_was_expanded,
        });
    }

    let build_res = WebviewWindowBuilder::new(&app, CAPTURE_WINDOW, WebviewUrl::App("index.html".into()))
        .title("截图识字")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .focused(true)
        .build();

    let window = match build_res {
        Ok(win) => win,
        Err(error) => {
            cleanup_capture_on_destroy(&app);
            crate::log_warn!("创建截图选区窗口失败：{error}");
            return Err("无法打开截图选区窗口".into());
        }
    };

    let _ = window.set_position(tauri::Position::Physical(PhysicalPosition::new(
        screen_x, screen_y,
    )));
    let _ = window.set_size(tauri::Size::Physical(PhysicalSize::new(width, height)));
    let _ = window.show();
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub fn get_capture_session(
    state: State<'_, OcrState>,
    window: WebviewWindow,
) -> Result<CaptureSessionInfo, String> {
    if window.label() != CAPTURE_WINDOW {
        return Err("请在截图窗口读取会话".into());
    }
    let guard = state.active.lock().unwrap_or_else(|e| e.into_inner());
    let session = guard.as_ref().ok_or("截图会话已失效，请重新截图")?;
    Ok(CaptureSessionInfo {
        session_id: session.session_id,
        data_uri: session.data_uri.clone(),
        width: session.width,
        height: session.height,
        screen_x: session.screen_x,
        screen_y: session.screen_y,
    })
}

#[tauri::command]
pub fn cancel_screen_ocr(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    if window.label() != CAPTURE_WINDOW {
        return Err("请在截图窗口取消截图".into());
    }
    cleanup_capture_on_destroy(&app);
    close_capture_window(&app);
    Ok(())
}

#[tauri::command]
pub async fn run_screen_ocr(
    app: AppHandle,
    window: WebviewWindow,
    session_id: u64,
    selection: CropSelection,
) -> Result<OcrResultPayload, String> {
    if window.label() != CAPTURE_WINDOW {
        return Err("请在截图窗口发起识字".into());
    }

    let captured = app
        .state::<OcrState>()
        .active
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    close_capture_window(&app);

    let Some(session) = captured else {
        return Err("截图会话已关闭，请重新截图".into());
    };
    if session.session_id != session_id {
        restore_bar_after_capture(
            &app,
            false,
            session.bar_was_visible,
            session.bar_was_expanded,
        );
        return Err("截图会话已过期，请重新截图".into());
    }

    let bar_visible = session.bar_was_visible;
    let bar_expanded = session.bar_was_expanded;
    let outcome = tauri::async_runtime::spawn_blocking(move || -> Result<OcrResultPayload, String> {
        let start = Instant::now();
        let (x, y, w, h) = selection_to_pixel_rect(&selection, session.width, session.height)?;
        let cropped = crop_bgra(&session.bgra, session.width, session.height, x, y, w, h)?;
        let (text, line_count, language) = recognize_bgra_windows(&cropped, w, h)?;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if text.is_empty() {
            return Err("未识别到清晰文字，请尝试放大选区或更换区域重试".into());
        }
        let char_count = text.chars().count();
        Ok(OcrResultPayload {
            text,
            line_count,
            char_count,
            language,
            elapsed_ms,
            source: "screen".into(),
        })
    })
    .await
    .map_err(|_| "文字识别任务未完成".to_string())?;

    match outcome {
        Ok(payload) => {
            crate::log_info!(
                "截图识字完成（语言 {}，耗时 {}ms，字符数 {}）",
                payload.language,
                payload.elapsed_ms,
                payload.char_count
            );
            restore_bar_after_capture(&app, true, bar_visible, bar_expanded);
            let _ = app.emit_to(crate::windows::BAR, OCR_RESULT_EVENT, &payload);
            Ok(payload)
        }
        Err(error) => {
            crate::log_warn!("截图识字未出结果：{error}");
            restore_bar_after_capture(&app, true, bar_visible, bar_expanded);
            let _ = app.emit_to(crate::windows::BAR, OCR_ERROR_EVENT, &error);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn ocr_clipboard_image(app: AppHandle, id: String) -> Result<OcrResultPayload, String> {
    let bytes = {
        let state = app.state::<AppState>();
        crate::clipboard_service::read_clipboard_image_bytes(&state, &id)?
    };
    let payload = tauri::async_runtime::spawn_blocking(move || -> Result<OcrResultPayload, String> {
        let start = Instant::now();
        let dynamic = image::load_from_memory(&bytes).map_err(|_| "图片格式无法解码")?;
        let rgba = dynamic.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let raw_rgba = rgba.into_raw();
        let mut bgra = vec![0u8; raw_rgba.len()];
        for (src, dst) in raw_rgba.chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = 255;
        }
        let (text, line_count, language) = recognize_bgra_windows(&bgra, width, height)?;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        if text.is_empty() {
            return Err("图片中未识别到清晰文字".into());
        }
        let char_count = text.chars().count();
        Ok(OcrResultPayload {
            text,
            line_count,
            char_count,
            language,
            elapsed_ms,
            source: "clipboard".into(),
        })
    })
    .await
    .map_err(|_| "图片文字识别任务未完成".to_string())??;

    crate::log_info!(
        "剪贴板图片识字完成（语言 {}，耗时 {}ms，字符数 {}）",
        payload.language,
        payload.elapsed_ms,
        payload.char_count
    );
    let _ = crate::set_island_expanded(app.clone(), true, Some(true));
    crate::show_window(&app, crate::windows::BAR);
    let _ = app.emit_to(crate::windows::BAR, OCR_RESULT_EVENT, &payload);
    Ok(payload)
}

#[tauri::command]
pub fn copy_ocr_text(app: AppHandle, text: String) -> Result<(), String> {
    if text.is_empty() {
        return Err("没有可复制的识别文字".into());
    }
    crate::clipboard_service::copy_plain_text(&app, &text)
}

#[cfg(all(test, windows))]
mod windows_ocr_tests {
    use super::*;

    #[test]
    fn smoke_test_real_windows_ocr_from_env_png() {
        let Ok(path) = std::env::var("DESKTOP_HUB_OCR_SMOKE_PNG") else {
            return;
        };
        let dynamic = image::open(&path).expect("读取合成测试图片失败");
        let rgba = dynamic.to_rgba8();
        let width = rgba.width();
        let height = rgba.height();
        let raw_rgba = rgba.into_raw();
        let mut bgra = vec![0u8; raw_rgba.len()];
        for (src, dst) in raw_rgba.chunks_exact(4).zip(bgra.chunks_exact_mut(4)) {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = 255;
        }
        let (text, line_count, lang) =
            recognize_bgra_windows(&bgra, width, height).expect("WinRT 离线 OCR 识别失败");
        println!("WinRT OCR 实测语言: {lang}, 行数: {line_count}, 识别文本: {text}");
        assert!(
            text.contains("DesktopHub"),
            "未识别到 DesktopHub，实际输出: {text}"
        );
        assert!(
            text.contains("截图识字"),
            "未识别到 截图识字，实际输出: {text}"
        );
        assert!(text.contains("2026"), "未识别到 2026，实际输出: {text}");
        assert!(
            !text.contains("截 图") && !text.contains("识 字"),
            "中文字符间存在未清理空格，实际输出: {text}"
        );
    }
}

// === TESTS ===
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_ocr_line_removes_cjk_spaces_and_preserves_ascii_word_spacing() {
        let raw = "  截 图 识 字  DesktopHub  Lite 0.1.3 ， 测 试  ";
        assert_eq!(clean_ocr_line(raw), "截图识字 DesktopHub Lite 0.1.3，测试");

        let lines = vec![
            "第 一 行 ： Hello   World ".to_string(),
            "   ".to_string(),
            "第 二 行 （ 完成 ） ".to_string(),
        ];
        assert_eq!(
            format_ocr_lines(&lines),
            "第一行：Hello World\n第二行（完成）"
        );
    }

    #[test]
    fn normalize_crop_rect_clamps_and_rejects_tiny_selection() {
        assert_eq!(
            normalize_crop_rect(-10, -5, 120, 80, 1920, 1080).unwrap(),
            (0, 0, 110, 75)
        );
        assert_eq!(
            normalize_crop_rect(1900, 1060, 100, 100, 1920, 1080).unwrap(),
            (1900, 1060, 20, 20)
        );
        assert!(normalize_crop_rect(10, 10, 3, 20, 1920, 1080).is_err());
        assert!(normalize_crop_rect(10, 10, 20, 2, 1920, 1080).is_err());
        assert!(normalize_crop_rect(10, 10, 0, 20, 1920, 1080).is_err());
    }

    #[test]
    fn crop_bgra_extracts_exact_subregion_pixels() {
        // 4x3 图像，每像素按 (x, y, 200, 255) 编码便于断言位置
        let mut img = Vec::with_capacity(4 * 3 * 4);
        for y in 0..3u8 {
            for x in 0..4u8 {
                img.extend_from_slice(&[x, y, 200, 255]);
            }
        }
        let sub = crop_bgra(&img, 4, 3, 1, 1, 2, 2).unwrap();
        assert_eq!(
            sub,
            vec![
                1, 1, 200, 255, 2, 1, 200, 255, // y = 1: x = 1, 2
                1, 2, 200, 255, 2, 2, 200, 255, // y = 2: x = 1, 2
            ]
        );
        assert!(crop_bgra(&img, 4, 3, 3, 1, 2, 2).is_err());
    }

    #[test]
    fn prepare_bgra_upscales_small_and_downscales_oversize() {
        // 小选区 60x30 应 2 倍放大为 120x60
        let small = vec![128u8; 60 * 30 * 4];
        let (up, uw, uh) = prepare_bgra_for_ocr(&small, 60, 30);
        assert_eq!((uw, uh), (120, 60));
        assert_eq!(up.len(), 120 * 60 * 4);

        // 正常尺寸 400x120 保持原尺寸不变
        let mid = vec![64u8; 400 * 120 * 4];
        let (same, mw, mh) = prepare_bgra_for_ocr(&mid, 400, 120);
        assert_eq!((mw, mh), (400, 120));
        assert_eq!(same.len(), 400 * 120 * 4);

        // 超大图像 3200x1600 应等比缩小至不超过 MAX_OCR_IMAGE_DIM (2500)
        let big = vec![200u8; 3200 * 1600 * 4];
        let (down, dw, dh) = prepare_bgra_for_ocr(&big, 3200, 1600);
        assert_eq!((dw, dh), (2500, 1250));
        assert_eq!(down.len(), 2500 * 1250 * 4);
    }
}
