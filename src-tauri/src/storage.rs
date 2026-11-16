//! 精简版的本地数据目录。
//!
//! 约束（实施方案 6.2、10）：
//! - 新数据目录与旧 WPF 版完全分离，绝不写入旧目录。
//! - 写入用「临时文件 + 改名」原子替换，避免半截文件。
//! - 读失败不静默用默认值覆盖：调用方拿到 None 后自行决定是种子数据还是进入故障状态。
//!
//! 开发验证时通过 HUB_LITE_DATA_DIR 指向 D:\codex\DesktopHub-work 下的隔离目录。

use serde::{de::DeserializeOwned, Serialize};
use std::path::{Path, PathBuf};

pub struct Store {
    root: PathBuf,
    gate: std::sync::Mutex<()>,
    blocked: std::sync::Mutex<std::collections::HashSet<String>>,
}

impl Store {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self { root, gate: std::sync::Mutex::new(()), blocked: std::sync::Mutex::new(std::collections::HashSet::new()) })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub fn read<T: DeserializeOwned>(&self, name: &str) -> Option<T> {
        let path = self.path(name);
        if !path.exists() { return None; }
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => { self.blocked.lock().unwrap_or_else(|e| e.into_inner()).insert(name.into()); return None; }
        };
        match serde_json::from_str(&text) {
            Ok(value) => Some(value),
            Err(error) => {
                self.blocked.lock().unwrap_or_else(|e| e.into_inner()).insert(name.into());
                // 只记文件名与错误类型，不打印文件内容。
                crate::log_warn!("无法解析 {name}: {error}");
                None
            }
        }
    }

    pub fn write<T: Serialize>(&self, name: &str, value: &T) -> Result<(), String> {
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if self.blocked.lock().unwrap_or_else(|e| e.into_inner()).contains(name) { return Err(format!("{name} 无法读取，已保留原文件并禁止覆盖")); }
        let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        let tmp = self.path(&format!("{name}.tmp"));
        std::fs::write(&tmp, text).map_err(|e| format!("写入 {name}.tmp 失败: {e}"))?;
        let target = self.path(name);
        if target.exists() { std::fs::copy(&target, self.path(&format!("{name}.bak"))).map_err(|_| format!("无法备份 {name}"))?; }
        std::fs::rename(&tmp, target).map_err(|e| format!("替换 {name} 失败: {e}"))
    }
}
