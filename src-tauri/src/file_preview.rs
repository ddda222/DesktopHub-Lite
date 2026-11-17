//! 中转文件的只读预览。仅按已有条目 ID 读取，限制读取量，不执行或渲染文件中的代码。
use std::{fs::File, io::Read, path::Path, time::UNIX_EPOCH};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::{AppHandle, Manager};

const TEXT_LIMIT: usize = 64 * 1024;
pub const IMAGE_LIMIT: u64 = 8 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    kind: &'static str,
    size: Option<u64>,
    modified_ms: Option<u64>,
    text: Option<String>,
    data_uri: Option<String>,
    notice: Option<String>,
}

fn decode_text(bytes: &[u8], truncated: bool) -> Option<String> {
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let little = bytes[0] == 0xff;
        if !truncated && (bytes.len() - 2) % 2 != 0 { return None; }
        let mut units: Vec<_> = bytes[2..].chunks_exact(2).map(|pair| {
            if little { u16::from_le_bytes([pair[0], pair[1]]) } else { u16::from_be_bytes([pair[0], pair[1]]) }
        }).collect();
        if truncated && units.last().is_some_and(|unit| (0xd800..=0xdbff).contains(unit)) { units.pop(); }
        return String::from_utf16(&units).ok().filter(|text| !text.contains('\0'));
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if bytes.contains(&0) { return None; }
    match std::str::from_utf8(bytes) {
        Ok(text) => Some(text.to_string()),
        Err(error) if truncated && error.error_len().is_none() =>
            std::str::from_utf8(&bytes[..error.valid_up_to()]).ok().map(str::to_string),
        Err(_) => None,
    }
}

fn read_preview(path: &Path) -> Result<FilePreview, String> {
    let metadata = path.metadata().map_err(|_| "原文件已移动、删除或无法访问")?;
    let mut result = FilePreview {
        kind: "info", size: if metadata.is_file() { Some(metadata.len()) } else { None },
        modified_ms: metadata.modified().ok().and_then(|time| time.duration_since(UNIX_EPOCH).ok()).map(|time| time.as_millis() as u64),
        text: None, data_uri: None, notice: None,
    };
    if metadata.is_dir() {
        result.kind = "folder";
        result.notice = Some("文件夹不展开读取，可打开所在位置查看内容".into());
        return Ok(result);
    }
    if !metadata.is_file() { return Err("暂不支持这种文件对象的预览".into()); }
    let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("").to_lowercase();
    let mime = match extension.as_str() {
        "png" => Some("image/png"), "jpg" | "jpeg" => Some("image/jpeg"), "gif" => Some("image/gif"),
        "webp" => Some("image/webp"), "bmp" => Some("image/bmp"), "ico" => Some("image/x-icon"), _ => None,
    };
    if let Some(mime) = mime {
        if metadata.len() > IMAGE_LIMIT {
            result.notice = Some("图片超过 8 MiB，请在所在位置查看".into());
            return Ok(result);
        }
        let mut bytes = Vec::new();
        File::open(path).map_err(|_| "无法读取图片")?.take(IMAGE_LIMIT + 1).read_to_end(&mut bytes).map_err(|_| "无法读取图片")?;
        if bytes.len() as u64 > IMAGE_LIMIT { return Err("图片已变大，请重新预览".into()); }
        result.kind = "image";
        result.data_uri = Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)));
        return Ok(result);
    }
    if matches!(extension.as_str(), "txt" | "md" | "markdown" | "json" | "csv" | "log" | "yaml" | "yml" | "toml" | "ini" | "xml" | "html" | "htm" | "css" | "scss" | "js" | "ts" | "jsx" | "tsx" | "vue" | "rs" | "py" | "java" | "sql" | "sh" | "ps1" | "bat" | "svg") {
        let mut bytes = Vec::new();
        File::open(path).map_err(|_| "无法读取文本")?.take((TEXT_LIMIT + 1) as u64).read_to_end(&mut bytes).map_err(|_| "无法读取文本")?;
        let truncated = bytes.len() > TEXT_LIMIT;
        bytes.truncate(TEXT_LIMIT);
        if let Some(text) = decode_text(&bytes, truncated) {
            result.kind = "text";
            result.text = Some(text);
            if truncated { result.notice = Some("仅预览前 64 KiB，原文件未改动".into()); }
        } else {
            result.notice = Some("此文件不是受支持的 UTF-8 / UTF-16 文本，请在所在位置查看".into());
        }
    } else {
        result.notice = Some("此格式暂不支持内容预览，可打开所在位置查看".into());
    }
    Ok(result)
}

#[tauri::command]
pub async fn preview_shelf_file(app: AppHandle, id: String) -> Result<FilePreview, String> {
    let paths = crate::file_shelf::selected_paths(&app.state::<crate::AppState>(), &[id])?;
    tauri::async_runtime::spawn_blocking(move || read_preview(Path::new(&paths[0])))
        .await.map_err(|_| "文件预览未完成，请重试".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_boundary_and_encoding() {
        assert_eq!(decode_text(&[b'a', 0xe4, 0xbd], true), Some("a".into()));
        assert_eq!(decode_text(&[b'a', 0xe4, 0xbd], false), None);
        assert_eq!(decode_text(&[0xff, 0xfe, 0x60, 0x4f], false), Some("你".into()));
        assert_eq!(decode_text(&[0xfe, 0xff, 0x4f, 0x60], false), Some("你".into()));
        assert_eq!(decode_text(b"a\0b", false), None);
    }
}
