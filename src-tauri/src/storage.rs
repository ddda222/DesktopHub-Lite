//! 精简版的本地数据目录。
//!
//! 约束（实施方案 6.2、10）：
//! - 新数据目录与旧 WPF 版完全分离，绝不写入旧目录。
//! - 写入用「临时文件 + 改名」原子替换，避免半截文件。
//! - 读失败不静默用默认值覆盖：调用方拿到 None 后自行决定是种子数据还是进入故障状态。
//!
//! 开发验证时通过 HUB_LITE_DATA_DIR 指向 D:\codex\DesktopHub-work 下的隔离目录。

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const CONFIG_FILES: [&str; 2] = ["applications.json", "ui-preferences.json"];
const JOURNAL: &str = "config-import.pending.json";
static NEXT_FILE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigurationJournal { version: u32, originals: [Option<String>; 2] }

/// A unique sibling file prevents overlapping exports from sharing a temporary file.
pub fn write_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path.file_name().ok_or("文件名无效")?.to_string_lossy();
    let sequence = NEXT_FILE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = path.with_file_name(format!(".{name}.{}-{sequence}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&temporary).map_err(|_| "无法创建临时配置文件")?;
        file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|_| "无法完整写入配置文件")?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|_| "无法替换配置文件，请检查权限和文件占用")
    })();
    if result.is_err() { let _ = std::fs::remove_file(&temporary); }
    result.map_err(str::to_owned)
}

fn restore_originals(root: &Path, journal: &ConfigurationJournal) -> Result<(), String> {
    let mut failed = false;
    for (name, original) in CONFIG_FILES.iter().zip(&journal.originals) {
        let path = root.join(name);
        let result = match original {
            Some(text) => write_file_atomic(&path, text.as_bytes()),
            None => match std::fs::remove_file(&path) { Ok(()) => Ok(()), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), Err(_) => Err("无法还原原有配置文件状态".into()) },
        };
        failed |= result.is_err();
    }
    if failed { Err("配置恢复未完成，已保留恢复记录；请关闭占用后重新启动".into()) } else { Ok(()) }
}

fn recover_configuration(root: &Path) -> std::io::Result<()> {
    let path = root.join(JOURNAL);
    let file = match std::fs::File::open(&path) { Ok(file) => file, Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()), Err(e) => return Err(e) };
    let mut bytes = Vec::new(); file.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    let invalid = || std::io::Error::other("配置恢复记录损坏；已保留文件，停止启动以避免覆盖原数据");
    if bytes.len() > 32 * 1024 * 1024 { return Err(invalid()); }
    let journal: ConfigurationJournal = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if journal.version != 1 { return Err(invalid()); }
    restore_originals(root, &journal).map_err(std::io::Error::other)?;
    std::fs::remove_file(path)
}

pub struct Store {
    root: PathBuf,
    gate: std::sync::Mutex<()>,
    blocked: std::sync::Mutex<std::collections::HashSet<String>>,
    /// 迁移或切换数据目录后冻结写入，避免旧内存状态在重启前覆写新数据。
    frozen: std::sync::atomic::AtomicBool,
}

impl Store {
    pub fn new(root: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        recover_configuration(&root)?;
        Ok(Self {
            root,
            gate: std::sync::Mutex::new(()),
            blocked: std::sync::Mutex::new(std::collections::HashSet::new()),
            frozen: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub fn freeze_until_restart(&self) {
        self.frozen.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write_configuration<A: Serialize, P: Serialize, B: Serialize>(&self, apps: &A, prefs: &P, backup: &B) -> Result<PathBuf, String> {
        self.write_configuration_inner(apps, prefs, backup, || Ok(()))
    }

    fn write_configuration_inner<A: Serialize, P: Serialize, B: Serialize>(&self, apps: &A, prefs: &P, backup: &B, after_first: impl FnOnce() -> Result<(), String>) -> Result<PathBuf, String> {
        let _guard = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        if self.is_frozen() {
            return Err("已完成数据迁移或切换，请重启程序后再保存修改".into());
        }
        if CONFIG_FILES.iter().any(|name| self.blocked.lock().unwrap_or_else(|e| e.into_inner()).contains(*name)) || self.path(JOURNAL).exists() {
            return Err("现有配置或恢复记录无法读取，已禁止覆盖".into());
        }
        let next = [serde_json::to_vec_pretty(apps), serde_json::to_vec_pretty(prefs)];
        let next = [next[0].as_ref().map_err(|_| "应用配置无法序列化")?, next[1].as_ref().map_err(|_| "界面配置无法序列化")?];
        let mut journal = ConfigurationJournal { version: 1, originals: [None, None] };
        for (i, name) in CONFIG_FILES.iter().enumerate() {
            journal.originals[i] = match std::fs::read_to_string(self.path(name)) {
                Ok(text) if text.len() <= 8 * 1024 * 1024 => Some(text),
                Ok(_) => return Err("原有配置过大，已停止恢复".into()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => return Err("无法读取原有配置，已停止恢复".into()),
            };
        }
        let directory = self.path("config-backups");
        std::fs::create_dir_all(&directory).map_err(|_| "无法创建备份目录，未更改配置")?;
        let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|_| "系统时间无效")?.as_millis();
        let sequence = NEXT_FILE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let backup_path = directory.join(format!("before-import-{timestamp}-{}-{sequence}.json", std::process::id()));
        let bytes = serde_json::to_vec_pretty(backup).map_err(|_| "无法生成配置备份")?;
        write_file_atomic(&backup_path, &bytes)?;
        if std::fs::read(&backup_path).map_err(|_| "无法验证配置备份，未更改配置")? != bytes { return Err("配置备份验证失败，未更改配置".into()); }
        let journal_bytes = serde_json::to_vec(&journal).map_err(|_| "无法生成配置恢复记录")?;
        write_file_atomic(&self.path(JOURNAL), &journal_bytes)?;
        let result = (|| {
            write_file_atomic(&self.path(CONFIG_FILES[0]), next[0])?;
            after_first()?;
            write_file_atomic(&self.path(CONFIG_FILES[1]), next[1])?;
            std::fs::remove_file(self.path(JOURNAL)).map_err(|_| "无法完成配置恢复记录".to_string())
        })();
        if let Err(error) = result {
            if restore_originals(&self.root, &journal).is_ok() && std::fs::remove_file(self.path(JOURNAL)).is_ok() {
                return Err(format!("恢复失败，原配置已还原：{error}"));
            }
            self.blocked.lock().unwrap_or_else(|e| e.into_inner()).extend(CONFIG_FILES.map(str::to_owned));
            return Err("恢复失败且暂时无法还原，已禁止继续写入并保留恢复记录；请关闭占用后重启".into());
        }
        Ok(backup_path)
    }

    pub fn update<T: Clone + Serialize>(&self, name: &str, current: &mut T, change: impl FnOnce(&mut T)) -> Result<T, String> {
        let mut next = current.clone();
        change(&mut next);
        self.write(name, &next)?;
        *current = next.clone();
        Ok(next)
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
        if self.is_frozen() {
            return Err("已完成数据迁移或切换，请重启程序后再保存修改".into());
        }
        if self.blocked.lock().unwrap_or_else(|e| e.into_inner()).contains(name) { return Err(format!("{name} 无法读取，已保留原文件并禁止覆盖")); }
        let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
        let tmp = self.path(&format!("{name}.tmp"));
        std::fs::write(&tmp, text).map_err(|e| format!("写入 {name}.tmp 失败: {e}"))?;
        let target = self.path(name);
        if target.exists() { std::fs::copy(&target, self.path(&format!("{name}.bak"))).map_err(|_| format!("无法备份 {name}"))?; }
        std::fs::rename(&tmp, target).map_err(|e| format!("替换 {name} 失败: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    fn root() -> PathBuf {
        let base = std::env::var("HUB_LITE_TEST_ROOT").expect("隔离测试必须指定 HUB_LITE_TEST_ROOT");
        PathBuf::from(base).join(format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)))
    }
    #[test]
    fn successful_update_changes_disk_and_memory_together() {
        let store = Store::new(root()).unwrap(); let mut value = vec![1];
        store.update("test.json", &mut value, |v| v.push(2)).unwrap();
        assert_eq!(value, vec![1, 2]); assert_eq!(store.read::<Vec<u32>>("test.json").unwrap(), value);
    }
    #[test]
    fn failed_update_preserves_memory_and_original_file() {
        let store = Store::new(root()).unwrap(); let mut value = vec![1];
        store.write("test.json", &value).unwrap();
        std::fs::create_dir(store.path("test.json.bak")).unwrap();
        assert!(store.update("test.json", &mut value, |v| v.push(2)).is_err());
        assert_eq!(value, vec![1]); assert_eq!(store.read::<Vec<u32>>("test.json").unwrap(), vec![1]);
    }
    #[test]
    fn corrupt_file_is_never_overwritten() {
        let store = Store::new(root()).unwrap(); std::fs::write(store.path("test.json"), b"{broken").unwrap();
        assert!(store.read::<Vec<u32>>("test.json").is_none());
        assert!(store.write("test.json", &vec![1]).is_err());
        assert_eq!(std::fs::read(store.path("test.json")).unwrap(), b"{broken");
    }
    #[test]
    fn configuration_commit_creates_readable_backup_and_updates_both_files() {
        let store=Store::new(root()).unwrap();store.write("applications.json", &vec![1]).unwrap();store.write("ui-preferences.json", &vec![2]).unwrap();
        let backup=serde_json::json!({"format":"synthetic-backup","old":[1,2]});
        let path=store.write_configuration(&vec![3], &vec![4], &backup).unwrap();
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).unwrap()).unwrap(),backup);
        assert_eq!(store.read::<Vec<u32>>("applications.json").unwrap(),vec![3]);assert_eq!(store.read::<Vec<u32>>("ui-preferences.json").unwrap(),vec![4]);
        assert!(!store.path("config-import.pending.json").exists());
    }
    #[test]
    fn configuration_backup_failure_changes_neither_file() {
        let store=Store::new(root()).unwrap();store.write("applications.json", &vec![1]).unwrap();store.write("ui-preferences.json", &vec![2]).unwrap();
        std::fs::write(store.path("config-backups"),b"blocked").unwrap();
        assert!(store.write_configuration(&vec![3],&vec![4],&vec![1,2]).is_err());
        assert_eq!(store.read::<Vec<u32>>("applications.json").unwrap(),vec![1]);assert_eq!(store.read::<Vec<u32>>("ui-preferences.json").unwrap(),vec![2]);
    }
    #[test]
    fn failed_second_write_rolls_back_exact_bytes_and_keeps_unrelated_data() {
        let store=Store::new(root()).unwrap();
        std::fs::write(store.path("applications.json"), b"[1] \n").unwrap();
        std::fs::write(store.path("ui-preferences.json"), b"[2]  ").unwrap();
        std::fs::write(store.path("untouched.json"), b"synthetic-unrelated").unwrap();
        assert!(store.write_configuration_inner(&vec![3],&vec![4],&vec![1,2],||Err("injected failure".into())).is_err());
        assert_eq!(std::fs::read(store.path("applications.json")).unwrap(),b"[1] \n");
        assert_eq!(std::fs::read(store.path("ui-preferences.json")).unwrap(),b"[2]  ");
        assert_eq!(std::fs::read(store.path("untouched.json")).unwrap(),b"synthetic-unrelated");
        assert!(!store.path(JOURNAL).exists());
    }
    #[test]
    fn startup_recovers_interrupted_transaction_and_original_absence() {
        let store=Store::new(root()).unwrap();
        let journal=ConfigurationJournal{version:1,originals:[Some("[1]".into()),None]};
        std::fs::write(store.path(JOURNAL),serde_json::to_vec(&journal).unwrap()).unwrap();
        std::fs::write(store.path("applications.json"),b"[3]").unwrap();std::fs::write(store.path("ui-preferences.json"),b"[4]").unwrap();
        let reopened=Store::new(store.root.clone()).unwrap();
        assert_eq!(reopened.read::<Vec<u32>>("applications.json").unwrap(),vec![1]);
        assert!(!reopened.path("ui-preferences.json").exists());assert!(!reopened.path(JOURNAL).exists());
    }
    #[test]
    fn invalid_journal_stops_startup_without_overwriting() {
        let store=Store::new(root()).unwrap();std::fs::write(store.path(JOURNAL),b"{broken").unwrap();
        std::fs::write(store.path("applications.json"),b"[1]").unwrap();
        assert!(Store::new(store.root.clone()).is_err());assert_eq!(std::fs::read(store.path("applications.json")).unwrap(),b"[1]");
    }
    #[test]
    fn frozen_store_rejects_writes_and_keeps_migrated_bytes() {
        let store = Store::new(root()).unwrap();
        store.write("applications.json", &vec![1]).unwrap();
        store.write("ui-preferences.json", &vec![2]).unwrap();
        std::fs::write(store.path("applications.json"), b"[99]").unwrap();
        store.freeze_until_restart();
        assert!(store.is_frozen());
        assert!(store.write("applications.json", &vec![1]).is_err());
        assert!(store.write_configuration(&vec![3], &vec![4], &vec![1, 2]).is_err());
        assert_eq!(std::fs::read(store.path("applications.json")).unwrap(), b"[99]");
    }
}
