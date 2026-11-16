//! Windows Shell 图标抽取与本地缓存（阶段 2）。
//!
//! 走 `IShellItemImageFactory::GetImage`：EXE、LNK、目录都能拿到 Shell 给出的
//! 真实图标（16/32 像素的小图标由 `SHGetFileInfoW` 提供，图标条要更清晰的大图）。
//! 拿不到就返回错误，由界面回退到首字母色块，不伪造图标。

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use windows::Win32::Foundation::{RPC_E_CHANGED_MODE, SIZE};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};

/// 图标缓存目录名（位于数据目录下）。
pub const ICON_DIR: &str = "icons";

/// 图标边长（逻辑像素）。图标条 36px 的方块，取 48 保证高 DPI 下也不糊。
pub const ICON_SIZE: u32 = 48;

/// 保证当前线程有 COM 公寓；已经初始化过（包括被别处以其它模式初始化）就不重复初始化。
struct ComGuard {
    owns: bool,
}

impl ComGuard {
    fn new() -> Result<Self, String> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_ok() {
                Ok(Self { owns: true })
            } else if hr == RPC_E_CHANGED_MODE {
                // 该线程已用别的公寓模型初始化，可以继续用，但不要负责反初始化。
                Ok(Self { owns: false })
            } else {
                Err(format!("COM 初始化失败：{hr:?}"))
            }
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.owns {
            unsafe { CoUninitialize() };
        }
    }
}

fn hash_key(target: &str, size: u32) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in target.as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    for byte in size.to_le_bytes() {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

/// 网址没有本地图标，直接说明拿不到。
pub fn is_remote_target(target: &str) -> bool {
    let lowered = target.trim().to_ascii_lowercase();
    lowered.starts_with("http://")
        || lowered.starts_with("https://")
        || lowered.starts_with("mailto:")
        || lowered.starts_with("ftp://")
}

/// 读取（必要时抽取并缓存）目标图标，返回可直接塞进 `<img src>` 的 data URI。
pub fn icon_data_uri(cache_root: &Path, target: &str, size: u32) -> Result<String, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("目标为空，无法读取图标".into());
    }
    if is_remote_target(target) {
        return Err("网址没有本地图标".into());
    }
    if !Path::new(target).exists() {
        return Err(format!("目标不存在：{target}"));
    }

    let dir = cache_root.join(ICON_DIR);
    let file: PathBuf = dir.join(format!("{}.png", hash_key(target, size)));
    if !file.exists() {
        let png = extract_png(target, size)?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("创建图标目录失败：{e}"))?;
        let tmp = dir.join(format!("{}.tmp", hash_key(target, size)));
        std::fs::write(&tmp, &png).map_err(|e| format!("写入图标缓存失败：{e}"))?;
        std::fs::rename(&tmp, &file).map_err(|e| format!("替换图标缓存失败：{e}"))?;
    }

    let bytes = std::fs::read(&file).map_err(|e| format!("读取图标缓存失败：{e}"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:image/png;base64,{encoded}"))
}

fn extract_png(target: &str, size: u32) -> Result<Vec<u8>, String> {
    let _com = ComGuard::new()?;
    let bitmap = unsafe { shell_bitmap(target, size)? };
    let rgba = unsafe { bitmap_to_rgba(bitmap) };
    unsafe {
        let _ = DeleteObject(HGDIOBJ::from(bitmap));
    }
    let (width, height, pixels) = rgba?;

    let mut out: Vec<u8> = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("写 PNG 头失败：{e}"))?;
        writer
            .write_image_data(&pixels)
            .map_err(|e| format!("写 PNG 数据失败：{e}"))?;
        writer.finish().map_err(|e| format!("收尾 PNG 失败：{e}"))?;
    }
    Ok(out)
}

unsafe fn shell_bitmap(target: &str, size: u32) -> Result<HBITMAP, String> {
    let wide = windows::core::HSTRING::from(target);
    let factory: IShellItemImageFactory =
        SHCreateItemFromParsingName(&wide, None).map_err(|e| format!("Shell 无法解析这个目标：{e}"))?;
    let request = SIZE {
        cx: size as i32,
        cy: size as i32,
    };
    factory
        .GetImage(request, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
        .map_err(|e| format!("Shell 没有返回图标：{e}"))
}

unsafe fn bitmap_to_rgba(bitmap: HBITMAP) -> Result<(u32, u32, Vec<u8>), String> {
    let mut info: BITMAP = core::mem::zeroed();
    let got = GetObjectW(
        HGDIOBJ::from(bitmap),
        core::mem::size_of::<BITMAP>() as i32,
        Some(&mut info as *mut _ as *mut c_void),
    );
    if got == 0 {
        return Err("读取位图信息失败".into());
    }
    let width = info.bmWidth;
    let height = info.bmHeight;
    if width <= 0 || height <= 0 {
        return Err("图标尺寸异常".into());
    }

    let mut dib: BITMAPINFO = core::mem::zeroed();
    dib.bmiHeader = BITMAPINFOHEADER {
        biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        // 负高度 = 自上而下，省得再翻转行序。
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let mut buffer = vec![0u8; (width as usize) * (height as usize) * 4];
    let hdc = GetDC(None);
    if hdc.is_invalid() {
        return Err("获取屏幕 DC 失败".into());
    }
    let lines = GetDIBits(
        hdc,
        bitmap,
        0,
        height as u32,
        Some(buffer.as_mut_ptr() as *mut c_void),
        &mut dib,
        DIB_RGB_COLORS,
    );
    let _ = ReleaseDC(None, hdc);
    if lines == 0 {
        return Err("读取图标像素失败".into());
    }

    // BGRA → RGBA
    let mut has_alpha = false;
    for chunk in buffer.chunks_exact_mut(4) {
        chunk.swap(0, 2);
        if chunk[3] != 0 {
            has_alpha = true;
        }
    }
    // 全透明通道的位图（部分旧图标）按不透明处理，否则整块看不见。
    if !has_alpha {
        for chunk in buffer.chunks_exact_mut(4) {
            chunk[3] = 255;
        }
    }

    Ok((width as u32, height as u32, buffer))
}
