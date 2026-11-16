//! 剪贴板索引与保留规则。

use serde::{Deserialize, Serialize};

pub const INDEX_FILE: &str = "clipboard-index.json";
pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_HISTORY_LIMIT: usize = 500;
pub const MIN_HISTORY_LIMIT: usize = 10;
pub const MAX_HISTORY_LIMIT: usize = 5000;

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
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub note: String,
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
    // 缺少此字段的旧索引保持不限量；用户确认后才应用上限。
    #[serde(default)]
    pub history_limit: Option<usize>,
    #[serde(default = "enabled")]
    pub capture_images: bool,
}

impl Default for ClipboardIndex {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            entries: Vec::new(),
            paused: false,
            groups: default_groups(),
            history_limit: Some(DEFAULT_HISTORY_LIMIT),
            capture_images: true,
        }
    }
}

fn default_groups() -> Vec<String> {
    vec!["未分组".into()]
}

fn enabled() -> bool {
    true
}

impl ClipboardIndex {
    pub fn record(&mut self, mut entry: ClipboardEntry) {
        if let Some(position) = self.entries.iter().position(|old| old.id == entry.id) {
            let old = self.entries.remove(position);
            entry.group = old.group;
            entry.favorite = old.favorite;
            entry.note = old.note;
        }
        self.entries.insert(0, entry);
        self.trim_history();
    }

    pub fn trim_history(&mut self) {
        let Some(limit) = self.history_limit else {
            return;
        };
        // 不依赖旧索引的排列顺序，始终按最近复制时间保留。
        self.entries
            .sort_by_key(|entry| std::cmp::Reverse(entry.copied_at));
        let mut ordinary = 0;
        self.entries.retain(|entry| {
            if entry.favorite {
                return true;
            }
            ordinary += 1;
            ordinary <= limit
        });
    }
}

/// 只识别由本程序按内容摘要生成的图片名。
pub fn is_managed_png(name: &str) -> bool {
    let Some(hash) = name.strip_suffix(".png") else {
        return false;
    };
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn cleanup_unreferenced_images(
    dir: &std::path::Path,
    previous: &ClipboardIndex,
    current: &ClipboardIndex,
) {
    let Ok(files) = std::fs::read_dir(dir) else {
        return;
    };
    let referenced: std::collections::HashSet<&str> = previous
        .entries
        .iter()
        .chain(&current.entries)
        .filter_map(|entry| {
            entry
                .image_file
                .as_deref()?
                .strip_prefix("clipboard-images/")
        })
        .collect();
    for file in files.flatten() {
        let name = file.file_name();
        let Some(name) = name.to_str() else { continue };
        if is_managed_png(name)
            && !referenced.contains(name)
            && file.file_type().is_ok_and(|kind| kind.is_file())
        {
            let _ = std::fs::remove_file(file.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_records_are_not_trimmed_and_repeat_preserves_metadata() {
        let mut index: ClipboardIndex = serde_json::from_str(
            r#"{
            "schemaVersion":1,"entries":[{"id":"a","kind":"text","text":"同一段",
            "group":"资料","copiedAt":1}]}"#,
        )
        .unwrap();
        assert_eq!(index.history_limit, None);
        assert!(index.capture_images);
        assert!(!index.entries[0].favorite);
        assert!(index.entries[0].note.is_empty());

        let mut repeat = index.entries[0].clone();
        repeat.copied_at = 3;
        repeat.group = "未分组".into();
        index.entries[0].favorite = true;
        index.entries[0].note = "备注".into();
        index.record(repeat);
        assert_eq!(index.entries.len(), 1);
        let entry = &index.entries[0];
        assert_eq!(
            (&*entry.group, &*entry.note, entry.favorite),
            ("资料", "备注", true)
        );
    }

    #[test]
    fn only_oldest_ordinary_records_are_trimmed() {
        let mut index: ClipboardIndex = serde_json::from_str(
            r#"{
            "schemaVersion":1,"historyLimit":1,"captureImages":false,"entries":[
            {"id":"old","kind":"text","group":"未分组","copiedAt":1},
            {"id":"keep","kind":"image","group":"未分组","copiedAt":0,"favorite":true},
            {"id":"new","kind":"text","group":"未分组","copiedAt":2}
            ]}"#,
        )
        .unwrap();
        index.trim_history();
        assert!(!index.capture_images);
        assert_eq!(
            index
                .entries
                .iter()
                .map(|entry| &*entry.id)
                .collect::<Vec<_>>(),
            ["new", "keep"]
        );
    }
    #[test]
    fn collecting_orphan_images_never_removes_current_or_backup_references() {
        let dir =
            std::env::temp_dir().join(format!("desktop-hub-clipboard-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let old_png = format!("{}.png", "a".repeat(64));
        let new_png = format!("{}.png", "b".repeat(64));
        let orphan = format!("{}.png", "c".repeat(64));
        for name in [&old_png, &new_png, &orphan, &String::from("manual.png")] {
            std::fs::write(dir.join(name), b"synthetic").unwrap();
        }
        let image = |name: &str| {
            serde_json::from_value::<ClipboardEntry>(serde_json::json!({
            "id": name, "kind": "image", "imageFile": format!("clipboard-images/{name}"), "group": "未分组", "copiedAt": 1
        })).unwrap()
        };
        let mut previous = ClipboardIndex::default();
        let mut current = ClipboardIndex::default();
        previous.entries.push(image(&old_png));
        current.entries.push(image(&new_png));
        cleanup_unreferenced_images(&dir, &previous, &current);
        assert!(dir.join(&old_png).exists());
        assert!(dir.join(&new_png).exists());
        assert!(dir.join("manual.png").exists());
        assert!(!dir.join(orphan).exists());
    }
}
