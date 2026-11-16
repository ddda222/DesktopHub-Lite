//! 应用快捷方式配置。
//!
//! 阶段 2 起：配置由用户手动添加或从桌面快捷方式导入得到真实目标，不做全盘扫描；
//! `seed()` 只收录本机确实存在的系统程序（逐个校验存在性），不写猜出来的路径。
//! 启动、图标读取分别在 `launcher.rs`、`icons.rs`。

use serde::{Deserialize, Serialize};

use crate::launcher;

pub const CONFIG_FILE: &str = "applications.json";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HubApp {
    pub id: String,
    pub name: String,
    pub target: String,
    pub category_id: String,
    pub color: String,
    pub initials: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broken: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub schema_version: u32,
    pub categories: Vec<Category>,
    pub apps: Vec<HubApp>,
}

/// 打开应用的返回：`launched=false` 时 `reason` 说明为什么没有启动。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenResult {
    pub launched: bool,
    pub reason: String,
}

fn cat(id: &str, name: &str) -> Category {
    Category {
        id: id.into(),
        name: name.into(),
    }
}

fn app(
    id: &str,
    name: &str,
    target: &str,
    category_id: &str,
    color: &str,
    initials: &str,
    hidden: bool,
) -> HubApp {
    HubApp {
        id: id.into(),
        name: name.into(),
        target: target.into(),
        category_id: category_id.into(),
        color: color.into(),
        initials: initials.into(),
        hidden,
        broken: None,
    }
}

/// 首次运行的初始配置。
///
/// 这里只放**本机确实存在**的 Windows 自带程序：候选目标逐个用 `exists` 校验，
/// 不存在的直接丢掉，所以界面上不会出现「猜出来的路径」冒充已发现的应用。
/// 其余应用由用户手动添加或从桌面快捷方式导入。
pub fn seed() -> AppConfig {
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    let candidates: Vec<(&str, String, &str, &str)> = vec![
        ("a-explorer", format!(r"{system_root}\explorer.exe"), "文件资源管理器", "c-common"),
        ("a-notepad", format!(r"{system_root}\System32\notepad.exe"), "记事本", "c-common"),
        ("a-cmd", format!(r"{system_root}\System32\cmd.exe"), "命令提示符", "c-dev"),
        ("a-powershell", format!(r"{system_root}\System32\WindowsPowerShell\v1.0\powershell.exe"), "PowerShell", "c-dev"),
        ("a-mspaint", format!(r"{system_root}\System32\mspaint.exe"), "画图", "c-common"),
        ("a-calc", format!(r"{system_root}\System32\calc.exe"), "计算器", "c-common"),
        ("a-taskmgr", format!(r"{system_root}\System32\Taskmgr.exe"), "任务管理器", "c-dev"),
        ("a-control", format!(r"{system_root}\System32\control.exe"), "控制面板", "c-common"),
    ];

    let apps = candidates
        .into_iter()
        .filter(|(_, target, _, _)| launcher::exists(target))
        .map(|(id, target, name, category)| {
            let mut entry = app(id, name, &target, category, &color_of(name), &initials_of(name), false);
            // 路径存在，但仍按真实类型标注，避免界面把 exe 当快捷方式。
            entry.broken = None;
            entry
        })
        .collect();

    AppConfig {
        schema_version: SCHEMA_VERSION,
        categories: vec![
            cat("all", "全部"),
            cat("c-common", "常用"),
            cat("c-dev", "开发"),
            cat("c-media", "影音"),
            cat("c-work", "办公"),
        ],
        apps,
    }
}

impl AppConfig {
    pub fn find(&self, id: &str) -> Option<&HubApp> {
        self.apps.iter().find(|a| a.id == id)
    }

    /// 按目标去重：同一个 exe / lnk 不重复添加。
    pub fn find_by_target(&self, target: &str) -> Option<&HubApp> {
        let needle = normalize_target(target);
        self.apps
            .iter()
            .find(|a| normalize_target(&a.target) == needle)
    }
}

/// 比较目标时忽略大小写与首尾空白（Windows 路径不区分大小写）。
pub fn normalize_target(target: &str) -> String {
    target.trim().to_ascii_lowercase()
}

/// 由名称推导两个字的缩写（中文取首字，英文取前两位大写）。
pub fn initials_of(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "?".into();
    }
    let mut chars = trimmed.chars();
    let first = chars.next().unwrap();
    if first.is_ascii_alphanumeric() {
        let mut out = String::new();
        out.push(first.to_ascii_uppercase());
        for ch in chars {
            if ch.is_ascii_alphanumeric() {
                out.push(ch.to_ascii_uppercase());
            } else {
                break;
            }
            if out.chars().count() >= 2 {
                break;
            }
        }
        return out;
    }
    first.to_string()
}

/// 稳定的占位色：同一个名称每次都得到同一个颜色，图标缺失时不难看。
pub fn color_of(name: &str) -> String {
    const PALETTE: [&str; 10] = [
        "#3C8DFF", "#22C55E", "#E0A33E", "#8B5CF6", "#F0673B", "#2E90C8", "#E23B3B", "#3A7D44",
        "#E879A6", "#57606A",
    ];
    let mut h: u32 = 2166136261;
    for byte in name.as_bytes() {
        h ^= u32::from(*byte);
        h = h.wrapping_mul(16777619);
    }
    PALETTE[(h as usize) % PALETTE.len()].to_string()
}

/// 生成新应用的 id：`a-` + 目标哈希，保证同一目标反复添加得到同一个 id。
pub fn id_for_target(target: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in normalize_target(target).as_bytes() {
        h ^= u64::from(*byte);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("a-{h:016x}")
}

/// 由真实目标构造一个应用条目。
pub fn app_from_target(target: &str, category_id: &str, name: Option<&str>) -> HubApp {
    let trimmed = target.trim();
    let name = name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| launcher::suggested_name(trimmed));
    let broken = if launcher::exists(trimmed) {
        None
    } else {
        Some(true)
    };
    HubApp {
        id: id_for_target(trimmed),
        name: name.clone(),
        target: trimmed.to_string(),
        category_id: category_id.to_string(),
        color: color_of(&name),
        initials: initials_of(&name),
        hidden: false,
        broken,
    }
}

/// 从当前用户桌面与公共桌面读取快捷方式（只读这两个目录，不做全盘扫描）。
pub fn desktop_shortcuts() -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for dir in desktop_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut items: Vec<_> = entries.flatten().collect();
        items.sort_by_key(|e| e.file_name());
        for entry in items {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase());
            if !matches!(ext.as_deref(), Some("lnk") | Some("url") | Some("exe")) {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .trim()
                .to_string();
            if name.is_empty() {
                continue;
            }
            let target = path.to_string_lossy().to_string();
            if found.iter().any(|(t, _)| normalize_target(t) == normalize_target(&target)) {
                continue;
            }
            found.push((target, name));
        }
    }
    found
}

#[cfg(windows)]
fn desktop_dirs() -> Vec<std::path::PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_PublicDesktop, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
    };

    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    for folder in [&FOLDERID_Desktop, &FOLDERID_PublicDesktop] {
        unsafe {
            if let Ok(raw) = SHGetKnownFolderPath(folder, KF_FLAG_DEFAULT, None) {
                let text = raw.to_string().unwrap_or_default();
                CoTaskMemFree(Some(raw.0 as *const core::ffi::c_void));
                if !text.trim().is_empty() {
                    dirs.push(std::path::PathBuf::from(text));
                }
            }
        }
    }
    dirs
}

#[cfg(not(windows))]
fn desktop_dirs() -> Vec<std::path::PathBuf> {
    Vec::new()
}
