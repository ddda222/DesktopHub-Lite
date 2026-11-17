//! 本机文件中转：仅持久化路径引用，移除条目不触碰原文件。
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use crate::AppState;

/// 只保留本次运行的最近十次移出，不写入用户文件或撤销历史文件。
#[derive(Default)]
pub struct ShelfHistory(pub std::sync::Mutex<Vec<Vec<(usize, ShelfEntry)>>>);

pub const INDEX_FILE: &str = "file-shelf.json";
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ShelfIndex { pub entries: Vec<ShelfEntry> }
#[derive(Clone, Serialize, Deserialize)]
pub struct ShelfEntry { pub id: String, pub path: String }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelfItem { id: String, path: String, name: String, is_dir: bool, available: bool }
#[derive(Serialize)]
pub struct AddResult { entries: Vec<ShelfItem>, added: usize }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelfSnapshot { entries: Vec<ShelfItem>, undo_count: usize }

fn snapshot(index: &ShelfIndex, history: &[Vec<(usize, ShelfEntry)>]) -> ShelfSnapshot {
    ShelfSnapshot { entries: items(index), undo_count: history.last().map_or(0, Vec::len) }
}

fn items(index: &ShelfIndex) -> Vec<ShelfItem> {
    index.entries.iter().map(|entry| {
        let path = Path::new(&entry.path);
        let metadata = path.metadata().ok();
        ShelfItem { id: entry.id.clone(), path: entry.path.clone(),
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| entry.path.clone()),
            is_dir: metadata.as_ref().is_some_and(|m| m.is_dir()), available: metadata.is_some() }
    }).collect()
}
#[tauri::command]
pub fn list_shelf_files(state: State<'_, AppState>, history: State<'_, ShelfHistory>) -> ShelfSnapshot {
    let index = state.file_shelf.lock().unwrap_or_else(|e| e.into_inner());
    snapshot(&index, &history.0.lock().unwrap_or_else(|e| e.into_inner()))
}
#[tauri::command]
pub fn add_shelf_files(app: AppHandle, paths: Vec<String>) -> Result<AddResult, String> {
    let mut incoming = Vec::new();
    for value in paths {
        let path = Path::new(&value);
        if !path.is_absolute() || !path.exists() { return Err("文件不存在或无法访问，请重新拖入".into()); }
        let normalized = value.replace('/', "\\");
        let id = format!("{:x}", Sha256::digest(normalized.to_lowercase().as_bytes()));
        incoming.push(ShelfEntry { id, path: normalized });
    }
    let state = app.state::<AppState>();
    let mut current = state.file_shelf.lock().unwrap_or_else(|e| e.into_inner());
    let mut next = current.clone();
    let before = next.entries.len();
    for entry in incoming {
        if !next.entries.iter().any(|old| old.id == entry.id) { next.entries.push(entry); }
    }
    let added = next.entries.len() - before;
    if added > 0 { state.store.write(INDEX_FILE, &next)?; *current = next; }
    let entries = items(&current);
    drop(current);
    if added > 0 { let _ = app.emit("file-shelf-changed", ()); }
    Ok(AddResult { entries, added })
}
#[tauri::command]
pub fn remove_shelf_files(app: AppHandle, ids: Vec<String>) -> Result<ShelfSnapshot, String> {
    let state = app.state::<AppState>();
    let mut current = state.file_shelf.lock().unwrap_or_else(|e| e.into_inner());
    let mut next = current.clone();
    let removed: Vec<_> = next.entries.iter().enumerate().filter(|(_, entry)| ids.contains(&entry.id))
        .map(|(position, entry)| (position, entry.clone())).collect();
    let history_state = app.state::<ShelfHistory>();
    let mut history = history_state.0.lock().unwrap_or_else(|e| e.into_inner());
    if removed.is_empty() { return Ok(snapshot(&current, &history)); }
    next.entries.retain(|entry| !ids.contains(&entry.id));
    state.store.write(INDEX_FILE, &next)?;
    *current = next;
    history.push(removed);
    if history.len() > 10 { history.remove(0); }
    let result = snapshot(&current, &history);
    drop(history);
    drop(current);
    let _ = app.emit("file-shelf-changed", ());
    Ok(result)
}

fn restore_entries(index: &mut ShelfIndex, removed: &[(usize, ShelfEntry)]) {
    for (position, entry) in removed {
        if !index.entries.iter().any(|existing| existing.id == entry.id) {
            index.entries.insert((*position).min(index.entries.len()), entry.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn entry(id: &str) -> ShelfEntry { ShelfEntry { id: id.into(), path: format!("D:\\sample\\{id}") } }
    fn ids(index: &ShelfIndex) -> Vec<&str> { index.entries.iter().map(|item| item.id.as_str()).collect() }
    #[test]
    fn undo_non_contiguous_batch_keeps_order() {
        let mut index = ShelfIndex { entries: vec![entry("b"), entry("d"), entry("new")] };
        restore_entries(&mut index, &[(0, entry("a")), (2, entry("c"))]);
        assert_eq!(ids(&index), vec!["a", "b", "c", "d", "new"]);
    }
    #[test]
    fn undo_keeps_readded_entry_and_restores_missing_reference() {
        let mut index = ShelfIndex { entries: vec![entry("a")] };
        restore_entries(&mut index, &[(0, entry("a")), (5, entry("missing"))]);
        assert_eq!(ids(&index), vec!["a", "missing"]);
    }
    #[test]
    fn consecutive_undo_restores_original_list() {
        let mut index = ShelfIndex { entries: vec![] };
        restore_entries(&mut index, &[(0, entry("b"))]);
        restore_entries(&mut index, &[(0, entry("a")), (2, entry("c"))]);
        assert_eq!(ids(&index), vec!["a", "b", "c"]);
    }
}

#[tauri::command]
pub fn undo_shelf_remove(app: AppHandle) -> Result<ShelfSnapshot, String> {
    let state = app.state::<AppState>();
    let mut current = state.file_shelf.lock().unwrap_or_else(|e| e.into_inner());
    let history_state = app.state::<ShelfHistory>();
    let mut history = history_state.0.lock().unwrap_or_else(|e| e.into_inner());
    let removed = history.last().ok_or("没有可以撤销的移出操作")?;
    let mut next = current.clone();
    restore_entries(&mut next, removed);
    // 文件失效也恢复引用；列表重新检查可用性。保存成功后才消费撤销记录。
    state.store.write(INDEX_FILE, &next)?;
    *current = next;
    history.pop();
    let result = snapshot(&current, &history);
    drop(history);
    drop(current);
    let _ = app.emit("file-shelf-changed", ());
    Ok(result)
}
#[tauri::command]
pub async fn pick_shelf_files(app: AppHandle) -> Result<Option<Vec<String>>, String> {
    use tauri_plugin_dialog::DialogExt;
    let dialog = app.dialog().file();
    tauri::async_runtime::spawn_blocking(move || dialog.blocking_pick_files().map(|files|
        files.into_iter().filter_map(|f| f.into_path().ok()).map(|p| p.to_string_lossy().into_owned()).collect()))
        .await.map_err(|_| "无法打开文件选择框".into())
}
pub(crate) fn selected_paths(state: &AppState, ids: &[String]) -> Result<Vec<String>, String> {
    let index = state.file_shelf.lock().unwrap_or_else(|e| e.into_inner());
    ids.iter().map(|id| {
        let entry = index.entries.iter().find(|e| &e.id == id).ok_or("中转条目已移除")?;
        if !Path::new(&entry.path).exists() { return Err("原文件已移动或删除，请重新添加".into()); }
        Ok(entry.path.clone())
    }).collect()
}
#[tauri::command]
pub fn reveal_shelf_file(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let paths = selected_paths(&state, &[id])?;
    crate::launcher::reveal(&paths[0])
}
#[tauri::command]
pub async fn drag_shelf_files(app: AppHandle, window: WebviewWindow, ids: Vec<String>) -> Result<bool, String> {
    if ids.is_empty() { return Err("请先选择文件".into()); }
    let paths = selected_paths(&app.state::<AppState>(), &ids)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    window.run_on_main_thread(move || { let _ = tx.send(native_drag(&paths)); })
        .map_err(|_| "无法启动文件拖出".to_string())?;
    rx.await.map_err(|_| "文件拖出未完成".to_string())?
}

fn native_drag(paths: &[String]) -> Result<bool, String> {
    use windows::{core::HSTRING, Win32::{System::{Com::IDataObject, Ole::{OleInitialize, OleUninitialize, DROPEFFECT_COPY}},
        UI::Shell::{ILCreateFromPathW, ILFree, SHCreateShellItemArrayFromIDLists, SHDoDragDrop, BHID_DataObject}}};
    unsafe {
        OleInitialize(None).map_err(|_| "Windows 拖放暂不可用".to_string())?;
        let result = (|| {
            let pidls: Vec<_> = paths.iter().map(|p| ILCreateFromPathW(&HSTRING::from(p))).collect();
            let array = if pidls.iter().any(|p| p.is_null()) { Err("无法读取文件拖放信息".to_string()) }
                else { SHCreateShellItemArrayFromIDLists(&pidls.iter().map(|p| *p as *const _).collect::<Vec<_>>())
                    .map_err(|_| "无法创建文件拖放对象".to_string()) };
            for pidl in pidls { if !pidl.is_null() { ILFree(Some(pidl)); } }
            let array = array?;
            let data: IDataObject = array.BindToHandler(None, &BHID_DataObject).map_err(|_| "文件拖放格式不受支持".to_string())?;
            // 仅允许复制效果，防止拖出时移动／删除原文件。取消不报成功。
            let effect = SHDoDragDrop(None, &data, None, DROPEFFECT_COPY).map_err(|_| "文件拖出失败".to_string())?;
            Ok(effect.0 != 0)
        })();
        OleUninitialize();
        result
    }
}
