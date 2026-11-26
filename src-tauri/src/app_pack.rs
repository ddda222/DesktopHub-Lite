//! 云打包流水线：HBuilderX CLI 提交云打包 → 取下载地址 → 下载 APK →（可选）adb 安装与端口映射。
//!
//! 本机 HBuilderX 4.87 只有 `cli pack`：没有 `pack status`、`pack cancel`、无头模式，
//! 所以这里把 `cli pack` 当成阻塞调用，边读输出边报进度，结束时从输出里取临时下载地址。
//! 取消只能终止本地进程，云端任务仍会在 DCloud 后台排队。
//!
//! 远程联调打包支持「忽略指定文件的本地更改」：
//! 1. 打包前校验目标文件存在于项目和 Git `HEAD` 中，并扫描 `HEAD` 非注释代码避免误留 `localhost` / `127.0.0.1`；
//! 2. 先落盘防崩溃恢复记录，再把工作区中对应文件临时恢复成 `HEAD` 内容（不动 Git 暂存区）；
//! 3. 一旦检测到 HBuilderX 已上传资源进入云端排队，立即把工作区文件还原为用户的本地修改；失败、取消或退出时也会兜底还原。

use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::{adb, exec, pack_scripts, storage, task_runner, AppState};

pub const CONFIG_FILE: &str = "app-pack.json";
pub const PACK_SETTINGS_EVENT: &str = "pack-settings-changed";
const RESTORE_BACKUP_FILE: &str = "pack-restore-backup.json";

const OPEN_TIMEOUT: Duration = Duration::from_secs(90);
/// 云端排队没有上限；高峰期实测约 15 分钟，留足 30 分钟。
const PACK_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);

fn default_reverse_port() -> u16 {
    adb::DEFAULT_PORT
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PackConfig {
    /// HBuilderX 的 cli.exe；留空时按 hbuilderRoot 找版本号最高的一个
    pub cli: String,
    pub hbuilder_root: String,
    pub project: String,
    /// Android 包名（cli pack 必填）
    pub packagename: String,
    /// 证书类型：0 自有、1 公共测试（4.87 已停用）、3 云端证书
    pub pack_type: String,
    /// APK 落地目录
    pub output_dir: String,
    /// adb.exe；留空时用 PATH 里的 adb
    pub adb: String,
    /// 指定设备序列号；留空时用唯一一台已授权设备
    pub device: String,
    /// ADB 默认反向转发端口（1～65535）
    #[serde(default = "default_reverse_port")]
    pub reverse_port: u16,
    /// 打包完成后是否自动装到手机
    pub auto_install: bool,
    /// 远程联调打包时临时忽略本地更改（恢复为 Git HEAD）的文件路径列表
    #[serde(default)]
    pub remote_ignore_files: Vec<String>,
}

impl Default for PackConfig {
    fn default() -> Self {
        Self {
            cli: String::new(),
            hbuilder_root: String::new(),
            project: String::new(),
            packagename: String::new(),
            pack_type: "3".into(),
            output_dir: String::new(),
            adb: String::new(),
            device: String::new(),
            reverse_port: adb::DEFAULT_PORT,
            // 默认不自动装机：避免把手机上的本地测试版覆盖掉。
            auto_install: false,
            remote_ignore_files: Vec::new(),
        }
    }
}

impl PackConfig {
    /// 返回有效的反向转发端口（配置为 0 时回落到默认端口）。
    pub fn effective_reverse_port(&self) -> u16 {
        if self.reverse_port == 0 {
            adb::DEFAULT_PORT
        } else {
            self.reverse_port
        }
    }
}

pub struct PackOutcome {
    pub note: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallApkResult {
    pub device: String,
    pub reversed_port: Option<u16>,
    pub adb: adb::AdbStatus,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupEntry {
    path: String,
    rel_path: String,
    original_b64: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreBackupFile {
    project: String,
    created_at_ms: u64,
    entries: Vec<BackupEntry>,
}

pub fn emit_settings(app: &AppHandle, config: &PackConfig) {
    let _ = app.emit(PACK_SETTINGS_EVENT, config);
}

pub fn load_config(app: &AppHandle) -> PackConfig {
    let store = &app.state::<AppState>().store;
    if let Some(mut config) = store.read::<PackConfig>(CONFIG_FILE) {
        if config.reverse_port == 0 {
            config.reverse_port = adb::DEFAULT_PORT;
        }
        return config;
    }
    let defaults = PackConfig::default();
    // 文件确实不存在时落一份默认值，方便用户以后直接改路径；
    // 解析失败时 Store 已禁止覆盖，这里写失败也不影响本次使用默认值。
    if !store.root().join(CONFIG_FILE).exists() {
        if let Err(error) = store.write(CONFIG_FILE, &defaults) {
            crate::log_warn!("写入打包配置失败：{error}");
        }
    }
    defaults
}

pub fn save_config(app: &AppHandle, config: &PackConfig) -> Result<(), String> {
    app.state::<AppState>().store.write(CONFIG_FILE, config)
}

#[cfg(windows)]
fn dedup_key(path: &str) -> String {
    path.to_ascii_lowercase()
}

#[cfg(not(windows))]
fn dedup_key(path: &str) -> String {
    path.to_string()
}

#[cfg(windows)]
fn strip_verbatim(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

#[cfg(not(windows))]
fn strip_verbatim(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn logical_components(path: &Path) -> Vec<String> {
    let clean = strip_verbatim(path);
    let mut parts: Vec<String> = Vec::new();
    for comp in clean.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.last().is_some_and(|p| p != "..") {
                    parts.pop();
                } else {
                    parts.push("..".into());
                }
            }
            Component::Prefix(prefix) => {
                parts.push(prefix.as_os_str().to_string_lossy().into_owned());
            }
            Component::RootDir => parts.push("/".into()),
            Component::Normal(seg) => parts.push(seg.to_string_lossy().into_owned()),
        }
    }
    parts
}

fn try_relative_posix(root: &Path, target: &Path) -> Option<String> {
    // 尽量用规范化真实路径对比，若目标文件尚未创建则回落到逻辑路径对比。
    let (r_path, t_path) = match (std::fs::canonicalize(root), std::fs::canonicalize(target)) {
        (Ok(r), Ok(t)) => (r, t),
        _ => (root.to_path_buf(), target.to_path_buf()),
    };
    let r_parts = logical_components(&r_path);
    let t_parts = logical_components(&t_path);
    if r_parts.is_empty() || t_parts.len() <= r_parts.len() {
        return None;
    }
    let matched = r_parts.iter().zip(&t_parts).all(|(a, b)| {
        #[cfg(windows)]
        {
            a.eq_ignore_ascii_case(b)
        }
        #[cfg(not(windows))]
        {
            a == b
        }
    });
    if !matched {
        return None;
    }
    Some(t_parts[r_parts.len()..].join("/"))
}

/// 清洗单条「远程忽略更改文件」路径：去空白/引号、归一化为 `/` 分隔；落在项目目录内时自动转为相对路径。
pub fn clean_ignore_entry(raw: &str, project_root: Option<&Path>) -> Option<String> {
    let trimmed = raw
        .trim()
        .trim_matches(|c: char| c == '"' || c == '\'')
        .trim();
    if trimmed.is_empty() {
        return None;
    }
    let normalized = trimmed.replace('\\', "/");
    let candidate = Path::new(&normalized);
    if candidate.is_absolute() {
        if let Some(root) = project_root.filter(|p| !p.as_os_str().is_empty()) {
            if let Some(rel) = try_relative_posix(root, candidate) {
                return Some(rel);
            }
        }
        return Some(normalized);
    }
    let mut s = normalized.as_str();
    loop {
        if let Some(rest) = s.strip_prefix("./") {
            s = rest;
        } else if let Some(rest) = s.strip_prefix('/') {
            s = rest;
        } else {
            break;
        }
    }
    let collapsed: String = s
        .split('/')
        .filter(|seg| !seg.is_empty() && *seg != ".")
        .collect::<Vec<_>>()
        .join("/");
    if collapsed.is_empty() { None } else { Some(collapsed) }
}

pub fn clean_ignore_list(raw_files: &[String], project_root: Option<&Path>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in raw_files {
        let Some(cleaned) = clean_ignore_entry(raw, project_root) else {
            continue;
        };
        if seen.insert(dedup_key(&cleaned)) {
            out.push(cleaned);
        }
    }
    out
}

#[tauri::command]
pub fn pack_settings(app: AppHandle) -> PackConfig {
    load_config(&app)
}

#[tauri::command]
pub fn save_pack_settings(app: AppHandle, settings: PackConfig) -> Result<PackConfig, String> {
    let mut next = settings;
    next.project = next.project.trim().to_string();
    next.packagename = next.packagename.trim().to_string();
    next.output_dir = next.output_dir.trim().to_string();
    next.pack_type = next.pack_type.trim().to_string();
    next.adb = next.adb.trim().to_string();
    next.device = next.device.trim().to_string();
    next.hbuilder_root = next.hbuilder_root.trim().to_string();
    next.cli = next.cli.trim().to_string();

    if next.reverse_port == 0 {
        return Err("ADB 转发端口需在 1～65535 之间".into());
    }
    if next.pack_type.is_empty() {
        next.pack_type = "3".into();
    } else if !matches!(next.pack_type.as_str(), "0" | "1" | "3") {
        return Err("证书类型只能是 0（自有）、1（公共测试）或 3（云端证书）".into());
    }
    if !next.project.is_empty() && !Path::new(&next.project).is_dir() {
        return Err(format!("项目目录不存在：{}", next.project));
    }
    if !next.hbuilder_root.is_empty() && !Path::new(&next.hbuilder_root).is_dir() {
        return Err(format!("HBuilderX 安装根目录不存在：{}", next.hbuilder_root));
    }
    if !next.cli.is_empty() && !Path::new(&next.cli).is_file() {
        return Err(format!("HBuilderX cli.exe 不存在：{}", next.cli));
    }
    if !next.adb.is_empty() && !Path::new(&next.adb).is_file() {
        return Err(format!("adb.exe 不存在：{}", next.adb));
    }

    let project_root = if next.project.is_empty() {
        None
    } else {
        Some(Path::new(&next.project))
    };
    next.remote_ignore_files = clean_ignore_list(&next.remote_ignore_files, project_root);

    save_config(&app, &next)?;
    emit_settings(&app, &next);
    Ok(next)
}

/// 打开 APK 目录：优先定位到具体文件，没有文件就打开目录本身。
#[tauri::command]
pub fn open_pack_dir(app: AppHandle, apk_path: Option<String>) -> Result<(), String> {
    if let Some(path) = apk_path.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()) {
        if Path::new(&path).is_file() {
            return crate::launcher::reveal(&path);
        }
    }
    let config = load_config(&app);
    let output_dir = config.output_dir.trim();
    if output_dir.is_empty() {
        return Err("请先在「设置 → 打包与联调」里配置 APK 输出目录".into());
    }
    let directory = PathBuf::from(output_dir);
    if !directory.is_dir() {
        std::fs::create_dir_all(&directory).map_err(|error| format!("无法创建输出目录：{error}"))?;
    }
    crate::launcher::launch(&directory.to_string_lossy())
}

/// 启动时检查是否存在上次异常退出遗留的远程打包还原记录，如有则自动恢复原文件内容。
pub fn recover_pending_backup(app: &AppHandle) {
    let backup_path = app.state::<AppState>().store.root().join(RESTORE_BACKUP_FILE);
    if !backup_path.is_file() {
        return;
    }
    let bytes = match std::fs::read(&backup_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            crate::log_warn!("[打包] 发现未清理的本地修改备份，但读取失败：{error}");
            return;
        }
    };
    let backup: RestoreBackupFile = match serde_json::from_slice(&bytes) {
        Ok(parsed) => parsed,
        Err(error) => {
            crate::log_warn!("[打包] 发现未清理的本地修改备份，但解析失败：{error}");
            return;
        }
    };
    let mut restored = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for entry in &backup.entries {
        let Ok(raw) = base64::engine::general_purpose::STANDARD.decode(&entry.original_b64) else {
            failed.push(format!("{}（备份内容损坏）", entry.rel_path));
            continue;
        };
        if let Err(error) = storage::write_file_atomic(Path::new(&entry.path), &raw) {
            failed.push(format!("{}（{error}）", entry.rel_path));
        } else {
            restored += 1;
        }
    }
    if failed.is_empty() {
        let _ = std::fs::remove_file(&backup_path);
        crate::log_warn!(
            "[打包] 检测到上次远程打包异常退出，已自动恢复 {restored} 个本地文件（项目：{}）",
            backup.project
        );
    } else {
        crate::log_warn!(
            "[打包] 恢复上次远程打包本地文件部分失败，已保留备份记录：{}",
            failed.join("；")
        );
    }
}

fn resolve_git_root(project: &Path) -> Result<PathBuf, String> {
    let output = exec::command(Path::new("git"))
        .args([
            "-C",
            &project.to_string_lossy(),
            "rev-parse",
            "--show-toplevel",
        ])
        .output()
        .map_err(|error| format!("无法执行 git 命令，无法按 Git HEAD 忽略更改：{error}"))?;
    if !output.status.success() {
        return Err("项目目录不是 Git 仓库，无法在远程打包时忽略本地更改".into());
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        return Err("无法识别项目对应的 Git 仓库根目录".into());
    }
    Ok(PathBuf::from(root))
}

fn resolve_ignore_target(
    project: &Path,
    git_root: &Path,
    raw: &str,
) -> Result<(PathBuf, String, String), String> {
    let cleaned = clean_ignore_entry(raw, Some(project))
        .ok_or_else(|| "远程忽略文件列表包含空路径".to_string())?;
    let rel_path = Path::new(&cleaned);
    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|comp| matches!(comp, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        return Err(format!("远程忽略文件必须位于项目目录内：{cleaned}"));
    }
    let mut abs = project.to_path_buf();
    for seg in cleaned.split('/').filter(|s| !s.is_empty()) {
        abs.push(seg);
    }
    if !abs.is_file() {
        return Err(format!("远程忽略文件不存在：{cleaned}"));
    }
    let canon_project = std::fs::canonicalize(project)
        .map_err(|error| format!("无法访问项目目录：{error}"))?;
    let canon_git = std::fs::canonicalize(git_root)
        .map_err(|error| format!("无法访问 Git 根目录：{error}"))?;
    let canon_file = std::fs::canonicalize(&abs)
        .map_err(|error| format!("无法访问远程忽略文件 {cleaned}：{error}"))?;
    if try_relative_posix(&canon_project, &canon_file).is_none() {
        return Err(format!("远程忽略文件超出了项目目录范围：{cleaned}"));
    }
    let git_rel = try_relative_posix(&canon_git, &canon_file)
        .ok_or_else(|| format!("文件 {cleaned} 不在 Git 仓库目录内"))?;
    Ok((abs, cleaned, git_rel))
}

fn read_git_head_bytes(git_root: &Path, git_rel_posix: &str, display_rel: &str) -> Result<Vec<u8>, String> {
    let spec = format!("HEAD:{git_rel_posix}");
    let output = exec::command(Path::new("git"))
        .args(["-C", &git_root.to_string_lossy(), "show", &spec])
        .output()
        .map_err(|error| format!("读取 Git HEAD 失败：{error}"))?;
    if !output.status.success() {
        let stderr = exec::decode(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            format!("退出码 {}", output.status.code().unwrap_or(-1))
        } else {
            stderr
        };
        return Err(format!("文件 {display_rel} 不在 Git HEAD 中或无法读取：{detail}"));
    }
    Ok(output.stdout)
}

fn contains_loopback_token(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    for needle in ["localhost", "127.0.0.1"] {
        let bytes = lower.as_bytes();
        let mut offset = 0usize;
        while let Some(pos) = lower[offset..].find(needle) {
            let start = offset + pos;
            let end = start + needle.len();
            let left_ok = start == 0 || {
                let prev = bytes[start - 1] as char;
                !prev.is_ascii_alphanumeric() && prev != '_'
            };
            let right_ok = end == bytes.len() || {
                let next = bytes[end] as char;
                !next.is_ascii_alphanumeric() && next != '_'
            };
            if left_ok && right_ok {
                return true;
            }
            offset = end;
        }
    }
    false
}

/// 扫描文件内容中**非注释部分**是否包含 `localhost` 或 `127.0.0.1`。
/// 正确识别单/双引号与模板字符串内的 `//` 和 `/*`，避免注释或 URL 中的 `//` 误判。
fn find_local_loopback_in_code(bytes: &[u8]) -> Option<(usize, String)> {
    let text = String::from_utf8_lossy(bytes);
    let mut in_block_comment = false;
    let mut quote: Option<char> = None;

    for (idx, line) in text.lines().enumerate() {
        let mut code_only = String::new();
        let mut chars = line.chars().peekable();
        let mut escaped = false;
        while let Some(c) = chars.next() {
            if in_block_comment {
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    in_block_comment = false;
                }
                continue;
            }
            if let Some(q) = quote {
                code_only.push(c);
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
                continue;
            }
            if c == '/' && chars.peek() == Some(&'/') {
                // 行注释：当前行剩余内容全部跳过
                break;
            }
            if c == '/' && chars.peek() == Some(&'*') {
                chars.next();
                in_block_comment = true;
                continue;
            }
            if c == '\'' || c == '"' || c == '`' {
                quote = Some(c);
            }
            code_only.push(c);
        }
        // 单双引号字面量不会跨物理行
        if matches!(quote, Some('\'') | Some('"')) {
            quote = None;
        }
        if contains_loopback_token(&code_only) {
            let preview: String = code_only.trim().chars().take(120).collect();
            return Some((idx + 1, preview));
        }
    }
    None
}

struct ActiveRestore {
    backup_path: PathBuf,
    entries: Vec<(PathBuf, String, Vec<u8>)>,
}

#[derive(Clone)]
struct RestoreHandle {
    app: AppHandle,
    inner: Arc<Mutex<Option<ActiveRestore>>>,
}

impl RestoreHandle {
    fn restore(&self, reason: &str) {
        let Some(active) = self.inner.lock().unwrap_or_else(|e| e.into_inner()).take() else {
            return;
        };
        let mut restored_names = Vec::new();
        let mut failed = Vec::new();
        for (abs_path, rel_path, original) in active.entries {
            if let Err(error) = storage::write_file_atomic(&abs_path, &original) {
                failed.push(format!("{rel_path}（{error}）"));
            } else {
                restored_names.push(rel_path);
            }
        }
        if failed.is_empty() {
            let _ = std::fs::remove_file(&active.backup_path);
            let msg = format!("[打包] {reason}：{}", restored_names.join("、"));
            crate::log_info!("{msg}");
            task_runner::log_line(&self.app, &msg);
        } else {
            let msg = format!(
                "[打包] 还原部分本地文件失败（备份仍保留在 {}）：{}",
                active.backup_path.display(),
                failed.join("；")
            );
            crate::log_warn!("{msg}");
            task_runner::log_line(&self.app, &msg);
        }
    }
}

struct RemoteIgnoreGuard {
    handle: RestoreHandle,
    restored_rel_paths: Vec<String>,
}

impl RemoteIgnoreGuard {
    fn prepare(
        app: &AppHandle,
        project: &Path,
        raw_files: &[String],
    ) -> Result<Option<Self>, String> {
        let cleaned = clean_ignore_list(raw_files, Some(project));
        if cleaned.is_empty() {
            return Ok(None);
        }
        let git_root = resolve_git_root(project)?;

        struct PreparedFile {
            abs_path: PathBuf,
            rel_path: String,
            original_bytes: Vec<u8>,
            head_bytes: Vec<u8>,
        }

        let mut prepared = Vec::with_capacity(cleaned.len());
        for entry in cleaned {
            let (abs_path, rel_path, git_rel) = resolve_ignore_target(project, &git_root, &entry)?;
            let head_bytes = read_git_head_bytes(&git_root, &git_rel, &rel_path)?;
            if let Some((line_no, snippet)) = find_local_loopback_in_code(&head_bytes) {
                return Err(format!(
                    "远程打包中止：Git HEAD 中的 {rel_path} 第 {line_no} 行仍未注释本地地址（{snippet}），请先提交远程地址后再打包"
                ));
            }
            let original_bytes = std::fs::read(&abs_path)
                .map_err(|error| format!("读取本地文件 {rel_path} 失败：{error}"))?;
            prepared.push(PreparedFile { abs_path, rel_path, original_bytes, head_bytes });
        }

        let backup_path = app.state::<AppState>().store.root().join(RESTORE_BACKUP_FILE);
        let backup = RestoreBackupFile {
            project: project.to_string_lossy().into_owned(),
            created_at_ms: pack_scripts::now_ms(),
            entries: prepared
                .iter()
                .map(|item| BackupEntry {
                    path: item.abs_path.to_string_lossy().into_owned(),
                    rel_path: item.rel_path.clone(),
                    original_b64: base64::engine::general_purpose::STANDARD.encode(&item.original_bytes),
                })
                .collect(),
        };
        let backup_json = serde_json::to_vec(&backup)
            .map_err(|error| format!("序列化本地修改备份失败：{error}"))?;
        storage::write_file_atomic(&backup_path, &backup_json)?;

        let mut written: Vec<&PreparedFile> = Vec::new();
        for item in &prepared {
            if let Err(error) = storage::write_file_atomic(&item.abs_path, &item.head_bytes) {
                for prev in written {
                    let _ = storage::write_file_atomic(&prev.abs_path, &prev.original_bytes);
                }
                let _ = std::fs::remove_file(&backup_path);
                return Err(format!("写入 Git HEAD 版本到 {} 失败：{error}", item.rel_path));
            }
            written.push(item);
        }

        let restored_rel_paths = prepared.iter().map(|item| item.rel_path.clone()).collect();
        let active = ActiveRestore {
            backup_path,
            entries: prepared
                .into_iter()
                .map(|item| (item.abs_path, item.rel_path, item.original_bytes))
                .collect(),
        };
        Ok(Some(Self {
            handle: RestoreHandle {
                app: app.clone(),
                inner: Arc::new(Mutex::new(Some(active))),
            },
            restored_rel_paths,
        }))
    }

    fn restore_handle(&self) -> RestoreHandle {
        self.handle.clone()
    }
}

impl Drop for RemoteIgnoreGuard {
    fn drop(&mut self) {
        self.handle.restore("任务结束，兜底还原本地修改");
    }
}

fn resolve_cli(config: &PackConfig) -> Result<PathBuf, String> {
    let configured = config.cli.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!("配置的 cli.exe 路径不存在：{}", path.display()))
        };
    }
    let root_text = config.hbuilder_root.trim();
    if root_text.is_empty() {
        return Err("请先在「设置 → 打包与联调」里配置 HBuilderX 路径或 cli.exe".into());
    }
    let root = Path::new(root_text);
    let mut newest: Option<(Vec<u64>, PathBuf)> = None;
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(version) = name.strip_prefix("HBuilderX.") else {
                continue;
            };
            let cli = entry.path().join("HBuilderX").join("cli.exe");
            if !cli.is_file() {
                continue;
            }
            let parsed: Vec<u64> = version.split('.').map(|part| part.parse().unwrap_or(0)).collect();
            if newest.as_ref().map_or(true, |(current, _)| parsed > *current) {
                newest = Some((parsed, cli));
            }
        }
    }
    newest.map(|(_, cli)| cli).ok_or_else(|| {
        format!(
            "在 {} 下没有找到 HBuilderX\\cli.exe，请在「设置 → 打包与联调」里填写 cli.exe 路径",
            root.display()
        )
    })
}

#[cfg(windows)]
fn hbuilder_running() -> bool {
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let tasklist = Path::new(&windir).join("System32").join("tasklist.exe");
    exec::command(&tasklist)
        .args(["/FI", "IMAGENAME eq HBuilderX.exe", "/NH"])
        .output()
        .map(|output| exec::decode(&output.stdout).contains("HBuilderX.exe"))
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn hbuilder_running() -> bool {
    false
}

/// 打包输出里的稳定进度节点；HBuilderX 升级后文案变化时这里要跟着改。
fn pack_progress(line: &str) -> Option<&'static str> {
    const STEPS: [(&str, &str); 9] = [
        ("检查云端打包状态", "检查云端打包状态…"),
        ("检查打包资源", "检查打包资源…"),
        ("开始编译", "编译打包资源…"),
        ("正在编译打包资源", "编译打包资源…"),
        ("压缩打包资源", "压缩打包资源…"),
        ("向云端发送打包请求", "已提交云端，等待排队…"),
        ("队列中", "云端排队中…"),
        ("正在云端打包", "云端打包中…"),
        ("打包成功", "云端打包完成"),
    ];
    STEPS
        .iter()
        .find(|(needle, _)| line.contains(needle))
        .map(|(_, label)| *label)
}

fn is_cloud_uploaded_stage(line: &str) -> bool {
    const CLOUD_MARKERS: [&str; 4] = [
        "向云端发送打包请求",
        "队列中",
        "正在云端打包",
        "打包成功",
    ];
    CLOUD_MARKERS.iter().any(|needle| line.contains(needle))
}

fn find_download_url(text: &str) -> Option<String> {
    let pattern = regex::Regex::new(r#"https://[^\s"'<>]*build/download/[^\s"'<>]+"#).ok()?;
    pattern.find(text).map(|found| {
        found
            .as_str()
            .trim_end_matches(|c: char| matches!(c, ')' | '）' | ',' | '，' | '。'))
            .to_string()
    })
}

/// 本机时间戳，用于 APK 文件名（yyyyMMdd-HHmmss）。
#[cfg(windows)]
fn local_stamp() -> String {
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        time.wYear, time.wMonth, time.wDay, time.wHour, time.wMinute, time.wSecond
    )
}

#[cfg(not(windows))]
fn local_stamp() -> String {
    format!("{}", pack_scripts::now_ms() / 1000)
}

fn sanitize_part(raw: &str, fallback: &str) -> String {
    let clean: String = raw
        .chars()
        .filter(|c| !"\\/:*?\"<>|".contains(*c))
        .collect::<String>()
        .trim_matches(|c: char| c.is_whitespace() || c == '.')
        .to_string();
    if clean.is_empty() { fallback.to_string() } else { clean }
}

fn apk_name(project: &Path, env: &str) -> String {
    let manifest = project.join("src").join("manifest.json");
    let (name, version) = std::fs::read_to_string(&manifest)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .map(|value| {
            let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("app").to_string();
            let version = value.get("versionName").and_then(|v| v.as_str()).unwrap_or("0").to_string();
            (name, version)
        })
        .unwrap_or_else(|| ("app".to_string(), "0".to_string()));
    let safe = sanitize_part(&name, "app");
    let safe_ver = sanitize_part(&version, "0");
    let env_tag = if env == "remote" { "remote" } else { "local" };
    format!("{safe}-{safe_ver}-{env_tag}-{}.apk", local_stamp())
}

async fn wait_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

struct Ctx {
    app: AppHandle,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
}

impl Ctx {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    fn stage(&self, message: &str) {
        task_runner::set_message(&self.app, message);
    }

    async fn run(&self, exe: &Path, args: Vec<String>, limit: Duration) -> Result<exec::Output, exec::CaptureError> {
        let cancel = self.cancel.clone();
        let pid = self.pid.clone();
        let exe = exe.to_path_buf();
        tauri::async_runtime::spawn_blocking(move || {
            exec::capture(
                &exe,
                &args,
                None,
                exec::Options {
                    limit,
                    on_line: None,
                    cancel: Some(cancel.as_ref()),
                    pid: Some(pid.as_ref()),
                },
            )
        })
        .await
        .map_err(|error| exec::CaptureError::Failed(format!("后台任务中断：{error}")))?
    }

    async fn run_pack(
        &self,
        exe: &Path,
        args: Vec<String>,
        limit: Duration,
        restore_on_cloud: Option<RestoreHandle>,
    ) -> Result<(exec::Output, Option<String>), exec::CaptureError> {
        let log_handle = self.app.clone();
        let captured_url = Arc::new(Mutex::new(None::<String>));
        let url_slot = captured_url.clone();
        let mut last = String::new();
        let mut progress = move |line: &str| {
            task_runner::log_line(&log_handle, line);
            if let Some(url) = find_download_url(line) {
                *url_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(url);
            }
            if is_cloud_uploaded_stage(line) {
                if let Some(handle) = restore_on_cloud.as_ref() {
                    handle.restore("已提交云端排队，已自动还原本地修改");
                }
            }
            // 只翻固定节点；没有匹配的行走通用文案，秒数交给界面自己算，
            // 否则消息会在两次输出之间停在一个旧数字上。
            let label = pack_progress(line).map(str::to_string).unwrap_or_else(|| "云端打包中…".to_string());
            if label != last {
                last = label.clone();
                task_runner::set_message(&log_handle, &label);
            }
        };
        let cancel = self.cancel.clone();
        let pid = self.pid.clone();
        let exe = exe.to_path_buf();
        let output = tauri::async_runtime::spawn_blocking(move || {
            exec::capture(
                &exe,
                &args,
                None,
                exec::Options {
                    limit,
                    on_line: Some(&mut progress),
                    cancel: Some(cancel.as_ref()),
                    pid: Some(pid.as_ref()),
                },
            )
        })
        .await
        .map_err(|error| exec::CaptureError::Failed(format!("后台任务中断：{error}")))??;
        let url = captured_url.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Ok((output, url))
    }
}

fn format_install_failure(output: &exec::Output) -> String {
    let text = output.text.to_lowercase();
    let hint = if text.contains("signatures do not match") {
        "（手机上是另一套签名安装的旧包，需先卸载再装）"
    } else {
        ""
    };
    format!("安装失败{hint}：{}", tail(&output.text))
}

/// 跑完整条云打包流水线。`Err(Cancelled)` 与失败要分开，界面文案不同。
pub async fn run_pipeline(
    app: &AppHandle,
    env: &str,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
) -> Result<PackOutcome, exec::CaptureError> {
    let ctx = Ctx { app: app.clone(), cancel, pid };
    let config = load_config(app);
    let project_text = config.project.trim();
    if project_text.is_empty() {
        return Err(exec::CaptureError::Failed("请先在「设置 → 打包与联调」里配置项目目录".into()));
    }
    let project = PathBuf::from(project_text);
    if !project.is_dir() {
        return Err(exec::CaptureError::Failed(format!(
            "项目目录不存在：{}",
            project.display()
        )));
    }
    if config.packagename.trim().is_empty() {
        return Err(exec::CaptureError::Failed("请先在「设置 → 打包与联调」里配置 Android 包名".into()));
    }
    let output_dir_text = config.output_dir.trim();
    if output_dir_text.is_empty() {
        return Err(exec::CaptureError::Failed("请先在「设置 → 打包与联调」里配置 APK 输出目录".into()));
    }
    let cli = resolve_cli(&config).map_err(exec::CaptureError::Failed)?;
    let project_arg = project.to_string_lossy().into_owned();

    // 1. 远程联调打包：在编译前把指定的配置文件临时恢复为 Git HEAD（不改 Git 暂存区）。
    let guard = if env == "remote" && !config.remote_ignore_files.is_empty() {
        if ctx.cancelled() {
            return Err(exec::CaptureError::Cancelled);
        }
        ctx.stage("切换远程配置（忽略本地更改）…");
        let app_handle = app.clone();
        let project_clone = project.clone();
        let files = config.remote_ignore_files.clone();
        let prepared = tauri::async_runtime::spawn_blocking(move || {
            RemoteIgnoreGuard::prepare(&app_handle, &project_clone, &files)
        })
        .await
        .map_err(|error| exec::CaptureError::Failed(format!("准备远程配置中断：{error}")))?
        .map_err(exec::CaptureError::Failed)?;
        if let Some(ref g) = prepared {
            let msg = format!(
                "[打包] 已临时恢复为 Git HEAD（云端排队后自动还原）：{}",
                g.restored_rel_paths.join("、")
            );
            crate::log_info!("{msg}");
            task_runner::log_line(app, &msg);
        }
        prepared
    } else {
        None
    };

    // 2. HBuilderX 4.87 的 CLI 必须依附已启动的程序，且没有无头模式。
    if !hbuilder_running() {
        ctx.stage("启动 HBuilderX…");
        let output = ctx.run(&cli, vec!["open".into()], OPEN_TIMEOUT).await?;
        if output.code != 0 {
            crate::log_warn!("cli open 返回 {}：{}", output.code, output.text.trim());
        }
    }
    ctx.stage("等待 HBuilderX 就绪…");
    let mut ready = false;
    let wait_start = std::time::Instant::now();
    while !ready {
        if ctx.cancelled() {
            return Err(exec::CaptureError::Cancelled);
        }
        let attempt = std::time::Instant::now();
        let output = ctx.run(&cli, vec!["ver".into()], Duration::from_secs(20)).await?;
        let version = output.text.trim().to_string();
        ready = output.code == 0 && version.chars().next().is_some_and(|c| c.is_ascii_digit());
        if !ready {
            crate::log_warn!(
                "[打包] ver 探测未就绪：code={} 用时={:?} 输出={:?}",
                output.code,
                attempt.elapsed(),
                version
            );
        } else {
            crate::log_info!("[打包] CLI 就绪：{version}（用时 {:?}）", attempt.elapsed());
        }
        if !ready {
            if wait_start.elapsed() > Duration::from_secs(60) {
                return Err(exec::CaptureError::Failed(
                    "HBuilderX 启动后命令行通道一直没有就绪，请手动打开 HBuilderX 后重试".into(),
                ));
            }
            tokio::select! {
                biased;
                _ = wait_cancel(&ctx.cancel) => return Err(exec::CaptureError::Cancelled),
                _ = tokio::time::sleep(Duration::from_millis(1500)) => {}
            }
        }
    }

    // 3. 让 HBuilderX 认识这个项目；已导入时这步只相当于打开，失败不阻断后续判断。
    ctx.stage("打开项目…");
    let opened = ctx
        .run(
            &cli,
            vec!["project".into(), "open".into(), "--path".into(), project_arg.clone()],
            OPEN_TIMEOUT,
        )
        .await?;
    if opened.code != 0 {
        crate::log_warn!("HBuilderX 打开项目返回 {}：{}", opened.code, opened.text.trim());
    }

    // 4. 云打包：阻塞到 CLI 自己结束（进入云端排队节点时回调会立即还原本地修改）。
    ctx.stage("提交云端打包…");
    let pack_type = {
        let kind = config.pack_type.trim();
        if kind.is_empty() { "3".to_string() } else { kind.to_string() }
    };
    let (packed, live_url) = ctx
        .run_pack(
            &cli,
            vec![
                "pack".into(),
                "--project".into(),
                project_arg,
                "--platform".into(),
                "android".into(),
                "--android.packagename".into(),
                config.packagename.trim().into(),
                "--android.androidpacktype".into(),
                pack_type,
            ],
            PACK_TIMEOUT,
            guard.as_ref().map(RemoteIgnoreGuard::restore_handle),
        )
        .await?;
    if let Some(ref g) = guard {
        g.restore_handle().restore("打包阶段结束，已还原本地修改");
    }
    let url = match live_url.or_else(|| find_download_url(&packed.text)) {
        Some(url) => url,
        None => {
            let hint = if packed.text.contains("连接已中断") {
                "HBuilderX 与命令行的连接中断，请再试一次".to_string()
            } else {
                tail(&packed.text)
            };
            return Err(exec::CaptureError::Failed(format!("打包结束但没有拿到下载地址：{hint}")));
        }
    };

    // 5. 下载到固定目录。云打包给的是临时地址（限次下载），必须立刻存到本地。
    if ctx.cancelled() {
        return Err(exec::CaptureError::Cancelled);
    }
    ctx.stage("下载 APK…");
    let directory = PathBuf::from(output_dir_text);
    std::fs::create_dir_all(&directory).map_err(|error| {
        exec::CaptureError::Failed(format!("无法创建输出目录 {}：{error}", directory.display()))
    })?;
    let target = directory.join(apk_name(&project, env));
    let client = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|error| exec::CaptureError::Failed(format!("无法创建下载客户端：{error}")))?;
    let response = tokio::select! {
        biased;
        _ = wait_cancel(&ctx.cancel) => return Err(exec::CaptureError::Cancelled),
        res = client.get(&url).send() => res.map_err(|error| exec::CaptureError::Failed(format!("下载 APK 失败：{error}")))?,
    };
    let response = response
        .error_for_status()
        .map_err(|error| exec::CaptureError::Failed(format!("下载地址返回错误：{error}")))?;
    let bytes = tokio::select! {
        biased;
        _ = wait_cancel(&ctx.cancel) => return Err(exec::CaptureError::Cancelled),
        res = response.bytes() => res.map_err(|error| exec::CaptureError::Failed(format!("读取 APK 内容失败：{error}")))?,
    };
    if ctx.cancelled() {
        return Err(exec::CaptureError::Cancelled);
    }
    if bytes.len() < 1024 || !bytes.starts_with(b"PK") {
        return Err(exec::CaptureError::Failed(
            "下载到的内容不是 APK（可能是过期的临时地址），请重新打包".into(),
        ));
    }
    std::fs::write(&target, &bytes)
        .map_err(|error| exec::CaptureError::Failed(format!("保存 APK 失败：{error}")))?;
    crate::log_info!("[打包] APK 已保存：{}", target.display());
    task_runner::mark_apk(app, &target.to_string_lossy());

    // 6. 下载完成后再检测手机：未开启自动装机时直接结束；未连接手机则温和跳过装机，不丢辛苦打出的 APK。
    if !config.auto_install {
        return Ok(PackOutcome { note: None });
    }
    if ctx.cancelled() {
        return Err(exec::CaptureError::Cancelled);
    }
    ctx.stage("检查手机连接…");
    let adb_path = adb::resolve(&config.adb);
    let wanted = config.device.clone();
    let device_res = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let probe = adb::probe_devices(&adb_path)?;
        adb::pick_from_probe(&probe, &wanted)
    })
    .await
    .map_err(|error| exec::CaptureError::Failed(format!("检查手机连接中断：{error}")))?;

    let device = match device_res {
        Ok(serial) => serial,
        Err(reason) => {
            let msg = format!("[打包] APK 已保存，跳过自动装机：{reason}");
            crate::log_info!("{msg}");
            task_runner::log_line(app, &msg);
            return Ok(PackOutcome {
                note: Some("APK 已保存 · 未连手机".into()),
            });
        }
    };

    ctx.stage("安装到手机…");
    let adb_path = adb::resolve(&config.adb);
    let installed = ctx
        .run(
            &adb_path,
            vec![
                "-s".into(),
                device.clone(),
                "install".into(),
                "-r".into(),
                target.to_string_lossy().into_owned(),
            ],
            INSTALL_TIMEOUT,
        )
        .await?;
    let text = installed.text.to_lowercase();
    if installed.code != 0 || !text.contains("success") {
        return Err(exec::CaptureError::Failed(format!(
            "APK 已保存但{}",
            format_install_failure(&installed)
        )));
    }
    crate::log_info!("[打包] 安装完成：{device}");

    if env == "local" {
        let port = config.effective_reverse_port();
        ctx.stage(&format!("建立端口转发 :{port}…"));
        let adb_path = adb::resolve(&config.adb);
        let serial = device.clone();
        let rev_res = tauri::async_runtime::spawn_blocking(move || adb::reverse(&adb_path, &serial, port))
            .await
            .map_err(|error| exec::CaptureError::Failed(format!("建立端口转发中断：{error}")))?;
        let note = match rev_res {
            Ok(()) => {
                crate::log_info!("[打包] 端口转发完成：tcp:{port}");
                format!("已装机并转发 :{port}")
            }
            Err(error) => {
                let msg = format!("[打包] 已安装到手机，但 adb reverse 失败：{error}");
                crate::log_warn!("{msg}");
                task_runner::log_line(app, &msg);
                format!("已装机（转发失败：{error}）")
            }
        };
        Ok(PackOutcome { note: Some(note) })
    } else {
        Ok(PackOutcome { note: Some("已安装到手机".into()) })
    }
}

/// 手动将已保存的 APK 安装到当前连接的手机；本地联调包安装后自动顺带建立 adb reverse。
#[tauri::command]
pub async fn install_pack_apk(
    app: AppHandle,
    apk_path: String,
    env: Option<String>,
) -> Result<InstallApkResult, String> {
    let trimmed = apk_path.trim().to_string();
    if trimmed.is_empty() {
        return Err("APK 文件路径不能为空".into());
    }
    let path = PathBuf::from(&trimmed);
    if !path.is_file() {
        return Err(format!("APK 文件不存在：{}", path.display()));
    }
    if !trimmed.to_ascii_lowercase().ends_with(".apk") {
        return Err("所选文件不是 .apk 安装包".into());
    }
    let config = load_config(&app);
    let adb_path = adb::resolve(&config.adb);
    let wanted = config.device.clone();
    let port = config.effective_reverse_port();
    let need_reverse = env.as_deref() == Some("local");

    tauri::async_runtime::spawn_blocking(move || {
        let probe = adb::probe_devices(&adb_path)?;
        let serial = adb::pick_from_probe(&probe, &wanted)?;
        let args = vec![
            "-s".into(),
            serial.clone(),
            "install".into(),
            "-r".into(),
            path.to_string_lossy().into_owned(),
        ];
        let installed = exec::capture(&adb_path, &args, None, exec::Options::quiet(INSTALL_TIMEOUT))
            .map_err(|error| error.text())?;
        let lower = installed.text.to_lowercase();
        if installed.code != 0 || !lower.contains("success") {
            return Err(format_install_failure(&installed));
        }
        let reversed_port = if need_reverse {
            adb::reverse(&adb_path, &serial, port)?;
            Some(port)
        } else {
            None
        };
        let adb_status = adb::snapshot(&adb_path, &serial);
        Ok(InstallApkResult {
            device: serial,
            reversed_port,
            adb: adb_status,
        })
    })
    .await
    .map_err(|error| format!("安装 APK 任务中断：{error}"))?
}

fn tail(text: &str) -> String {
    // CLI 的错误正文是 HTML 片段，先转成纯文本再交给界面。
    let plain = exec::plain(text);
    let lines: Vec<&str> = plain.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(3)..].join(" / ");
    if tail.chars().count() > 400 {
        tail.chars().take(400).collect::<String>() + "…"
    } else if tail.is_empty() {
        "（没有输出）".into()
    } else {
        tail
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_ignore_entry_normalizes_quotes_and_relative_paths() {
        let project = Path::new(r"D:\work\elder_gov_miniprogram");
        assert_eq!(
            clean_ignore_entry(r#" "D:\work\elder_gov_miniprogram\src\api\gov-auth.js" "#, Some(project)),
            Some("src/api/gov-auth.js".to_string())
        );
        assert_eq!(
            clean_ignore_entry(r".\src\config\area\areaConfig.json", Some(project)),
            Some("src/config/area/areaConfig.json".to_string())
        );
        assert_eq!(clean_ignore_entry("   \"\"  ", Some(project)), None);
    }

    #[test]
    fn loopback_scan_ignores_comments_and_catches_active_urls() {
        let safe_js = br#"
// export const GOV_API_BASE = 'http://localhost:11034';
export const GOV_API_BASE = 'https://demo3.eldercare.net.cn/prod-api';
"#;
        assert_eq!(find_local_loopback_in_code(safe_js), None);

        let safe_jsonc = br#"{
  "rawAreaList": [
    {
      // "baseUrl": "http://localhost:11034",
      "baseUrl": "https://demo3.eldercare.net.cn/prod-api",
      /* "orgUrl": "http://127.0.0.1:11034" */
      "govImageUrl": "https://files.eldercare.net.cn/public/newElderMiniProgram"
    }
  ]
}"#;
        assert_eq!(find_local_loopback_in_code(safe_jsonc), None);

        let unsafe_js = br#"
// export const GOV_API_BASE = 'https://demo3.eldercare.net.cn/prod-api';
export const GOV_API_BASE = 'http://localhost:11034';
"#;
        let hit = find_local_loopback_in_code(unsafe_js).expect("应检测到未注释的 localhost");
        assert_eq!(hit.0, 3);
        assert!(hit.1.contains("http://localhost:11034"));

        let unsafe_jsonc = br#"{
  "baseUrl": "http://127.0.0.1:11034"
}"#;
        let hit2 = find_local_loopback_in_code(unsafe_jsonc).expect("应检测到 127.0.0.1");
        assert_eq!(hit2.0, 2);
    }

    #[test]
    fn apk_name_adds_environment_suffix() {
        let dir = Path::new(".");
        assert!(apk_name(dir, "local").contains("-local-"));
        assert!(apk_name(dir, "remote").contains("-remote-"));
    }
}
