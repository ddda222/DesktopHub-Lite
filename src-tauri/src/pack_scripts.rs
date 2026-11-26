//! 打包 / 联调台的脚本项与历史耗时。
//!
//! 脚本项存在数据目录的 `pack-scripts.json`；每个脚本只记录**成功完成**的耗时，
//! 最多保留最近 5 次，用于估算「约剩多久」。

use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::AppState;

pub const SCRIPTS_FILE: &str = "pack-scripts.json";
pub const SCHEMA_VERSION: u32 = 1;
const HISTORY_LIMIT: usize = 5;
const MAX_SCRIPTS: usize = 32;
const MAX_SECONDS: f64 = 7200.0;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Script {
    pub id: String,
    pub name: String,
    /// local（本地联调）/ remote（正式远程）
    pub env: String,
    /// command（短命令）/ pack（云打包长任务）
    pub kind: String,
    /// 命令正文，短命令必填（或改用脚本路径）
    pub command: String,
    /// .ps1 / .bat 脚本绝对路径
    pub script_path: String,
    /// 工作目录；留空时脚本取自身目录，命令取程序目录
    pub work_dir: String,
    /// 运行主任务前先执行的命令，失败则不继续
    pub pre_command: String,
    /// 预估时长（秒），没有历史时用它显示「约剩」
    pub estimate_sec: u32,
    /// 成功完成的耗时（秒），最近 5 次
    pub history: Vec<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptsFile {
    pub schema_version: u32,
    pub scripts: Vec<Script>,
}

impl Default for ScriptsFile {
    fn default() -> Self {
        Self { schema_version: SCHEMA_VERSION, scripts: seed() }
    }
}

fn script(id: &str, name: &str, env: &str, kind: &str) -> Script {
    Script {
        id: id.into(),
        name: name.into(),
        env: env.into(),
        kind: kind.into(),
        estimate_sec: 0,
        ..Default::default()
    }
}

/// 两个内置模板：本地联调云打包（按当前工作区打包，装机并转发）、远程联调云打包（忽略本地配置修改，只打包）。
fn seed() -> Vec<Script> {
    let mut local = script("builtin-local-pack", "本地联调云打包", "local", "pack");
    local.estimate_sec = 900;

    let mut remote = script("builtin-remote-pack", "远程联调云打包", "remote", "pack");
    remote.estimate_sec = 900;

    vec![local, remote]
}

fn read_file(app: &AppHandle) -> Result<ScriptsFile, String> {
    let store = &app.state::<AppState>().store;
    let exists = store.root().join(SCRIPTS_FILE).exists();
    match store.read::<ScriptsFile>(SCRIPTS_FILE) {
        Some(mut file) => {
            if file.schema_version != SCHEMA_VERSION {
                return Err("打包脚本版本不兼容，原文件已保留".into());
            }
            if file.scripts.len() > MAX_SCRIPTS {
                return Err("打包脚本数量异常，原文件已保留".into());
            }
            for item in &mut file.scripts {
                normalize_stored(item)?;
            }
            Ok(file)
        }
        None if exists => Err("打包脚本无法读取，原文件已保留".into()),
        None => {
            let seeded = ScriptsFile::default();
            store.write(SCRIPTS_FILE, &seeded)?;
            Ok(seeded)
        }
    }
}

fn write_file(app: &AppHandle, file: &ScriptsFile) -> Result<(), String> {
    app.state::<AppState>().store.write(SCRIPTS_FILE, file)
}

/// 仅做格式规范化与基础校验，不校验磁盘路径是否存在，避免历史失效脚本阻断列表加载。
fn normalize_stored(item: &mut Script) -> Result<(), String> {
    item.name = item.name.trim().to_string();
    item.command = item.command.trim().to_string();
    item.script_path = item.script_path.trim().to_string();
    item.work_dir = item.work_dir.trim().to_string();
    item.pre_command = item.pre_command.trim().to_string();
    if item.id.trim().is_empty() {
        item.id = format!("s-{}-{}", now_ms(), (item.name.len() as u64) % 97);
    }
    if item.name.is_empty() || item.name.chars().count() > 40 {
        return Err("脚本名称需要 1～40 个字符".into());
    }
    if item.env != "local" && item.env != "remote" {
        return Err("环境只能是 local（本地联调）或 remote（正式远程）".into());
    }
    if item.kind != "command" && item.kind != "pack" {
        return Err("类型只能是 command（短命令）或 pack（打包长任务）".into());
    }
    if item.kind == "pack" {
        item.command.clear();
        item.script_path.clear();
        item.work_dir.clear();
        item.pre_command.clear();
    } else {
        if item.command.is_empty() && item.script_path.is_empty() {
            return Err("短命令需要填写命令，或选择一个 .ps1/.bat 脚本".into());
        }
        if !item.script_path.is_empty() {
            let lower = item.script_path.to_ascii_lowercase();
            if !(lower.ends_with(".ps1") || lower.ends_with(".bat") || lower.ends_with(".cmd")) {
                return Err("脚本只支持 .ps1 / .bat / .cmd".into());
            }
        }
    }
    if item.estimate_sec > MAX_SECONDS as u32 {
        return Err("预估时长不能超过 7200 秒".into());
    }
    item.history.retain(|value| value.is_finite() && *value > 0.0);
    if item.history.len() > HISTORY_LIMIT {
        let skip = item.history.len() - HISTORY_LIMIT;
        item.history.drain(0..skip);
    }
    Ok(())
}

/// 校验脚本关联的本地文件与工作目录是否存在（仅在保存单条脚本与运行前调用）。
pub fn validate_paths(item: &Script) -> Result<(), String> {
    if item.kind == "command" && !item.script_path.trim().is_empty() {
        if !Path::new(item.script_path.trim()).is_file() {
            return Err(format!("脚本文件不存在：{}", item.script_path.trim()));
        }
    }
    if !item.work_dir.trim().is_empty() && !Path::new(item.work_dir.trim()).is_dir() {
        return Err(format!("工作目录不存在：{}", item.work_dir.trim()));
    }
    Ok(())
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

/// 估算时长：优先用历史平均，没有历史时用配置的预估值。
pub fn estimate(item: &Script) -> Option<f64> {
    if !item.history.is_empty() {
        let sum: f64 = item.history.iter().sum();
        return Some(sum / item.history.len() as f64);
    }
    if item.estimate_sec > 0 {
        return Some(item.estimate_sec as f64);
    }
    None
}

/// 记录一次成功耗时（失败与取消不写入）。
pub fn record_success(app: &AppHandle, id: &str, seconds: f64) -> Result<(), String> {
    if !seconds.is_finite() || seconds <= 0.0 || seconds > MAX_SECONDS {
        return Ok(());
    }
    let mut file = read_file(app)?;
    let Some(item) = file.scripts.iter_mut().find(|item| item.id == id) else {
        return Ok(());
    };
    item.history.push(seconds);
    if item.history.len() > HISTORY_LIMIT {
        let skip = item.history.len() - HISTORY_LIMIT;
        item.history.drain(0..skip);
    }
    write_file(app, &file)
}

pub fn find(app: &AppHandle, id: &str) -> Result<Script, String> {
    read_file(app)?
        .scripts
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| "脚本不存在，可能已被删除".to_string())
}

#[tauri::command]
pub fn list_pack_scripts(app: AppHandle) -> Result<Vec<Script>, String> {
    Ok(read_file(&app)?.scripts)
}

#[tauri::command]
pub fn save_pack_script(app: AppHandle, mut script: Script) -> Result<Vec<Script>, String> {
    normalize_stored(&mut script)?;
    validate_paths(&script)?;
    let mut file = read_file(&app)?;
    match file.scripts.iter().position(|item| item.id == script.id) {
        Some(index) => file.scripts[index] = script,
        None => {
            if file.scripts.len() >= MAX_SCRIPTS {
                return Err(format!("最多保存 {MAX_SCRIPTS} 个脚本"));
            }
            file.scripts.push(script);
        }
    }
    write_file(&app, &file)?;
    Ok(file.scripts)
}

#[tauri::command]
pub fn delete_pack_script(app: AppHandle, id: String) -> Result<Vec<Script>, String> {
    let mut file = read_file(&app)?;
    file.scripts.retain(|item| item.id != id);
    write_file(&app, &file)?;
    Ok(file.scripts)
}