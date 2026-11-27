//! 仅处理任务栏请求关注，不读取聊天、不接触其他进程内存。
use crate::{broadcast_ui, state::UiPreferences, AppState, UI_PREFS_FILE};
use std::{
    path::Path,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager, State};
use windows::{
    core::{w, HSTRING},
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::{
            LibraryLoader::GetModuleHandleW,
            Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
        UI::{Shell::ShellExecuteW, WindowsAndMessaging::*},
    },
};

fn process_id(hwnd: HWND) -> u32 {
    if hwnd.0.is_null() {
        return 0;
    }
    let mut id = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut id));
    }
    id
}

fn is_wechat(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
    else {
        return false;
    };
    let mut image = [0u16; 1024];
    let mut length = image.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(image.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    if result.is_err() {
        return false;
    }
    let image = String::from_utf16_lossy(&image[..length as usize]);
    Path::new(&image)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("Weixin.exe") || name.eq_ignore_ascii_case("WeChat.exe")
        })
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let mut running = false;
        let mut hook: Option<(HWND, u32)> = None;
        let mut last_flash: Option<Instant> = None;
        let mut retry_at = Instant::now();
        while state.running.load(Ordering::Relaxed) {
            let enabled = state.ui.snapshot().wechat_enabled;
            if !enabled && running {
                if let Some((hwnd, _)) = hook.take() {
                    unsafe {
                        let _ = DeregisterShellHookWindow(hwnd);
                        let _ = DestroyWindow(hwnd);
                    }
                }
                running = false;
                last_flash = None;
                retry_at = Instant::now();
            } else if enabled && hook.is_none() && Instant::now() >= retry_at {
                running = true;
                hook = register_listener().ok();
                let status = if hook.is_some() { "listening" } else { "error" };
                state.ui.update(|ui| {
                    if ui.wechat_enabled {
                        ui.wechat_status = status.into();
                    }
                });
                broadcast_ui(&app);
                retry_at = Instant::now() + Duration::from_secs(8);
            }

            if let Some((_, message_id)) = hook {
                let mut message = MSG::default();
                while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() } {
                    if message.message == message_id {
                        // HSHELL_FLASH = HSHELL_REDRAW | HSHELL_HIGHBIT，不计数，不处理普通重绘。
                        if message.wParam.0 != 0x8006 {
                            continue;
                        }
                        let pid = process_id(HWND(message.lParam.0 as *mut std::ffi::c_void));
                        let foreground = process_id(unsafe { GetForegroundWindow() });
                        if pid == foreground || is_wechat(foreground) || !is_wechat(pid) {
                            continue;
                        }
                        let now = Instant::now();
                        if last_flash
                            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(2))
                        {
                            continue;
                        }
                        last_flash = Some(now);
                        state.ui.update(|ui| {
                            if !ui.wechat_enabled {
                                return;
                            }
                            ui.wechat_attention = true;
                            ui.wechat_event = ui.wechat_event.wrapping_add(1);
                        });
                        broadcast_ui(&app);
                    } else {
                        let _ = unsafe { TranslateMessage(&message) };
                        unsafe {
                            DispatchMessageW(&message);
                        }
                    }
                }
            }
            // 用户从任务栏、托盘或快捷键自行打开微信，也应视为已经查看。
            // 只判断微信主窗口进入前台，不推断单条消息的已读或未读数量。
            if enabled && state.ui.snapshot().wechat_attention {
                let foreground = unsafe { GetForegroundWindow() };
                if has_main_window_shape(foreground)
                    && unsafe { IsWindowVisible(foreground).as_bool() && !IsIconic(foreground).as_bool() }
                    && is_wechat(process_id(foreground))
                {
                    clear_wechat_attention(app.clone(), app.state::<AppState>());
                    last_flash = None;
                }
            }
            // 只有存在待清除的提醒时才需要高频检查前台窗口；其余时间等待任务栏消息或 500ms 超时。
            // 任务栏闪烁消息到达会立即唤醒线程，不增加提醒延迟。
            let pending = enabled && state.ui.snapshot().wechat_attention;
            let timeout = if pending { 85 } else { 500 };
            let _ = unsafe {
                MsgWaitForMultipleObjectsEx(None, timeout, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
            };
        }
        if let Some((hwnd, _)) = hook {
            unsafe {
                let _ = DeregisterShellHookWindow(hwnd);
                let _ = DestroyWindow(hwnd);
            }
        }
    });
}

unsafe extern "system" fn listener_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn register_listener() -> Result<(HWND, u32), String> {
    static CLASS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let module = unsafe { GetModuleHandleW(None) }.map_err(|_| "无法获取程序句柄")?;
    let class_name = w!("DesktopHubLiteAttention");
    if !*CLASS.get_or_init(|| unsafe {
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(listener_window_proc),
            hInstance: module.into(),
            lpszClassName: class_name,
            ..Default::default()
        }) != 0
    }) {
        return Err("无法注册提醒窗口".into());
    }
    let message = unsafe { RegisterWindowMessageW(w!("SHELLHOOK")) };
    if message == 0 {
        return Err("无法注册任务栏提醒消息".into());
    }
    // 隐藏的顶层窗口；消息专用窗口收不到这类提醒。
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name,
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(module.into()),
            None,
        )
    }
    .map_err(|_| "无法创建提醒窗口")?;
    if !unsafe { RegisterShellHookWindow(hwnd) }.as_bool() {
        let _ = unsafe { DestroyWindow(hwnd) };
        return Err("Windows 未开启任务栏提醒监听".into());
    }
    Ok((hwnd, message))
}

#[tauri::command]
pub fn set_wechat_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _gate = state.configuration_gate.lock().map_err(|_| "配置暂时不可用")?;
    let mut next = state.ui.snapshot();
    next.wechat_enabled = enabled;
    state
        .store
        .write(UI_PREFS_FILE, &UiPreferences::from_state(&next))?;
    state.ui.update(|ui| {
        ui.wechat_enabled = enabled;
        ui.wechat_status = if enabled { "starting" } else { "off" }.into();
        if !enabled {
            ui.wechat_attention = false;
        }
    });
    broadcast_ui(&app);
    Ok(())
}

#[tauri::command]
pub fn clear_wechat_attention(app: AppHandle, state: State<'_, AppState>) {
    state.ui.update(|ui| ui.wechat_attention = false);
    broadcast_ui(&app);
}

#[derive(Default)]
struct Candidates {
    visible: Option<u32>,
    hidden: Option<u32>,
}

fn has_main_window_shape(hwnd: HWND) -> bool {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok()
        && rect.right - rect.left >= 560
        && rect.bottom - rect.top >= 380
        && !unsafe { GetWindow(hwnd, GW_OWNER) }.is_ok_and(|owner| !owner.0.is_null())
}

fn original_main_window_is_foreground(pid: u32) -> bool {
    let hwnd = unsafe { GetForegroundWindow() };
    process_id(hwnd) == pid
        && has_main_window_shape(hwnd)
        && unsafe { IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() }
}

unsafe extern "system" fn find_main(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
    let candidates = unsafe { &mut *(data.0 as *mut Candidates) };
    let pid = process_id(hwnd);
    if has_main_window_shape(hwnd) && is_wechat(pid) {
        if unsafe { IsWindowVisible(hwnd).as_bool() } {
            candidates.visible.get_or_insert(pid);
        } else {
            candidates.hidden.get_or_insert(pid);
        }
    }
    windows::core::BOOL(1)
}

#[tauri::command]
pub async fn open_existing_wechat(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if !state.ui.snapshot().wechat_enabled {
        return Err("请先启用微信普通提醒".into());
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let mut candidates = Candidates::default();
        unsafe {
            EnumWindows(
                Some(find_main),
                LPARAM((&mut candidates as *mut Candidates) as isize),
            )
        }
        .map_err(|_| "无法检查正在运行的微信窗口")?;
        let pid = candidates
            .visible
            .or(candidates.hidden)
            .ok_or("未找到已登录的微信主窗口，请先从托盘打开并登录")?;
        if original_main_window_is_foreground(pid) {
            return Ok(());
        }
        let target = HSTRING::from("weixin://");
        let accepted =
            unsafe { ShellExecuteW(None, w!("open"), &target, None, None, SW_SHOWNORMAL) };
        if accepted.0 as isize <= 32 {
            return Err("系统无法打开微信协议，请从托盘唤回微信".into());
        }
        for _ in 0..18 {
            std::thread::sleep(Duration::from_millis(120));
            if original_main_window_is_foreground(pid) {
                return Ok(());
            }
        }
        Err("未确认原微信实例恢复到前台，请从托盘打开".into())
    })
    .await
    .map_err(|_| "微信唤回检查未完成".to_string())??;
    clear_wechat_attention(app.clone(), app.state::<AppState>());
    Ok(())
}
