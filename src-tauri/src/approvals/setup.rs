use crate::{storage::write_file_atomic, windows::SETTINGS};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

const DISCOVER: &str = include_str!("discover.ps1");
const CODEX_ADAPTER: &[u8] = include_bytes!("../../../integrations/codex.ps1");
const PLUGIN: &str = "dsh-desktophub-approvals";
const PLUGIN_FILES: &[(&str, &[u8])] = &[
    ("questions.js", include_bytes!("../../../integrations/dsh-approvals/questions.js")),
    ("index.js", include_bytes!("../../../integrations/dsh-approvals/index.js")),
    ("package.json", include_bytes!("../../../integrations/dsh-approvals/package.json")),
    ("cordis.patch.yml", include_bytes!("../../../integrations/dsh-approvals/cordis.patch.yml")),
];
static INSTALL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Agent { Codex, Dsh }

#[derive(Deserialize)]
struct Discovery { codex: bool, dsh: Option<PathBuf> }

#[derive(Serialize)]
pub(crate) struct Row {
    agent: Agent,
    state: &'static str,
    detail: String,
}

fn directory(variable: &str) -> Result<PathBuf, String> {
    env::var_os(variable).map(PathBuf::from).filter(|p| p.is_absolute())
        .ok_or_else(|| format!("无法定位 {variable} 目录"))
}

fn profile(home: &str, default_name: &str) -> Result<PathBuf, String> {
    if env::var_os(home).filter(|s| !s.is_empty()).is_some() { directory(home) }
    else { Ok(directory("USERPROFILE")?.join(default_name)) }
}

fn system_shell() -> Result<PathBuf, String> {
    Ok(directory("SystemRoot")?.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
}

fn background(exe: &Path) -> Command {
    let mut command = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    command.stdin(Stdio::null());
    command
}

fn discover() -> Result<Discovery, String> {
    let result = background(&system_shell()?)
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", DISCOVER])
        .output().map_err(|_| "无法启动 Windows 应用检测")?;
    if !result.status.success() { return Err("读取本机应用列表失败".into()); }
    serde_json::from_slice(&result.stdout).map_err(|_| "无法识别本机应用检测结果".into())
}

fn read(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(format!("无法读取 {}", path.display())),
    }
}

fn decode(bytes: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
        .map_err(|_| "现有接入配置无法解析，已保留原文件".into())
}

fn json_file(path: &Path) -> Result<Option<Value>, String> {
    read(path)?.as_deref().map(decode).transpose()
}

fn write_changed(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if read(path)?.as_deref() == Some(bytes) { return Ok(()); }
    let parent = path.parent().ok_or("接入目录无效")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建接入目录")?;
    write_file_atomic(path, bytes)
}

fn root() -> Result<PathBuf, String> {
    Ok(directory("LOCALAPPDATA")?.join("DesktopHub-Lite/Approvals"))
}

impl Agent {
    fn connection_file(self) -> &'static str {
        match self { Self::Codex => "codex.connected", Self::Dsh => "dsh.connected" }
    }
}

pub(super) fn remember_connection(app: &AppHandle, source: super::Source) {
    let agent = match source { super::Source::Codex => Agent::Codex, super::Source::DSH => Agent::Dsh };
    if root().and_then(|dir| write_changed(&dir.join(agent.connection_file()), b"1")).is_err() {
        crate::log_warn!("无法保存智能体的成功连接记录");
    }
    let _ = app.emit_to(SETTINGS, "approval-agent-connected", agent);
}

#[tauri::command]
pub(crate) fn connected_approval_agents(window: WebviewWindow) -> Result<Vec<Agent>, String> {
    if window.label() != SETTINGS { return Err("请在设置中查看智能体连接".into()); }
    let dir = root()?;
    let mut connected = Vec::new();
    for agent in [Agent::Codex, Agent::Dsh] {
        if read(&dir.join(agent.connection_file()))?.as_deref() == Some(b"1") {
            connected.push(agent);
        } else if matches!(agent, Agent::Dsh) {
            // 旧版只保存最后一次 DSH 结果。从已有的回执接续记录，不启动应用检测。
            let legacy = profile("DSH_HOME", ".dsh").and_then(|dir|
                json_file(&dir.join("profiles/desktop/node_modules").join(PLUGIN).join("runtime-status.json")));
            if let Ok(Some(value)) = legacy {
                if matches!(value["stage"].as_str(), Some("answered" | "question-answered")) {
                    remember_connection(window.app_handle(), super::Source::DSH);
                    connected.push(agent);
                }
            }
        }
    }
    Ok(connected)
}

fn codex_command() -> Result<String, String> {
    // 与已有登记脚本的命令一致，移动便携目录时无需更改受信任的钩子定义。
    let shell = system_shell()?.to_string_lossy().replace('\\', "/");
    let program = if shell.chars().any(char::is_whitespace) { format!("\"{shell}\"") } else { shell };
    Ok(format!("{program} -NoLogo -NoProfile -NonInteractive -Command \"& ([IO.Path]::Combine([Environment]::GetFolderPath('LocalApplicationData'),'DesktopHub-Lite','Approvals','codex.ps1'))\""))
}

fn has_hook(config: &Value, command: &str) -> Result<bool, String> {
    let obj = config.as_object().ok_or("现有 hooks.json 必须为对象")?;
    let Some(hooks) = obj.get("hooks") else { return Ok(false) };
    let hooks = hooks.as_object().ok_or("现有 hooks 节点格式无法识别")?;
    let Some(groups) = hooks.get("PermissionRequest") else { return Ok(false) };
    let groups = groups.as_array().ok_or("现有审批钩子列表格式无法识别")?;
    Ok(groups.iter().filter_map(|group| group["hooks"].as_array()).flatten()
        .any(|h| h["type"] == "command" && h["command"].as_str() == Some(command)))
}

fn points_here(path: Option<&str>, exe: &Path) -> bool {
    path.is_some_and(|path| Path::new(path.trim()) == exe)
}

fn codex_state(exe: &Path) -> Result<(&'static str, &'static str), String> {
    let config = json_file(&profile("CODEX_HOME", ".codex")?.join("hooks.json"))?.unwrap_or(json!({}));
    if !has_hook(&config, &codex_command()?)? { return Ok(("available", "已识别，可添加审批钩子")); }
    let folder = root()?;
    let target = read(&folder.join("executable.txt"))?;
    if !points_here(target.as_deref().and_then(|b| std::str::from_utf8(b).ok()), exe)
        || read(&folder.join("codex.ps1"))?.as_deref() != Some(CODEX_ADAPTER) {
        return Ok(("repair", "钩子已登记，需要关联当前便携版"));
    }
    Ok(("registered", "已登记。首次使用请在 Codex 的钩子设置中信任「等待灵动岛审批」。"))
}

fn install_codex(exe: &Path) -> Result<(), String> {
    let config_dir = profile("CODEX_HOME", ".codex")?;
    let path = config_dir.join("hooks.json");
    let original = read(&path)?;
    let mut config = original.as_deref().map(decode).transpose()?.unwrap_or(json!({}));
    let command = codex_command()?;
    let present = has_hook(&config, &command)?;
    let folder = root()?;
    write_changed(&folder.join("codex.ps1"), CODEX_ADAPTER)?;
    write_changed(&folder.join("executable.txt"), exe.to_string_lossy().as_bytes())?;
    if present { return Ok(()); }

    if config.get("hooks").is_none() { config["hooks"] = json!({}); }
    if config["hooks"].get("PermissionRequest").is_none() { config["hooks"]["PermissionRequest"] = json!([]); }
    let groups = config["hooks"]["PermissionRequest"].as_array_mut().ok_or("审批钩子列表格式错误")?;
    groups.push(json!({"matcher":"*", "hooks":[{"type":"command","command":command,"timeout":270,"statusMessage":"等待灵动岛审批"}]}));
    let bytes = serde_json::to_vec_pretty(&config).map_err(|_| "无法保存钩子设置")?;
    fs::create_dir_all(&config_dir).map_err(|_| "无法创建 Codex 配置目录")?;
    if let Some(previous) = &original {
        let backup = config_dir.join(format!("hooks.json.desktophub-{}.bak", uuid::Uuid::new_v4()));
        write_file_atomic(&backup, previous)?;
    }
    if read(&path)? != original { return Err("Codex 配置已变化，请重新检测后再添加".into()); }
    write_file_atomic(&path, &bytes)
}

fn dsh_cli(exe: &Path) -> Option<PathBuf> {
    let resources = exe.parent()?.join("resources");
    if !resources.join("runtime/cli/bin/dsh.cmd").is_file() || !resources.join("app.asar").is_file() { return None; }
    Some(resources.join("app.asar/dsh/node_modules/@deepseek-ai/dsh-desktop-host/lib/cli.js"))
}

fn dsh_runtime_detail(installed: &Path, registration: &str) -> &'static str {
    let status = match json_file(&installed.join("runtime-status.json")) {
        Ok(Some(status)) => status,
        Ok(None) => return "桌面配置已登记，尚未读到本次接入的加载记录。请完全退出并重新打开 DSH，再点「重新检测」。",
        Err(_) => return "已登记，暂时无法读取 DSH 插件状态。请点「重新检测」。",
    };
    if status["registration"].as_str() != Some(registration) {
        return "等待 DSH 加载本次接入。请完全退出并重新打开 DSH，再点「重新检测」。";
    }
    match status["stage"].as_str() {
        Some("loaded") => "DSH 已加载提问和审批监听。",
        Some("question-forwarding") => "最近一次：DSH 已向灵动岛转交提问。",
        Some("question-answered") => "最近一次：灵动岛已向 DSH 返回回答。",
        Some("question-delegated") => "最近一次：提问已交回 DSH 回答。",
        Some("stopped") => "最近一次：DSH 已卸载审批监听。请重新打开 DSH，再点「重新检测」。",
        Some("forwarding") => "最近一次：DSH 插件已开始向灵动岛转交审批。",
        Some("answered") => "最近一次：灵动岛已返回本次审批的决定。",
        Some("cancelled") => "最近一次：DSH 已撤回工具审批。",
        Some("unmatched") => "最近一次：无法唯一匹配待执行的工具操作，已交回 DSH。",
        Some("oversize") => "最近一次：操作详情过长，已交回 DSH 显示完整内容。",
        Some("delegated") => "最近一次：Lite 已交回 DSH。请确认当前便携版正在运行，且灵动岛可见。",
        Some("unreachable") => "最近一次：未收到 Lite 审批程序的有效应答，已交回 DSH。",
        _ => "已登记，DSH 插件状态未知。",
    }
}

fn dsh_state(dsh: &Path, exe: &Path) -> Result<(&'static str, &'static str), String> {
    if dsh_cli(dsh).is_none() { return Ok(("unsupported", "此版本缺少桌面插件安装入口")); }
    let dir = profile("DSH_HOME", ".dsh")?.join("profiles/desktop");
    let manifest = json_file(&dir.join("package.json"))?.unwrap_or(json!({}));
    if manifest["dependencies"].get(PLUGIN).is_none() { return Ok(("available", "已识别，可添加灵动岛审批插件")); }
    let installed = dir.join("node_modules").join(PLUGIN);
    let pointer = json_file(&installed.join("executable.json"))?;
    let correct = points_here(pointer.as_ref().and_then(|v| v["path"].as_str()), exe);
    if !correct { return Ok(("repair", "需要将审批插件关联到当前便携版")); }
    for &(name, bytes) in PLUGIN_FILES {
        if read(&installed.join(name))?.as_deref() != Some(bytes) {
            return Ok(("repair", "需要更新审批插件文件"));
        }
    }
    let active = manifest.pointer("/dsh/profile/bundles").and_then(Value::as_array)
        .is_some_and(|bundles| bundles.iter().any(|v| v.as_str() == Some(PLUGIN)));
    if !active { return Ok(("disabled", "插件已安装，请在 DSH 插件管理中启用")); }
    let Some(registration) = pointer.as_ref().and_then(|v| v["registration"].as_str()) else {
        return Ok(("repair", "需要更新本次接入的插件状态记录"));
    };
    Ok(("registered", dsh_runtime_detail(&installed, registration)))
}

fn install_dsh(dsh: &Path, exe: &Path) -> Result<(), String> {
    let cli = dsh_cli(dsh).ok_or("当前 DSH 版本缺少桌面插件安装入口")?;
    let plugin = root()?.join(PLUGIN);
    for &(name, bytes) in PLUGIN_FILES { write_changed(&plugin.join(name), bytes)?; }
    let pointer = serde_json::to_vec(&json!({"path":exe, "registration":uuid::Uuid::new_v4().to_string()}))
        .map_err(|_| "无法生成程序路径")?;
    write_changed(&plugin.join("executable.json"), &pointer)?;
    // 链接安装的插件已随便携目录更新，只在需要安装或修复时调用宿主。
    if matches!(dsh_state(dsh, exe)?.0, "registered" | "disabled") { return Ok(()); }
    let spec = format!("{PLUGIN}@link:{}", plugin.to_string_lossy().replace('\\', "/"));
    let result = background(dsh).env("ELECTRON_RUN_AS_NODE", "1")
        .arg("--expose-internals").arg(cli).args(["plugin", "--profile", "desktop", "add"]).arg(spec)
        .stdout(Stdio::null()).stderr(Stdio::null()).status().map_err(|_| "无法启动 DSH 插件安装程序")?;
    if !result.success() { return Err("DSH 安装失败，请在 DSH 插件管理中查看操作日志后重试".into()); }
    Ok(())
}

fn state(agent: Agent, found: &Discovery, exe: &Path) -> Row {
    let result = match agent {
        Agent::Codex if found.codex => codex_state(exe),
        Agent::Dsh if found.dsh.is_some() => dsh_state(found.dsh.as_deref().unwrap(), exe),
        _ => Ok(("missing", "未检测到此应用")),
    };
    let (state, detail) = match result {
        Ok((state, detail)) => (state, detail.to_owned()),
        Err(error) => ("unknown", error),
    };
    Row { agent, state, detail }
}

#[tauri::command]
pub(crate) async fn list_approval_agents(window: WebviewWindow) -> Result<Vec<Row>, String> {
    if window.label() != SETTINGS { return Err("请在设置中查看本机智能体".into()); }
    tauri::async_runtime::spawn_blocking(|| {
        let exe = env::current_exe().map_err(|_| "无法定位当前便携版")?;
        let found = discover()?;
        Ok([Agent::Codex, Agent::Dsh].into_iter().map(|agent| state(agent, &found, &exe)).collect())
    }).await.map_err(|_| "本机智能体检测中断")?
}

#[tauri::command]
pub(crate) async fn connect_approval_agent(window: WebviewWindow, agent: Agent) -> Result<Row, String> {
    if window.label() != SETTINGS { return Err("请在设置中添加审批钩子".into()); }
    let _guard = INSTALL.lock().await;
    tauri::async_runtime::spawn_blocking(move || {
        let exe = env::current_exe().map_err(|_| "无法定位当前便携版")?;
        let found = discover()?;
        match agent {
            Agent::Codex if found.codex => install_codex(&exe)?,
            Agent::Dsh => install_dsh(found.dsh.as_deref().ok_or("未检测到 DSH 桌面版")?, &exe)?,
            _ => return Err("未检测到 Codex".into()),
        }
        let row = state(agent, &found, &exe);
        if !matches!(row.state, "registered" | "disabled") {
            return Err(format!("接入尚未完成：{}", row.detail));
        }
        Ok(row)
    }).await.map_err(|_| "审批钩子接入中断")?
}
