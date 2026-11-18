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

/// 目标是否真实存在（网址一律视为可交给系统）。
pub fn exists(target: &str) -> bool {
    if crate::system_apps::is_target(target.trim()) { return crate::system_apps::Item::resolve(target.trim()).is_ok(); }
    match kind_of(target) {
        TargetKind::Url => true,
        _ => Path::new(target.trim()).exists(),
    }
}

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
