//! ADB 联调：设备枚举、端口反向映射（adb reverse）。
//!
//! 多台设备同时连接时，所有命令都带 `-s <serial>`，避免 adb 报「more than one device」。

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Serialize;
use tauri::AppHandle;

use crate::{app_pack, exec};

/// 手机访问电脑 dev server 的默认端口。
pub const DEFAULT_PORT: u16 = 11034;
const ADB_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub serial: String,
    pub model: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdbStatus {
    /// 实际使用的 adb 路径（空配置时就是 PATH 里的 adb）
    pub adb: String,
    pub devices: Vec<DeviceInfo>,
    /// 当前用于 reverse 的设备；多台且未选择时为空
    pub serial: String,
    /// `adb reverse --list` 的原始行
    pub reverse: Vec<String>,
    pub error: String,
}

pub fn resolve(configured: &str) -> PathBuf {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        PathBuf::from("adb")
    } else {
        PathBuf::from(trimmed)
    }
}

fn with_serial(serial: &str) -> Vec<String> {
    let trimmed = serial.trim();
    if trimmed.is_empty() {
        Vec::new()
    } else {
        vec!["-s".to_string(), trimmed.to_string()]
    }
}

fn run(adb: &Path, args: &[String]) -> Result<exec::Output, String> {
    exec::capture(adb, args, None, exec::Options::quiet(ADB_TIMEOUT)).map_err(|error| error.text())
}

pub struct DeviceProbe {
    pub devices: Vec<DeviceInfo>,
    pub unauthorized: usize,
    pub offline: usize,
}

fn command_error(action: &str, output: &exec::Output) -> String {
    let text = output.text.trim();
    if text.is_empty() {
        format!("{action}失败（退出码 {}）", output.code)
    } else {
        text.to_string()
    }
}

pub fn probe_devices(adb: &Path) -> Result<DeviceProbe, String> {
    let output = run(adb, &["devices".to_string(), "-l".to_string()])?;
    if output.code != 0 {
        return Err(command_error("adb devices", &output));
    }
    let mut devices = Vec::new();
    let mut unauthorized = 0usize;
    let mut offline = 0usize;
    for line in output.text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(serial), Some(state)) = (parts.next(), parts.next()) else {
            continue;
        };
        if state == "unauthorized" {
            unauthorized += 1;
            continue;
        }
        if state == "offline" {
            offline += 1;
            continue;
        }
        if state != "device" {
            continue;
        }
        let model = parts
            .find_map(|part| part.strip_prefix("model:"))
            .unwrap_or("")
            .replace('_', " ");
        devices.push(DeviceInfo {
            serial: serial.to_string(),
            model,
        });
    }
    Ok(DeviceProbe { devices, unauthorized, offline })
}

fn empty_device_hint(probe: &DeviceProbe) -> String {
    if probe.unauthorized > 0 {
        "检测到未授权设备，请在手机上点击「允许 USB 调试」后刷新".into()
    } else if probe.offline > 0 {
        "检测到离线设备，请重新插拔数据线或重启 ADB 后刷新".into()
    } else {
        "没有检测到已授权的手机：请用数据线连接，并在手机上允许 USB 调试".into()
    }
}

pub fn pick_from_probe(probe: &DeviceProbe, wanted: &str) -> Result<String, String> {
    let wanted = wanted.trim();
    if probe.devices.is_empty() {
        return Err(empty_device_hint(probe));
    }
    if !wanted.is_empty() {
        return if probe.devices.iter().any(|device| device.serial == wanted) {
            Ok(wanted.to_string())
        } else {
            Err(format!("之前选择的设备 {wanted} 不在了，请刷新设备列表后重新选择"))
        };
    }
    if probe.devices.len() > 1 {
        return Err(format!(
            "检测到多台设备，请先在上方选择一台：{}",
            probe.devices.iter().map(|device| device.serial.as_str()).collect::<Vec<_>>().join("、")
        ));
    }
    Ok(probe.devices[0].serial.clone())
}

pub fn reverse_list(adb: &Path, serial: &str) -> Result<Vec<String>, String> {
    let mut args = with_serial(serial);
    args.push("reverse".into());
    args.push("--list".into());
    let output = run(adb, &args)?;
    if output.code != 0 {
        return Err(command_error("adb reverse --list", &output));
    }
    Ok(output
        .text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

pub fn reverse(adb: &Path, serial: &str, port: u16) -> Result<(), String> {
    let mut args = with_serial(serial);
    for part in ["reverse", &format!("tcp:{port}"), &format!("tcp:{port}")] {
        args.push(part.to_string());
    }
    let output = run(adb, &args)?;
    if output.code != 0 {
        return Err(command_error("adb reverse", &output));
    }
    Ok(())
}

pub fn snapshot(adb: &Path, wanted: &str) -> AdbStatus {
    let mut status = AdbStatus {
        adb: adb.display().to_string(),
        devices: Vec::new(),
        serial: String::new(),
        reverse: Vec::new(),
        error: String::new(),
    };
    match probe_devices(adb) {
        Ok(probe) => {
            let wanted = wanted.trim().to_string();
            status.devices = probe.devices.clone();
            let chosen = if !wanted.is_empty() && status.devices.iter().any(|d| d.serial == wanted) {
                wanted
            } else if status.devices.len() == 1 {
                status.devices[0].serial.clone()
            } else {
                String::new()
            };
            if status.devices.is_empty() {
                status.error = empty_device_hint(&probe);
            } else if chosen.is_empty() {
                status.error = "检测到多台设备，请先选择一台".into();
            } else {
                status.serial = chosen;
                match reverse_list(adb, &status.serial) {
                    Ok(list) => status.reverse = list,
                    Err(error) => status.error = error,
                }
            }
        }
        Err(error) => status.error = error,
    }
    status
}

#[tauri::command]
pub async fn adb_status(app: AppHandle) -> Result<AdbStatus, String> {
    let config = app_pack::load_config(&app);
    let adb = resolve(&config.adb);
    let device = config.device;
    tauri::async_runtime::spawn_blocking(move || snapshot(&adb, &device))
        .await
        .map_err(|error| format!("读取 ADB 状态中断：{error}"))
}

#[tauri::command]
pub async fn adb_set_device(app: AppHandle, serial: String) -> Result<AdbStatus, String> {
    let mut config = app_pack::load_config(&app);
    config.device = serial.trim().to_string();
    app_pack::save_config(&app, &config)?;
    app_pack::emit_settings(&app, &config);
    let adb = resolve(&config.adb);
    let device = config.device;
    tauri::async_runtime::spawn_blocking(move || snapshot(&adb, &device))
        .await
        .map_err(|error| format!("切换 ADB 设备中断：{error}"))
}

#[tauri::command]
pub async fn adb_reverse(app: AppHandle, port: Option<u16>) -> Result<AdbStatus, String> {
    if matches!(port, Some(0)) {
        return Err("端口需在 1～65535 之间".into());
    }
    let config = app_pack::load_config(&app);
    let effective_port = port.unwrap_or_else(|| config.effective_reverse_port());
    let adb = resolve(&config.adb);
    let wanted = config.device;
    tauri::async_runtime::spawn_blocking(move || {
        let probe = probe_devices(&adb)?;
        let serial = pick_from_probe(&probe, &wanted)?;
        reverse(&adb, &serial, effective_port)?;
        Ok(snapshot(&adb, &serial))
    })
    .await
    .map_err(|error| format!("执行端口映射中断：{error}"))?
}