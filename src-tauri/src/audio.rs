//! 应用播放会话控制。所有 COM 接口只在创建它们的 MTA 线程内使用，不记录进程路径或会话内容。
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

const PROFILES_FILE: &str = "audio-profiles.json";
#[derive(Default)]
pub struct AudioState {
    operations: tokio::sync::Mutex<()>,
    profiles: Mutex<()>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String, pub name: String, pub device_id: String, pub device_name: String,
    pub app_key: Option<String>, pub pid: u32, pub volume: f32, pub muted: bool, pub active: bool,
}
#[derive(Serialize)]
pub struct Snapshot { pub sessions: Vec<Session>, pub warnings: Vec<String> }
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setting { pub app_key: String, pub device_id: String, pub volume: f32, pub muted: bool }
#[derive(Clone, Deserialize, Serialize)]
pub struct Profile { pub name: String, pub settings: Vec<Setting> }
#[derive(Serialize)]
pub struct ApplyResult { applied: usize, skipped: usize, failed: Vec<String>, snapshot: Snapshot }

fn validate_profile(profile: &mut Profile) -> Result<(), String> {
    profile.name = profile.name.trim().into();
    if profile.name.is_empty() || profile.name.chars().count() > 20 { return Err("配置名称需要 1～20 个字符".into()); }
    if profile.settings.is_empty() || profile.settings.len() > 128 { return Err("配置需要包含 1～128 个应用/设备组合".into()); }
    let mut keys = std::collections::HashSet::new();
    for row in &profile.settings {
        if !row.volume.is_finite() || !(0.0..=100.0).contains(&row.volume) { return Err("音量必须在 0～100 之间".into()); }
        if row.app_key.is_empty() || row.app_key.len() > 32768 || row.device_id.is_empty() || row.device_id.len() > 4096 { return Err("应用或设备标识无效".into()); }
        if !keys.insert((&row.app_key, &row.device_id)) { return Err("同一应用/设备在配置中只能出现一次".into()); }
    }
    Ok(())
}

async fn run<T: Send + 'static>(job: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    let (send, receive) = tokio::sync::oneshot::channel();
    std::thread::Builder::new().name("audio-session".into()).spawn(move || {
        let result = platform::with_com(job);
        let _ = send.send(result);
    }).map_err(|_| "无法启动音频工作线程")?;
    receive.await.map_err(|_| "音频操作未完成，请刷新后重试".to_string())?
}

#[tauri::command]
pub async fn list_audio_sessions(state: State<'_, AudioState>) -> Result<Snapshot, String> {
    let _guard = state.operations.lock().await;
    run(platform::snapshot).await
}
#[tauri::command]
pub async fn update_audio_session(state: State<'_, AudioState>, id: String, volume: Option<f32>, muted: Option<bool>) -> Result<Session, String> {
    if volume.is_some_and(|v| !v.is_finite() || !(0.0..=100.0).contains(&v)) { return Err("音量必须在 0～100 之间".into()); }
    if volume.is_none() && muted.is_none() { return Err("没有指定音频操作".into()); }
    let _guard = state.operations.lock().await;
    run(move || platform::update(&id, volume, muted)).await
}
#[tauri::command]
pub fn list_audio_profiles(app: AppHandle, audio: State<'_, AudioState>) -> Result<Vec<Profile>, String> {
    let _guard = audio.profiles.lock().map_err(|_| "声音配置暂时不可用")?;
    read_profiles(&app)
}
fn read_profiles(app: &AppHandle) -> Result<Vec<Profile>, String> {
    let state = app.state::<crate::AppState>();
    let exists = state.store.root().join(PROFILES_FILE).exists();
    let value: Option<Vec<Profile>> = state.store.read(PROFILES_FILE);
    match value { Some(mut rows) => { if rows.len() > 16 { return Err("声音配置数量异常，原文件已保留".into()); } for row in &mut rows { validate_profile(row)?; } Ok(rows) }, None if exists => Err("声音配置无法读取，原文件已保留".into()), None => Ok(vec![]) }
}
#[tauri::command]
pub fn save_audio_profile(app: AppHandle, audio: State<'_, AudioState>, mut profile: Profile) -> Result<Vec<Profile>, String> {
    validate_profile(&mut profile)?;
    let _guard = audio.profiles.lock().map_err(|_| "声音配置暂时不可用")?;
    let mut profiles = read_profiles(&app)?;
    if let Some(index) = profiles.iter().position(|p| p.name == profile.name) { profiles[index] = profile; }
    else { if profiles.len() >= 16 { return Err("最多保存 16 个声音配置".into()); } profiles.push(profile); }
    app.state::<crate::AppState>().store.write(PROFILES_FILE, &profiles)?;
    Ok(profiles)
}
#[tauri::command]
pub async fn apply_audio_profile(app: AppHandle, audio: State<'_, AudioState>, name: String) -> Result<ApplyResult, String> {
    let profile = {
        let _guard = audio.profiles.lock().map_err(|_| "声音配置暂时不可用")?;
        read_profiles(&app)?.into_iter().find(|p| p.name == name).ok_or("声音配置不存在")?
    };
    let _guard = audio.operations.lock().await;
    run(move || {
        let snapshot = platform::snapshot()?;
        let mut applied = 0; let mut skipped = 0; let mut failed = Vec::new();
        for setting in profile.settings {
            let matches: Vec<_> = snapshot.sessions.iter().filter(|s| s.app_key.as_deref() == Some(&setting.app_key) && s.device_id == setting.device_id).collect();
            if matches.is_empty() { skipped += 1; continue; }
            for session in matches {
                match platform::update(&session.id, Some(setting.volume), Some(setting.muted)) {
                    Ok(_) => applied += 1,
                    Err(error) => failed.push(format!("{}：{error}", session.name)),
                }
            }
        }
        // 写入后即使刷新失败，也保留已执行的结果，不能把部分操作描述成未执行。
        let snapshot = platform::snapshot().unwrap_or_else(|error| Snapshot { sessions: vec![], warnings: vec![format!("操作后刷新失败：{error}")] });
        Ok(ApplyResult { applied, skipped, failed, snapshot })
    }).await
}

#[cfg(windows)]
mod platform {
    use super::*;
    use sha2::{Digest, Sha256};
    use windows::{core::{Interface, PWSTR}, Win32::{
        Foundation::{CloseHandle, PROPERTYKEY},
        Media::Audio::*,
        System::{Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ, StructuredStorage::{PropVariantClear, PropVariantToStringAlloc}}, Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_NAME_WIN32}},
    }};
    struct ComGuard;
    impl Drop for ComGuard { fn drop(&mut self) { unsafe { CoUninitialize() } } }
    pub fn with_com<T>(job: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok().map_err(|_| "无法初始化 Windows 音频接口")?; }
        let _guard = ComGuard;
        job()
    }
    unsafe fn owned_string(value: PWSTR) -> String {
        if value.is_null() { return String::new(); }
        let text = value.to_string().unwrap_or_default();
        CoTaskMemFree(Some(value.0.cast())); text
    }
    fn process_name(pid: u32) -> Option<String> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut path = vec![0u16; 32768]; let mut length = path.len() as u32;
            let result = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(path.as_mut_ptr()), &mut length);
            let _ = CloseHandle(process);
            result.ok()?; Some(String::from_utf16_lossy(&path[..length as usize]))
        }
    }
    unsafe fn device_name(device: &IMMDevice, index: u32) -> String {
        const FRIENDLY: PROPERTYKEY = PROPERTYKEY { fmtid: windows::core::GUID::from_u128(0xa45c254e_df1c_4efd_8020_67d146a850e0), pid: 14 };
        if let Ok(store) = device.OpenPropertyStore(STGM_READ) {
            if let Ok(mut value) = store.GetValue(&FRIENDLY) {
                let name = PropVariantToStringAlloc(&value).ok().map(|text| owned_string(text));
                let _ = PropVariantClear(&mut value);
                if let Some(name) = name.filter(|name| !name.is_empty()) { return name; }
            }
        }
        format!("播放设备 {}", index + 1)
    }
    struct Entry { info: Session, volume: ISimpleAudioVolume }
    fn entries() -> Result<(Vec<Entry>, Vec<String>), String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|_| "Windows 音频服务不可用")?;
            let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).map_err(|_| "无法枚举播放设备")?;
            let count = devices.GetCount().map_err(|_| "无法读取播放设备数量")?;
            let mut entries = Vec::new(); let mut warnings = Vec::new();
            for index in 0..count {
                let result = (|| -> windows::core::Result<()> {
                    let device = devices.Item(index)?;
                    let device_id = owned_string(device.GetId()?);
                    let device_name = device_name(&device, index);
                    let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
                    let sessions = manager.GetSessionEnumerator()?;
                    for number in 0..sessions.GetCount()? {
                        let result = (|| -> windows::core::Result<Option<Entry>> {
                            let control = sessions.GetSession(number)?;
                            let status = control.GetState()?;
                            if status == AudioSessionStateExpired { return Ok(None); }
                            let control2: IAudioSessionControl2 = control.cast()?;
                            let volume: ISimpleAudioVolume = control.cast()?;
                            let instance = owned_string(control2.GetSessionInstanceIdentifier()?);
                            let pid = control2.GetProcessId()?;
                            let system = control2.IsSystemSoundsSession().0 == 0;
                            let path = if system { None } else { process_name(pid) };
                            let display = owned_string(control.GetDisplayName()?);
                            let name = if system { "系统声音".into() } else if !display.is_empty() && !display.starts_with('@') { display } else {
                                path.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| format!("应用 PID {pid}"))
                            };
                            let key = if system { Some("system-sounds".into()) } else { path.map(|p| p.to_lowercase()) };
                            let id = format!("{:x}", Sha256::digest(format!("{device_id}\n{instance}")));
                            Ok(Some(Entry { info: Session { id, name, device_id: device_id.clone(), device_name: device_name.clone(), app_key: key, pid,
                                volume: (volume.GetMasterVolume()? * 100.0).round(), muted: volume.GetMute()?.as_bool(), active: status == AudioSessionStateActive }, volume }))
                        })();
                        match result { Ok(Some(entry)) => entries.push(entry), Ok(None) => {}, Err(_) => warnings.push(format!("{device_name}：部分会话已退出或无法访问")) }
                    }
                    Ok(())
                })();
                if result.is_err() { warnings.push(format!("播放设备 {} 暂时不可用，请刷新", index + 1)); }
            }
            if count == 0 { warnings.push("没有可用的播放设备".into()); }
            warnings.sort(); warnings.dedup();
            Ok((entries, warnings))
        }
    }
    pub fn snapshot() -> Result<Snapshot, String> { let (entries, warnings) = entries()?; Ok(Snapshot { sessions: entries.into_iter().map(|e| e.info).collect(), warnings }) }
    pub fn update(id: &str, volume: Option<f32>, muted: Option<bool>) -> Result<Session, String> {
        let (entries, _) = entries()?;
        let mut entry = entries.into_iter().find(|e| e.info.id == id).ok_or("会话已退出或设备已变化，请刷新")?;
        unsafe {
            if let Some(value) = volume { entry.volume.SetMasterVolume(value / 100.0, std::ptr::null()).map_err(|_| "音量设置失败，请刷新实际状态")?; }
            if let Some(value) = muted { entry.volume.SetMute(value, std::ptr::null()).map_err(|_| "静音设置失败；音量可能已更新，请刷新")?; }
            entry.info.volume = (entry.volume.GetMasterVolume().map_err(|_| "设置后无法读回音量，请刷新")? * 100.0).round();
            entry.info.muted = entry.volume.GetMute().map_err(|_| "设置后无法读回静音状态，请刷新")?.as_bool();
            if volume.is_some_and(|v| (v - entry.info.volume).abs() > 1.0) || muted.is_some_and(|m| m != entry.info.muted) { return Err("系统读回值与请求不同，可能被其他应用修改，请刷新".into()); }
        }
        Ok(entry.info)
    }
}
#[cfg(not(windows))]
mod platform {
    use super::*;
    pub fn with_com<T>(job: impl FnOnce() -> Result<T, String>) -> Result<T, String> { job() }
    pub fn snapshot() -> Result<Snapshot, String> { Err("当前系统不支持 Windows 应用声音调节".into()) }
    pub fn update(_: &str, _: Option<f32>, _: Option<bool>) -> Result<Session, String> { Err("当前系统不支持 Windows 应用声音调节".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "手动运行的只读 Windows 会话检查，不设置音量"]
    fn read_only_windows_sessions() {
        let snapshot = platform::with_com(platform::snapshot).expect("读取 Windows 会话");
        for session in &snapshot.sessions { assert!((0.0..=100.0).contains(&session.volume)); assert!(!session.id.is_empty()); }
        println!("实际会话 {}，设备提示 {}；没有执行音量或静音写入", snapshot.sessions.len(), snapshot.warnings.len());
    }
    fn profile() -> Profile { Profile { name: " 开会 ".into(), settings: vec![Setting { app_key: "meeting.exe".into(), device_id: "speakers".into(), volume: 65.0, muted: false }] } }
    #[test] fn profile_bounds_and_identity() {
        let mut p = profile(); assert!(validate_profile(&mut p).is_ok()); assert_eq!(p.name, "开会");
        p.settings[0].volume = f32::NAN; assert!(validate_profile(&mut p).is_err());
        p.settings[0].volume = 101.0; assert!(validate_profile(&mut p).is_err());
        p.settings[0].volume = 0.0; assert!(validate_profile(&mut p).is_ok());
        p.settings.push(p.settings[0].clone()); assert!(validate_profile(&mut p).is_err());
        p.settings[1].device_id = "headphones".into(); assert!(validate_profile(&mut p).is_ok());
        p.settings[1].app_key.clear(); assert!(validate_profile(&mut p).is_err());
    }
}
