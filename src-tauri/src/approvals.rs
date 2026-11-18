//! 一个连接对应一次人工决定。审批正文只存在于请求进程、队列和灵动岛内存中。
mod pipe;
mod questions;
pub(crate) mod setup;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::VecDeque, io::{Read, Write}, sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use tokio::sync::oneshot;

use crate::windows::BAR;
use questions::{Question, Response};

const MAX_BYTES: usize = 128 * 1024;
const WAIT: Duration = Duration::from_secs(240);
const CHANGED: &str = "approvals-changed";

#[derive(Clone, Copy, Serialize, Deserialize)]
enum Source { Codex, DSH }

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind { #[default] Approval, Questions, Notice }

#[derive(Clone, Serialize, Deserialize)]
struct Request {
    source: Source,
    #[serde(default)]
    kind: Kind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    questions: Vec<Question>,
    tool: String,
    title: String,
    context: String,
    details: String,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Answer { Allow, Deny, Delegate, Answers(Vec<Response>) }

struct Pending {
    id: u64,
    request: Request,
    reply: oneshot::Sender<Answer>,
}

#[derive(Default)]
pub(crate) struct Approvals(Mutex<Queue>);

#[derive(Default)]
struct Queue {
    pending: VecDeque<Pending>,
    revision: u64,
    ready: bool,
    visible: bool,
}

#[derive(Serialize)]
pub(crate) struct Snapshot {
    revision: u64,
    count: usize,
    current: Option<Card>,
}

#[derive(Serialize)]
struct Card {
    id: String,
    #[serde(flatten)]
    request: Request,
}

fn changed(app: &AppHandle) {
    let _ = app.emit_to(BAR, CHANGED, ());
}

pub(crate) fn visibility(app: &AppHandle, visible: bool) {
    let Some(state) = app.try_state::<Approvals>() else { return };
    let mut q = state.0.lock().unwrap();
    q.visible = visible;
    if visible || q.pending.is_empty() { return; }
    q.pending.clear(); // 关闭每条应答通道，立即交回原程序。
    q.revision += 1;
    drop(q);
    changed(app);
}

#[tauri::command]
pub(crate) fn approval_snapshot(window: WebviewWindow, state: State<'_, Approvals>) -> Result<Snapshot, String> {
    if window.label() != BAR { return Err("请在灵动岛查看审批".into()); }
    let mut q = state.0.lock().unwrap();
    q.ready = true;
    Ok(Snapshot {
        revision: q.revision,
        count: q.pending.len(),
        current: q.pending.front().map(|p| Card { id: p.id.to_string(), request: p.request.clone() }),
    })
}

#[tauri::command]
pub(crate) fn answer_approval(window: WebviewWindow, app: AppHandle, state: State<'_, Approvals>, id: String, answer: Answer) -> Result<(), String> {
    if window.label() != BAR { return Err("请在灵动岛处理审批".into()); }
    let mut q = state.0.lock().unwrap();
    let id: u64 = id.parse().map_err(|_| "审批编号无效")?;
    if !q.visible || q.pending.front().map(|p| p.id) != Some(id) { return Err("这项审批已结束".into()); }
    let request = &q.pending.front().unwrap().request;
    match (&answer, request.kind) {
        (Answer::Delegate, _) | (Answer::Allow | Answer::Deny, Kind::Approval) => {}
        (Answer::Answers(values), Kind::Questions) if questions::valid_answers(&request.questions, values) => {
            if serde_json::to_vec(&answer).map_err(|_| "无法读取回答")?.len() > MAX_BYTES {
                return Err("回答过长，请缩短内容或回 DSH 回答".into());
            }
        }
        _ => return Err("回答与当前问题不匹配".into()),
    }
    let pending = q.pending.pop_front().unwrap();
    q.revision += 1;
    drop(q);
    let result = pending.reply.send(answer);
    changed(&app);
    result.map_err(|_| "原任务已撤回审批".into())
}

fn enqueue(app: &AppHandle, request: Request) -> Option<(u64, oneshot::Receiver<Answer>)> {
    let state = app.state::<Approvals>();
    let mut q = state.0.lock().unwrap();
    if !q.ready || !q.visible || q.pending.len() >= 32 { return None; }
    q.revision += 1;
    let id = q.revision;
    let (reply, receive) = oneshot::channel();
    // 聊天中的问句是提醒，等待用户输入的正式请求优先。
    let index = if request.kind == Kind::Notice { q.pending.len() } else {
        q.pending.iter().position(|p| p.request.kind == Kind::Notice).unwrap_or(q.pending.len())
    };
    q.pending.insert(index, Pending { id, request, reply });
    drop(q);
    changed(app);
    Some((id, receive))
}

fn withdraw(app: &AppHandle, id: u64) {
    let state = app.state::<Approvals>();
    let mut q = state.0.lock().unwrap();
    if let Some(index) = q.pending.iter().position(|p| p.id == id) {
        q.pending.remove(index);
        q.revision += 1;
        drop(q);
        changed(app);
    }
}

pub(crate) fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if pipe::serve(app).await.is_err() {
            crate::log_warn!("灵动岛审批连接不可用，请在原应用中审批");
        }
    });
}

fn text_field<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str().filter(|v| !v.trim().is_empty())
}

fn adapter_request(source: Source, question_channel: bool) -> Option<Request> {
    let mut bytes = Vec::new();
    std::io::stdin().take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() > MAX_BYTES { return None; }
    let input: Value = serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes)).ok()?;
    if question_channel {
        let kind: Kind = serde_json::from_value(input.get("kind")?.clone()).ok()?;
        let questions: Vec<Question> = match kind {
            Kind::Questions => {
                let list = serde_json::from_value::<Vec<Question>>(input.get("questions")?.clone()).ok()?;
                if !questions::valid_questions(&list) { return None; }
                list
            }
            Kind::Notice => Vec::new(),
            Kind::Approval => return None,
        };
        return Some(Request { source, kind, questions, tool: "提问".into(),
            title: text_field(&input, "title")?.into(),
            context: text_field(&input, "context").unwrap_or_default().into(),
            details: input["details"].as_str().unwrap_or_default().into() });
    }
    let (tool, title, details, context) = match source {
        Source::Codex => {
            if text_field(&input, "hook_event_name") != Some("PermissionRequest") { return None; }
            let tool = text_field(&input, "tool_name")?;
            let args = input.get("tool_input")?.as_object()?;
            if args.is_empty() { return None; }
            let title = args.get("description").and_then(Value::as_str).filter(|d| !d.trim().is_empty())
                .map(str::to_owned).unwrap_or_else(|| format!("请求使用 {tool}"));
            let details = serde_json::to_string_pretty(args).ok()?;
            let context = ["cwd", "session_id"].iter().filter_map(|key| text_field(&input, key)).collect::<Vec<_>>().join(" · ");
            (tool, title, details, context)
        }
        Source::DSH => (text_field(&input, "tool")?, text_field(&input, "title")?.to_owned(),
            text_field(&input, "details")?.to_owned(), text_field(&input, "context")?.to_owned()),
    };
    Some(Request { source, kind: Kind::Approval, questions: Vec::new(), tool: tool.into(), title, details, context })
}

/// 在注册 Tauri 单实例插件之前处理；关闭灵动岛时调用适配器不会启动主窗口。
pub(crate) fn run_adapter() -> bool {
    let (source, question_channel) = match std::env::args_os().nth(1).as_deref().and_then(|arg| arg.to_str()) {
        Some("--codex-approval") => (Source::Codex, false),
        Some("--dsh-approval") => (Source::DSH, false),
        Some("--dsh-question") => (Source::DSH, true),
        _ => return false,
    };
    let answer = adapter_request(source, question_channel).and_then(|req| {
        let supervisor = std::env::args_os().nth(2)?.to_str()?.parse::<u32>().ok()?;
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
        runtime.block_on(pipe::ask(req, supervisor)).ok()
    }).unwrap_or(Answer::Delegate);
    let output = match (source, answer) {
        (Source::Codex, Answer::Delegate | Answer::Answers(_)) => json!({}),
        (Source::Codex, decision) => json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest",
            "decision": {"behavior": if matches!(decision, Answer::Allow) { "allow" } else { "deny" }}}}),
        (Source::DSH, Answer::Answers(values)) if question_channel => json!({"answers": values}),
        (Source::DSH, Answer::Allow) if !question_channel => json!("allowed-once"),
        (Source::DSH, Answer::Deny) if !question_channel => json!("rejected"),
        (Source::DSH, _) => json!("delegate"),
    };
    let _ = writeln!(std::io::stdout().lock(), "{output}");
    true
}
