use serde::{Deserialize, Serialize};
use std::{os::windows::process::CommandExt, path::PathBuf, process::Command};
use windows::{
    core::{w, HSTRING},
    Win32::{System::Com::CoTaskMemFree, UI::{
        Shell::{Common::ITEMIDLIST, SHParseDisplayName, ShellExecuteExW, SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW},
        WindowsAndMessaging::SW_SHOWNORMAL,
    }},
};

const PREFIX: &str = "shell:AppsFolder\\";
const LIST: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$apps = @(Get-StartApps | Select-Object Name, AppID)
ConvertTo-Json -InputObject $apps -Compress
"#;

#[derive(Deserialize)]
struct StartApp {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "AppID")]
    id: String,
}

#[derive(Serialize)]
pub(crate) struct SystemApp { pub name: String, pub target: String }

pub(crate) fn list() -> Result<Vec<SystemApp>, String> {
    let windows = std::env::var_os("SystemRoot").ok_or("无法定位 Windows 目录")?;
    let output = Command::new(PathBuf::from(windows).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
        .creation_flags(0x08000000).args(["-NoProfile", "-NoLogo", "-NonInteractive", "-Command", LIST])
        .output().map_err(|_| "无法读取 Windows 开始菜单的应用列表")?;
    if !output.status.success() { return Err("Windows 应用列表读取失败，请重试".into()); }
    let apps: Vec<StartApp> = serde_json::from_slice(&output.stdout).map_err(|_| "无法解析 Windows 应用列表")?;
    Ok(apps.into_iter().filter(|a| !a.name.trim().is_empty() && !a.id.trim().is_empty())
        .map(|a| SystemApp { name: a.name, target: format!("{PREFIX}{}", a.id) }).collect())
}

pub(crate) fn is_target(target: &str) -> bool {
    target.get(..PREFIX.len()).is_some_and(|p| p.eq_ignore_ascii_case(PREFIX)) && target.len() > PREFIX.len()
}

/// 探测 `shell:AppsFolder\` 目标是否存在：仅在系统明确返回对象/路径不存在错误时判为 false，
/// COM 初始化或其它瞬态异常一律保守返回 true，杜绝误标失效导致误删正常应用。
pub(crate) fn probe_exists(target: &str) -> bool {
    let Ok(_com) = crate::icons::ComGuard::new() else {
        return true;
    };
    let mut pidl = std::ptr::null_mut();
    match unsafe { SHParseDisplayName(&HSTRING::from(target), None, &mut pidl, 0, None) } {
        Ok(()) => {
            if !pidl.is_null() {
                unsafe { CoTaskMemFree(Some(pidl.cast())) };
            }
            true
        }
        Err(error) => {
            const HRESULT_FILE_NOT_FOUND: u32 = 0x8007_0002;
            const HRESULT_PATH_NOT_FOUND: u32 = 0x8007_0003;
            const HRESULT_INVALID_ARG: u32 = 0x8007_0057;
            const HRESULT_NOT_FOUND: u32 = 0x8007_0490;
            let code = error.code().0 as u32;
            !matches!(
                code,
                HRESULT_FILE_NOT_FOUND
                    | HRESULT_PATH_NOT_FOUND
                    | HRESULT_INVALID_ARG
                    | HRESULT_NOT_FOUND
            )
        }
    }
}

// AppsFolder 既包含传统桌面程序也包含商店应用，使用 Shell 项保留系统登记的启动方式。
pub(crate) struct Item {
    pidl: *mut ITEMIDLIST,
    _com: crate::icons::ComGuard,
}

impl Item {
    pub fn resolve(target: &str) -> Result<Self, String> {
        let com = crate::icons::ComGuard::new()?;
        let mut pidl = std::ptr::null_mut();
        unsafe { SHParseDisplayName(&HSTRING::from(target), None, &mut pidl, 0, None) }
            .map_err(|_| "系统中已找不到此应用")?;
        Ok(Self { pidl, _com: com })
    }

    pub fn open(&self) -> Result<(), String> {
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_INVOKEIDLIST,
            lpVerb: w!("open"),
            lpIDList: self.pidl.cast(),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        unsafe { ShellExecuteExW(&mut info) }.map_err(|_| "Windows 未能启动此系统应用".into())
    }
}

impl Drop for Item {
    fn drop(&mut self) { unsafe { CoTaskMemFree(Some(self.pidl.cast())) }; }
}
