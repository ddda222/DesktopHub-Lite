//! 通用后台任务执行器：打包 / 联调台的短命令与云打包长任务共用一套状态、日志与取消。
//!
//! 同一时间只允许一个任务；状态与日志通过事件推给灵动岛所在窗口，
//! 取消时终止整棵子进程树（`taskkill /T /F`）。

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::{app_pack, exec, pack_scripts};

pub const STATE_EVENT: &str = "task-state";
pub const LOG_EVENT: &str = "task-log";
const LOG_LIMIT: usize = 200;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// 日志事件的最快推送间隔，避免构建脚本刷屏时把 WebView 打满。
const LOG_THROTTLE: Duration = Duration::from_millis(120);

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskState {
    /// idle / running / done / error / cancelled
    pub stage: String,
    pub script_id: String,
    pub name: String,
    pub env: String,
    pub kind: String,
    pub message: String,
    pub detail: String,
    /// 启动时刻（epoch 毫秒），界面据此每秒刷新已用时间
    pub started_at: u64,
    pub elapsed_ms: u64,
    /// 估算总时长（秒）；没有历史也没有预估时为 null
    pub estimate_sec: Option<f64>,
    pub apk_path: String,
    pub cancelable: bool,
}

impl TaskState {
    fn running(&self) -> bool {
        self.stage == "running"
    }
}

pub struct TaskRunner {
    state: Mutex<TaskState>,
    log: Mutex<VecDeque<String>>,
    pending_log: Mutex<Vec<String>>,
    running: AtomicBool,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
    last_emit: Mutex<Instant>,
}

impl Default for TaskRunner {
    fn default() -> Self {
        Self {
            state: Mutex::new(TaskState { stage: "idle".into(), ..Default::default() }),
            log: Mutex::new(VecDeque::new()),
            pending_log: Mutex::new(Vec::new()),
            running: AtomicBool::new(false),
            cancel: Arc::new(AtomicBool::new(false)),
            pid: Arc::new(AtomicU32::new(0)),
            last_emit: Mutex::new(Instant::now() - LOG_THROTTLE),
        }
    }
}

impl TaskRunner {
    /// 退出前终止正在运行的任务进程树，避免留下孤儿进程。
    pub fn shutdown(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        let pid = self.pid.load(Ordering::SeqCst);
        if pid != 0 {
            exec::kill_tree(pid);
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

fn snapshot(runner: &TaskRunner) -> TaskState {
    let mut state = runner.state.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if state.running() && state.started_at > 0 {
        state.elapsed_ms = now_ms().saturating_sub(state.started_at);
    }
    state
}

fn emit_state(app: &AppHandle) {
    if let Some(runner) = app.try_state::<TaskRunner>() {
        let _ = app.emit(STATE_EVENT, snapshot(&runner));
    }
}

fn update(app: &AppHandle, change: impl FnOnce(&mut TaskState)) {
    let Some(runner) = app.try_state::<TaskRunner>() else {
        return;
    };
    {
        let mut state = runner.state.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut state);
    }
    emit_state(app);
}

/// 云打包流水线用：只改当前提示，不动阶段。
pub(crate) fn set_message(app: &AppHandle, message: &str) {
    update(app, |state| state.message = message.to_string());
}

/// 云打包流水线用：记录已产出的 APK 路径。
pub(crate) fn mark_apk(app: &AppHandle, path: &str) {
    update(app, |state| state.apk_path = path.to_string());
}

/// 追加一行输出；使用批量节流推送，避免高并发输出刷满 WebView 且不丢中间行。
pub(crate) fn log_line(app: &AppHandle, line: &str) {
    let Some(runner) = app.try_state::<TaskRunner>() else {
        return;
    };
    // CLI 可能打印 HTML 片段，日志面板里只显示纯文本。
    let clean = exec::plain(line);
    let clean = clean.as_str();
    if clean.is_empty() {
        return;
    }
    {
        let mut log = runner.log.lock().unwrap_or_else(|e| e.into_inner());
        log.push_back(clean.to_string());
        while log.len() > LOG_LIMIT {
            log.pop_front();
        }
    }
    {
        let mut pending = runner.pending_log.lock().unwrap_or_else(|e| e.into_inner());
        pending.push(clean.to_string());
    }
    let mut last = runner.last_emit.lock().unwrap_or_else(|e| e.into_inner());
    if last.elapsed() >= LOG_THROTTLE {
        let batch = {
            let mut pending = runner.pending_log.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut *pending)
        };
        if !batch.is_empty() {
            *last = Instant::now();
            let _ = app.emit(LOG_EVENT, batch);
        }
    }
}

fn flush_log(app: &AppHandle) {
    let Some(runner) = app.try_state::<TaskRunner>() else {
        return;
    };
    let batch = {
        let mut pending = runner.pending_log.lock().unwrap_or_else(|e| e.into_inner());
        std::mem::take(&mut *pending)
    };
    if !batch.is_empty() {
        let _ = app.emit(LOG_EVENT, batch);
    }
}

fn clear_log(app: &AppHandle) {
    if let Some(runner) = app.try_state::<TaskRunner>() {
        runner.log.lock().unwrap_or_else(|e| e.into_inner()).clear();
        runner.pending_log.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

fn shell() -> PathBuf {
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    Path::new(&windir)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
}

fn cmd_exe() -> PathBuf {
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    Path::new(&windir).join("System32").join("cmd.exe")
}

fn command_invocation(text: &str) -> (PathBuf, Vec<String>) {
    (
        shell(),
        vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-ExecutionPolicy".into(),
            "Bypass".into(),
            "-Command".into(),
            text.to_string(),
        ],
    )
}

fn script_invocation(script_path: &Path) -> (PathBuf, Vec<String>) {
    let lower = script_path.to_string_lossy().to_ascii_lowercase();
    if lower.ends_with(".ps1") {
        (
            shell(),
            vec![
                "-NoProfile".into(),
                "-ExecutionPolicy".into(),
                "Bypass".into(),
                "-File".into(),
                script_path.to_string_lossy().into_owned(),
            ],
        )
    } else {
        (
            cmd_exe(),
            vec![
                "/d".into(),
                "/c".into(),
                "call".into(),
                script_path.to_string_lossy().into_owned(),
            ],
        )
    }
}

fn working_dir(script: &pack_scripts::Script) -> Option<PathBuf> {
    if !script.work_dir.trim().is_empty() {
        return Some(PathBuf::from(script.work_dir.trim()));
    }
    if !script.script_path.trim().is_empty() {
        return Path::new(script.script_path.trim()).parent().map(Path::to_path_buf);
    }
    None
}

fn minutes_seconds(total: Duration) -> String {
    let seconds = total.as_secs();
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn finish(app: &AppHandle, script: &pack_scripts::Script, elapsed: Duration, outcome: Result<Option<String>, exec::CaptureError>) {
    flush_log(app);
    if let Some(runner) = app.try_state::<TaskRunner>() {
        runner.running.store(false, Ordering::SeqCst);
    }
    match outcome {
        Ok(note) => {
            // 只有成功完成才计入历史耗时，失败与取消都不计。
            if let Err(error) = pack_scripts::record_success(app, &script.id, elapsed.as_secs_f64()) {
                crate::log_warn!("记录脚本耗时失败：{error}");
            }
            let suffix = note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| format!(" · {value}"))
                .unwrap_or_default();
            crate::log_info!("[任务] {} 完成，用时 {}{suffix}", script.name, minutes_seconds(elapsed));
            update(app, |state| {
                state.stage = "done".into();
                state.message = format!("完成 · 用时 {}{suffix}", minutes_seconds(elapsed));
                state.detail.clear();
                state.elapsed_ms = elapsed.as_millis() as u64;
                state.cancelable = false;
            });
        }
        Err(exec::CaptureError::Cancelled) => {
            update(app, |state| {
                state.stage = "cancelled".into();
                state.message = format!("已取消 · 用时 {}", minutes_seconds(elapsed));
                state.detail.clear();
                state.elapsed_ms = elapsed.as_millis() as u64;
                state.cancelable = false;
            });
        }
        Err(error) => {
            let detail = error.text();
            crate::log_info!("[任务] {} 失败：{detail}", script.name);
            update(app, |state| {
                state.stage = "error".into();
                state.message = "失败".into();
                state.detail = detail;
                state.elapsed_ms = elapsed.as_millis() as u64;
                state.cancelable = false;
            });
        }
    }
}

async fn capture_blocking(
    app: &AppHandle,
    exe: PathBuf,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    limit: Duration,
    cancel: Arc<AtomicBool>,
    pid: Arc<AtomicU32>,
) -> Result<exec::Output, exec::CaptureError> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        exec::capture(
            &exe,
            &args,
            cwd.as_deref(),
            exec::Options {
                limit,
                on_line: Some(&mut |line| log_line(&handle, line)),
                cancel: Some(cancel.as_ref()),
                pid: Some(pid.as_ref()),
            },
        )
    })
    .await
    .map_err(|error| exec::CaptureError::Failed(format!("后台任务中断：{error}")))?
}

async fn run_command(app: &AppHandle, script: &pack_scripts::Script) -> Result<(), exec::CaptureError> {
    pack_scripts::validate_paths(script).map_err(exec::CaptureError::Failed)?;
    let runner = app.state::<TaskRunner>();
    let cancel = runner.cancel.clone();
    let pid = runner.pid.clone();
    let cwd = working_dir(script);

    // 前置命令：先跑 adb reverse 之类；失败就不继续。
    if !script.pre_command.trim().is_empty() {
        set_message(app, "执行前置命令…");
        let (exe, args) = command_invocation(script.pre_command.trim());
        let out = capture_blocking(
            app,
            exe,
            args,
            cwd.clone(),
            Duration::from_secs(120),
            cancel.clone(),
            pid.clone(),
        )
        .await?;
        if out.code != 0 {
            return Err(exec::CaptureError::Failed(format!(
                "前置命令失败（退出码 {}）：{}",
                out.code,
                out.text.trim()
            )));
        }
    }

    let (exe, args) = if !script.script_path.trim().is_empty() {
        script_invocation(Path::new(script.script_path.trim()))
    } else {
        command_invocation(script.command.trim())
    };
    set_message(app, if script.kind == "command" { "执行中…" } else { "运行中…" });

    let out = capture_blocking(app, exe, args, cwd, COMMAND_TIMEOUT, cancel, pid).await?;
    if out.code != 0 {
        return Err(exec::CaptureError::Failed(format!(
            "命令退出码 {}：{}",
            out.code,
            out.text.trim()
        )));
    }
    Ok(())
}

#[tauri::command]
pub fn task_state(runner: State<'_, TaskRunner>) -> TaskState {
    snapshot(&runner)
}

#[tauri::command]
pub fn task_log(runner: State<'_, TaskRunner>) -> Vec<String> {
    runner.log.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect()
}

#[tauri::command]
pub fn start_pack_task(app: AppHandle, id: String) -> Result<(), String> {
    let runner = app.state::<TaskRunner>();
    if runner.running.swap(true, Ordering::SeqCst) {
        return Err("已有任务在运行，请先等它结束或点取消".into());
    }
    let started = start_inner(&app, &id);
    if started.is_err() {
        runner.running.store(false, Ordering::SeqCst);
    }
    started
}

fn start_inner(app: &AppHandle, id: &str) -> Result<(), String> {
    let script = pack_scripts::find(app, id)?;
    let estimate = pack_scripts::estimate(&script);
    clear_log(app);
    {
        let runner = app.state::<TaskRunner>();
        runner.cancel.store(false, Ordering::SeqCst);
        runner.pid.store(0, Ordering::SeqCst);
        let mut state = runner.state.lock().unwrap_or_else(|e| e.into_inner());
        *state = TaskState {
            stage: "running".into(),
            script_id: script.id.clone(),
            name: script.name.clone(),
            env: script.env.clone(),
            kind: script.kind.clone(),
            message: "准备中…".into(),
            detail: String::new(),
            started_at: now_ms(),
            elapsed_ms: 0,
            estimate_sec: estimate,
            apk_path: String::new(),
            cancelable: true,
        };
    }
    emit_state(app);

    let handle = app.clone();
    let script = script.clone();
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let outcome = if script.kind == "pack" {
            let runner = handle.state::<TaskRunner>();
            let cancel = runner.cancel.clone();
            let pid = runner.pid.clone();
            match app_pack::run_pipeline(&handle, &script.env, cancel, pid).await {
                Ok(pack_outcome) => Ok(pack_outcome.note),
                Err(error) => Err(error),
            }
        } else {
            run_command(&handle, &script).await.map(|()| None)
        };
        finish(&handle, &script, started.elapsed(), outcome);
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_pack_task(app: AppHandle) -> Result<(), String> {
    let runner = app.state::<TaskRunner>();
    if !runner.running.load(Ordering::SeqCst) {
        return Err("当前没有正在运行的任务".into());
    }
    runner.cancel.store(true, Ordering::SeqCst);
    let pid = runner.pid.load(Ordering::SeqCst);
    if pid != 0 {
        exec::kill_tree(pid);
    }
    set_message(&app, "正在取消…");
    Ok(())
}