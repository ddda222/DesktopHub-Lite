//! 应用播放会话控制。所有 COM 接口只在创建它们的 MTA 线程内使用，不记录进程路径或会话内容。
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};

const PROFILES_FILE: &str = "audio-profiles.json";
pub const AUDIO_DEVICES_CHANGED_EVENT: &str = "audio-devices-changed";
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
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}
#[derive(Serialize)]
pub struct Snapshot {
    pub devices: Vec<OutputDevice>,
    pub sessions: Vec<Session>,
    pub warnings: Vec<String>,
}
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
pub async fn set_default_audio_device(state: State<'_, AudioState>, device_id: String) -> Result<Snapshot, String> {
    let trimmed = device_id.trim().to_string();
    if trimmed.is_empty() || trimmed.len() > 4096 {
        return Err("播放设备标识无效".into());
    }
    let _guard = state.operations.lock().await;
    run(move || platform::set_default_device(&trimmed)).await
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MixerItemKind {
    Device,
    App,
}

#[tauri::command]
pub fn set_mixer_item_visible(
    app: AppHandle,
    kind: MixerItemKind,
    id: String,
    name: String,
    visible: bool,
) -> Result<crate::state::MixerVisibility, String> {
    let next = crate::update_ui_preferences(&app, |ui| {
        let hidden = match kind {
            MixerItemKind::Device => &mut ui.mixer_visibility.devices,
            MixerItemKind::App => &mut ui.mixer_visibility.apps,
        };
        if visible {
            hidden.remove(&id);
        } else {
            hidden.insert(id, name);
        }
    })?;
    crate::broadcast_ui(&app);
    Ok(next.mixer_visibility)
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
pub fn delete_audio_profile(app: AppHandle, audio: State<'_, AudioState>, name: String) -> Result<Vec<Profile>, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() { return Err("请指定要删除的声音配置".into()); }
    let _guard = audio.profiles.lock().map_err(|_| "声音配置暂时不可用")?;
    let mut profiles = read_profiles(&app)?;
    let before = profiles.len();
    profiles.retain(|p| p.name != trimmed);
    if profiles.len() == before {
        return Err("未找到对应的声音配置".into());
    }
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
        let mut offline_devices = 0usize;
        for setting in profile.settings {
            if !snapshot.devices.iter().any(|d| d.id == setting.device_id) {
                offline_devices += 1;
                skipped += 1;
                continue;
            }
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
        let mut snapshot = platform::snapshot().unwrap_or_else(|error| Snapshot { devices: vec![], sessions: vec![], warnings: vec![format!("操作后刷新失败：{error}")] });
        if offline_devices > 0 {
            snapshot.warnings.push(format!("预设中有 {offline_devices} 项对应的播放设备当前已断开或未连接"));
        }
        Ok(ApplyResult { applied, skipped, failed, snapshot })
    }).await
}

/// 轻量监测 Windows 默认输出设备切换或热插拔变化，及时推送给灵动岛与调音台同步选中态。
pub fn start_device_watch(app: AppHandle) {
    #[cfg(windows)]
    {
        let _ = std::thread::Builder::new()
            .name("audio-device-watch".into())
            .spawn(move || {
                let _: Result<(), String> = platform::with_com(|| {
                    let mut last_sig: Option<String> = None;
                    loop {
                        if let Ok(sig) = platform::device_signature() {
                            if let Some(ref prev) = last_sig {
                                if prev != &sig {
                                    let _ = app.emit(AUDIO_DEVICES_CHANGED_EVENT, ());
                                }
                            }
                            last_sig = Some(sig);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(1800));
                    }
                });
            });
    }
    #[cfg(not(windows))]
    {
        let _ = app;
    }
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
    #[derive(Default)]
    struct AudioEnumeration {
        devices: Vec<OutputDevice>,
        entries: Vec<Entry>,
        warnings: Vec<String>,
    }
    fn enumerate() -> Result<AudioEnumeration, String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|_| "Windows 音频服务不可用")?;
            let default_device_id = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .ok()
                .and_then(|dev| dev.GetId().ok().map(|raw| owned_string(raw)));
            let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).map_err(|_| "无法枚举播放设备")?;
            let count = devices.GetCount().map_err(|_| "无法读取播放设备数量")?;
            let mut result = AudioEnumeration::default();
            for index in 0..count {
                let device_result = (|| -> windows::core::Result<()> {
                    let device = devices.Item(index)?;
                    let device_id = owned_string(device.GetId()?);
                    let device_name = device_name(&device, index);
                    let is_default = default_device_id.as_deref() == Some(device_id.as_str());
                    result.devices.push(OutputDevice { id: device_id.clone(), name: device_name.clone(), is_default });
                    let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
                    let sessions = manager.GetSessionEnumerator()?;
                    for number in 0..sessions.GetCount()? {
                        let session_result = (|| -> windows::core::Result<Option<Entry>> {
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
                        match session_result {
                            Ok(Some(entry)) => result.entries.push(entry),
                            Ok(None) => {},
                            Err(_) => result.warnings.push(format!("{device_name}：部分会话已退出或无法访问")),
                        }
                    }
                    Ok(())
                })();
                if device_result.is_err() { result.warnings.push(format!("播放设备 {} 暂时不可用，请刷新", index + 1)); }
            }
            if count == 0 { result.warnings.push("没有可用的播放设备".into()); }
            result.warnings.sort(); result.warnings.dedup();
            Ok(result)
        }
    }
    pub fn snapshot() -> Result<Snapshot, String> {
        let result = enumerate()?;
        Ok(Snapshot {
            devices: result.devices,
            sessions: result.entries.into_iter().map(|e| e.info).collect(),
            warnings: result.warnings,
        })
    }

    /// 轻量读取当前默认播放设备与活跃设备集合指纹（不激活会话管理器，极低开销）。
    pub fn device_signature() -> Result<String, String> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|_| "Windows 音频服务不可用")?;
            let default_id = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .ok()
                .and_then(|dev| dev.GetId().ok().map(|raw| owned_string(raw)))
                .unwrap_or_default();
            let devices = enumerator
                .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
                .map_err(|_| "无法枚举播放设备")?;
            let count = devices.GetCount().unwrap_or(0);
            let mut ids = Vec::with_capacity(count as usize);
            for index in 0..count {
                if let Ok(dev) = devices.Item(index) {
                    if let Ok(raw) = dev.GetId() {
                        ids.push(owned_string(raw));
                    }
                }
            }
            ids.sort();
            Ok(format!("{default_id}|{}", ids.join(",")))
        }
    }
    pub fn update(id: &str, volume: Option<f32>, muted: Option<bool>) -> Result<Session, String> {
        let entry = enumerate()?.entries.into_iter().find(|e| e.info.id == id);
        let mut entry = entry.ok_or("会话已退出或设备已变化，请刷新")?;
        unsafe {
            if let Some(value) = volume { entry.volume.SetMasterVolume(value / 100.0, std::ptr::null()).map_err(|_| "音量设置失败，请刷新实际状态")?; }
            if let Some(value) = muted { entry.volume.SetMute(value, std::ptr::null()).map_err(|_| "静音设置失败；音量可能已更新，请刷新")?; }
            entry.info.volume = (entry.volume.GetMasterVolume().map_err(|_| "设置后无法读回音量，请刷新")? * 100.0).round();
            entry.info.muted = entry.volume.GetMute().map_err(|_| "设置后无法读回静音状态，请刷新")?.as_bool();
            if volume.is_some_and(|v| (v - entry.info.volume).abs() > 1.0) || muted.is_some_and(|m| m != entry.info.muted) { return Err("系统读回值与请求不同，可能被其他应用修改，请刷新".into()); }
        }
        Ok(entry.info)
    }

    /// 通过 Windows COM `IPolicyConfig::SetDefaultEndpoint` 切换系统默认音频播放设备（同时覆盖控制台/多媒体/通信三角色）。
    pub fn set_default_device(device_id: &str) -> Result<Snapshot, String> {
        use core::ffi::c_void;
        use windows::core::{GUID, HRESULT};

        // 先校验目标设备确实处于当前活动的播放端点列表
        let before = enumerate()?;
        if !before.devices.iter().any(|d| d.id == device_id) {
            return Err("目标播放设备已断开或不可用，请刷新列表".into());
        }

        // CLSID_CPolicyConfigClient: {870AF99C-171D-4F9E-AF0D-E63DF40C2BC9}
        const CLSID_POLICY_CONFIG_CLIENT: GUID =
            GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);
        // IID_IPolicyConfig (Win7/10/11): {F8679F50-850A-41CF-9C72-430F290290C8}
        const IID_POLICY_CONFIG: GUID =
            GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
        // IID_IPolicyConfigVista (兼容回退): {568B9108-44BF-40B4-9006-86AFE5B5A620}
        const IID_POLICY_CONFIG_VISTA: GUID =
            GUID::from_u128(0x568b9108_44bf_40b4_9006_86afe5b5a620);

        #[link(name = "ole32")]
        extern "system" {
            #[link_name = "CoCreateInstance"]
            fn ole32_co_create_instance(
                rclsid: *const GUID,
                punk_outer: *mut c_void,
                dw_cls_context: u32,
                riid: *const GUID,
                ppv: *mut *mut c_void,
            ) -> i32;
        }

        let wide_id: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let mut raw_ptr: *mut c_void = core::ptr::null_mut();
            let mut hr = ole32_co_create_instance(
                &CLSID_POLICY_CONFIG_CLIENT,
                core::ptr::null_mut(),
                CLSCTX_ALL.0,
                &IID_POLICY_CONFIG,
                &mut raw_ptr,
            );
            if hr < 0 || raw_ptr.is_null() {
                raw_ptr = core::ptr::null_mut();
                hr = ole32_co_create_instance(
                    &CLSID_POLICY_CONFIG_CLIENT,
                    core::ptr::null_mut(),
                    CLSCTX_ALL.0,
                    &IID_POLICY_CONFIG_VISTA,
                    &mut raw_ptr,
                );
            }
            if hr < 0 || raw_ptr.is_null() {
                return Err("当前系统不支持快速切换默认播放设备".into());
            }

            // COM 虚表布局：slot 2 = IUnknown::Release，slot 13 = IPolicyConfig::SetDefaultEndpoint(PCWSTR, ERole)
            let vtable = *(raw_ptr as *const *const *const c_void);
            let release: unsafe extern "system" fn(*mut c_void) -> u32 =
                core::mem::transmute(*vtable.add(2));
            let set_default: unsafe extern "system" fn(*mut c_void, *const u16, u32) -> HRESULT =
                core::mem::transmute(*vtable.add(13));

            let mut failed_role = false;
            for role in [0u32, 1u32, 2u32] {
                if set_default(raw_ptr, wide_id.as_ptr(), role).is_err() {
                    failed_role = true;
                }
            }
            release(raw_ptr);

            if failed_role {
                let after = snapshot()?;
                if after.devices.iter().any(|d| d.id == device_id && d.is_default) {
                    return Ok(after);
                }
                return Err("切换系统默认音频输出设备失败，请重试".into());
            }
        }
        snapshot()
    }
}
#[cfg(not(windows))]
mod platform {
    use super::*;
    pub fn with_com<T>(job: impl FnOnce() -> Result<T, String>) -> Result<T, String> { job() }
    pub fn snapshot() -> Result<Snapshot, String> { Err("当前系统不支持 Windows 应用声音调节".into()) }
    pub fn update(_: &str, _: Option<f32>, _: Option<bool>) -> Result<Session, String> { Err("当前系统不支持 Windows 应用声音调节".into()) }
    pub fn set_default_device(_: &str) -> Result<Snapshot, String> { Err("当前系统不支持 Windows 音频设备切换".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "手动运行的只读 Windows 会话检查，不设置音量"]
    fn read_only_windows_sessions() {
        let snapshot = platform::with_com(platform::snapshot).expect("读取 Windows 会话");
        for session in &snapshot.sessions { assert!((0.0..=100.0).contains(&session.volume)); assert!(!session.id.is_empty()); }
        for dev in &snapshot.devices {
            println!("播放设备：{} [default={}] ({})", dev.name, dev.is_default, dev.id);
        }
        println!("实际设备 {}，会话 {}，设备提示 {}；没有执行音量或静音写入", snapshot.devices.len(), snapshot.sessions.len(), snapshot.warnings.len());
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
