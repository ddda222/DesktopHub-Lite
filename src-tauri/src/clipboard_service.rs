use std::{borrow::Cow, io::Cursor, sync::atomic::{AtomicU32, Ordering}, time::{Duration, SystemTime, UNIX_EPOCH}};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};
use crate::{AppState, clipboard::{ClipboardEntry, ClipboardIndex, INDEX_FILE}};

static SELF_SEQUENCE: AtomicU32 = AtomicU32::new(0);
#[link(name = "user32")]
unsafe extern "system" { fn GetClipboardSequenceNumber() -> u32; }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardStatus { groups: Vec<String>, paused: bool }
pub fn changed(app: &AppHandle) { let _ = app.emit("clipboard-changed", ()); }
fn now() -> i64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as i64 }
fn image_path(entry: &ClipboardEntry, state: &AppState) -> Result<std::path::PathBuf, String> {
    let file = entry.image_file.as_deref().ok_or("图片记录缺少文件")?;
    if !file.starts_with("clipboard-images/") || file.contains("..") || file.contains('\\') { return Err("图片路径无效".into()); }
    Ok(state.store.root().join(file))
}
fn encode_image(image: &arboard::ImageData<'_>) -> Result<Vec<u8>, String> {
    if image.width == 0 || image.height == 0 || image.width > 16384 || image.height > 16384 || image.width.saturating_mul(image.height) > 25_000_000 { return Err("图片尺寸超过限制".into()); }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba); encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().map_err(|_| "图片编码失败")?.write_image_data(&image.bytes).map_err(|_| "图片编码失败")?;
    }
    if bytes.len() > 32 * 1024 * 1024 { return Err("图片超过 32 MiB".into()); }
    Ok(bytes)
}
fn write_text(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|_| "无法访问系统剪贴板")?;
    clipboard.set_text(text).map_err(|_| "剪贴板被占用，请重试")?;
    SELF_SEQUENCE.store(unsafe { GetClipboardSequenceNumber() }, Ordering::SeqCst); Ok(())
}
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut sequence = unsafe { GetClipboardSequenceNumber() };
        let mut failures = 0u8;
        while app.state::<AppState>().running.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(450));
            if !app.state::<AppState>().running.load(Ordering::Relaxed) { break; }
            let current = unsafe { GetClipboardSequenceNumber() };
            if current == 0 || current == sequence { continue; }
            let state = app.state::<AppState>();
            if state.clipboard.lock().unwrap_or_else(|e| e.into_inner()).paused || current == SELF_SEQUENCE.load(Ordering::SeqCst) { sequence = current; continue; }
            let captured = (|| -> Result<Option<(ClipboardEntry, Option<Vec<u8>>)>, String> {
                let mut source = arboard::Clipboard::new().map_err(|_| "剪贴板暂时不可访问")?; let timestamp = now();
                if let Ok(image) = source.get_image() {
                    let bytes = encode_image(&image)?; let hash = format!("{:x}", Sha256::digest(&bytes));
                    return Ok(Some((ClipboardEntry { id: format!("image-{hash}"), kind: "image".into(), text: None, image_file: Some(format!("clipboard-images/{hash}.png")), group: "未分组".into(), copied_at: timestamp, width: Some(image.width as u32), height: Some(image.height as u32), hash }, Some(bytes))));
                }
                if let Ok(text) = source.get_text() {
                    if text.is_empty() || text.chars().count() > 1_000_000 { return Ok(None); }
                    let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
                    return Ok(Some((ClipboardEntry { id: format!("text-{hash}"), kind: "text".into(), text: Some(text), image_file: None, group: "未分组".into(), copied_at: timestamp, width: None, height: None, hash }, None)));
                }
                Ok(None)
            })();
            if unsafe { GetClipboardSequenceNumber() } != current { continue; }
            match captured {
                Ok(Some((mut entry, image))) => {
                    let result = (|| -> Result<(), String> {
                        let mut index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner()); if index.paused { return Ok(()); }
                        let mut next = index.clone();
                        if let Some(position) = next.entries.iter().position(|item| item.id == entry.id) { entry.group = next.entries.remove(position).group; }
                        if let Some(bytes) = image {
                            let path = image_path(&entry, &state)?;
                            std::fs::create_dir_all(path.parent().ok_or("图片目录无效")?).map_err(|_| "无法创建图片目录")?;
                            if !path.exists() { std::fs::write(path, bytes).map_err(|_| "图片保存失败")?; }
                        }
                        next.entries.insert(0, entry); state.store.write(INDEX_FILE, &next)?; *index = next; Ok(())
                    })();
                    sequence = current;
                    failures = 0;
                    match result { Ok(()) => changed(&app), Err(_) => { let _ = app.emit("clipboard-notice", "剪贴板历史保存失败，请检查数据目录"); } }
                }
                Ok(None) => { sequence = current; failures = 0; }
                Err(message) => {
                    failures = failures.saturating_add(1);
                    if failures >= 6 || message.contains("限制") || message.contains("32 MiB") {
                        sequence = current; failures = 0;
                        let _ = app.emit("clipboard-notice", message);
                    }
                }
            }
        }
    });
}
#[tauri::command]
pub fn clipboard_status(state: State<'_, AppState>) -> ClipboardStatus {
    let index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner()); ClipboardStatus { groups: index.groups.clone(), paused: index.paused }
}
#[tauri::command]
pub fn copy_clipboard_entry(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner()); let entry = index.entries.iter().find(|item| item.id == id).ok_or("记录已不存在")?;
    if entry.kind == "text" { return write_text(entry.text.as_deref().ok_or("文字记录无效")?); }
    let bytes = std::fs::read(image_path(entry, &state)?).map_err(|_| "图片文件缺失")?;
    let mut decoder = png::Decoder::new(Cursor::new(bytes)); decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| "图片无法解码")?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("图片尺寸无效")?];
    let info = reader.next_frame(&mut buffer).map_err(|_| "图片无法解码")?; let data = &buffer[..info.buffer_size()];
    if info.color_type != png::ColorType::Rgba { return Err("不支持的图片格式".into()); }
    let mut target = arboard::Clipboard::new().map_err(|_| "无法访问系统剪贴板")?;
    target.set_image(arboard::ImageData { width: info.width as usize, height: info.height as usize, bytes: Cow::Borrowed(data) }).map_err(|_| "剪贴板被占用，请重试")?;
    SELF_SEQUENCE.store(unsafe { GetClipboardSequenceNumber() }, Ordering::SeqCst); Ok(())
}
#[tauri::command]
pub fn copy_translation_result(text: String) -> Result<(), String> { write_text(&text) }
#[tauri::command]
pub fn read_clipboard_thumbnail(state: State<'_, AppState>, id: String, full: Option<bool>) -> Result<String, String> {
    let index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner()); let entry = index.entries.iter().find(|item| item.id == id).ok_or("记录已不存在")?;
    let bytes = std::fs::read(image_path(entry, &state)?).map_err(|_| "图片文件缺失")?;
    if bytes.len() > 32 * 1024 * 1024 { return Err("图片超过限制".into()); }
    if full.unwrap_or(false) { return Ok(format!("data:image/png;base64,{}", STANDARD.encode(bytes))); }
    let image = image::load_from_memory(&bytes).map_err(|_| "图片无法解码")?.thumbnail(320, 240);
    let mut output = Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Png).map_err(|_| "缩略图生成失败")?;
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(output.into_inner())))
}
fn mutate(app: &AppHandle, change: impl FnOnce(&mut ClipboardIndex) -> Result<(), String>) -> Result<(), String> {
    let state = app.state::<AppState>(); let mut index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner()); let mut next = index.clone();
    change(&mut next)?; state.store.write(INDEX_FILE, &next)?;
    for removed in index.entries.iter().filter(|entry| entry.kind == "image" && !next.entries.iter().any(|current| current.id == entry.id)) {
        if let Ok(path) = image_path(removed, &state) { let _ = std::fs::remove_file(path); }
    }
    *index = next; drop(index); changed(app); Ok(())
}
#[tauri::command]
pub fn delete_clipboard_entry(app: AppHandle, id: String) -> Result<(), String> { mutate(&app, |index| { index.entries.retain(|item| item.id != id); Ok(()) }) }
#[tauri::command]
pub fn clear_clipboard_entries(app: AppHandle) -> Result<(), String> { mutate(&app, |index| { index.entries.clear(); Ok(()) }) }
fn group_name(group: &str) -> Result<String, String> {
    let group = group.trim(); if group.is_empty() || group == "全部" || group.chars().count() > 40 || group.chars().any(char::is_control) { return Err("分组名需为 1–40 个可显示字符，且不能使用“全部”".into()); } Ok(group.into())
}
#[tauri::command]
pub fn create_clipboard_group(app: AppHandle, group: String) -> Result<(), String> {
    let group = group_name(&group)?; mutate(&app, |index| { if !index.groups.contains(&group) { index.groups.push(group); } Ok(()) })
}
#[tauri::command]
pub fn remove_clipboard_group(app: AppHandle, group: String) -> Result<(), String> {
    if group == "未分组" { return Err("默认分组不能删除".into()); }
    mutate(&app, |index| { index.groups.retain(|name| name != &group); for entry in &mut index.entries { if entry.group == group { entry.group = "未分组".into(); } } Ok(()) })
}
#[tauri::command]
pub fn assign_clipboard_group(app: AppHandle, id: String, group: String) -> Result<(), String> {
    let group = group_name(&group)?; mutate(&app, |index| { if !index.groups.contains(&group) { return Err("分组不存在".into()); } index.entries.iter_mut().find(|entry| entry.id == id).ok_or("记录已不存在")?.group = group; Ok(()) })
}
