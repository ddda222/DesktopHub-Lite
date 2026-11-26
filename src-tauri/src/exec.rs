//! 外部进程执行与输出解码。
//!
//! 桌面程序没有控制台，所有子进程都加 `CREATE_NO_WINDOW`，避免闪黑框。
//! HBuilderX CLI 在中文 Windows 上按系统代码页输出，所以文本先按 UTF-8 解，
//! 解不出来再回落到系统 ANSI 代码页，避免中文变成乱码。

use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read},
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// 抓取输出时保留的上限，只用于把失败原因交给界面。
pub const OUTPUT_LIMIT: usize = 256 * 1024;

pub struct Output {
    pub code: i32,
    pub text: String,
}

pub enum CaptureError {
    Cancelled,
    Timeout(Duration),
    Failed(String),
}

impl CaptureError {
    pub fn text(&self) -> String {
        match self {
            CaptureError::Cancelled => "已取消".to_string(),
            CaptureError::Timeout(limit) => {
                format!("命令超过 {} 秒没有结束，已停止等待", limit.as_secs())
            }
            CaptureError::Failed(message) => message.clone(),
        }
    }
}

/// 统一的进程构造：不接标准输入、不弹控制台窗口。
pub fn command(exe: &Path) -> Command {
    let mut command = Command::new(exe);
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command
}

/// 结束整棵进程树：子进程可能再拉起编译器之类，单杀一个会留下孤儿。
pub fn kill_tree(pid: u32) {
    if pid == 0 {
        return;
    }
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let taskkill = Path::new(&windir).join("System32").join("taskkill.exe");
    let _ = command(&taskkill)
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
}

pub struct Options<'a> {
    pub limit: Duration,
    /// 逐行回调（用于进度解析与日志推送）
    pub on_line: Option<&'a mut dyn FnMut(&str)>,
    /// 外部取消信号
    pub cancel: Option<&'a AtomicBool>,
    /// 当前子进程号，供外部立即终止
    pub pid: Option<&'a AtomicU32>,
}

impl<'a> Options<'a> {
    pub fn quiet(limit: Duration) -> Self {
        Self { limit, on_line: None, cancel: None, pid: None }
    }
}

fn pump<R: Read>(pipe: R, sender: mpsc::Sender<String>) {
    let mut reader = BufReader::new(pipe);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                // CLI 即使写管道也会带 ANSI 颜色码，先剥掉再交给上层匹配。
                let text = strip_ansi(&decode(&buffer));
                let line = text.trim_end_matches(|c| c == '\r' || c == '\n').to_string();
                if !line.trim().is_empty() && sender.send(line).is_err() {
                    break;
                }
            }
        }
    }
}

/// 去掉 ANSI 转义序列（CSI 与其它的两字节转义）。
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(current) = chars.next() {
        if current != '\u{1b}' {
            out.push(current);
            continue;
        }
        if chars.peek() == Some(&'[') {
            chars.next();
            // CSI 结束于 0x40..=0x7E 的最终字节
            while let Some(&next) = chars.peek() {
                chars.next();
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
            continue;
        }
        // 其它 ESC 序列：吞掉参数后的一字节
        chars.next();
    }
    out
}

pub fn capture(
    exe: &Path,
    args: &[String],
    cwd: Option<&Path>,
    options: Options<'_>,
) -> Result<Output, CaptureError> {
    let mut child = command(exe);
    child.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(dir) = cwd {
        child.current_dir(dir);
    }
    let mut child = child
        .spawn()
        .map_err(|error| CaptureError::Failed(format!("无法启动 {}：{error}", exe.display())))?;
    if let Some(slot) = options.pid {
        slot.store(child.id(), Ordering::SeqCst);
    }

    let (sender, receiver) = mpsc::channel::<String>();
    let mut readers = Vec::new();
    if let Some(pipe) = child.stdout.take() {
        let sender = sender.clone();
        readers.push(std::thread::spawn(move || pump(pipe, sender)));
    }
    if let Some(pipe) = child.stderr.take() {
        let sender = sender.clone();
        readers.push(std::thread::spawn(move || pump(pipe, sender)));
    }
    drop(sender);

    let mut on_line = options.on_line;
    let start = Instant::now();
    let mut lines: VecDeque<String> = VecDeque::new();
    let mut total_bytes: usize = 0;
    let mut result = Ok(());
    loop {
        match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                if let Some(callback) = on_line.as_deref_mut() {
                    callback(&line);
                }
                total_bytes = total_bytes.saturating_add(line.len() + 1);
                lines.push_back(line);
                while total_bytes > OUTPUT_LIMIT && lines.len() > 1 {
                    if let Some(dropped) = lines.pop_front() {
                        total_bytes = total_bytes.saturating_sub(dropped.len() + 1);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
        if options.cancel.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            result = Err(CaptureError::Cancelled);
            kill_tree(child.id());
            let _ = child.kill();
            break;
        }
        if start.elapsed() > options.limit {
            result = Err(CaptureError::Timeout(options.limit));
            kill_tree(child.id());
            let _ = child.kill();
            break;
        }
    }
    let status = child.wait().map_err(|error| CaptureError::Failed(format!("等待进程结束失败：{error}")))?;
    for reader in readers {
        let _ = reader.join();
    }
    if let Some(slot) = options.pid {
        slot.store(0, Ordering::SeqCst);
    }
    result?;
    let text = lines.into_iter().collect::<Vec<_>>().join("\n");
    Ok(Output { code: status.code().unwrap_or(-1), text })
}

/// 先按 UTF-8 试，失败再按系统 ANSI 代码页（中文 Windows 为 936）解。
pub fn decode(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => decode_ansi(bytes).unwrap_or_else(|| String::from_utf8_lossy(bytes).into_owned()),
    }
}

/// 把 CLI 输出里的内容转成可读纯文本：去标签、解常见实体、压掉多余空白。
/// 保留换行，供日志面板按行展示。
pub fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for current in text.chars() {
        match current {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(current),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(windows)]
fn decode_ansi(bytes: &[u8]) -> Option<String> {
    use windows::Win32::Globalization::{MultiByteToWideChar, MULTI_BYTE_TO_WIDE_CHAR_FLAGS};
    if bytes.is_empty() {
        return Some(String::new());
    }
    // 代码页 0 = CP_ACP
    let capacity = unsafe { MultiByteToWideChar(0, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None) };
    if capacity <= 0 {
        return None;
    }
    let mut buffer = vec![0u16; capacity as usize];
    let written =
        unsafe { MultiByteToWideChar(0, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, Some(&mut buffer)) };
    if written <= 0 {
        return None;
    }
    buffer.truncate(written as usize);
    Some(String::from_utf16_lossy(&buffer))
}

#[cfg(not(windows))]
fn decode_ansi(_bytes: &[u8]) -> Option<String> {
    None
}