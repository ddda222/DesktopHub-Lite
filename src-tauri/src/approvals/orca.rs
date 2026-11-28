//! Orca 智能体状态监听与窗口聚焦联动：
//! - 零侵入读取本机运行中的 Orca 会话状态（`orca agent list --json`），覆盖原生 Antigravity Chat 与各类终端 Agent；
//! - 仅在用户主动启用 Orca 连接、灵动岛可见且 Orca.exe 正在运行时后台低频检测（每 3 秒）；
//! - 会话跃迁为等待处理（blocked）、等待回复（waiting）或后台任务完成（done）时向灵动岛推送提醒卡，支持一键切回 Orca 窗口。

use super::{enqueue, setup, withdraw, Answer, Kind, Request, Source};
use crate::{storage::write_file_atomic, windows::BAR, AppState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{atomic::Ordering, Arc, Mutex},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};

const ORCA_CONFIG_FILE: &str = "orca.json";
pub const ORCA_STATUS_EVENT: &str = "island-orca-status";

/// 广播给灵动岛胶囊的 Orca 轻量实时状态快照。
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OrcaStatusSnapshot {
    pub enabled: bool,
    pub running: bool,
    pub working_count: usize,
    pub blocked_count: usize,
    pub waiting_count: usize,
    /// "idle" | "working" | "waiting" | "blocked"
    pub phase: String,
    /// 胶囊微徽标短文案（idle 时为空）
    pub label: String,
    /// 悬停完整详情
    pub tooltip: String,
}

/// 供 Tauri 状态管理的最新 Orca 状态快照。
#[derive(Default)]
pub struct OrcaStatusState(pub Arc<Mutex<OrcaStatusSnapshot>>);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrcaConfig {
    pub enabled: bool,
    pub cli_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrcaSession {
    pub id: String,
    pub kind: String,
    pub agent: String,
    pub state: String,
    pub repo: String,
    pub workspace: String,
    pub title: String,
    pub last_message: String,
    pub updated_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrcaNoticeEvent {
    pub session_id: String,
    pub tool: String,
    pub title: String,
    pub context: String,
    pub details: String,
}

pub(crate) fn config_path() -> Result<PathBuf, String> {
    let local = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| "无法定位 LOCALAPPDATA 目录".to_string())?;
    Ok(local.join("DesktopHub-Lite/Approvals").join(ORCA_CONFIG_FILE))
}

pub(crate) fn load_config() -> Result<Option<OrcaConfig>, String> {
    let path = config_path()?;
    match fs::read(&path) {
        Ok(bytes) => {
            let clean = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
            serde_json::from_slice(clean)
                .map(Some)
                .map_err(|_| "Orca 接入配置损坏，请重新启用".to_string())
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(format!("无法读取 {}", path.display())),
    }
}

pub(crate) fn save_config(cfg: &OrcaConfig) -> Result<(), String> {
    let path = config_path()?;
    let parent = path.parent().ok_or("接入目录无效")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建接入目录")?;
    let bytes = serde_json::to_vec_pretty(cfg).map_err(|_| "无法编码 Orca 接入配置")?;
    write_file_atomic(&path, &bytes)
}

/// 解析 `orca agent list --json` 的标准输出为会话清单。
pub(crate) fn parse_orca_agent_list(bytes: &[u8]) -> Option<Vec<OrcaSession>> {
    let val: Value = serde_json::from_slice(bytes).ok()?;
    if val.get("ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let agents = val.pointer("/result/agents")?.as_array()?;
    let mut out = Vec::with_capacity(agents.len());
    for item in agents {
        let Some(id) = item.get("orcaSessionId").and_then(Value::as_str).filter(|s| !s.is_empty()) else {
            continue;
        };
        let state = item
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_ascii_lowercase();
        let kind = item.get("kind").and_then(Value::as_str).unwrap_or("chat").to_string();
        let agent = item.get("agent").and_then(Value::as_str).unwrap_or("agent").to_string();
        let repo = item.get("repo").and_then(Value::as_str).unwrap_or_default().to_string();
        let workspace = item.get("workspace").and_then(Value::as_str).unwrap_or_default().to_string();
        let title = item.get("title").and_then(Value::as_str).unwrap_or_default().trim().to_string();
        let last_message = item
            .get("lastAssistantMessage")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        let updated_at = item.get("updatedAt").and_then(Value::as_u64).unwrap_or(0);
        out.push(OrcaSession {
            id: id.to_string(),
            kind,
            agent,
            state,
            repo,
            workspace,
            title,
            last_message,
            updated_at,
        });
    }
    Some(out)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{head}…")
    } else {
        head
    }
}

fn build_context(session: &OrcaSession) -> String {
    let mut parts = Vec::new();
    if !session.repo.is_empty() && !session.workspace.is_empty() {
        parts.push(format!("{} / {}", session.repo, session.workspace));
    } else if !session.workspace.is_empty() {
        parts.push(session.workspace.clone());
    } else if !session.repo.is_empty() {
        parts.push(session.repo.clone());
    }
    if !session.agent.is_empty() {
        parts.push(session.agent.clone());
    }
    if !session.title.is_empty() {
        parts.push(truncate_chars(&session.title, 36));
    }
    parts.join(" · ")
}

/// 对比前后两帧 Orca 会话快照，返回需要弹出灵动岛提醒的事件列表及需要撤回的会话 ID 列表。
pub(crate) fn detect_orca_transitions(
    prev: &HashMap<String, (String, u64)>,
    current: &[OrcaSession],
    orca_foreground: bool,
) -> (Vec<OrcaNoticeEvent>, Vec<String>) {
    let mut notices = Vec::new();
    let mut active_ids = HashMap::with_capacity(current.len());

    for s in current {
        active_ids.insert(s.id.as_str(), s.state.as_str());
        let old = prev.get(&s.id);
        let old_state = old.map(|(st, _)| st.as_str());
        let old_ts = old.map(|(_, ts)| *ts).unwrap_or(0);
        let state_changed = old_state != Some(s.state.as_str());
        let updated_newer = s.updated_at > old_ts && old_ts > 0;

        let ctx = build_context(s);
        let preview = if !s.last_message.is_empty() {
            truncate_chars(&s.last_message, 320)
        } else if !s.title.is_empty() {
            format!("会话「{}」状态已更新，点击下方切回 Orca 查看。", truncate_chars(&s.title, 60))
        } else {
            "Orca 会话有新进展，点击下方切回 Orca 查看。".to_string()
        };

        if s.state == "blocked" && (state_changed || updated_newer) {
            notices.push(OrcaNoticeEvent {
                session_id: s.id.clone(),
                tool: "等待处理".to_string(),
                title: "Orca 需要你确认或审批".to_string(),
                context: ctx,
                details: preview,
            });
        } else if s.state == "waiting" && (old_state == Some("working") || (state_changed && old_state.is_some())) {
            notices.push(OrcaNoticeEvent {
                session_id: s.id.clone(),
                tool: "等待回复".to_string(),
                title: "Orca 等待你的输入".to_string(),
                context: ctx,
                details: preview,
            });
        } else if s.state == "done" && old_state == Some("working") && !orca_foreground {
            notices.push(OrcaNoticeEvent {
                session_id: s.id.clone(),
                tool: "任务完成".to_string(),
                title: "Orca 已完成回复".to_string(),
                context: ctx,
                details: preview,
            });
        }
    }

    // 已经重新开始运行或已关闭的会话，撤回其在灵动岛中残留的旧提醒。
    let mut withdraw_sessions = Vec::new();
    for sid in prev.keys() {
        match active_ids.get(sid.as_str()) {
            None => withdraw_sessions.push(sid.clone()),
            Some(&"working") => withdraw_sessions.push(sid.clone()),
            _ => {}
        }
    }
    (notices, withdraw_sessions)
}

#[cfg(windows)]
mod platform {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows::{
        core::{BOOL, PWSTR},
        Win32::{
            Foundation::{HWND, LPARAM},
            System::Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
            UI::WindowsAndMessaging::{
                EnumWindows, GetForegroundWindow, GetWindowTextLengthW, GetWindowThreadProcessId,
                IsIconic, IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE,
            },
        },
    };

    fn is_orca_pid(pid: u32) -> bool {
        if pid == 0 {
            return false;
        }
        unsafe {
            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return false;
            };
            let _owned = OwnedHandle::from_raw_handle(process.0);
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_err() {
                return false;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let file_name = std::path::Path::new(&path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            file_name.eq_ignore_ascii_case("orca.exe")
        }
    }

    pub fn is_orca_foreground() -> bool {
        unsafe {
            let fg = GetForegroundWindow();
            if fg.0.is_null() {
                return false;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(fg, Some(&mut pid));
            is_orca_pid(pid)
        }
    }

    struct FindCtx {
        found: Option<HWND>,
    }

    unsafe extern "system" fn enum_orca_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
        if !IsWindowVisible(hwnd).as_bool() {
            return BOOL(1);
        }
        if GetWindowTextLengthW(hwnd) <= 0 {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if is_orca_pid(pid) {
            let ctx = &mut *(lparam.0 as *mut FindCtx);
            ctx.found = Some(hwnd);
            return BOOL(0);
        }
        BOOL(1)
    }

    fn find_orca_window() -> Option<HWND> {
        let mut ctx = FindCtx { found: None };
        unsafe {
            let _ = EnumWindows(Some(enum_orca_cb), LPARAM(&mut ctx as *mut FindCtx as isize));
        }
        ctx.found
    }

    /// 轻量检测当前桌面是否存在运行中的 Orca 主窗口（< 1ms），Orca 未打开时不拉起 CLI。
    pub fn is_orca_running() -> bool {
        find_orca_window().is_some()
    }

    /// 将本机正在运行的 Orca 主窗口恢复并切回前台。
    pub fn focus_orca_main_window() -> Result<(), String> {
        let Some(hwnd) = find_orca_window() else {
            return Err("未找到正在运行的 Orca 窗口".into());
        };
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = SetForegroundWindow(hwnd);
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn is_orca_running() -> bool {
        false
    }
    pub fn is_orca_foreground() -> bool {
        false
    }
    pub fn focus_orca_main_window() -> Result<(), String> {
        Err("仅支持在 Windows 桌面版中切回 Orca".into())
    }
}

fn fetch_orca_sessions(cli_path: &Path) -> Option<Vec<OrcaSession>> {
    if !cli_path.is_file() {
        return None;
    }
    let mut cmd = Command::new(cli_path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = cmd
        .args(["agent", "list", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_orca_agent_list(&output.stdout)
}

/// 汇总当前 Orca 活跃会话为收起态胶囊轻量状态快照（优先级：blocked > waiting > working > idle）。
pub(crate) fn summarize_orca_status(
    enabled: bool,
    running: bool,
    sessions: &[OrcaSession],
) -> OrcaStatusSnapshot {
    if !enabled || !running {
        return OrcaStatusSnapshot {
            enabled,
            running,
            phase: "idle".to_string(),
            ..Default::default()
        };
    }

    let mut blocked: Vec<&OrcaSession> = Vec::new();
    let mut waiting: Vec<&OrcaSession> = Vec::new();
    let mut working: Vec<&OrcaSession> = Vec::new();
    for s in sessions {
        match s.state.as_str() {
            "blocked" => blocked.push(s),
            "waiting" => waiting.push(s),
            "working" => working.push(s),
            _ => {}
        }
    }

    let (phase, label, focus_list): (&str, String, &[&OrcaSession]) = if !blocked.is_empty() {
        let lbl = if blocked.len() > 1 {
            format!("Orca 待处理·{}", blocked.len())
        } else {
            "Orca 待处理".to_string()
        };
        ("blocked", lbl, &blocked)
    } else if !waiting.is_empty() {
        let lbl = if waiting.len() > 1 {
            format!("Orca 待回复·{}", waiting.len())
        } else {
            "Orca 待回复".to_string()
        };
        ("waiting", lbl, &waiting)
    } else if !working.is_empty() {
        let lbl = if working.len() > 1 {
            format!("Orca 思考中·{}", working.len())
        } else {
            "Orca 思考中".to_string()
        };
        ("working", lbl, &working)
    } else {
        ("idle", String::new(), &[])
    };

    let tooltip = if focus_list.is_empty() {
        String::new()
    } else {
        let mut lines = Vec::new();
        for s in focus_list.iter().take(3) {
            let ctx = build_context(s);
            if !ctx.is_empty() {
                lines.push(ctx);
            }
        }
        let body = if lines.is_empty() {
            label.clone()
        } else {
            lines.join(" / ")
        };
        format!("{label} · {body}（点击切回 Orca）")
    };

    OrcaStatusSnapshot {
        enabled,
        running,
        working_count: working.len(),
        blocked_count: blocked.len(),
        waiting_count: waiting.len(),
        phase: phase.to_string(),
        label,
        tooltip,
    }
}

fn publish_orca_status(
    app: &AppHandle,
    last: &mut Option<OrcaStatusSnapshot>,
    next: OrcaStatusSnapshot,
) {
    if last.as_ref() == Some(&next) {
        return;
    }
    if let Some(state) = app.try_state::<OrcaStatusState>() {
        *state.0.lock().unwrap_or_else(|e| e.into_inner()) = next.clone();
    }
    let _ = app.emit_to(BAR, ORCA_STATUS_EVENT, &next);
    *last = Some(next);
}

#[tauri::command]
pub(crate) fn orca_status(state: tauri::State<'_, OrcaStatusState>) -> OrcaStatusSnapshot {
    state.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub(crate) fn focus_orca_window() -> Result<(), String> {
    platform::focus_orca_main_window()
}

/// 启动 Orca 后台状态监视线程，自动对接灵动岛提醒队列与收起态状态徽标。
pub(super) fn start(app: AppHandle) {
    std::thread::Builder::new()
        .name("island-orca-watch".into())
        .spawn(move || {
            let mut baseline_ready = false;
            let mut prev_states: HashMap<String, (String, u64)> = HashMap::new();
            let mut active_cards: HashMap<String, u64> = HashMap::new();
            let mut last_status: Option<OrcaStatusSnapshot> = None;

            while app.state::<AppState>().running.load(Ordering::Relaxed) {
                let cfg = load_config().ok().flatten();
                let enabled = cfg.as_ref().is_some_and(|c| c.enabled && !c.cli_path.trim().is_empty());
                let bar_visible = app
                    .get_webview_window(BAR)
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);
                let running = enabled && platform::is_orca_running();

                if !enabled || !bar_visible || !running {
                    publish_orca_status(&app, &mut last_status, summarize_orca_status(enabled, running, &[]));
                    if !enabled && !active_cards.is_empty() {
                        for (_, card_id) in active_cards.drain() {
                            withdraw(&app, card_id);
                        }
                        prev_states.clear();
                        baseline_ready = false;
                    }
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }

                let cli_path = PathBuf::from(cfg.unwrap().cli_path);
                if let Some(sessions) = fetch_orca_sessions(&cli_path) {
                    publish_orca_status(&app, &mut last_status, summarize_orca_status(true, true, &sessions));
                    if !baseline_ready {
                        baseline_ready = true;
                        for s in sessions {
                            prev_states.insert(s.id, (s.state, s.updated_at));
                        }
                    } else {
                        let fg = platform::is_orca_foreground();
                        let (events, withdraw_sessions) =
                            detect_orca_transitions(&prev_states, &sessions, fg);

                        for sid in withdraw_sessions {
                            if let Some(card_id) = active_cards.remove(&sid) {
                                withdraw(&app, card_id);
                            }
                        }

                        for ev in events {
                            if let Some(old_id) = active_cards.remove(&ev.session_id) {
                                withdraw(&app, old_id);
                            }
                            let req = Request {
                                source: Source::Orca,
                                kind: Kind::Notice,
                                questions: Vec::new(),
                                tool: ev.tool,
                                title: ev.title,
                                context: ev.context,
                                details: ev.details,
                            };
                            if let Some((card_id, rx)) = enqueue(&app, req) {
                                active_cards.insert(ev.session_id.clone(), card_id);
                                let app_handle = app.clone();
                                tauri::async_runtime::spawn(async move {
                                    if let Ok(Ok(Answer::Delegate)) =
                                        tokio::time::timeout(Duration::from_secs(600), rx).await
                                    {
                                        setup::remember_connection(&app_handle, Source::Orca);
                                    } else {
                                        withdraw(&app_handle, card_id);
                                    }
                                });
                            }
                        }

                        prev_states.clear();
                        for s in sessions {
                            prev_states.insert(s.id, (s.state, s.updated_at));
                        }
                    }
                }

                std::thread::sleep(Duration::from_secs(3));
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_orca_agent_list_json_shape() {
        let raw = r#"{
          "ok": true,
          "result": {
            "agents": [
              {
                "orcaSessionId": "s-1",
                "kind": "chat",
                "agent": "antigravity",
                "state": "done",
                "workspace": "main",
                "repo": "DesktopHub-Lite",
                "title": "测试任务",
                "lastAssistantMessage": "已完成修复。",
                "updatedAt": 1000
              }
            ]
          }
        }"#;
        let list = parse_orca_agent_list(raw.as_bytes()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "s-1");
        assert_eq!(list[0].state, "done");
        assert_eq!(list[0].repo, "DesktopHub-Lite");
        assert_eq!(list[0].last_message, "已完成修复。");
    }

    #[test]
    fn detects_blocked_waiting_and_background_done_without_false_positives() {
        let mut prev = HashMap::new();
        prev.insert("s-1".to_string(), ("working".to_string(), 100));
        prev.insert("s-2".to_string(), ("working".to_string(), 100));
        prev.insert("s-3".to_string(), ("done".to_string(), 100));

        let current = vec![
            OrcaSession {
                id: "s-1".into(),
                kind: "terminal".into(),
                agent: "codex".into(),
                state: "blocked".into(),
                repo: "Demo".into(),
                workspace: "main".into(),
                title: "执行构建".into(),
                last_message: "等待确认".into(),
                updated_at: 200,
            },
            OrcaSession {
                id: "s-2".into(),
                kind: "chat".into(),
                agent: "antigravity".into(),
                state: "done".into(),
                repo: "Demo".into(),
                workspace: "main".into(),
                title: "调研".into(),
                last_message: "调研报告完成".into(),
                updated_at: 200,
            },
            OrcaSession {
                id: "s-3".into(),
                kind: "chat".into(),
                agent: "antigravity".into(),
                state: "done".into(),
                repo: "Demo".into(),
                workspace: "main".into(),
                title: "旧任务".into(),
                last_message: "旧回复".into(),
                updated_at: 100,
            },
        ];

        // Orca 在后台时：s-1(blocked) 和 s-2(working->done) 都提醒，s-3(维持done) 不提醒
        let (events_bg, _) = detect_orca_transitions(&prev, &current, false);
        assert_eq!(events_bg.len(), 2);
        assert_eq!(events_bg[0].session_id, "s-1");
        assert_eq!(events_bg[0].tool, "等待处理");
        assert_eq!(events_bg[1].session_id, "s-2");
        assert_eq!(events_bg[1].tool, "任务完成");

        // Orca 在前台时：仅提醒 s-1(blocked)，不打扰正在看的 s-2(done)
        let (events_fg, _) = detect_orca_transitions(&prev, &current, true);
        assert_eq!(events_fg.len(), 1);
        assert_eq!(events_fg[0].session_id, "s-1");
    }

    #[test]
    fn summarizes_orca_capsule_status_by_priority() {
        let mk = |id: &str, state: &str, title: &str| OrcaSession {
            id: id.into(),
            kind: "chat".into(),
            agent: "antigravity".into(),
            state: state.into(),
            repo: "DesktopHub-Lite".into(),
            workspace: "main".into(),
            title: title.into(),
            last_message: String::new(),
            updated_at: 100,
        };
        let idle = summarize_orca_status(true, true, &[mk("1", "done", "已完成")]);
        assert_eq!(idle.phase, "idle");
        assert!(idle.label.is_empty());

        let working = summarize_orca_status(true, true, &[mk("1", "working", "实现新功能"), mk("2", "working", "跑测试")]);
        assert_eq!(working.phase, "working");
        assert_eq!(working.working_count, 2);
        assert_eq!(working.label, "Orca 思考中·2");

        let waiting = summarize_orca_status(true, true, &[mk("1", "working", "A"), mk("2", "waiting", "等你回复")]);
        assert_eq!(waiting.phase, "waiting");
        assert_eq!(waiting.label, "Orca 待回复");

        let blocked = summarize_orca_status(true, true, &[mk("1", "waiting", "A"), mk("2", "blocked", "待审批")]);
        assert_eq!(blocked.phase, "blocked");
        assert_eq!(blocked.label, "Orca 待处理");
        assert!(blocked.tooltip.contains("点击切回 Orca"));
    }
}
