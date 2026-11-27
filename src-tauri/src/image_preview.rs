use std::{collections::HashMap, sync::{Mutex, atomic::{AtomicU64, Ordering}}};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview { data_uri: String, name: String }
#[derive(Default)]
pub struct Previews(pub Mutex<HashMap<String, Preview>>);
static NEXT: AtomicU64 = AtomicU64::new(1);

#[tauri::command]
pub async fn open_image_preview(app: AppHandle, source: String, id: String) -> Result<(), String> {
    let preview = match source.as_str() {
        "clipboard" => Preview {
            data_uri: crate::clipboard_service::read_clipboard_thumbnail(
                app.clone(),
                id,
                Some(true),
            )
            .await?,
            name: "剪贴板图片".into(),
        },
        "shelf" => {
            let state = app.state::<crate::AppState>();
            let paths = crate::file_shelf::selected_paths(&state, &[id])?;
            let path = std::path::Path::new(&paths[0]);
            let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            let mime = match extension.as_str() {
                "png" => "image/png", "jpg" | "jpeg" => "image/jpeg", "gif" => "image/gif",
                "webp" => "image/webp", "bmp" => "image/bmp", "ico" => "image/x-icon",
                _ => return Err("暂不支持这种文件的图片预览".into()),
            };
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(path).map_err(|_| "图片不存在或无法读取")?
                .take(crate::file_preview::IMAGE_LIMIT + 1).read_to_end(&mut bytes).map_err(|_| "图片无法读取")?;
            if bytes.len() as u64 > crate::file_preview::IMAGE_LIMIT { return Err("图片超过 8 MiB，请在所在位置查看".into()); }
            Preview { data_uri: format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
                name: path.file_name().unwrap_or_default().to_string_lossy().into_owned() }
        },
        _ => return Err("未知图片来源".into()),
    };
    let label = format!("image-preview-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    app.state::<Previews>().0.lock().unwrap_or_else(|e| e.into_inner()).insert(label.clone(), preview);
    let result = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("index.html".into()))
        .title("图片预览 · DesktopHub Lite").inner_size(640.0, 480.0)
        .min_inner_size(280.0, 180.0).resizable(true).focused(false).visible(true)
        .build();
    if let Err(error) = result {
        forget(&app, &label);
        return Err(format!("无法打开图片预览：{error}"));
    }
    Ok(())
}

#[tauri::command]
pub fn image_preview_content(state: State<'_, Previews>, window: WebviewWindow) -> Result<Preview, String> {
    state.0.lock().unwrap_or_else(|e| e.into_inner()).get(window.label()).cloned().ok_or("预览已关闭，请重新打开".into())
}

#[tauri::command]
pub fn pin_image_preview(window: WebviewWindow, pinned: bool) -> Result<(), String> {
    if !window.label().starts_with("image-preview-") { return Err("当前窗口不是图片预览".into()); }
    window.set_always_on_top(pinned).map_err(|_| "无法切换图片置顶状态".into())
}

pub fn forget(app: &AppHandle, label: &str) {
    if let Some(state) = app.try_state::<Previews>() {
        state.0.lock().unwrap_or_else(|e| e.into_inner()).remove(label);
    }
}
