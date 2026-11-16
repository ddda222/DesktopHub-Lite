use crate::{
    clipboard::{
        cleanup_unreferenced_images, is_managed_png, ClipboardEntry, ClipboardIndex,
        DEFAULT_HISTORY_LIMIT, INDEX_FILE, MAX_HISTORY_LIMIT, MIN_HISTORY_LIMIT,
    },
    AppState,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::HashSet,
    fs,
    io::Cursor,
    sync::atomic::{AtomicU32, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};

static SELF_SEQUENCE: AtomicU32 = AtomicU32::new(0);
#[link(name = "user32")]
unsafe extern "system" {
    fn GetClipboardSequenceNumber() -> u32;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardStatus {
    groups: Vec<String>,
    paused: bool,
    capture_images: bool,
    history_limit: Option<usize>,
    default_history_limit: usize,
}
pub fn changed(app: &AppHandle) {
    let _ = app.emit("clipboard-changed", ());
}
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
const IMAGE_DIR: &str = "clipboard-images";

fn image_directory(state: &AppState) -> Result<std::path::PathBuf, String> {
    let dir = state.store.root().join(IMAGE_DIR);
    match fs::symlink_metadata(&dir) {
        Ok(info) if !info.file_type().is_dir() => return Err("图片目录不是普通目录".into()),
        #[cfg(windows)]
        Ok(info)
            if {
                use std::os::windows::fs::MetadataExt;
                info.file_attributes() & 0x400 != 0 // 拒绝目录联接等重解析点。
            } =>
        {
            return Err("图片目录不能是链接".into())
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err("图片目录不可访问".into()),
    }
    Ok(dir)
}

fn image_path(entry: &ClipboardEntry, state: &AppState) -> Result<std::path::PathBuf, String> {
    let name = entry
        .image_file
        .as_deref()
        .and_then(|path| path.strip_prefix("clipboard-images/"))
        .filter(|name| is_managed_png(name))
        .ok_or("图片路径无效")?;
    let path = image_directory(state)?.join(name);
    if let Ok(info) = fs::symlink_metadata(&path) {
        if !info.file_type().is_file() {
            return Err("图片不是普通文件".into());
        }
    }
    Ok(path)
}

// 保留当前索引及刚轮换出来的备份所引用的图片；更早的孤立图片
// 在下一次成功写入后回收。失败时不删除任何现存引用。
fn commit_index(
    state: &AppState,
    index: &mut ClipboardIndex,
    next: ClipboardIndex,
) -> Result<(), String> {
    state.store.write(INDEX_FILE, &next)?;
    if let Ok(dir) = image_directory(state) {
        cleanup_unreferenced_images(&dir, index, &next);
    }
    *index = next;
    Ok(())
}

fn save_image(state: &AppState, entry: &ClipboardEntry, bytes: Vec<u8>) -> Result<(), String> {
    let path = image_path(entry, state)?;
    if path.exists() {
        return Ok(());
    }
    let dir = image_directory(state)?;
    fs::create_dir_all(&dir).map_err(|_| "无法创建图片目录")?;
    let temporary = path.with_extension("tmp");
    // create_new 避免覆盖残留文件或跟随被放入目录的链接。
    if let Ok(info) = fs::symlink_metadata(&temporary) {
        if !info.file_type().is_file() {
            return Err("图片暂存路径无效".into());
        }
        fs::remove_file(&temporary).map_err(|_| "无法清理图片暂存文件")?;
    }
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| "无法暂存图片")?;
    if file
        .write_all(&bytes)
        .and_then(|_| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err("图片保存失败".into());
    }
    drop(file);
    fs::rename(&temporary, path).map_err(|_| "图片保存失败".into())
}
fn encode_image(image: &arboard::ImageData<'_>) -> Result<Vec<u8>, String> {
    if image.width == 0
        || image.height == 0
        || image.width > 16384
        || image.height > 16384
        || image.width.saturating_mul(image.height) > 25_000_000
    {
        return Err("图片尺寸超过限制".into());
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.width as u32, image.height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(|_| "图片编码失败")?
            .write_image_data(&image.bytes)
            .map_err(|_| "图片编码失败")?;
    }
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("图片超过 32 MiB".into());
    }
    Ok(bytes)
}
fn write_text(text: &str) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|_| "无法访问系统剪贴板")?;
    clipboard
        .set_text(text)
        .map_err(|_| "剪贴板被占用，请重试")?;
    SELF_SEQUENCE.store(unsafe { GetClipboardSequenceNumber() }, Ordering::SeqCst);
    Ok(())
}
pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut sequence = unsafe { GetClipboardSequenceNumber() };
        let mut failures = 0u8;
        while app.state::<AppState>().running.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(450));
            if !app.state::<AppState>().running.load(Ordering::Relaxed) {
                break;
            }
            let current = unsafe { GetClipboardSequenceNumber() };
            if current == 0 || current == sequence {
                continue;
            }
            let state = app.state::<AppState>();
            let (paused, capture_images) = {
                let index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner());
                (index.paused, index.capture_images)
            };
            if paused || current == SELF_SEQUENCE.load(Ordering::SeqCst) {
                sequence = current;
                continue;
            }
            let captured = (|| -> Result<Option<(ClipboardEntry, Option<Vec<u8>>)>, String> {
                let mut source = arboard::Clipboard::new().map_err(|_| "剪贴板暂时不可访问")?;
                let timestamp = now();
                if capture_images {
                    if let Ok(image) = source.get_image() {
                        let bytes = encode_image(&image)?;
                        let hash = format!("{:x}", Sha256::digest(&bytes));
                        return Ok(Some((
                            ClipboardEntry {
                                id: format!("image-{hash}"),
                                kind: "image".into(),
                                text: None,
                                image_file: Some(format!("clipboard-images/{hash}.png")),
                                group: "未分组".into(),
                                copied_at: timestamp,
                                width: Some(image.width as u32),
                                height: Some(image.height as u32),
                                hash,
                                favorite: false,
                                note: String::new(),
                            },
                            Some(bytes),
                        )));
                    }
                }
                if let Ok(text) = source.get_text() {
                    if text.is_empty() || text.chars().count() > 1_000_000 {
                        return Ok(None);
                    }
                    let hash = format!("{:x}", Sha256::digest(text.as_bytes()));
                    return Ok(Some((
                        ClipboardEntry {
                            id: format!("text-{hash}"),
                            kind: "text".into(),
                            text: Some(text),
                            image_file: None,
                            group: "未分组".into(),
                            copied_at: timestamp,
                            width: None,
                            height: None,
                            hash,
                            favorite: false,
                            note: String::new(),
                        },
                        None,
                    )));
                }
                Ok(None)
            })();
            if unsafe { GetClipboardSequenceNumber() } != current {
                continue;
            }
            match captured {
                Ok(Some((entry, image))) => {
                    let result = (|| -> Result<(), String> {
                        let mut index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner());
                        if index.paused || (image.is_some() && !index.capture_images) {
                            return Ok(());
                        }
                        if let Some(bytes) = image {
                            save_image(&state, &entry, bytes)?;
                        }
                        let mut next = index.clone();
                        next.record(entry);
                        commit_index(&state, &mut index, next)
                    })();
                    match result {
                        Ok(()) => {
                            sequence = current;
                            failures = 0;
                            changed(&app);
                        }
                        Err(_) => {
                            failures = failures.saturating_add(1);
                            if failures >= 3 {
                                sequence = current;
                                failures = 0;
                                let _ = app
                                    .emit("clipboard-notice", "剪贴板历史保存失败，请检查数据目录");
                            }
                        }
                    }
                }
                Ok(None) => {
                    sequence = current;
                    failures = 0;
                }
                Err(message) => {
                    failures = failures.saturating_add(1);
                    if failures >= 6 || message.contains("限制") || message.contains("32 MiB") {
                        sequence = current;
                        failures = 0;
                        let _ = app.emit("clipboard-notice", message);
                    }
                }
            }
        }
    });
}
#[tauri::command]
pub fn clipboard_status(state: State<'_, AppState>) -> ClipboardStatus {
    let index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner());
    ClipboardStatus {
        groups: index.groups.clone(),
        paused: index.paused,
        capture_images: index.capture_images,
        history_limit: index.history_limit,
        default_history_limit: DEFAULT_HISTORY_LIMIT,
    }
}
#[tauri::command]
pub fn copy_clipboard_entry(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner());
    let entry = index
        .entries
        .iter()
        .find(|item| item.id == id)
        .ok_or("记录已不存在")?;
    if entry.kind == "text" {
        return write_text(entry.text.as_deref().ok_or("文字记录无效")?);
    }
    let bytes = std::fs::read(image_path(entry, &state)?).map_err(|_| "图片文件缺失")?;
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|_| "图片无法解码")?;
    let mut buffer = vec![0; reader.output_buffer_size().ok_or("图片尺寸无效")?];
    let info = reader.next_frame(&mut buffer).map_err(|_| "图片无法解码")?;
    let data = &buffer[..info.buffer_size()];
    if info.color_type != png::ColorType::Rgba {
        return Err("不支持的图片格式".into());
    }
    let mut target = arboard::Clipboard::new().map_err(|_| "无法访问系统剪贴板")?;
    target
        .set_image(arboard::ImageData {
            width: info.width as usize,
            height: info.height as usize,
            bytes: Cow::Borrowed(data),
        })
        .map_err(|_| "剪贴板被占用，请重试")?;
    SELF_SEQUENCE.store(unsafe { GetClipboardSequenceNumber() }, Ordering::SeqCst);
    Ok(())
}
#[tauri::command]
pub fn copy_translation_result(text: String) -> Result<(), String> {
    write_text(&text)
}
#[tauri::command]
pub fn read_clipboard_thumbnail(
    state: State<'_, AppState>,
    id: String,
    full: Option<bool>,
) -> Result<String, String> {
    let index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner());
    let entry = index
        .entries
        .iter()
        .find(|item| item.id == id)
        .ok_or("记录已不存在")?;
    let bytes = std::fs::read(image_path(entry, &state)?).map_err(|_| "图片文件缺失")?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("图片超过限制".into());
    }
    if full.unwrap_or(false) {
        return Ok(format!("data:image/png;base64,{}", STANDARD.encode(bytes)));
    }
    let image = image::load_from_memory(&bytes)
        .map_err(|_| "图片无法解码")?
        .thumbnail(320, 240);
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|_| "缩略图生成失败")?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(output.into_inner())
    ))
}
fn mutate(
    app: &AppHandle,
    change: impl FnOnce(&mut ClipboardIndex) -> Result<(), String>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner());
    let mut next = index.clone();
    change(&mut next)?;
    commit_index(&state, &mut index, next)?;
    drop(index);
    changed(app);
    Ok(())
}

#[tauri::command]
pub fn set_clipboard_capture_images(app: AppHandle, enabled: bool) -> Result<(), String> {
    mutate(&app, |index| {
        index.capture_images = enabled;
        Ok(())
    })
}

#[tauri::command]
pub fn set_clipboard_history_limit(
    app: AppHandle,
    limit: usize,
    expected_removed: usize,
) -> Result<(), String> {
    if !(MIN_HISTORY_LIMIT..=MAX_HISTORY_LIMIT).contains(&limit) {
        return Err("数量上限须为 10–5000".into());
    }
    mutate(&app, |index| {
        let ordinary = index.entries.iter().filter(|entry| !entry.favorite).count();
        if ordinary.saturating_sub(limit) != expected_removed {
            return Err("记录数量已变化，请重新确认清理范围".into());
        }
        index.history_limit = Some(limit);
        index.trim_history();
        Ok(())
    })
}

#[tauri::command]
pub fn edit_clipboard_entry(
    app: AppHandle,
    id: String,
    favorite: bool,
    note: String,
) -> Result<(), String> {
    let note = note.trim();
    if note.chars().count() > 200 {
        return Err("备注不能超过 200 字".into());
    }
    mutate(&app, |index| {
        let entry = index
            .entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or("记录已不存在")?;
        entry.favorite = favorite;
        entry.note = note.into();
        // 取消收藏不是删除操作。超出的普通记录从下次捕获开始清理。
        Ok(())
    })
}

#[tauri::command]
pub fn delete_clipboard_entries(
    app: AppHandle,
    ids: Vec<String>,
    expected_favorites: usize,
) -> Result<(), String> {
    let ids = ids.into_iter().collect::<HashSet<_>>();
    mutate(&app, |index| {
        if index
            .entries
            .iter()
            .filter(|item| item.favorite && ids.contains(&item.id))
            .count()
            != expected_favorites
        {
            return Err("选中记录含收藏，请重新确认后删除".into());
        }
        index.entries.retain(|item| !ids.contains(&item.id));
        Ok(())
    })
}

#[tauri::command]
pub fn delete_clipboard_entry(app: AppHandle, id: String) -> Result<(), String> {
    delete_clipboard_entries(app, vec![id], 0)
}
#[tauri::command]
pub fn clear_clipboard_entries(app: AppHandle) -> Result<(), String> {
    mutate(&app, |index| {
        index.entries.clear();
        Ok(())
    })
}
fn group_name(group: &str) -> Result<String, String> {
    let group = group.trim();
    if group.is_empty()
        || group == "全部"
        || group.chars().count() > 40
        || group.chars().any(char::is_control)
    {
        return Err("分组名需为 1–40 个可显示字符，且不能使用“全部”".into());
    }
    Ok(group.into())
}
#[tauri::command]
pub fn create_clipboard_group(app: AppHandle, group: String) -> Result<(), String> {
    let group = group_name(&group)?;
    mutate(&app, |index| {
        if !index.groups.contains(&group) {
            index.groups.push(group);
        }
        Ok(())
    })
}
#[tauri::command]
pub fn remove_clipboard_group(app: AppHandle, group: String) -> Result<(), String> {
    if group == "未分组" {
        return Err("默认分组不能删除".into());
    }
    mutate(&app, |index| {
        index.groups.retain(|name| name != &group);
        for entry in &mut index.entries {
            if entry.group == group {
                entry.group = "未分组".into();
            }
        }
        Ok(())
    })
}
#[tauri::command]
pub fn assign_clipboard_group(app: AppHandle, id: String, group: String) -> Result<(), String> {
    let group = group_name(&group)?;
    mutate(&app, |index| {
        if !index.groups.contains(&group) {
            return Err("分组不存在".into());
        }
        index
            .entries
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or("记录已不存在")?
            .group = group;
        Ok(())
    })
}
