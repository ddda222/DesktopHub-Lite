//! 极简文件日志。
//!
//! 桌面程序不能带控制台窗口（那会多出一块挡住桌面的黑框），
//! 所以 `windows_subsystem = "windows"` 之后 stderr 无处可去，
//! 需要把关键诊断写进数据目录里的日志文件。
//!
//! 只记录操作类型、状态和必要错误；剪贴板正文、图片、翻译原文/译文、密钥一律不写。

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static LOG_PATH: OnceLock<Mutex<PathBuf>> = OnceLock::new();

/// 设置日志文件位置。数据目录就绪后调用一次。
pub fn init(path: PathBuf) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = LOG_PATH.set(Mutex::new(path));
}

pub fn write(level: &str, message: &str) {
    let Some(path) = LOG_PATH.get() else {
        return;
    };
    let Ok(guard) = path.lock() else {
        return;
    };
    // 简单截断，避免日志无限增长
    if let Ok(meta) = std::fs::metadata(&*guard) {
        if meta.len() > 512 * 1024 {
            let _ = std::fs::remove_file(&*guard);
        }
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&*guard)
    {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(file, "[{stamp}] {level} {message}");
    }
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        $crate::logging::write("WARN", &format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        $crate::logging::write("INFO", &format!($($arg)*))
    };
}
