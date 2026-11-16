//! 剪贴板历史（阶段 3 落地）。
//!
//! 阶段 1 只保留数据形状与命令签名，`list_clipboard_entries` 明确返回空列表，
//! 不读取真实剪贴板、不写任何真实内容，避免「看起来接通了但其实没接」。
//! 阶段 3 才加入原生消息窗口监听、PNG 落盘、去重与暂停。

use serde::{Deserialize, Serialize};

pub const INDEX_FILE: &str = "clipboard-index.json";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardEntry {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_file: Option<String>,
    pub group: String,
    pub copied_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(default)]
    pub hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardIndex {
    pub schema_version: u32,
    pub entries: Vec<ClipboardEntry>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default = "default_groups")]
    pub groups: Vec<String>,
}

impl Default for ClipboardIndex {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            entries: Vec::new(),
            paused: false,
            groups: default_groups(),
        }
    }
}

fn default_groups() -> Vec<String> { vec!["未分组".into()] }
