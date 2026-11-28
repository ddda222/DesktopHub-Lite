//! 新安装应用增量发现与已收录应用健康巡检。
//!
//! 设计原则：
//! - 首次运行静默建立本机已装应用基线（绝不把 100+ 系统预装软件一次性倾倒炸屏）；
//! - 仅在开始菜单 / Packages 目录 mtime 变化（<1ms 零闲置开销）或用户手动触发时调用 Get-StartApps；
//! - 跨 .exe / .lnk / shell:AppsFolder 智能去重并过滤卸载器/帮助文档等噪声项；
//! - 发现新应用仅保存在 `app-discovery.json` 并由设置中心静默展示角标与顶部待分类卡片，零打扰弹窗。

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::applications::{Category, HubApp};

pub const DISCOVERY_FILE: &str = "app-discovery.json";
pub const DISCOVERY_EVENT: &str = "app-discovery-changed";
pub const DISCOVERY_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredApp {
    pub name: String,
    pub target: String,
    pub suggested_category_id: String,
    pub discovered_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DismissedApp {
    pub name: String,
    pub target: String,
    pub dismissed_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryState {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub initialized: bool,
    #[serde(default)]
    pub last_scan_ms: u64,
    #[serde(default)]
    pub folder_fingerprints: BTreeMap<String, u64>,
    #[serde(default)]
    pub seen_targets: Vec<String>,
    #[serde(default)]
    pub dismissed_targets: Vec<String>,
    #[serde(default)]
    pub dismissed: Vec<DismissedApp>,
    #[serde(default)]
    pub pending: Vec<DiscoveredApp>,
}

fn default_schema_version() -> u32 {
    DISCOVERY_SCHEMA_VERSION
}

impl Default for DiscoveryState {
    fn default() -> Self {
        Self {
            schema_version: DISCOVERY_SCHEMA_VERSION,
            initialized: false,
            last_scan_ms: 0,
            folder_fingerprints: BTreeMap::new(),
            seen_targets: Vec::new(),
            dismissed_targets: Vec::new(),
            dismissed: Vec::new(),
            pending: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoverySnapshot {
    pub initialized: bool,
    pub last_scan_ms: u64,
    pub pending: Vec<DiscoveredApp>,
    pub dismissed: Vec<DismissedApp>,
}

impl DiscoveryState {
    pub fn snapshot(&self) -> DiscoverySnapshot {
        DiscoverySnapshot {
            initialized: self.initialized,
            last_scan_ms: self.last_scan_ms,
            pending: self.pending.clone(),
            dismissed: self.dismissed.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileOutcome {
    pub initialized_now: bool,
    pub added_count: usize,
    pub pruned_count: usize,
}

pub fn normalize_target_key(target: &str) -> String {
    target.trim().to_ascii_lowercase()
}

/// 归一化应用显示名（去空白、去常见平台/架构/版本后缀），用于跨 .exe / .lnk / shell:AppsFolder 去重。
pub fn normalize_app_name(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    let stripped = lower
        .replace("(64-bit)", " ")
        .replace("(32-bit)", " ")
        .replace("(x64)", " ")
        .replace("(x86)", " ")
        .replace("64-bit", " ")
        .replace("32-bit", " ")
        .replace("desktop", " ")
        .replace("app", " ");
    let mut out = String::with_capacity(stripped.len());
    for ch in stripped.chars() {
        if ch.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch) {
            out.push(ch);
        }
    }
    out
}

/// 过滤明显不是主程序的开始菜单噪声条目（卸载器、文档、帮助、命令行工具等）。
pub fn is_noise_app(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    const NOISE_KEYWORDS: &[&str] = &[
        "uninstall",
        "卸载",
        "readme",
        "read me",
        "帮助",
        "help",
        "documentation",
        "manual",
        "使用说明",
        "说明文档",
        "website",
        "官网",
        "homepage",
        "release notes",
        "changelog",
        "license",
        "许可协议",
        "command prompt",
        "developer powershell",
        "developer command",
        "installer",
        "setup",
        "repair",
        "修复工具",
        "update utility",
        "检查更新",
        "feedback hub",
        "get started",
    ];
    NOISE_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

fn target_stem_key(target: &str) -> Option<String> {
    let trimmed = target.trim();
    if trimmed.to_ascii_lowercase().starts_with("shell:appsfolder\\") {
        // 例如 Microsoft.VisualStudioCode 或 AppId!App -> 取最后一段可读词
        let tail = trimmed.split('\\').last()?;
        let core = tail.split('!').next().unwrap_or(tail);
        let segment = core.split('.').last().unwrap_or(core);
        let key = normalize_app_name(segment);
        return (!key.is_empty() && key.len() >= 3).then_some(key);
    }
    let stem = Path::new(trimmed).file_stem()?.to_str()?;
    let key = normalize_app_name(stem);
    (!key.is_empty() && key.len() >= 3).then_some(key)
}

/// 判断候选系统应用是否与用户已收录应用重复（跨 .exe / .lnk / shell:AppsFolder）。
pub fn matches_existing_app(existing: &[HubApp], name: &str, target: &str) -> bool {
    let target_key = normalize_target_key(target);
    let name_key = normalize_app_name(name);
    let cand_stem = target_stem_key(target);

    for app in existing {
        if normalize_target_key(&app.target) == target_key {
            return true;
        }
        let existing_name = normalize_app_name(&app.name);
        if !name_key.is_empty() && !existing_name.is_empty() && existing_name == name_key {
            return true;
        }
        if let Some(existing_stem) = target_stem_key(&app.target) {
            if !name_key.is_empty() && (name_key == existing_stem || name_key.ends_with(&existing_stem)) && existing_stem.len() >= 4 {
                return true;
            }
            if let Some(ref cstem) = cand_stem {
                if cstem == &existing_stem && existing_stem.len() >= 4 {
                    return true;
                }
            }
        }
    }
    false
}

/// 基于现有分类与常见软件关键词给出智能预选分类 ID。
pub fn suggest_category(name: &str, target: &str, categories: &[Category]) -> String {
    let hay = format!("{} {}", name.to_ascii_lowercase(), target.to_ascii_lowercase());

    let find_cat = |id_hints: &[&str], name_hints: &[&str]| -> Option<String> {
        categories
            .iter()
            .find(|cat| {
                if cat.id == "all" {
                    return false;
                }
                let cid = cat.id.to_ascii_lowercase();
                let cname = cat.name.to_ascii_lowercase();
                id_hints.iter().any(|h| cid.contains(h)) || name_hints.iter().any(|h| cname.contains(h))
            })
            .map(|cat| cat.id.clone())
    };

    const DEV_KW: &[&str] = &[
        "code", "cursor", "vscode", "visual studio", "git", "github", "docker", "dbeaver",
        "postman", "apifox", "orca", "idea", "pycharm", "webstorm", "goland", "clion",
        "android studio", "terminal", "powershell", "wsl", "node", "python", "rust", "figma",
        "wireshark", "fiddler", "charles", "mobaxterm", "xshell", "navicat", "hbuilder",
    ];
    if DEV_KW.iter().any(|kw| hay.contains(kw)) {
        if let Some(id) = find_cat(&["dev"], &["开发", "工具", "编程"]) {
            return id;
        }
    }

    const MEDIA_KW: &[&str] = &[
        "music", "spotify", "网易云", "qq音乐", "酷狗", "potplayer", "vlc", "mpv", "obs",
        "bilibili", "哔哩哔哩", "剪映", "capcut", "premiere", "photoshop", "steam", "epic",
        "wegame", "爱奇艺", "腾讯视频", "优酷", "blender",
    ];
    if MEDIA_KW.iter().any(|kw| hay.contains(kw)) {
        if let Some(id) = find_cat(&["media"], &["影音", "娱乐", "媒体", "游戏"]) {
            return id;
        }
    }

    const WORK_KW: &[&str] = &[
        "wps", "word", "excel", "powerpoint", "office", "notion", "obsidian", "typora",
        "飞书", "feishu", "lark", "钉钉", "dingtalk", "企业微信", "wecom", "teams", "zoom",
        "腾讯会议", "slack", "xmind", "acrobat", "pdf", "onenote", "outlook", "foxmail",
    ];
    if WORK_KW.iter().any(|kw| hay.contains(kw)) {
        if let Some(id) = find_cat(&["work", "office"], &["办公", "效率", "文档"]) {
            return id;
        }
    }

    find_cat(&["common"], &["常用"])
        .or_else(|| categories.iter().find(|c| c.id != "all").map(|c| c.id.clone()))
        .unwrap_or_else(|| "all".into())
}

/// 将一轮系统应用列表合并进发现快照：
/// - 首次调用仅静默记录基线 `seen_targets`，绝不推入 `pending`；
/// - 后续调用仅增量检出新装应用，并自动清理已卸载或已被用户收录的过期条目。
pub fn reconcile_discovery(
    state: &mut DiscoveryState,
    system_apps: &[(String, String)],
    existing_apps: &[HubApp],
    categories: &[Category],
    now_ms: u64,
) -> ReconcileOutcome {
    let mut seen_set: HashSet<String> = state.seen_targets.iter().cloned().collect();
    let dismissed_set: HashSet<String> = state.dismissed_targets.iter().cloned().collect();

    if !state.initialized {
        for (_, target) in system_apps {
            let key = normalize_target_key(target);
            if !key.is_empty() && seen_set.insert(key.clone()) {
                state.seen_targets.push(key);
            }
        }
        state.initialized = true;
        state.last_scan_ms = now_ms;
        state.pending.clear();
        return ReconcileOutcome {
            initialized_now: true,
            added_count: 0,
            pruned_count: 0,
        };
    }

    let live_targets: HashSet<String> = system_apps
        .iter()
        .map(|(_, target)| normalize_target_key(target))
        .filter(|k| !k.is_empty())
        .collect();

    let before_pending = state.pending.len();
    state.pending.retain(|item| {
        let key = normalize_target_key(&item.target);
        live_targets.contains(&key)
            && !dismissed_set.contains(&key)
            && !matches_existing_app(existing_apps, &item.name, &item.target)
    });
    let pruned_count = before_pending.saturating_sub(state.pending.len());

    let mut pending_keys: HashSet<String> = state
        .pending
        .iter()
        .map(|item| normalize_target_key(&item.target))
        .collect();
    let mut pending_names: HashSet<String> = state
        .pending
        .iter()
        .map(|item| normalize_app_name(&item.name))
        .filter(|k| !k.is_empty())
        .collect();

    let mut added_count = 0usize;
    for (name, target) in system_apps {
        let key = normalize_target_key(target);
        if key.is_empty() {
            continue;
        }
        let already_seen = !seen_set.insert(key.clone());
        if !already_seen {
            state.seen_targets.push(key.clone());
        }
        if already_seen || dismissed_set.contains(&key) || pending_keys.contains(&key) {
            continue;
        }
        if is_noise_app(name) {
            continue;
        }
        if matches_existing_app(existing_apps, name, target) {
            continue;
        }
        let nkey = normalize_app_name(name);
        if !nkey.is_empty() && !pending_names.insert(nkey) {
            continue;
        }
        pending_keys.insert(key);
        state.pending.push(DiscoveredApp {
            name: name.trim().to_string(),
            target: target.trim().to_string(),
            suggested_category_id: suggest_category(name, target, categories),
            discovered_at_ms: now_ms,
        });
        added_count += 1;
    }

    // 按发现时间倒序、名称正序排列，让最新安装的应用排在最上方。
    state.pending.sort_by(|a, b| {
        b.discovered_at_ms
            .cmp(&a.discovered_at_ms)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    state.last_scan_ms = now_ms;
    ReconcileOutcome {
        initialized_now: false,
        added_count,
        pruned_count,
    }
}

pub fn dismiss_pending_target(state: &mut DiscoveryState, target: &str, now_ms: u64) -> bool {
    let key = normalize_target_key(target);
    let Some(pos) = state
        .pending
        .iter()
        .position(|item| normalize_target_key(&item.target) == key)
    else {
        return false;
    };
    let removed = state.pending.remove(pos);
    if !state.dismissed_targets.iter().any(|k| k == &key) {
        state.dismissed_targets.push(key.clone());
    }
    state
        .dismissed
        .retain(|item| normalize_target_key(&item.target) != key);
    state.dismissed.insert(
        0,
        DismissedApp {
            name: removed.name,
            target: removed.target,
            dismissed_at_ms: now_ms,
        },
    );
    if state.dismissed.len() > 80 {
        state.dismissed.truncate(80);
    }
    true
}

pub fn dismiss_all_pending(state: &mut DiscoveryState, now_ms: u64) -> usize {
    let items: Vec<String> = state.pending.iter().map(|i| i.target.clone()).collect();
    let mut count = 0;
    for target in items {
        if dismiss_pending_target(state, &target, now_ms) {
            count += 1;
        }
    }
    count
}

pub fn restore_dismissed_target(
    state: &mut DiscoveryState,
    target: &str,
    categories: &[Category],
    now_ms: u64,
) -> bool {
    let key = normalize_target_key(target);
    let Some(pos) = state
        .dismissed
        .iter()
        .position(|item| normalize_target_key(&item.target) == key)
    else {
        return false;
    };
    let item = state.dismissed.remove(pos);
    state.dismissed_targets.retain(|k| k != &key);
    if !state
        .pending
        .iter()
        .any(|p| normalize_target_key(&p.target) == key)
    {
        state.pending.insert(
            0,
            DiscoveredApp {
                suggested_category_id: suggest_category(&item.name, &item.target, categories),
                name: item.name,
                target: item.target,
                discovered_at_ms: now_ms,
            },
        );
    }
    true
}

pub fn remove_pending_target(state: &mut DiscoveryState, target: &str) -> bool {
    let key = normalize_target_key(target);
    let before = state.pending.len();
    state
        .pending
        .retain(|item| normalize_target_key(&item.target) != key);
    if !state.seen_targets.iter().any(|k| k == &key) {
        state.seen_targets.push(key);
    }
    state.pending.len() != before
}

/// 批量校验已收录应用的路径有效性，返回 `broken` 状态发生变化的条目数量。
pub fn reconcile_broken_apps<F>(apps: &mut [HubApp], mut exists_fn: F) -> usize
where
    F: FnMut(&str) -> bool,
{
    let mut changed = 0usize;
    for app in apps.iter_mut() {
        let next = if exists_fn(&app.target) {
            None
        } else {
            Some(true)
        };
        if app.broken != next {
            app.broken = next;
            changed += 1;
        }
    }
    changed
}

// === TAURI_COMMANDS ===

use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::applications::{app_from_target, AppConfig, CONFIG_FILE};

#[derive(Default)]
pub struct DiscoveryManager {
    lock: Mutex<()>,
    last_check_ms: Mutex<u64>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 读取开始菜单与商店应用目录的秒级 mtime 指纹（纯元数据查询，<1ms，零 PowerShell 开销）。
fn collect_folder_fingerprints() -> BTreeMap<String, u64> {
    let mut map = BTreeMap::new();
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(Path::new(&appdata).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Ok(programdata) = std::env::var("PROGRAMDATA") {
        dirs.push(Path::new(&programdata).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        dirs.push(Path::new(&local).join("Packages"));
    }
    for dir in dirs {
        let mtime = std::fs::metadata(&dir)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        map.insert(dir.to_string_lossy().to_ascii_lowercase(), mtime);
    }
    map
}

fn load_state(app: &AppHandle) -> DiscoveryState {
    let app_state = app.state::<crate::AppState>();
    app_state
        .store
        .read::<DiscoveryState>(DISCOVERY_FILE)
        .unwrap_or_default()
}

fn save_state(app: &AppHandle, state: &DiscoveryState) -> Result<(), String> {
    let app_state = app.state::<crate::AppState>();
    app_state.store.write(DISCOVERY_FILE, state)
}

fn broadcast_discovery(app: &AppHandle, snapshot: &DiscoverySnapshot) {
    let _ = app.emit(DISCOVERY_EVENT, snapshot);
}

fn perform_scan_sync(app: &AppHandle, force: bool) -> Result<DiscoverySnapshot, String> {
    let mgr = app.state::<DiscoveryManager>();
    let _guard = mgr.lock.lock().unwrap_or_else(|e| e.into_inner());
    let now = now_ms();

    let mut state = load_state(app);
    let (existing_apps, categories) = {
        let app_state = app.state::<crate::AppState>();
        let cfg = app_state.apps.lock().unwrap_or_else(|e| e.into_inner());
        (cfg.apps.clone(), cfg.categories.clone())
    };

    // 同步清理 pending 中已被用户通过其他入口添加的应用
    let before_sync = state.pending.len();
    state
        .pending
        .retain(|item| !matches_existing_app(&existing_apps, &item.name, &item.target));
    let mut dirty = state.pending.len() != before_sync;

    // 60 秒防抖保护（手动点击强制扫描跳过防抖）
    if !force && state.initialized {
        let mut last_check = mgr.last_check_ms.lock().unwrap_or_else(|e| e.into_inner());
        if now.saturating_sub(*last_check) < 60_000 {
            if dirty {
                let _ = save_state(app, &state);
            }
            return Ok(state.snapshot());
        }
        *last_check = now;
    } else {
        *mgr.last_check_ms.lock().unwrap_or_else(|e| e.into_inner()) = now;
    }

    let fingerprints = collect_folder_fingerprints();
    let folders_changed = !state.initialized || state.folder_fingerprints != fingerprints;
    if !force && !folders_changed {
        if dirty {
            let _ = save_state(app, &state);
        }
        return Ok(state.snapshot());
    }

    let system_apps: Vec<(String, String)> = crate::system_apps::list()?
        .into_iter()
        .map(|item| (item.name, item.target))
        .collect();

    let outcome = reconcile_discovery(&mut state, &system_apps, &existing_apps, &categories, now);
    state.folder_fingerprints = fingerprints;
    dirty = dirty || outcome.initialized_now || outcome.added_count > 0 || outcome.pruned_count > 0 || force || folders_changed;
    if dirty {
        save_state(app, &state)?;
    }
    let snap = state.snapshot();
    if outcome.added_count > 0 || outcome.pruned_count > 0 || force {
        broadcast_discovery(app, &snap);
    }
    Ok(snap)
}

/// 启动后台低频门控监测：首启延迟 15 秒建基线/检查，之后每 10 分钟执行一次 <1ms 目录 mtime 检查。
pub fn start(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("app-discovery".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(15));
            let _ = perform_scan_sync(&app, false);
            loop {
                std::thread::sleep(Duration::from_secs(600));
                let _ = perform_scan_sync(&app, false);
            }
        });
}

#[tauri::command]
pub fn list_discovered_apps(app: AppHandle, mgr: State<'_, DiscoveryManager>) -> DiscoverySnapshot {
    let _guard = mgr.lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state(&app);
    let existing = {
        let app_state = app.state::<crate::AppState>();
        let cloned = app_state.apps.lock().unwrap_or_else(|e| e.into_inner()).apps.clone();
        cloned
    };
    let before = state.pending.len();
    state
        .pending
        .retain(|item| !matches_existing_app(&existing, &item.name, &item.target));
    if state.pending.len() != before {
        let _ = save_state(&app, &state);
    }
    state.snapshot()
}

#[tauri::command]
pub async fn scan_discovered_apps(app: AppHandle, force: Option<bool>) -> Result<DiscoverySnapshot, String> {
    let force_scan = force.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || perform_scan_sync(&app, force_scan))
        .await
        .map_err(|_| "扫描新安装应用失败".to_string())?
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptDiscoveredResult {
    pub config: AppConfig,
    pub discovery: DiscoverySnapshot,
}

#[tauri::command]
pub fn accept_discovered_app(
    app: AppHandle,
    mgr: State<'_, DiscoveryManager>,
    target: String,
    name: String,
    category_id: String,
) -> Result<AcceptDiscoveredResult, String> {
    let trimmed_target = target.trim().to_string();
    if trimmed_target.is_empty() {
        return Err("应用目标为空".into());
    }
    let app_state = app.state::<crate::AppState>();
    let _gate = app_state
        .configuration_gate
        .lock()
        .map_err(|_| "配置暂时不可用")?;
    let next_config = {
        let mut config = app_state.apps.lock().unwrap_or_else(|e| e.into_inner());
        let resolved_cat = if category_id == "all"
            || config.categories.iter().any(|c| c.id == category_id)
        {
            category_id
        } else {
            "all".into()
        };
        if config.find_by_target(&trimmed_target).is_some() {
            config.clone()
        } else {
            let entry = app_from_target(
                &trimmed_target,
                &resolved_cat,
                (!name.trim().is_empty()).then_some(name.trim()),
            );
            app_state
                .store
                .update(CONFIG_FILE, &mut *config, |next| next.apps.push(entry))?
        }
    };
    crate::broadcast_apps(&app, &next_config);

    let _guard = mgr.lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut dstate = load_state(&app);
    remove_pending_target(&mut dstate, &trimmed_target);
    let _ = save_state(&app, &dstate);
    let snap = dstate.snapshot();
    broadcast_discovery(&app, &snap);

    Ok(AcceptDiscoveredResult {
        config: next_config,
        discovery: snap,
    })
}

#[tauri::command]
pub fn dismiss_discovered_app(
    app: AppHandle,
    mgr: State<'_, DiscoveryManager>,
    target: String,
) -> Result<DiscoverySnapshot, String> {
    let _guard = mgr.lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state(&app);
    if dismiss_pending_target(&mut state, &target, now_ms()) {
        save_state(&app, &state)?;
    }
    let snap = state.snapshot();
    broadcast_discovery(&app, &snap);
    Ok(snap)
}

#[tauri::command]
pub fn dismiss_all_discovered_apps(
    app: AppHandle,
    mgr: State<'_, DiscoveryManager>,
) -> Result<DiscoverySnapshot, String> {
    let _guard = mgr.lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state(&app);
    if dismiss_all_pending(&mut state, now_ms()) > 0 {
        save_state(&app, &state)?;
    }
    let snap = state.snapshot();
    broadcast_discovery(&app, &snap);
    Ok(snap)
}

#[tauri::command]
pub fn restore_dismissed_app(
    app: AppHandle,
    mgr: State<'_, DiscoveryManager>,
    target: String,
) -> Result<DiscoverySnapshot, String> {
    let categories = {
        let app_state = app.state::<crate::AppState>();
        let cloned = app_state
            .apps
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .categories
            .clone();
        cloned
    };
    let _guard = mgr.lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut state = load_state(&app);
    if restore_dismissed_target(&mut state, &target, &categories, now_ms()) {
        save_state(&app, &state)?;
    }
    let snap = state.snapshot();
    broadcast_discovery(&app, &snap);
    Ok(snap)
}

// === TESTS ===
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_categories() -> Vec<Category> {
        vec![
            Category { id: "all".into(), name: "全部".into() },
            Category { id: "c-common".into(), name: "常用".into() },
            Category { id: "c-dev".into(), name: "开发".into() },
            Category { id: "c-media".into(), name: "影音".into() },
            Category { id: "c-work".into(), name: "办公".into() },
        ]
    }

    fn sample_hub_app(id: &str, name: &str, target: &str) -> HubApp {
        HubApp {
            id: id.into(),
            name: name.into(),
            target: target.into(),
            category_id: "c-dev".into(),
            color: "#3C8DFF".into(),
            initials: "A".into(),
            hidden: false,
            broken: None,
        }
    }

    #[test]
    fn first_run_builds_silent_baseline_without_flooding_pending() {
        let mut state = DiscoveryState::default();
        let cats = sample_categories();
        let system = vec![
            ("Calculator".into(), r"shell:AppsFolder\Microsoft.WindowsCalculator_8wekyb3d8bbwe!App".into()),
            ("Xbox".into(), r"shell:AppsFolder\Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.App".into()),
        ];
        let outcome = reconcile_discovery(&mut state, &system, &[], &cats, 1000);
        assert!(outcome.initialized_now);
        assert_eq!(outcome.added_count, 0);
        assert!(state.initialized);
        assert!(state.pending.is_empty(), "首启基线绝不能把预装应用推入 pending 炸屏");
        assert_eq!(state.seen_targets.len(), 2);
    }

    #[test]
    fn incremental_scan_detects_new_apps_dedups_existing_and_filters_noise() {
        let mut state = DiscoveryState::default();
        let cats = sample_categories();
        let baseline = vec![
            ("Calculator".into(), r"shell:AppsFolder\Microsoft.WindowsCalculator!App".into()),
        ];
        reconcile_discovery(&mut state, &baseline, &[], &cats, 1000);

        let existing = vec![
            sample_hub_app("a1", "Visual Studio Code", r"C:\Users\me\AppData\Local\Programs\Microsoft VS Code\Code.exe"),
        ];
        let next_system = vec![
            ("Calculator".into(), r"shell:AppsFolder\Microsoft.WindowsCalculator!App".into()),
            // 跨格式重复（已有 .exe，再出现 shell:AppsFolder）
            ("Visual Studio Code".into(), r"shell:AppsFolder\Microsoft.VisualStudioCode".into()),
            // 噪声项（卸载器 / 帮助文档）
            ("Uninstall Cursor".into(), r"shell:AppsFolder\Cursor.Uninstall".into()),
            ("Cursor Readme".into(), r"shell:AppsFolder\Cursor.Readme".into()),
            // 真正新装应用
            ("Cursor".into(), r"shell:AppsFolder\Anysphere.Cursor".into()),
            ("Spotify".into(), r"shell:AppsFolder\SpotifyAB.SpotifyMusic!Spotify".into()),
            ("飞书".into(), r"shell:AppsFolder\ByteDance.Feishu".into()),
        ];

        let outcome = reconcile_discovery(&mut state, &next_system, &existing, &cats, 2000);
        assert!(!outcome.initialized_now);
        assert_eq!(outcome.added_count, 3);
        assert_eq!(state.pending.len(), 3);
        let cursor = state.pending.iter().find(|p| p.name == "Cursor").unwrap();
        assert_eq!(cursor.suggested_category_id, "c-dev");
        let spotify = state.pending.iter().find(|p| p.name == "Spotify").unwrap();
        assert_eq!(spotify.suggested_category_id, "c-media");
        let feishu = state.pending.iter().find(|p| p.name == "飞书").unwrap();
        assert_eq!(feishu.suggested_category_id, "c-work");

        // 卸载 Cursor 后再次扫描，自动从 pending 清理掉死条目
        let after_uninstall = vec![
            ("Calculator".into(), r"shell:AppsFolder\Microsoft.WindowsCalculator!App".into()),
            ("Spotify".into(), r"shell:AppsFolder\SpotifyAB.SpotifyMusic!Spotify".into()),
            ("飞书".into(), r"shell:AppsFolder\ByteDance.Feishu".into()),
        ];
        let pruned = reconcile_discovery(&mut state, &after_uninstall, &existing, &cats, 3000);
        assert_eq!(pruned.pruned_count, 1);
        assert!(!state.pending.iter().any(|p| p.name == "Cursor"));
    }

    #[test]
    fn dismiss_and_restore_work_without_retriggering() {
        let mut state = DiscoveryState::default();
        let cats = sample_categories();
        reconcile_discovery(&mut state, &[], &[], &cats, 1000);
        let sys = vec![("Cursor".into(), r"shell:AppsFolder\Anysphere.Cursor".into())];
        reconcile_discovery(&mut state, &sys, &[], &cats, 2000);
        assert_eq!(state.pending.len(), 1);

        assert!(dismiss_pending_target(&mut state, r"shell:AppsFolder\Anysphere.Cursor", 2500));
        assert!(state.pending.is_empty());
        assert_eq!(state.dismissed.len(), 1);

        // 已忽略的应用后续扫描绝不再次弹出
        let rescan = reconcile_discovery(&mut state, &sys, &[], &cats, 3000);
        assert_eq!(rescan.added_count, 0);
        assert!(state.pending.is_empty());

        // 恢复已忽略应用回到待分类区
        assert!(restore_dismissed_target(&mut state, r"shell:AppsFolder\Anysphere.Cursor", &cats, 3500));
        assert_eq!(state.pending.len(), 1);
        assert!(state.dismissed.is_empty());
    }

    #[test]
    fn reconcile_broken_apps_marks_missing_and_clears_recovered() {
        let mut apps = vec![
            sample_hub_app("ok", "记事本", r"C:\Windows\System32\notepad.exe"),
            sample_hub_app("gone", "已卸载应用", r"C:\Missing\app.exe"),
            HubApp {
                broken: Some(true),
                ..sample_hub_app("back", "已重装应用", r"C:\Recovered\app.exe")
            },
        ];
        let changed = reconcile_broken_apps(&mut apps, |target| !target.contains("Missing"));
        assert_eq!(changed, 2);
        assert_eq!(apps[0].broken, None);
        assert_eq!(apps[1].broken, Some(true));
        assert_eq!(apps[2].broken, None);
    }
}
