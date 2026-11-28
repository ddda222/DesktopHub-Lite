//! 启动外部目标（阶段 2）。
//!
//! 一律走 `ShellExecuteW`，交给系统决定怎么打开：
//! - `.exe` 直接执行；
//! - `.lnk` 交给系统解析（保留快捷方式自己的启动参数、工作目录、管理员标记），
//!   不自己把 `.lnk` 解析成 exe 再启动；
//! - 目录用资源管理器打开；
//! - `http/https/mailto` 用默认浏览器 / 邮件客户端打开。
//!
//! `ShellExecuteW` 的返回值只代表「系统接受了启动请求」，**不代表目标窗口已经打开**，
//! 所以返回文案只说「已交给系统启动」。

use std::path::Path;

use windows::core::HSTRING;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// 目标类型，用于界面提示与图标读取分支。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetKind {
    Exe,
    Shortcut,
    Folder,
    Url,
    Other,
}

impl TargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TargetKind::Exe => "exe",
            TargetKind::Shortcut => "shortcut",
            TargetKind::Folder => "folder",
            TargetKind::Url => "url",
            TargetKind::Other => "other",
        }
    }
}

pub fn kind_of(target: &str) -> TargetKind {
    let trimmed = target.trim();
    if crate::system_apps::is_target(trimmed) { return TargetKind::Shortcut; }
    let lowered = trimmed.to_ascii_lowercase();
    if lowered.starts_with("http://")
        || lowered.starts_with("https://")
        || lowered.starts_with("mailto:")
        || lowered.starts_with("ftp://")
    {
        return TargetKind::Url;
    }
    let path = Path::new(trimmed);
    if path.is_dir() {
        return TargetKind::Folder;
    }
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("exe") | Some("com") | Some("bat") | Some("cmd") => TargetKind::Exe,
        Some("lnk") | Some("url") | Some("appref-ms") => TargetKind::Shortcut,
        _ => TargetKind::Other,
    }
}

/// 目标是否真实存在（网址一律视为可交给系统；对无法明确判定的目标保守返回 true，杜绝误删正常应用）。
pub fn exists(target: &str) -> bool {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return false;
    }
    if crate::system_apps::is_target(trimmed) {
        return crate::system_apps::probe_exists(trimmed);
    }
    probe_local_target_exists(
        trimmed,
        |key| std::env::var(key).ok(),
        std::env::var("PATH").ok().as_deref(),
        std::env::var("PATHEXT").ok().as_deref(),
        |candidate| Path::new(candidate).exists(),
    )
}

// === EXISTS_CORE_START ===
const KNOWN_EXEC_EXTENSIONS: &[&str] = &[
    ".exe", ".com", ".bat", ".cmd", ".lnk", ".msc", ".cpl", ".url", ".appref-ms",
];

fn has_uri_scheme(trimmed: &str) -> bool {
    let Some(colon) = trimmed.find(':') else {
        return false;
    };
    // 单字母盘符（如 C:\...）不是 URI scheme
    if colon < 2 {
        return false;
    }
    let scheme = &trimmed[..colon];
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

pub fn expand_env_vars<F>(input: &str, lookup: &F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if let Some(rel_end) = input[i + 1..].find('%') {
                let end = i + 1 + rel_end;
                let var_name = &input[i + 1..end];
                if !var_name.is_empty() && !var_name.contains(char::is_whitespace) {
                    if let Some(val) = lookup(var_name) {
                        out.push_str(&val);
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        let ch = input[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn contains_unexpanded_env(input: &str) -> bool {
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            if !name.is_empty() && !name.contains(char::is_whitespace) {
                return true;
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    false
}

/// 从带引号或带命令行参数的目标串中提取可执行文件/路径部分。
pub fn extract_executable_candidate(trimmed: &str) -> String {
    if let Some(rest) = trimmed.strip_prefix('"') {
        if let Some(end_quote) = rest.find('"') {
            return rest[..end_quote].trim().to_string();
        }
        return rest.trim().to_string();
    }
    let lower = trimmed.to_ascii_lowercase();
    for ext in KNOWN_EXEC_EXTENSIONS {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(ext) {
            let abs_end = search_from + pos + ext.len();
            let boundary_ok = abs_end == trimmed.len()
                || trimmed[abs_end..].starts_with(char::is_whitespace);
            if boundary_ok {
                return trimmed[..abs_end].trim().to_string();
            }
            search_from = abs_end;
        }
    }
    trimmed
        .split_whitespace()
        .next()
        .unwrap_or(trimmed)
        .to_string()
}

fn is_drive_prefixed(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

fn has_file_extension(s: &str) -> bool {
    std::path::Path::new(s).extension().is_some()
}

fn pathext_list(pathext_var: Option<&str>) -> Vec<String> {
    let raw = pathext_var.unwrap_or(".COM;.EXE;.BAT;.CMD;.MSC;.CPL");
    let mut list: Vec<String> = raw
        .split(';')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .map(|e| if e.starts_with('.') { e.to_string() } else { format!(".{e}") })
        .collect();
    if list.is_empty() {
        list.push(".EXE".into());
    }
    list
}

/// 可离线单测的本地目标存在性探测核心：
/// 支持去引号、展开 `%ENV%`、拆分命令行参数、以及查询 `PATH`/`PATHEXT`；
/// 对网络共享路径、URI scheme 或无法解析的环境变量保守返回 `true`。
pub fn probe_local_target_exists<FLookup, FExists>(
    target: &str,
    env_lookup: FLookup,
    path_var: Option<&str>,
    pathext_var: Option<&str>,
    path_exists: FExists,
) -> bool
where
    FLookup: Fn(&str) -> Option<String>,
    FExists: Fn(&str) -> bool,
{
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return false;
    }
    if has_uri_scheme(trimmed) {
        return true;
    }

    // 1. 直接展开环境变量后整串探测（兼容含空格的无引号路径）
    let expanded_full = expand_env_vars(trimmed, &env_lookup);
    let unquoted_full = expanded_full.trim().trim_matches('"').trim();
    if !unquoted_full.is_empty() && path_exists(unquoted_full) {
        return true;
    }

    // 2. 提取带引号或带参数命令行中的程序主体并展开环境变量
    let raw_candidate = extract_executable_candidate(trimmed);
    let candidate = expand_env_vars(&raw_candidate, &env_lookup);
    let candidate = candidate.trim().trim_matches('"').trim();
    if candidate.is_empty() {
        return true;
    }
    if has_uri_scheme(candidate) || contains_unexpanded_env(candidate) {
        return true;
    }
    // UNC 网络路径避免离线/网络抖动造成误删，保守返回 true
    if candidate.starts_with(r"\\") || candidate.starts_with("//") {
        return true;
    }

    let exts = pathext_list(pathext_var);
    let has_sep_or_drive = candidate.contains('\\') || candidate.contains('/') || is_drive_prefixed(candidate);
    if has_sep_or_drive {
        if path_exists(candidate) {
            return true;
        }
        if !has_file_extension(candidate) {
            for ext in &exts {
                let with_ext = format!("{candidate}{ext}");
                if path_exists(&with_ext) || path_exists(&format!("{candidate}{}", ext.to_ascii_lowercase())) {
                    return true;
                }
            }
        }
        return false;
    }

    // 3. 无目录前缀的可执行文件名（如 wt.exe / notepad.exe / calc）：在 PATH 中探测
    let Some(path_str) = path_var else {
        return true;
    };
    let dirs: Vec<&str> = path_str
        .split(';')
        .map(|s| s.trim().trim_matches('"').trim())
        .filter(|s| !s.is_empty())
        .collect();
    if dirs.is_empty() {
        return true;
    }
    for dir in dirs {
        let sep = if dir.ends_with('\\') || dir.ends_with('/') { "" } else { "\\" };
        let direct = format!("{dir}{sep}{candidate}");
        if path_exists(&direct) {
            return true;
        }
        if !has_file_extension(candidate) {
            for ext in &exts {
                if path_exists(&format!("{direct}{ext}"))
                    || path_exists(&format!("{direct}{}", ext.to_ascii_lowercase()))
                {
                    return true;
                }
            }
        }
    }
    false
}
// === EXISTS_CORE_END ===

/// 交给系统启动。`Ok(())` 只表示系统接受了请求。
pub fn launch(target: &str) -> Result<(), String> {
    let trimmed = target.trim();
    if crate::system_apps::is_target(trimmed) { return crate::system_apps::Item::resolve(trimmed)?.open(); }
    if trimmed.is_empty() {
        return Err("目标为空".into());
    }
    if !exists(trimmed) {
        return Err(format!("目标不存在：{trimmed}"));
    }

    let verb = HSTRING::from("open");
    let file = HSTRING::from(trimmed);
    let empty = windows::core::PCWSTR::null();
    let result = unsafe {
        ShellExecuteW(
            None,
            &verb,
            &file,
            empty,
            empty,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW 的返回值 <= 32 表示失败，具体含义见 Win32 文档。
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(match code {
            0 => "系统内存不足，启动失败".to_string(),
            2 => "找不到文件".to_string(),
            3 => "找不到路径".to_string(),
            5 => "拒绝访问".to_string(),
            8 => "内存不足".to_string(),
            26 => "文件共享冲突".to_string(),
            27 => "文件关联不完整".to_string(),
            28 => "启动超时".to_string(),
            29 => "启动失败（DDE 超时）".to_string(),
            30 => "启动失败（DDE 忙）".to_string(),
            31 => "没有关联的程序能打开这个文件".to_string(),
            32 => "找不到关联的程序".to_string(),
            other => format!("系统拒绝启动（错误码 {other}）"),
        })
    }
}

/// 在资源管理器中定位目标（用于「打开所在文件夹」）。
pub fn reveal(target: &str) -> Result<(), String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err("目标为空".into());
    }
    let path = Path::new(trimmed);
    if !path.exists() {
        return Err(format!("目标不存在：{trimmed}"));
    }
    let verb = HSTRING::from("open");
    let dir = HSTRING::from("explorer.exe");
    let arg = if path.is_dir() {
        HSTRING::from(format!("\"{trimmed}\""))
    } else {
        HSTRING::from(format!("/select,\"{trimmed}\""))
    };
    let result = unsafe {
        ShellExecuteW(
            None,
            &verb,
            &dir,
            &arg,
            windows::core::PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if (result.0 as isize) > 32 {
        Ok(())
    } else {
        Err("资源管理器拒绝打开".into())
    }
}

/// 从目标推导一个可读名字：`C:\...\Code.exe` → `Code`。
pub fn suggested_name(target: &str) -> String {
    let trimmed = target.trim();
    if kind_of(trimmed) == TargetKind::Url {
        let without_scheme = trimmed
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(trimmed);
        let host = without_scheme.split('/').next().unwrap_or(without_scheme);
        return host.trim_start_matches("www.").to_string();
    }
    let path = Path::new(trimmed);
    path.file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

// === TESTS ===
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn mock_env(key: &str) -> Option<String> {
        match key.to_ascii_uppercase().as_str() {
            "WINDIR" | "SYSTEMROOT" => Some(r"C:\Windows".into()),
            "LOCALAPPDATA" => Some(r"C:\Users\Tester\AppData\Local".into()),
            _ => None,
        }
    }

    #[test]
    fn probe_supports_quoted_paths_args_env_vars_and_path_lookup() {
        let existing: HashSet<String> = [
            r"c:\program files\my app\tool.exe",
            r"c:\windows\system32\notepad.exe",
            r"c:\windows\system32\cmd.exe",
            r"c:\users\tester\appdata\local\microsoft\windowsapps\wt.exe",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        let exists_fn = |p: &str| existing.contains(&p.to_ascii_lowercase());
        let path_var = Some(r"C:\Windows\System32;C:\Users\Tester\AppData\Local\Microsoft\WindowsApps");
        let pathext = Some(".COM;.EXE;.BAT;.CMD");

        // 带引号的空格路径与带参数命令行
        assert!(probe_local_target_exists(r#""C:\Program Files\My App\tool.exe""#, mock_env, path_var, pathext, exists_fn));
        assert!(probe_local_target_exists(r#""C:\Program Files\My App\tool.exe" --profile dev"#, mock_env, path_var, pathext, exists_fn));
        assert!(probe_local_target_exists(r#"C:\Program Files\My App\tool.exe --flag 1"#, mock_env, path_var, pathext, exists_fn));

        // %WINDIR% 环境变量展开
        assert!(probe_local_target_exists(r"%WINDIR%\System32\notepad.exe", mock_env, path_var, pathext, exists_fn));
        assert!(probe_local_target_exists(r#""%WINDIR%\System32\notepad.exe" notes.txt"#, mock_env, path_var, pathext, exists_fn));

        // PATH 可执行文件名（含无扩展名 cmd）
        assert!(probe_local_target_exists("wt.exe", mock_env, path_var, pathext, exists_fn));
        assert!(probe_local_target_exists("notepad.exe", mock_env, path_var, pathext, exists_fn));
        assert!(probe_local_target_exists("cmd", mock_env, path_var, pathext, exists_fn));

        // 明确不存在的本地绝对路径应返回 false
        assert!(!probe_local_target_exists(r"C:\Missing\gone.exe", mock_env, path_var, pathext, exists_fn));
        assert!(!probe_local_target_exists(r#""C:\Missing\gone.exe" --arg"#, mock_env, path_var, pathext, exists_fn));
        assert!(!probe_local_target_exists("definitely-not-on-path.exe", mock_env, path_var, pathext, exists_fn));

        // 保守策略：自定义 URI scheme、UNC 共享路径、未知环境变量一律返回 true，绝不误删正常应用
        assert!(probe_local_target_exists("ms-settings:display", mock_env, path_var, pathext, exists_fn));
        assert!(probe_local_target_exists("steam://rungameid/730", mock_env, path_var, pathext, exists_fn));
        assert!(probe_local_target_exists(r"\\nas-server\share\tool.exe", mock_env, path_var, pathext, exists_fn));
        assert!(probe_local_target_exists(r"%CUSTOM_MISSING_VAR%\bin\app.exe", mock_env, path_var, pathext, exists_fn));
    }
}
