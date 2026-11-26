//! 数据目录定位、历史旧版扫描与事务级安全迁移。
//!
//! 设计目标：
//! - 将程序文件与用户数据分离：默认共享固定数据目录（Windows 上有 D 盘时优先 `D:\DesktopHub-Data`，
//!   否则回退到 `%LOCALAPPDATA%\com.desktophub.lite\data`），仅在应用本地数据目录保存极小的指针文件。
//! - 兼容已存在真实数据的旧版便携目录与显式隔离目录，绝不静默丢弃或覆盖旧数据。
//! - 所有跨目录复制、回填均只读写白名单内的用户数据文件与子目录，先完整备份再原子写入并核验大小，
//!   绝不把日志、临时文件或旧内存状态覆写到目标位置。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const POINTER_FILE: &str = "data-location.json";
pub const DEFAULT_WIN_DATA_DIR: &str = r"D:\DesktopHub-Data";
pub const LEGACY_APPLY_DATA_DIR: &str = r"D:\apply\DesktopHub-Lite\data";
pub const ISOLATED_FLAG: &str = "portable-isolated.flag";
pub const LEGACY_PORTABLE_FLAG: &str = "portable.flag";

pub const USER_JSON_FILES: [&str; 12] = [
    "applications.json",
    "ui-preferences.json",
    "clipboard-index.json",
    "snippets.json",
    "translation.json",
    "mail-accounts.json",
    "file-shelf.json",
    "audio-profiles.json",
    "shortcuts.json",
    "window-state.json",
    "app-pack.json",
    "pack-scripts.json",
];

pub const USER_SUBDIRS: [&str; 2] = ["clipboard-images", "icons"];

const SEED_APP_IDS: [&str; 8] = [
    "a-explorer",
    "a-notepad",
    "a-cmd",
    "a-powershell",
    "a-mspaint",
    "a-calc",
    "a-taskmgr",
    "a-control",
];

const SEED_CATEGORY_IDS: [&str; 5] = ["all", "c-common", "c-dev", "c-media", "c-work"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DataLocationPointer {
    pub version: u32,
    #[serde(default)]
    pub shared_data_dir: Option<String>,
    #[serde(default)]
    pub recent_dirs: Vec<String>,
}

impl Default for DataLocationPointer {
    fn default() -> Self {
        Self {
            version: 1,
            shared_data_dir: None,
            recent_dirs: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedDataRoot {
    pub path: PathBuf,
    pub mode: String,
    pub pointer_file: PathBuf,
    pub default_shared_dir: PathBuf,
    pub should_persist_pointer: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DataDirSummary {
    pub exists: bool,
    pub has_meaningful_data: bool,
    pub app_count: usize,
    pub category_count: usize,
    pub clip_count: usize,
    pub clip_image_count: usize,
    pub snippet_count: usize,
    pub mail_account_count: usize,
    pub shelf_count: usize,
    pub audio_profile_count: usize,
    pub has_translation_config: bool,
    pub has_ui_preferences: bool,
    pub has_shortcuts: bool,
    pub icon_cache_count: usize,
    pub total_files: usize,
    pub total_bytes: u64,
    pub corrupted_files: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CandidateOldDir {
    pub path: String,
    pub label: String,
    pub summary: DataDirSummary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyReport {
    pub copied_files: usize,
    pub copied_bytes: u64,
    pub backup_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoMigrateOutcome {
    pub source_dir: PathBuf,
    pub copied_files: usize,
    pub copied_bytes: u64,
    pub backup_dir: Option<PathBuf>,
}

pub fn default_shared_data_dir(fallback_app_local_dir: &Path) -> PathBuf {
    #[cfg(windows)]
    if Path::new(r"D:\").exists() {
        return PathBuf::from(DEFAULT_WIN_DATA_DIR);
    }
    fallback_app_local_dir.join("data")
}

#[cfg(windows)]
fn is_reparse_point(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_meta: &fs::Metadata) -> bool {
    false
}

fn ensure_safe_existing_dir(path: &Path, label: &str) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!("{label}不存在")
        } else {
            format!("无法访问{label}：{e}")
        }
    })?;
    if meta.file_type().is_symlink() || is_reparse_point(&meta) {
        return Err(format!("{label}不能是符号链接或目录联接"));
    }
    if !meta.file_type().is_dir() {
        return Err(format!("{label}不是有效文件夹"));
    }
    Ok(())
}

fn ensure_safe_target_ancestors(target: &Path) -> Result<(), String> {
    if target.as_os_str().is_empty() {
        return Err("目标目录路径不能为空".into());
    }
    if target.exists() {
        return ensure_safe_existing_dir(target, "目标目录");
    }
    let mut probe = target.parent();
    while let Some(cur) = probe {
        if cur.exists() {
            return ensure_safe_existing_dir(cur, "目标父目录");
        }
        probe = cur.parent();
    }
    Err("目标目录所在的磁盘或根目录不存在".into())
}

fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

fn resolve_logical_path(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            break;
        };
        tail.push(name.to_os_string());
        let Some(parent) = existing.parent() else {
            break;
        };
        existing = parent.to_path_buf();
    }
    let base = if existing.exists() {
        fs::canonicalize(&existing)
            .map(|p| strip_verbatim_prefix(&p))
            .unwrap_or_else(|_| strip_verbatim_prefix(&existing))
    } else {
        strip_verbatim_prefix(&existing)
    };
    let mut out = PathBuf::new();
    for comp in base.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    for seg in tail.into_iter().rev() {
        out.push(seg);
    }
    out
}

fn path_key(path: &Path) -> String {
    let resolved = resolve_logical_path(path);
    let text = resolved.to_string_lossy().replace('/', "\\");
    let trimmed = text.trim_end_matches('\\');
    #[cfg(windows)]
    {
        trimmed.to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        trimmed.to_string()
    }
}

pub fn paths_overlap(a: &Path, b: &Path) -> bool {
    let ra = resolve_logical_path(a);
    let rb = resolve_logical_path(b);
    let ka = path_key(&ra);
    let kb = path_key(&rb);
    if ka == kb {
        return true;
    }
    let a_prefix = format!("{ka}\\");
    let b_prefix = format!("{kb}\\");
    kb.starts_with(&a_prefix) || ka.starts_with(&b_prefix)
}

pub fn load_pointer(pointer_file: &Path) -> DataLocationPointer {
    let Ok(text) = fs::read_to_string(pointer_file) else {
        return DataLocationPointer::default();
    };
    let Ok(mut pointer) = serde_json::from_str::<DataLocationPointer>(&text) else {
        return DataLocationPointer::default();
    };
    if pointer.version == 0 {
        pointer.version = 1;
    }
    pointer.shared_data_dir = pointer
        .shared_data_dir
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    pointer
}

pub fn save_pointer(pointer_file: &Path, pointer: &DataLocationPointer) -> Result<(), String> {
    if let Some(parent) = pointer_file.parent() {
        fs::create_dir_all(parent).map_err(|_| "无法创建数据位置配置目录")?;
    }
    let bytes = serde_json::to_vec_pretty(pointer).map_err(|_| "无法序列化数据位置配置")?;
    crate::storage::write_file_atomic(pointer_file, &bytes)
}

pub fn record_recent_dir(pointer_file: &Path, dir: &Path) {
    if !has_meaningful_data(dir) {
        return;
    }
    let mut pointer = load_pointer(pointer_file);
    let current_key = path_key(dir);
    let display = strip_verbatim_prefix(&resolve_logical_path(dir))
        .to_string_lossy()
        .into_owned();
    let mut next = vec![display];
    for item in &pointer.recent_dirs {
        if path_key(Path::new(item)) != current_key && next.len() < 8 {
            next.push(item.clone());
        }
    }
    if next != pointer.recent_dirs {
        pointer.recent_dirs = next;
        let _ = save_pointer(pointer_file, &pointer);
    }
}

fn read_json_value(path: &Path) -> Result<Option<Value>, ()> {
    let meta = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    if !meta.file_type().is_file() || is_reparse_point(&meta) {
        return Err(());
    }
    let text = fs::read_to_string(path).map_err(|_| ())?;
    let val = serde_json::from_str::<Value>(&text).map_err(|_| ())?;
    Ok(Some(val))
}

fn count_regular_files_in_dir(dir: &Path) -> Result<(usize, u64), String> {
    let meta = match fs::symlink_metadata(dir) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(e) => return Err(format!("无法访问目录：{e}")),
    };
    if meta.file_type().is_symlink() || is_reparse_point(&meta) || !meta.file_type().is_dir() {
        return Err("子目录不是普通文件夹".into());
    }
    let mut count = 0usize;
    let mut bytes = 0u64;
    let entries = fs::read_dir(dir).map_err(|e| format!("无法读取子目录：{e}"))?;
    for entry in entries.flatten() {
        let Ok(m) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if m.file_type().is_file() && !is_reparse_point(&m) {
            count += 1;
            bytes = bytes.saturating_add(m.len());
        }
    }
    Ok((count, bytes))
}

/// 判断目录是否包含用户真实使用过的数据（排除首次启动生成的空白种子文件）。
pub fn has_meaningful_data(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    for marker in [
        "ui-preferences.json",
        "translation.json",
        "audio-profiles.json",
        "file-shelf.json",
        "shortcuts.json",
    ] {
        if dir.join(marker).is_file() {
            return true;
        }
    }
    if let Ok(Some(val)) = read_json_value(&dir.join("clipboard-index.json")) {
        let entries_non_empty = val
            .get("entries")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty());
        let custom_groups = val
            .get("groups")
            .and_then(Value::as_array)
            .is_some_and(|a| a.len() > 1);
        if entries_non_empty || custom_groups {
            return true;
        }
    }
    if let Ok(Some(val)) = read_json_value(&dir.join("snippets.json")) {
        if val
            .get("entries")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
        {
            return true;
        }
    }
    if let Ok(Some(val)) = read_json_value(&dir.join("mail-accounts.json")) {
        if val
            .get("accounts")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
        {
            return true;
        }
    }
    if let Ok((img_count, _)) = count_regular_files_in_dir(&dir.join("clipboard-images")) {
        if img_count > 0 {
            return true;
        }
    }
    if let Ok(Some(val)) = read_json_value(&dir.join("applications.json")) {
        let has_custom_app = val
            .get("apps")
            .and_then(Value::as_array)
            .is_some_and(|apps| {
                apps.iter().any(|item| {
                    item.get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| !SEED_APP_IDS.contains(&id))
                })
            });
        let has_custom_cat = val
            .get("categories")
            .and_then(Value::as_array)
            .is_some_and(|cats| {
                cats.iter().any(|item| {
                    item.get("id")
                        .and_then(Value::as_str)
                        .is_some_and(|id| !SEED_CATEGORY_IDS.contains(&id))
                })
            });
        if has_custom_app || has_custom_cat {
            return true;
        }
    }
    if let Ok(Some(val)) = read_json_value(&dir.join("app-pack.json")) {
        let non_empty_str = |key: &str| {
            val.get(key)
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty())
        };
        let has_paths = [
            "cli",
            "hbuilderRoot",
            "project",
            "packagename",
            "outputDir",
            "adb",
            "device",
        ]
        .iter()
        .any(|k| non_empty_str(k));
        let auto_install = val.get("autoInstall").and_then(Value::as_bool).unwrap_or(false);
        let custom_port = val
            .get("reversePort")
            .and_then(Value::as_u64)
            .is_some_and(|p| p != 11034);
        let has_ignore_files = val
            .get("remoteIgnoreFiles")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty());
        if has_paths || auto_install || custom_port || has_ignore_files {
            return true;
        }
    }
    if let Ok(Some(val)) = read_json_value(&dir.join("pack-scripts.json")) {
        if let Some(scripts) = val.get("scripts").and_then(Value::as_array) {
            let has_custom = scripts.iter().any(|item| {
                let id = item.get("id").and_then(Value::as_str).unwrap_or("");
                let is_builtin = matches!(
                    id,
                    "builtin-local-reverse" | "builtin-local-pack" | "builtin-remote-pack"
                );
                let has_history = item
                    .get("history")
                    .and_then(Value::as_array)
                    .is_some_and(|h| !h.is_empty());
                !is_builtin || has_history
            });
            if has_custom {
                return true;
            }
        }
    }
    false
}

pub fn resolve_data_root_with_default(
    env_override: Option<PathBuf>,
    exe_dir: Option<&Path>,
    app_local_dir: &Path,
    default_shared_dir: PathBuf,
) -> ResolvedDataRoot {
    let pointer_file = app_local_dir.join(POINTER_FILE);
    if let Some(dir) = env_override.filter(|p| !p.as_os_str().is_empty()) {
        return ResolvedDataRoot {
            path: dir,
            mode: "env".into(),
            pointer_file,
            default_shared_dir,
            should_persist_pointer: false,
        };
    }
    if let Some(dir) = exe_dir {
        if dir.join(ISOLATED_FLAG).exists() {
            return ResolvedDataRoot {
                path: dir.join("data"),
                mode: "isolated".into(),
                pointer_file,
                default_shared_dir,
                should_persist_pointer: false,
            };
        }
    }
    let pointer = load_pointer(&pointer_file);
    if let Some(shared) = pointer.shared_data_dir {
        return ResolvedDataRoot {
            path: PathBuf::from(shared),
            mode: "shared".into(),
            pointer_file,
            default_shared_dir,
            should_persist_pointer: false,
        };
    }
    if let Some(dir) = exe_dir {
        let legacy_data = dir.join("data");
        if dir.join(LEGACY_PORTABLE_FLAG).exists() && has_meaningful_data(&legacy_data) {
            return ResolvedDataRoot {
                path: legacy_data,
                mode: "legacy_portable".into(),
                pointer_file,
                default_shared_dir,
                should_persist_pointer: false,
            };
        }
    }
    ResolvedDataRoot {
        path: default_shared_dir.clone(),
        mode: "shared".into(),
        pointer_file,
        default_shared_dir,
        should_persist_pointer: true,
    }
}

pub fn resolve_data_root(
    env_override: Option<PathBuf>,
    exe_dir: Option<&Path>,
    app_local_dir: &Path,
) -> ResolvedDataRoot {
    let default_shared = default_shared_data_dir(app_local_dir);
    resolve_data_root_with_default(env_override, exe_dir, app_local_dir, default_shared)
}

/// 兼容用户选择旧版程序根目录（自动识别下级 `data/`）或直接选择 `data` 目录。
pub fn normalize_source_data_dir(selected: &Path) -> Result<PathBuf, String> {
    ensure_safe_existing_dir(selected, "所选目录")?;
    if USER_JSON_FILES
        .iter()
        .any(|name| selected.join(name).is_file())
    {
        return Ok(selected.to_path_buf());
    }
    let nested = selected.join("data");
    if nested.exists() {
        ensure_safe_existing_dir(&nested, "数据子目录")?;
        if USER_JSON_FILES
            .iter()
            .any(|name| nested.join(name).is_file())
        {
            return Ok(nested);
        }
    }
    Err("所选文件夹中未找到 DesktopHub Lite 的数据文件，请选择旧版程序目录或其中的 data 文件夹".into())
}

/// 扫描并生成脱敏统计摘要：绝不读取或暴露剪贴板正文、图片字节、应用目标路径或任何密钥。
pub fn inspect_data_dir(dir: &Path) -> Result<DataDirSummary, String> {
    if !dir.exists() {
        return Ok(DataDirSummary::default());
    }
    ensure_safe_existing_dir(dir, "数据目录")?;
    let mut summary = DataDirSummary {
        exists: true,
        has_meaningful_data: has_meaningful_data(dir),
        ..Default::default()
    };

    for name in USER_JSON_FILES {
        let path = dir.join(name);
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                summary.corrupted_files.push(name.into());
                continue;
            }
        };
        if !meta.file_type().is_file() || is_reparse_point(&meta) {
            summary.corrupted_files.push(name.into());
            continue;
        }
        summary.total_files += 1;
        summary.total_bytes = summary.total_bytes.saturating_add(meta.len());
        match read_json_value(&path) {
            Ok(Some(val)) => match name {
                "applications.json" => {
                    summary.app_count = val
                        .get("apps")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                    summary.category_count = val
                        .get("categories")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                }
                "clipboard-index.json" => {
                    summary.clip_count = val
                        .get("entries")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                }
                "snippets.json" => {
                    summary.snippet_count = val
                        .get("entries")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                }
                "mail-accounts.json" => {
                    summary.mail_account_count = val
                        .get("accounts")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                }
                "file-shelf.json" => {
                    summary.shelf_count = val
                        .get("entries")
                        .and_then(Value::as_array)
                        .map_or(0, Vec::len);
                }
                "audio-profiles.json" => {
                    summary.audio_profile_count = val.as_array().map_or(0, Vec::len);
                }
                "translation.json" => summary.has_translation_config = true,
                "ui-preferences.json" => summary.has_ui_preferences = true,
                "shortcuts.json" => summary.has_shortcuts = true,
                _ => {}
            },
            _ => summary.corrupted_files.push(name.into()),
        }
    }

    if let Ok((count, bytes)) = count_regular_files_in_dir(&dir.join("clipboard-images")) {
        summary.clip_image_count = count;
        summary.total_files += count;
        summary.total_bytes = summary.total_bytes.saturating_add(bytes);
    }
    if let Ok((count, bytes)) = count_regular_files_in_dir(&dir.join("icons")) {
        summary.icon_cache_count = count;
        summary.total_files += count;
        summary.total_bytes = summary.total_bytes.saturating_add(bytes);
    }

    Ok(summary)
}

/// 收集开机启动项指向目录以及历史固定安装目录等额外候选数据路径（只读检测）。
pub fn collect_extra_hint_dirs(startup_exe_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = startup_exe_dir {
        dirs.push(dir.join("data"));
    }
    #[cfg(windows)]
    if Path::new(LEGACY_APPLY_DATA_DIR).is_dir() {
        dirs.push(PathBuf::from(LEGACY_APPLY_DATA_DIR));
    }
    dirs
}

fn data_richness_score(summary: &DataDirSummary) -> usize {
    summary.app_count
        + summary.clip_count
        + summary.snippet_count
        + summary.mail_account_count
        + summary.shelf_count
        + summary.audio_profile_count
}

pub fn discover_candidate_old_dirs_with_hints(
    current_dir: &Path,
    exe_dir: Option<&Path>,
    app_local_dir: &Path,
    default_shared_dir: &Path,
    pointer: &DataLocationPointer,
    extra_hint_dirs: &[PathBuf],
) -> Vec<CandidateOldDir> {
    let mut raw_candidates: Vec<(PathBuf, String)> = Vec::new();
    for item in &pointer.recent_dirs {
        let path = PathBuf::from(item);
        let label = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| format!("曾用目录（{}）", n.to_string_lossy()))
            .unwrap_or_else(|| "曾用数据目录".into());
        raw_candidates.push((path, label));
    }
    for path in extra_hint_dirs {
        let label = path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| format!("历史安装目录（{}）", n.to_string_lossy()))
            .unwrap_or_else(|| "历史安装目录".into());
        raw_candidates.push((path.clone(), label));
    }
    raw_candidates.push((default_shared_dir.to_path_buf(), "推荐共享目录".into()));
    raw_candidates.push((app_local_dir.join("data"), "默认本地数据目录".into()));

    if let Some(dir) = exe_dir {
        raw_candidates.push((dir.join("data"), "当前程序旁 data 目录".into()));
        if let Some(parent) = dir.parent() {
            if let Ok(entries) = fs::read_dir(parent) {
                let mut siblings: Vec<(std::time::SystemTime, PathBuf, String)> = Vec::new();
                for entry in entries.flatten().take(80) {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if !name.to_ascii_lowercase().starts_with("desktophub-lite") {
                        continue;
                    }
                    let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                        continue;
                    };
                    if !meta.file_type().is_dir() || is_reparse_point(&meta) {
                        continue;
                    }
                    let modified = meta.modified().unwrap_or(UNIX_EPOCH);
                    siblings.push((modified, entry.path().join("data"), name));
                }
                siblings.sort_by(|a, b| b.0.cmp(&a.0));
                for (_, data_dir, name) in siblings.into_iter().take(30) {
                    raw_candidates.push((data_dir, format!("历史版本（{name}）")));
                }
            }
        }
    }

    let current_key = path_key(current_dir);
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(current_key);

    let mut result: Vec<CandidateOldDir> = Vec::new();
    for (path, label) in raw_candidates {
        let key = path_key(&path);
        if !seen.insert(key) {
            continue;
        }
        let Ok(summary) = inspect_data_dir(&path) else {
            continue;
        };
        if !summary.exists || !summary.has_meaningful_data {
            continue;
        }
        let clean_path = strip_verbatim_prefix(&resolve_logical_path(&path))
            .to_string_lossy()
            .into_owned();
        result.push(CandidateOldDir {
            path: clean_path,
            label,
            summary,
        });
    }
    result.sort_by(|a, b| {
        a.summary
            .corrupted_files
            .is_empty()
            .cmp(&b.summary.corrupted_files.is_empty())
            .reverse()
            .then_with(|| data_richness_score(&b.summary).cmp(&data_richness_score(&a.summary)))
            .then_with(|| b.summary.total_files.cmp(&a.summary.total_files))
            .then_with(|| b.summary.total_bytes.cmp(&a.summary.total_bytes))
    });
    result.truncate(6);
    result
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn discover_candidate_old_dirs(
    current_dir: &Path,
    exe_dir: Option<&Path>,
    app_local_dir: &Path,
    default_shared_dir: &Path,
    pointer: &DataLocationPointer,
) -> Vec<CandidateOldDir> {
    discover_candidate_old_dirs_with_hints(
        current_dir,
        exe_dir,
        app_local_dir,
        default_shared_dir,
        pointer,
        &[],
    )
}

fn copy_and_verify_file(src: &Path, dst: &Path) -> Result<u64, String> {
    let bytes = fs::read(src).map_err(|_| format!("无法读取文件 {}", src.display()))?;
    crate::storage::write_file_atomic(dst, &bytes)?;
    let written_len = fs::metadata(dst)
        .map_err(|_| format!("无法验证已写入文件 {}", dst.display()))?
        .len();
    if written_len != bytes.len() as u64 {
        return Err(format!("文件写入校验失败：{}", dst.display()));
    }
    Ok(written_len)
}

fn target_has_existing_user_data(target: &Path) -> bool {
    if USER_JSON_FILES
        .iter()
        .any(|name| target.join(name).exists())
    {
        return true;
    }
    USER_SUBDIRS.iter().any(|sub| {
        count_regular_files_in_dir(&target.join(sub))
            .map(|(count, _)| count > 0)
            .unwrap_or(false)
    })
}

fn backup_target_data(target: &Path) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "系统时间无效")?
        .as_millis();
    let backup_dir = target
        .join("migration-backups")
        .join(format!("before-migrate-{timestamp}-{}", std::process::id()));
    fs::create_dir_all(&backup_dir).map_err(|_| "无法创建迁移前备份目录，已中止操作")?;

    for name in USER_JSON_FILES {
        let src = target.join(name);
        if src.is_file() {
            copy_and_verify_file(&src, &backup_dir.join(name))?;
        }
    }
    for sub in USER_SUBDIRS {
        let src_sub = target.join(sub);
        if !src_sub.exists() {
            continue;
        }
        ensure_safe_existing_dir(&src_sub, "目标原有子目录")?;
        let dst_sub = backup_dir.join(sub);
        fs::create_dir_all(&dst_sub).map_err(|_| "无法创建子目录备份")?;
        let entries = fs::read_dir(&src_sub).map_err(|_| "无法读取原有子目录")?;
        for entry in entries.flatten() {
            let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.file_type().is_file() && !is_reparse_point(&meta) {
                copy_and_verify_file(&entry.path(), &dst_sub.join(entry.file_name()))?;
            }
        }
    }
    Ok(backup_dir)
}

/// 事务级复制用户白名单数据到目标目录，必要时先完整备份目标原有数据并逐文件核对字节大小。
pub fn copy_user_data_to(
    source: &Path,
    target: &Path,
    backup_existing_target: bool,
) -> Result<CopyReport, String> {
    ensure_safe_existing_dir(source, "源数据目录")?;
    ensure_safe_target_ancestors(target)?;
    if paths_overlap(source, target) {
        return Err("源目录与目标目录相同或存在包含关系，无法执行迁移".into());
    }

    let summary = inspect_data_dir(source)?;
    if !summary.corrupted_files.is_empty() {
        return Err(format!(
            "源数据目录存在损坏的配置文件，已停止迁移：{}",
            summary.corrupted_files.join("、")
        ));
    }
    if summary.total_files == 0 {
        return Err("源数据目录中没有可迁移的数据文件".into());
    }

    fs::create_dir_all(target).map_err(|e| format!("无法创建目标数据目录：{e}"))?;
    ensure_safe_existing_dir(target, "目标目录")?;

    let backup_dir = if backup_existing_target && target_has_existing_user_data(target) {
        Some(backup_target_data(target)?)
    } else {
        None
    };

    let mut copied_files = 0usize;
    let mut copied_bytes = 0u64;

    for name in USER_JSON_FILES {
        let src = source.join(name);
        if src.is_file() {
            copied_bytes = copied_bytes.saturating_add(copy_and_verify_file(&src, &target.join(name))?);
            copied_files += 1;
        }
    }

    for sub in USER_SUBDIRS {
        let src_sub = source.join(sub);
        if !src_sub.exists() {
            continue;
        }
        ensure_safe_existing_dir(&src_sub, "源子目录")?;
        let dst_sub = target.join(sub);
        fs::create_dir_all(&dst_sub).map_err(|_| "无法创建目标子目录")?;
        ensure_safe_existing_dir(&dst_sub, "目标子目录")?;
        let entries = fs::read_dir(&src_sub).map_err(|_| "无法读取源子目录")?;
        for entry in entries.flatten() {
            let Ok(meta) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if meta.file_type().is_file() && !is_reparse_point(&meta) {
                let bytes = copy_and_verify_file(&entry.path(), &dst_sub.join(entry.file_name()))?;
                copied_bytes = copied_bytes.saturating_add(bytes);
                copied_files += 1;
            }
        }
    }

    Ok(CopyReport {
        copied_files,
        copied_bytes,
        backup_dir,
    })
}

/// 首次启用尚无真实数据的共享目录时，自动从最佳完好历史目录迁入用户数据（保留原目录不动）。
pub fn auto_migrate_empty_shared_dir(
    target_dir: &Path,
    exe_dir: Option<&Path>,
    app_local_dir: &Path,
    default_shared_dir: &Path,
    pointer: &DataLocationPointer,
    extra_hint_dirs: &[PathBuf],
) -> Result<Option<AutoMigrateOutcome>, String> {
    // 核心守卫：目标目录只要已有任意真实用户数据，就绝不自动覆盖。
    if has_meaningful_data(target_dir) {
        return Ok(None);
    }
    let candidates = discover_candidate_old_dirs_with_hints(
        target_dir,
        exe_dir,
        app_local_dir,
        default_shared_dir,
        pointer,
        extra_hint_dirs,
    );
    let Some(best) = candidates
        .into_iter()
        .find(|item| item.summary.corrupted_files.is_empty() && item.summary.has_meaningful_data)
    else {
        return Ok(None);
    };
    let source_dir = PathBuf::from(&best.path);
    let report = copy_user_data_to(&source_dir, target_dir, true)?;
    Ok(Some(AutoMigrateOutcome {
        source_dir,
        copied_files: report.copied_files,
        copied_bytes: report.copied_bytes,
        backup_dir: report.backup_dir,
    }))
}

// === TAURI_COMMANDS ===

use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

pub struct DataLocationState {
    pub mode: String,
    pub pointer_file: PathBuf,
    pub default_shared_dir: PathBuf,
    pub exe_dir: Option<PathBuf>,
    pub app_local_dir: PathBuf,
    pub extra_hint_dirs: Vec<PathBuf>,
    pub auto_migrated_from: Option<String>,
    pub auto_migrated_files: usize,
    pub pending_restart: AtomicBool,
}

impl DataLocationState {
    pub fn new(
        resolved: ResolvedDataRoot,
        exe_dir: Option<PathBuf>,
        app_local_dir: PathBuf,
        extra_hint_dirs: Vec<PathBuf>,
        auto_migrated_from: Option<String>,
        auto_migrated_files: usize,
    ) -> Self {
        Self {
            mode: resolved.mode,
            pointer_file: resolved.pointer_file,
            default_shared_dir: resolved.default_shared_dir,
            exe_dir,
            app_local_dir,
            extra_hint_dirs,
            auto_migrated_from,
            auto_migrated_files,
            pending_restart: AtomicBool::new(false),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataStorageStatus {
    pub current_dir: String,
    pub default_shared_dir: String,
    pub mode: String,
    pub pending_restart: bool,
    pub auto_migrated_from: Option<String>,
    pub auto_migrated_files: usize,
    pub current_summary: DataDirSummary,
    pub candidates: Vec<CandidateOldDir>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchDataDirPreview {
    pub current_dir: String,
    pub target_dir: String,
    pub target_exists: bool,
    pub target_has_data: bool,
    pub current_summary: DataDirSummary,
    pub target_summary: DataDirSummary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchDataDirResult {
    pub target_dir: String,
    pub strategy: String,
    pub copied_files: usize,
    pub backup_dir: Option<String>,
    pub pending_restart: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOldDataPreview {
    pub source_dir: String,
    pub current_dir: String,
    pub source_summary: DataDirSummary,
    pub current_summary: DataDirSummary,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOldDataResult {
    pub source_dir: String,
    pub copied_files: usize,
    pub backup_dir: Option<String>,
    pub pending_restart: bool,
}

fn settings_only(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == crate::windows::SETTINGS {
        Ok(())
    } else {
        Err("请在设置窗口操作数据目录".into())
    }
}

fn display_clean(path: &Path) -> String {
    strip_verbatim_prefix(&resolve_logical_path(path))
        .to_string_lossy()
        .into_owned()
}

fn build_switch_preview(
    current_dir: &Path,
    target_dir: &Path,
) -> Result<SwitchDataDirPreview, String> {
    ensure_safe_target_ancestors(target_dir)?;
    if paths_overlap(current_dir, target_dir) {
        return Err("请选择与当前数据目录不同且无包含关系的文件夹".into());
    }
    let current_summary = inspect_data_dir(current_dir)?;
    let target_summary = inspect_data_dir(target_dir)?;
    Ok(SwitchDataDirPreview {
        current_dir: display_clean(current_dir),
        target_dir: display_clean(target_dir),
        target_exists: target_summary.exists,
        target_has_data: target_summary.has_meaningful_data || target_summary.total_files > 0,
        current_summary,
        target_summary,
    })
}

fn build_import_preview(
    current_dir: &Path,
    source_dir: &Path,
) -> Result<ImportOldDataPreview, String> {
    let normalized = normalize_source_data_dir(source_dir)?;
    if paths_overlap(&normalized, current_dir) {
        return Err("所选目录就是当前正在使用的数据目录，无需导入".into());
    }
    let source_summary = inspect_data_dir(&normalized)?;
    let current_summary = inspect_data_dir(current_dir)?;
    Ok(ImportOldDataPreview {
        source_dir: display_clean(&normalized),
        current_dir: display_clean(current_dir),
        source_summary,
        current_summary,
    })
}

#[tauri::command]
pub fn get_data_storage_status(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<DataStorageStatus, String> {
    settings_only(&window)?;
    let state = app.state::<crate::AppState>();
    let loc = app.state::<DataLocationState>();
    let current_dir = state.store.root();
    let pointer = load_pointer(&loc.pointer_file);
    let current_summary = inspect_data_dir(current_dir)?;
    let candidates = discover_candidate_old_dirs_with_hints(
        current_dir,
        loc.exe_dir.as_deref(),
        &loc.app_local_dir,
        &loc.default_shared_dir,
        &pointer,
        &loc.extra_hint_dirs,
    );
    Ok(DataStorageStatus {
        current_dir: display_clean(current_dir),
        default_shared_dir: display_clean(&loc.default_shared_dir),
        mode: loc.mode.clone(),
        pending_restart: loc.pending_restart.load(Ordering::Relaxed),
        auto_migrated_from: loc.auto_migrated_from.clone(),
        auto_migrated_files: loc.auto_migrated_files,
        current_summary,
        candidates,
    })
}

#[tauri::command]
pub fn open_current_data_dir(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    settings_only(&window)?;
    let root = app.state::<crate::AppState>().store.root().to_path_buf();
    crate::launcher::launch(&root.to_string_lossy())
}

#[tauri::command]
pub async fn pick_data_dir_for_switch(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Option<SwitchDataDirPreview>, String> {
    settings_only(&window)?;
    let dialog = app.dialog().file();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(folder) = dialog.blocking_pick_folder() else {
            return Ok(None);
        };
        let path = folder.into_path().map_err(|_| "只支持选择本机文件夹")?;
        let state = app.state::<crate::AppState>();
        build_switch_preview(state.store.root(), &path).map(Some)
    })
    .await
    .map_err(|_| "目录选择未完成")?
}

#[tauri::command]
pub fn preview_switch_data_dir(
    app: AppHandle,
    window: WebviewWindow,
    target_dir: String,
) -> Result<SwitchDataDirPreview, String> {
    settings_only(&window)?;
    let trimmed = target_dir.trim();
    if trimmed.is_empty() {
        crate::log_warn!("数据目录操作未完成（预览切换）：目标目录为空");
        return Err("请先指定目标目录".into());
    }
    let state = app.state::<crate::AppState>();
    build_switch_preview(state.store.root(), Path::new(trimmed)).map_err(|error| {
        crate::log_warn!("数据目录操作未完成（预览切换）：{error}");
        error
    })
}

#[tauri::command]
pub async fn apply_switch_data_dir(
    app: AppHandle,
    window: WebviewWindow,
    target_dir: String,
    strategy: String,
) -> Result<SwitchDataDirResult, String> {
    settings_only(&window)?;
    if strategy != "copy_current" && strategy != "use_existing" {
        crate::log_warn!("数据目录操作未完成（应用切换）：无效的切换方式");
        return Err("无效的切换方式".into());
    }
    let trimmed = target_dir.trim().to_string();
    if trimmed.is_empty() {
        crate::log_warn!("数据目录操作未完成（应用切换）：目标目录为空");
        return Err("请先指定目标目录".into());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::AppState>();
        let loc = app.state::<DataLocationState>();
        let _gate = state
            .configuration_gate
            .lock()
            .map_err(|_| "配置正在写入，请稍后重试")?;
        let current_dir = state.store.root();
        let target = PathBuf::from(&trimmed);
        ensure_safe_target_ancestors(&target)?;
        if paths_overlap(current_dir, &target) {
            return Err("请选择与当前数据目录不同且无包含关系的文件夹".into());
        }

        let (copied_files, backup_dir) = if strategy == "copy_current" {
            let report = copy_user_data_to(current_dir, &target, true)?;
            (report.copied_files, report.backup_dir)
        } else {
            ensure_safe_existing_dir(&target, "目标数据目录")?;
            let target_summary = inspect_data_dir(&target)?;
            if !target_summary.corrupted_files.is_empty() {
                return Err(format!(
                    "目标目录存在损坏文件，已停止切换：{}",
                    target_summary.corrupted_files.join("、")
                ));
            }
            (0, None)
        };

        let clean_target = display_clean(&target);
        let mut pointer = load_pointer(&loc.pointer_file);
        pointer.shared_data_dir = Some(clean_target.clone());
        save_pointer(&loc.pointer_file, &pointer)?;
        record_recent_dir(&loc.pointer_file, current_dir);

        state.running.store(false, Ordering::Relaxed);
        state.store.freeze_until_restart();
        loc.pending_restart.store(true, Ordering::Relaxed);

        crate::log_info!(
            "数据目录已设置为（{}）{}，等待重启生效",
            strategy,
            clean_target
        );
        Ok(SwitchDataDirResult {
            target_dir: clean_target,
            strategy,
            copied_files,
            backup_dir: backup_dir.map(|p| display_clean(&p)),
            pending_restart: true,
        })
    })
    .await
    .map_err(|_| "切换数据目录任务未完成".to_string())?;
    if let Err(ref error) = result {
        crate::log_warn!("数据目录操作未完成（应用切换）：{error}");
    }
    result
}

#[tauri::command]
pub async fn pick_old_data_dir_for_import(
    app: AppHandle,
    window: WebviewWindow,
) -> Result<Option<ImportOldDataPreview>, String> {
    settings_only(&window)?;
    let dialog = app.dialog().file();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(folder) = dialog.blocking_pick_folder() else {
            return Ok(None);
        };
        let path = folder.into_path().map_err(|_| "只支持选择本机文件夹")?;
        let state = app.state::<crate::AppState>();
        build_import_preview(state.store.root(), &path).map(Some)
    })
    .await
    .map_err(|_| "旧版目录选择未完成")?
}

#[tauri::command]
pub fn preview_old_data_import(
    app: AppHandle,
    window: WebviewWindow,
    source_dir: String,
) -> Result<ImportOldDataPreview, String> {
    settings_only(&window)?;
    let trimmed = source_dir.trim();
    if trimmed.is_empty() {
        crate::log_warn!("数据目录操作未完成（预览旧版导入）：源目录为空");
        return Err("请先指定旧版数据目录".into());
    }
    let state = app.state::<crate::AppState>();
    build_import_preview(state.store.root(), Path::new(trimmed)).map_err(|error| {
        crate::log_warn!("数据目录操作未完成（预览旧版导入）：{error}");
        error
    })
}

#[tauri::command]
pub async fn apply_old_data_import(
    app: AppHandle,
    window: WebviewWindow,
    source_dir: String,
) -> Result<ImportOldDataResult, String> {
    settings_only(&window)?;
    let trimmed = source_dir.trim().to_string();
    if trimmed.is_empty() {
        crate::log_warn!("数据目录操作未完成（应用旧版导入）：源目录为空");
        return Err("请先指定旧版数据目录".into());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<crate::AppState>();
        let loc = app.state::<DataLocationState>();
        let _gate = state
            .configuration_gate
            .lock()
            .map_err(|_| "配置正在写入，请稍后重试")?;
        let current_dir = state.store.root();
        let normalized_source = normalize_source_data_dir(Path::new(&trimmed))?;
        let was_running = state.running.swap(false, Ordering::Relaxed);

        let report = match copy_user_data_to(&normalized_source, current_dir, true) {
            Ok(report) => report,
            Err(error) => {
                state.running.store(was_running, Ordering::Relaxed);
                return Err(error);
            }
        };

        state.store.freeze_until_restart();
        loc.pending_restart.store(true, Ordering::Relaxed);
        record_recent_dir(&loc.pointer_file, &normalized_source);

        let clean_source = display_clean(&normalized_source);
        crate::log_info!(
            "已从旧版目录导入 {} 个文件至当前数据目录，等待重启生效",
            report.copied_files
        );
        Ok(ImportOldDataResult {
            source_dir: clean_source,
            copied_files: report.copied_files,
            backup_dir: report.backup_dir.map(|p| display_clean(&p)),
            pending_restart: true,
        })
    })
    .await
    .map_err(|_| "旧版数据导入未完成".to_string())?;
    if let Err(ref error) = result {
        crate::log_warn!("数据目录操作未完成（应用旧版导入）：{error}");
    }
    result
}

#[tauri::command]
pub fn restart_for_data_change(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    settings_only(&window)?;
    if let Some(state) = app.try_state::<crate::AppState>() {
        state.running.store(false, Ordering::Relaxed);
        for (_, pending) in state
            .translation_cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain()
        {
            let _ = pending.sender.send(());
        }
        state.store.freeze_until_restart();
    }
    app.request_restart();
    Ok(())
}

// === TESTS ===
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_CASE: AtomicU64 = AtomicU64::new(1);

    fn temp_case(name: &str) -> PathBuf {
        let base = std::env::var("HUB_LITE_TEST_ROOT")
            .expect("隔离测试必须指定 HUB_LITE_TEST_ROOT");
        let dir = PathBuf::from(base).join(format!(
            "data-loc-{name}-{}-{}",
            std::process::id(),
            NEXT_CASE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_seed_apps(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        let seeded = serde_json::json!({
            "schemaVersion": 1,
            "categories": [
                { "id": "all", "name": "全部" },
                { "id": "c-common", "name": "常用" }
            ],
            "apps": [
                { "id": "a-notepad", "name": "记事本", "target": "notepad.exe", "categoryId": "c-common", "color": "#000", "initials": "记" }
            ]
        });
        fs::write(dir.join("applications.json"), serde_json::to_vec(&seeded).unwrap()).unwrap();
    }

    fn write_rich_user_data(dir: &Path) {
        fs::create_dir_all(dir.join("clipboard-images")).unwrap();
        fs::create_dir_all(dir.join("icons")).unwrap();
        let apps = serde_json::json!({
            "schemaVersion": 1,
            "categories": [
                { "id": "all", "name": "全部" },
                { "id": "cat-user", "name": "自建分类" }
            ],
            "apps": [
                { "id": "a-notepad", "name": "记事本", "target": "notepad.exe", "categoryId": "all", "color": "#000", "initials": "记" },
                { "id": "user-app-1", "name": "我的工具", "target": "D:\\Tools\\demo.exe", "categoryId": "cat-user", "color": "#111", "initials": "工" }
            ]
        });
        fs::write(dir.join("applications.json"), serde_json::to_vec(&apps).unwrap()).unwrap();
        fs::write(dir.join("ui-preferences.json"), br#"{"theme":"midnight"}"#).unwrap();
        fs::write(
            dir.join("clipboard-index.json"),
            r#"{"schemaVersion":1,"groups":["未分组","代码"],"entries":[{"id":"c1","kind":"text","text":"SECRET_CLIP_BODY"}]}"#.as_bytes(),
        )
        .unwrap();
        fs::write(
            dir.join("snippets.json"),
            r#"{"schemaVersion":1,"groups":["未分组"],"entries":[{"id":"s1","title":"t","content":"b","group":"未分组","pinned":false,"updatedAt":1}]}"#.as_bytes(),
        )
        .unwrap();
        fs::write(dir.join("translation.json"), br#"{"providerId":"openai","model":"gpt-4o-mini"}"#).unwrap();
        fs::write(
            dir.join("mail-accounts.json"),
            br#"{"accounts":[{"id":"m1","provider":"qq","address":"demo@qq.com"}]}"#,
        )
        .unwrap();
        fs::write(
            dir.join("file-shelf.json"),
            br#"{"entries":[{"id":"f1","path":"C:\\a.txt","name":"a.txt","isDir":false}]}"#,
        )
        .unwrap();
        fs::write(
            dir.join("audio-profiles.json"),
            r#"[{"name":"开会","items":[]},{"name":"静音","items":[]}]"#.as_bytes(),
        )
        .unwrap();
        fs::write(dir.join("shortcuts.json"), br#"{"bar":"Ctrl+Alt+Space"}"#).unwrap();
        fs::write(dir.join("window-state.json"), br#"{"barX":100,"barY":0}"#).unwrap();
        fs::write(dir.join("clipboard-images").join("clip-1.png"), b"PNGDATA").unwrap();
        fs::write(dir.join("icons").join("icon-1.png"), b"ICONDATA").unwrap();
    }

    #[test]
    fn default_and_resolve_priority() {
        let root = temp_case("priority");
        let app_local = root.join("local");
        let exe_dir = root.join("app-v1");
        let shared_default = root.join("SharedDefault");
        fs::create_dir_all(&app_local).unwrap();
        fs::create_dir_all(exe_dir.join("data")).unwrap();

        // 1. 仅存在空白种子文件的旧 portable.flag 目录不锁定旧版，直接导向默认共享目录
        fs::write(exe_dir.join(LEGACY_PORTABLE_FLAG), b"").unwrap();
        write_seed_apps(&exe_dir.join("data"));
        let r0 = resolve_data_root_with_default(None, Some(&exe_dir), &app_local, shared_default.clone());
        assert_eq!(r0.mode, "shared");
        assert_eq!(r0.path, shared_default);
        assert!(r0.should_persist_pointer);

        // 2. 旧版 portable.flag 下若已有真实用户数据且尚未写入共享指针，兼容继续沿用旧目录
        write_rich_user_data(&exe_dir.join("data"));
        let r1 = resolve_data_root_with_default(None, Some(&exe_dir), &app_local, shared_default.clone());
        assert_eq!(r1.mode, "legacy_portable");
        assert_eq!(r1.path, exe_dir.join("data"));
        assert!(!r1.should_persist_pointer);

        // 3. 一旦设置共享指针，优先级高于老式 portable.flag
        let custom_shared = root.join("CustomShared");
        save_pointer(
            &app_local.join(POINTER_FILE),
            &DataLocationPointer {
                version: 1,
                shared_data_dir: Some(custom_shared.to_string_lossy().into_owned()),
                recent_dirs: Vec::new(),
            },
        )
        .unwrap();
        let r2 = resolve_data_root_with_default(None, Some(&exe_dir), &app_local, shared_default.clone());
        assert_eq!(r2.mode, "shared");
        assert_eq!(r2.path, custom_shared);

        // 4. isolated 标记优先于共享指针
        fs::write(exe_dir.join(ISOLATED_FLAG), b"").unwrap();
        let r3 = resolve_data_root_with_default(None, Some(&exe_dir), &app_local, shared_default.clone());
        assert_eq!(r3.mode, "isolated");
        assert_eq!(r3.path, exe_dir.join("data"));

        // 5. env 变量优先级最高
        let env_dir = root.join("EnvDir");
        let r4 = resolve_data_root_with_default(
            Some(env_dir.clone()),
            Some(&exe_dir),
            &app_local,
            shared_default,
        );
        assert_eq!(r4.mode, "env");
        assert_eq!(r4.path, env_dir);
    }

    #[test]
    fn new_version_with_legacy_flag_still_uses_registered_shared_dir() {
        let root = temp_case("upgrade");
        let app_local = root.join("local");
        let shared = root.join("DesktopHub-Data");
        save_pointer(
            &app_local.join(POINTER_FILE),
            &DataLocationPointer {
                version: 1,
                shared_data_dir: Some(shared.to_string_lossy().into_owned()),
                recent_dirs: Vec::new(),
            },
        )
        .unwrap();

        let new_exe_dir = root.join("DesktopHub-Lite-new");
        fs::create_dir_all(new_exe_dir.join("data")).unwrap();
        fs::write(new_exe_dir.join(LEGACY_PORTABLE_FLAG), b"").unwrap();

        let resolved = resolve_data_root_with_default(
            None,
            Some(&new_exe_dir),
            &app_local,
            root.join("OtherDefault"),
        );
        assert_eq!(resolved.mode, "shared");
        assert_eq!(resolved.path, shared);
    }

    #[test]
    fn normalize_and_inspect_all_user_files() {
        let root = temp_case("inspect");
        let pkg_dir = root.join("old-pkg");
        let data_dir = pkg_dir.join("data");
        write_rich_user_data(&data_dir);

        assert_eq!(normalize_source_data_dir(&pkg_dir).unwrap(), data_dir);
        assert_eq!(normalize_source_data_dir(&data_dir).unwrap(), data_dir);
        assert!(normalize_source_data_dir(&root).is_err());

        let summary = inspect_data_dir(&data_dir).unwrap();
        assert!(summary.exists);
        assert!(summary.has_meaningful_data);
        assert_eq!(summary.app_count, 2);
        assert_eq!(summary.category_count, 2);
        assert_eq!(summary.clip_count, 1);
        assert_eq!(summary.clip_image_count, 1);
        assert_eq!(summary.snippet_count, 1);
        assert_eq!(summary.mail_account_count, 1);
        assert_eq!(summary.shelf_count, 1);
        assert_eq!(summary.audio_profile_count, 2);
        assert!(summary.has_translation_config);
        assert!(summary.has_ui_preferences);
        assert!(summary.has_shortcuts);
        assert_eq!(summary.icon_cache_count, 1);
        assert_eq!(summary.total_files, 12);
        assert!(summary.corrupted_files.is_empty());

        let json_out = serde_json::to_string(&summary).unwrap();
        assert!(!json_out.contains("SECRET_CLIP_BODY"));

        fs::write(data_dir.join("snippets.json"), b"{broken").unwrap();
        let broken_summary = inspect_data_dir(&data_dir).unwrap();
        assert_eq!(broken_summary.corrupted_files, vec!["snippets.json"]);
    }

    #[test]
    fn copy_user_data_creates_backup_verifies_bytes_and_rejects_unsafe_paths() {
        let root = temp_case("copy");
        let src = root.join("src-data");
        let dst = root.join("dst-data");
        write_rich_user_data(&src);
        fs::write(src.join("desktophub-lite.log"), b"log-should-skip").unwrap();
        fs::write(src.join("applications.json.bak"), b"bak-should-skip").unwrap();
        fs::write(src.join("applications.json.tmp"), b"tmp-should-skip").unwrap();

        fs::create_dir_all(&dst).unwrap();
        fs::write(dst.join("applications.json"), b"{\"old\":true}").unwrap();

        let report = copy_user_data_to(&src, &dst, true).unwrap();
        assert_eq!(report.copied_files, 12);
        let backup = report.backup_dir.expect("应生成原有目标数据的备份目录");
        assert_eq!(
            fs::read(backup.join("applications.json")).unwrap(),
            b"{\"old\":true}"
        );
        assert_eq!(
            fs::read(dst.join("applications.json")).unwrap(),
            fs::read(src.join("applications.json")).unwrap()
        );
        assert_eq!(
            fs::read(dst.join("clipboard-images").join("clip-1.png")).unwrap(),
            b"PNGDATA"
        );
        assert!(!dst.join("desktophub-lite.log").exists());
        assert!(!dst.join("applications.json.bak").exists());
        assert!(!dst.join("applications.json.tmp").exists());

        // 禁止自身与父子包含迁移
        assert!(copy_user_data_to(&src, &src, true).is_err());
        assert!(copy_user_data_to(&src, &src.join("nested"), true).is_err());

        // 损坏源 JSON 阻止迁移
        fs::write(src.join("translation.json"), b"{invalid").unwrap();
        assert!(copy_user_data_to(&src, &root.join("dst-2"), true).is_err());
    }

    #[test]
    fn discover_candidates_finds_sibling_and_recent_dirs_deduped() {
        let root = temp_case("discover");
        let packages = root.join("packages");
        let cur_exe = packages.join("DesktopHub-Lite-0.1.3-new");
        let old_exe = packages.join("DesktopHub-Lite-0.1.3-old");
        let empty_exe = packages.join("DesktopHub-Lite-0.1.3-empty");
        let recent_dir = root.join("MyRecentData");
        let current_dir = root.join("CurrentShared");

        fs::create_dir_all(cur_exe.join("data")).unwrap();
        write_seed_apps(&empty_exe.join("data"));
        write_rich_user_data(&old_exe.join("data"));
        write_rich_user_data(&recent_dir);
        fs::create_dir_all(&current_dir).unwrap();

        let pointer = DataLocationPointer {
            version: 1,
            shared_data_dir: Some(current_dir.to_string_lossy().into_owned()),
            recent_dirs: vec![
                recent_dir.to_string_lossy().into_owned(),
                current_dir.to_string_lossy().into_owned(),
            ],
        };

        let list = discover_candidate_old_dirs(
            &current_dir,
            Some(&cur_exe),
            &root.join("local"),
            &root.join("DefaultShared"),
            &pointer,
        );
        let paths: Vec<String> = list.iter().map(|c| c.path.clone()).collect();
        assert_eq!(list.len(), 2);
        assert!(paths.iter().any(|p| path_key(Path::new(p)) == path_key(&old_exe.join("data"))));
        assert!(paths.iter().any(|p| path_key(Path::new(p)) == path_key(&recent_dir)));
        assert!(!paths.iter().any(|p| path_key(Path::new(p)) == path_key(&empty_exe.join("data"))));
    }

    #[test]
    fn auto_migrate_copies_sibling_old_data_into_empty_shared_dir() {
        let root = temp_case("auto_migrate_sibling");
        let packages = root.join("packages");
        let new_exe = packages.join("DesktopHub-Lite-0.1.3-new");
        let old_exe = packages.join("DesktopHub-Lite-0.1.3-old");
        let shared = root.join("SharedData");
        let app_local = root.join("local");
        fs::create_dir_all(&new_exe).unwrap();
        fs::create_dir_all(&app_local).unwrap();
        write_rich_user_data(&old_exe.join("data"));
        // 即使目标目录事先只有默认空白种子，也视为尚未承载真实数据而自动迁入并备份种子
        write_seed_apps(&shared);

        let outcome = auto_migrate_empty_shared_dir(
            &shared,
            Some(&new_exe),
            &app_local,
            &shared,
            &DataLocationPointer::default(),
            &[],
        )
        .unwrap()
        .expect("应自动发现并迁入同级旧版目录数据");

        assert_eq!(path_key(&outcome.source_dir), path_key(&old_exe.join("data")));
        assert_eq!(outcome.copied_files, 12);
        assert!(outcome.backup_dir.is_some());
        assert!(has_meaningful_data(&shared));
        assert_eq!(
            fs::read(shared.join("clipboard-index.json")).unwrap(),
            fs::read(old_exe.join("data").join("clipboard-index.json")).unwrap()
        );
        // 原目录数据保持完好不动
        assert!(old_exe.join("data").join("applications.json").is_file());
    }

    #[test]
    fn auto_migrate_never_overwrites_shared_dir_with_existing_meaningful_data() {
        let root = temp_case("auto_migrate_guard");
        let packages = root.join("packages");
        let new_exe = packages.join("DesktopHub-Lite-0.1.3-new");
        let old_exe = packages.join("DesktopHub-Lite-0.1.3-old");
        let shared = root.join("SharedData");
        let app_local = root.join("local");
        fs::create_dir_all(&new_exe).unwrap();
        fs::create_dir_all(&app_local).unwrap();
        fs::create_dir_all(&shared).unwrap();
        write_rich_user_data(&old_exe.join("data"));
        fs::write(shared.join("ui-preferences.json"), br#"{"theme":"midnight","custom":1}"#).unwrap();

        let original_bytes = fs::read(shared.join("ui-preferences.json")).unwrap();
        let outcome = auto_migrate_empty_shared_dir(
            &shared,
            Some(&new_exe),
            &app_local,
            &shared,
            &DataLocationPointer::default(),
            &[],
        )
        .unwrap();

        assert!(outcome.is_none());
        assert_eq!(fs::read(shared.join("ui-preferences.json")).unwrap(), original_bytes);
        assert!(!shared.join("clipboard-index.json").exists());
    }

    #[test]
    fn auto_migrate_discovers_startup_hint_dir_outside_siblings() {
        let root = temp_case("auto_migrate_hint");
        let new_exe = root.join("isolated-download").join("DesktopHub-Lite-new");
        let old_startup_dir = root.join("custom-install").join("DesktopHub-Lite");
        let shared = root.join("SharedData");
        let app_local = root.join("local");
        fs::create_dir_all(&new_exe).unwrap();
        fs::create_dir_all(&app_local).unwrap();
        write_rich_user_data(&old_startup_dir.join("data"));

        let hints = vec![old_startup_dir.join("data")];
        let outcome = auto_migrate_empty_shared_dir(
            &shared,
            Some(&new_exe),
            &app_local,
            &shared,
            &DataLocationPointer::default(),
            &hints,
        )
        .unwrap()
        .expect("即使不在同级目录下，也应通过启动项提示路径自动发现并迁入");

        assert_eq!(path_key(&outcome.source_dir), path_key(&old_startup_dir.join("data")));
        assert_eq!(outcome.copied_files, 12);
        assert_eq!(
            fs::read(shared.join("snippets.json")).unwrap(),
            fs::read(old_startup_dir.join("data").join("snippets.json")).unwrap()
        );
    }

    #[test]
    fn pack_files_are_migrated_and_seed_is_not_meaningful() {
        let root = temp_case("pack_migrate");
        let seed_dir = root.join("seed-only");
        fs::create_dir_all(&seed_dir).unwrap();
        fs::write(
            seed_dir.join("app-pack.json"),
            br#"{"cli":"","hbuilderRoot":"","project":"","packagename":"","packType":"3","outputDir":"","adb":"","device":"","reversePort":11034,"autoInstall":false,"remoteIgnoreFiles":[]}"#,
        )
        .unwrap();
        fs::write(
            seed_dir.join("pack-scripts.json"),
            br#"{"schemaVersion":1,"scripts":[{"id":"builtin-local-pack","history":[]},{"id":"builtin-remote-pack","history":[]}]}"#,
        )
        .unwrap();
        assert!(!has_meaningful_data(&seed_dir));

        let ignore_only_dir = root.join("ignore-only");
        fs::create_dir_all(&ignore_only_dir).unwrap();
        fs::write(
            ignore_only_dir.join("app-pack.json"),
            br#"{"remoteIgnoreFiles":["src/api/gov-auth.js"]}"#,
        )
        .unwrap();
        assert!(has_meaningful_data(&ignore_only_dir));

        let src = root.join("src-pack");
        let dst = root.join("dst-pack");
        fs::create_dir_all(&src).unwrap();
        fs::write(
            src.join("app-pack.json"),
            br#"{"project":"D:\\demo\\proj","reversePort":9527}"#,
        )
        .unwrap();
        fs::write(
            src.join("pack-scripts.json"),
            br#"{"schemaVersion":1,"scripts":[{"id":"custom-1","history":[12.5]}]}"#,
        )
        .unwrap();
        assert!(has_meaningful_data(&src));

        let report = copy_user_data_to(&src, &dst, false).unwrap();
        assert_eq!(report.copied_files, 2);
        assert_eq!(
            fs::read(dst.join("app-pack.json")).unwrap(),
            fs::read(src.join("app-pack.json")).unwrap()
        );
        assert_eq!(
            fs::read(dst.join("pack-scripts.json")).unwrap(),
            fs::read(src.join("pack-scripts.json")).unwrap()
        );
    }
}
