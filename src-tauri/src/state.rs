//! 跨窗口共享的界面状态。
//!
//! 每个 Tauri 窗口是独立 WebView，各自有独立的 JS 上下文，所以「当前打开哪个弹层」
//! 「当前分类」「当前主题」这类状态不能放在 Vue store 里，必须由 Rust 持有并广播给所有窗口。

use serde::{Deserialize, Serialize};
use std::sync::Mutex;

pub const PANEL_CLIPBOARD: &str = "clipboard";
pub const PANEL_TRANSLATION: &str = "translation";
pub const PANEL_CATEGORY: &str = "category";
pub const PANEL_OVERFLOW: &str = "overflow";

pub const THEME_DARK: &str = "dark";
pub const THEME_LIGHT: &str = "light";
pub const THEME_SYSTEM: &str = "system";

pub const CATEGORY_ALL: &str = "all";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub island_expanded: bool,
    pub island_anchor: String,
    /// 当前打开的弹层：clipboard / translation / category / overflow / null
    pub panel: Option<String>,
    pub settings_open: bool,
    /// 暂停记录剪贴板（阶段 1 只保存开关，不监听系统剪贴板）
    pub paused: bool,
    /// 图标条当前分类，弹层窗口也要按同一分类展示
    pub active_category_id: String,
    /// dark / light / system
    pub theme: String,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            island_expanded: false,
            island_anchor: "center".into(),
            panel: None,
            settings_open: false,
            paused: false,
            active_category_id: CATEGORY_ALL.to_string(),
            theme: THEME_DARK.to_string(),
        }
    }
}

pub struct UiStateHandle(pub Mutex<UiState>);

impl UiStateHandle {
    pub fn new(initial: UiState) -> Self {
        Self(Mutex::new(initial))
    }

    pub fn snapshot(&self) -> UiState {
        self.0.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// 修改状态并返回快照。锁中毒时不 panic，返回当前默认值。
    pub fn update<F: FnOnce(&mut UiState)>(&self, f: F) -> UiState {
        match self.0.lock() {
            Ok(mut guard) => {
                f(&mut guard);
                guard.clone()
            }
            Err(_) => UiState::default(),
        }
    }
}

/// 需要跨重启保留的界面偏好（分类与主题）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiPreferences {
    #[serde(default = "default_anchor")]
    pub island_anchor: String,
    pub active_category_id: String,
    pub theme: String,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            island_anchor: default_anchor(),
            active_category_id: CATEGORY_ALL.to_string(),
            theme: THEME_DARK.to_string(),
        }
    }
}

impl UiPreferences {
    pub fn from_state(state: &UiState) -> Self {
        Self {
            island_anchor: state.island_anchor.clone(),
            active_category_id: state.active_category_id.clone(),
            theme: state.theme.clone(),
        }
    }
}

fn default_anchor() -> String { "center".into() }

