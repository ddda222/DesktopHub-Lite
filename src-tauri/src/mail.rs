//! QQ 和 Gmail 的个人收件提醒。磁盘只保留绑定信息和 IMAP 游标。
mod content;
mod inbox;

use crate::{clipboard_service, launcher, storage::Store, windows::{BAR, SETTINGS}, AppState};
use content::Content;
use inbox::Inbox;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, VecDeque},
    net::{Shutdown, TcpStream},
    sync::{Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};

const FILE: &str = "mail-accounts.json";
const RETENTION: Duration = Duration::from_secs(30 * 60);
const NO_SECRET: &str = "本机缺少该邮箱凭据，请重新绑定";

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Provider { Qq, Gmail }

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SetupPage { QqAccount, QqGuide, GoogleAppPasswords, GoogleTwoStep, GoogleGuide }

#[tauri::command]
pub(crate) fn open_mail_setup_page(window: WebviewWindow, page: SetupPage) -> Result<(), String> {
    require(&window, SETTINGS)?;
    launcher::launch(match page {
        SetupPage::QqAccount => "https://wx.mail.qq.com/account",
        SetupPage::QqGuide => "https://help.mail.qq.com/detail/106/985",
        SetupPage::GoogleAppPasswords => "https://myaccount.google.com/apppasswords",
        SetupPage::GoogleTwoStep => "https://myaccount.google.com/signinoptions/two-step-verification",
        SetupPage::GoogleGuide => "https://support.google.com/accounts/answer/185833?hl=zh-Hans",
    })
}

#[derive(Clone, Copy, Serialize, Deserialize)]
struct Cursor { validity: u32, last: u32 }

#[derive(Clone, Serialize, Deserialize)]
struct Binding { address: String, key: String, cursor: Cursor }

#[derive(Clone, Default, Serialize, Deserialize)]
struct Config {
    #[serde(deserialize_with = "read_bindings")]
    accounts: BTreeMap<Provider, Vec<Binding>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    retired: Vec<String>,
}

// 旧版每个服务商直接保存一个绑定。转成列表时保留凭据标识和收件游标。
fn read_bindings<'de, D: serde::Deserializer<'de>>(source: D) -> Result<BTreeMap<Provider, Vec<Binding>>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored { List(Vec<Binding>), Single(Binding) }
    let stored = BTreeMap::<Provider, Stored>::deserialize(source)?;
    Ok(stored.into_iter().map(|(provider, value)| {
        let entries = match value { Stored::List(entries) => entries, Stored::Single(entry) => vec![entry] };
        (provider, entries)
    }).collect())
}

impl Config {
    fn bindings(&self, provider: Provider) -> &[Binding] {
        self.accounts.get(&provider).map(Vec::as_slice).unwrap_or_default()
    }

    fn target(&self, provider: Provider, address: &str) -> Option<&Binding> {
        self.bindings(provider).iter().find(|b| provider == Provider::Qq || b.address.eq_ignore_ascii_case(address))
    }
}

#[derive(Clone, Serialize)]
pub(crate) struct Notice {
    id: String,
    provider: Provider,
    address: String,
    #[serde(flatten)]
    content: Content,
    #[serde(skip)]
    expires: Instant,
}

#[derive(Default)]
struct Runtime {
    config: Config,
    status: BTreeMap<String, String>,
    sockets: BTreeMap<String, TcpStream>,
    notices: VecDeque<Notice>,
    expiring: bool,
}

pub(crate) struct Mail { inner: Mutex<Runtime>, wake: Condvar }

impl Mail {
    pub fn load(store: &Store) -> Self {
        Self { inner: Mutex::new(Runtime { config: store.read(FILE).unwrap_or_default(), ..Runtime::default() }), wake: Condvar::new() }
    }
    fn lock(&self) -> MutexGuard<'_, Runtime> { self.inner.lock().unwrap_or_else(|e| e.into_inner()) }
}

fn require(window: &WebviewWindow, label: &str) -> Result<(), String> {
    if window.label() != label { return Err("此窗口无法操作邮箱".into()); }
    Ok(())
}

fn entry(key: &str) -> Result<keyring::Entry, &'static str> {
    keyring::Entry::new("DesktopHub.Lite.Mail", key).map_err(|_| "无法访问 Windows 凭据管理器")
}

fn erase(key: &str) -> Result<(), &'static str> {
    match entry(key)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("无法清理旧邮箱凭据"),
    }
}

fn persist(app: &AppHandle, r: &mut Runtime, next: Config) -> Result<(), String> {
    app.state::<AppState>().store.write(FILE, &next)?;
    r.config = next;
    Ok(())
}

// 先提交账号变更，再删除旧凭据。若两步之间退出，下次启动继续清理。
fn clean_retired(app: &AppHandle, r: &mut Runtime) {
    if r.config.retired.is_empty() { return; }
    let mut next = r.config.clone();
    next.retired.retain(|key| erase(key).is_err());
    if next.retired.len() != r.config.retired.len() { let _ = persist(app, r, next); }
}

fn changed(app: &AppHandle) { let _ = app.emit_to(BAR, "mail-notices-changed", ()); }
fn accounts_changed(app: &AppHandle) { let _ = app.emit_to(SETTINGS, "mail-accounts-changed", ()); }

fn current(r: &Runtime, provider: Provider, key: &str) -> bool {
    r.config.bindings(provider).iter().any(|b| b.key == key)
}

fn disconnect_runtime(r: &mut Runtime, provider: Provider, old: &Binding) {
    if let Some(socket) = r.sockets.remove(&old.key) { let _ = socket.shutdown(Shutdown::Both); }
    r.status.remove(&old.key);
    r.notices.retain(|n| n.provider != provider || !n.address.eq_ignore_ascii_case(&old.address));
}

#[derive(Serialize)]
pub(crate) struct Account { provider: Provider, address: String, status: String }

#[tauri::command]
pub(crate) fn mail_accounts(window: WebviewWindow, mail: State<'_, Mail>) -> Result<Vec<Account>, String> {
    require(&window, SETTINGS)?;
    let r = mail.lock();
    let mut accounts = Vec::new();
    for (&provider, bindings) in &r.config.accounts {
        for b in bindings {
            accounts.push(Account { provider, address: b.address.clone(),
                status: r.status.get(&b.key).cloned().unwrap_or_else(|| "正在连接".into()) });
        }
    }
    Ok(accounts)
}

#[tauri::command]
pub(crate) async fn bind_mail_account(window: WebviewWindow, app: AppHandle, provider: Provider, address: String, secret: String) -> Result<(), String> {
    require(&window, SETTINGS)?;
    tauri::async_runtime::spawn_blocking(move || bind(&app, provider, address, secret)).await.map_err(|_| "绑定任务中断")?
}

fn bind(app: &AppHandle, provider: Provider, address: String, secret: String) -> Result<(), String> {
    let address = address.trim().to_owned();
    if !address.contains('@') || address.len() > 320 || address.chars().any(char::is_whitespace) {
        return Err("请输入完整邮箱地址".into());
    }
    let secret = secret.trim().replace(' ', "");
    if secret.is_empty() || secret.len() > 256 || secret.chars().any(char::is_control) {
        return Err(String::from("请输入邮箱授权码或应用专用密码"));
    }
    let mail = app.state::<Mail>();
    let expected = mail.lock().config.target(provider, &address).map(|b| b.key.clone());
    let inbox = Inbox::connect(provider, &address, &secret)?;
    let mut r = mail.lock();
    let old = r.config.target(provider, &address).cloned();
    if old.as_ref().map(|b| &b.key) != expected.as_ref() { return Err("邮箱设置已变化，请重新绑定".into()); }
    let cursor = old.as_ref().filter(|b| b.address.eq_ignore_ascii_case(&address) && b.cursor.validity == inbox.baseline.validity)
        .map(|b| b.cursor).unwrap_or(inbox.baseline);
    let key = uuid::Uuid::new_v4().to_string();
    let binding = Binding { address, key: key.clone(), cursor };
    let mut next = r.config.clone();
    let bindings = next.accounts.entry(provider).or_default();
    if let Some(previous) = &old {
        let slot = bindings.iter_mut().find(|b| b.key == previous.key).ok_or("邮箱设置已变化，请重试")?;
        *slot = binding.clone();
        next.retired.push(previous.key.clone());
    } else {
        bindings.push(binding.clone());
    }
    entry(&key)?.set_password(&secret).map_err(|_| "无法保存邮箱凭据")?;
    if let Err(error) = persist(app, &mut r, next) {
        erase(&key)?;
        return Err(error);
    }
    if let Some(previous) = &old { disconnect_runtime(&mut r, provider, previous); }
    clean_retired(app, &mut r);
    drop(r);
    mail.wake.notify_all();
    changed(app);
    accounts_changed(app);
    spawn_worker(app.clone(), provider, binding, Some(inbox));
    Ok(())
}

#[tauri::command]
pub(crate) fn unbind_mail_account(window: WebviewWindow, app: AppHandle, provider: Provider, address: Option<String>) -> Result<(), String> {
    require(&window, SETTINGS)?;
    let mail = app.state::<Mail>();
    let mut r = mail.lock();
    let mut next = r.config.clone();
    let Some(bindings) = next.accounts.get_mut(&provider) else { return Ok(()) };
    // 兼容旧界面的单账号解绑，多账号时必须明确指定地址。
    let index = match address {
        Some(address) => bindings.iter().position(|b| b.address.eq_ignore_ascii_case(&address)),
        None if bindings.len() == 1 => Some(0),
        None => return Err("请选择要解绑的邮箱".into()),
    };
    let Some(index) = index else { return Ok(()) };
    let old = bindings.remove(index);
    if bindings.is_empty() { next.accounts.remove(&provider); }
    next.retired.push(old.key.clone());
    persist(&app, &mut r, next)?;
    disconnect_runtime(&mut r, provider, &old);
    clean_retired(&app, &mut r);
    let pending = r.config.retired.contains(&old.key);
    drop(r);
    mail.wake.notify_all();
    accounts_changed(&app);
    changed(&app);
    if pending { return Err("已断开收信，系统凭据清理将在下次启动时重试".into()); }
    Ok(())
}

#[tauri::command]
pub(crate) fn mail_notices(window: WebviewWindow, mail: State<'_, Mail>) -> Result<Vec<Notice>, String> {
    require(&window, BAR)?;
    Ok(mail.lock().notices.iter().rev().filter(|n| n.expires > Instant::now()).cloned().collect())
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Action { CopyCode, CopyLink, OpenLink, Dismiss }

#[tauri::command]
pub(crate) fn mail_action(window: WebviewWindow, app: AppHandle, id: String, action: Action, index: Option<usize>) -> Result<(), String> {
    require(&window, BAR)?;
    let mail = app.state::<Mail>();
    let mut r = mail.lock();
    let pos = r.notices.iter().position(|n| n.id == id && n.expires > Instant::now()).ok_or("这条邮件提醒已过期")?;
    let content = &r.notices[pos].content;
    match action {
        Action::Dismiss => { r.notices.remove(pos); drop(r); changed(&app); Ok(()) }
        Action::CopyCode => clipboard_service::copy_plain_text(&app, content.code.as_deref().ok_or("未识别到验证码")?),
        Action::OpenLink | Action::CopyLink => {
            let link = index.and_then(|i| content.links.get(i)).ok_or("链接已失效")?;
            let url = content::web_url(&link.url).ok_or("仅支持网页链接")?;
            if matches!(action, Action::OpenLink) { launcher::launch(url.as_str()) }
            else { clipboard_service::copy_plain_text(&app, url.as_str()) }
        }
    }
}

pub(crate) fn start(app: AppHandle) {
    let mail = app.state::<Mail>();
    let mut r = mail.lock();
    clean_retired(&app, &mut r);
    for (&provider, bindings) in &r.config.accounts {
        for b in bindings { spawn_worker(app.clone(), provider, b.clone(), None); }
    }
}

fn expire_notices(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            let next = {
                let mail = app.state::<Mail>();
                let mut r = mail.lock();
                let count = r.notices.len();
                r.notices.retain(|n| n.expires > Instant::now());
                if r.notices.len() != count { changed(&app); }
                let Some(n) = r.notices.front() else { r.expiring = false; return };
                n.expires.saturating_duration_since(Instant::now())
            };
            tokio::time::sleep(next).await;
        }
    });
}

fn commit(app: &AppHandle, provider: Provider, binding: &mut Binding, cursor: Cursor, content: Option<Content>) -> Result<(), &'static str> {
    let mail = app.state::<Mail>();
    let mut r = mail.lock();
    let mut next = r.config.clone();
    let stored = next.accounts.get_mut(&provider).and_then(|bindings|
        bindings.iter_mut().find(|b| b.key == binding.key)).ok_or("邮箱已解绑")?;
    stored.cursor = cursor;
    persist(app, &mut r, next).map_err(|_| "无法保存收件进度")?;
    binding.cursor = cursor;
    if let Some(content) = content {
        if r.notices.len() >= 32 { r.notices.pop_front(); }
        r.notices.push_back(Notice {
            id: format!("{}-{}-{}", binding.key, cursor.validity, cursor.last),
            provider, address: binding.address.clone(), content, expires: Instant::now() + RETENTION,
        });
        if !r.expiring { r.expiring = true; expire_notices(app.clone()); }
        drop(r);
        changed(app);
    }
    Ok(())
}

fn status(app: &AppHandle, provider: Provider, key: &str, value: &str) -> bool {
    let mail = app.state::<Mail>();
    let mut r = mail.lock();
    if !current(&r, provider, key) { return false; }
    if r.status.get(key).map(String::as_str) != Some(value) {
        r.status.insert(key.to_owned(), value.to_owned());
        drop(r);
        accounts_changed(app);
    }
    true
}

fn wait_current(app: &AppHandle, provider: Provider, key: &str, seconds: u64) -> bool {
    let mail = app.state::<Mail>();
    let (r, _) = mail.wake.wait_timeout_while(mail.lock(), Duration::from_secs(seconds), |r| current(r, provider, key))
        .unwrap_or_else(|e| e.into_inner());
    current(&r, provider, key)
}

fn synchronize(app: &AppHandle, provider: Provider, binding: &mut Binding, mut inbox: Inbox, use_idle: &mut bool, backoff: &mut u64) -> Result<(), &'static str> {
    let mail = app.state::<Mail>();
    {
        let mut r = mail.lock();
        if !current(&r, provider, &binding.key) { return Ok(()); }
        r.sockets.insert(binding.key.clone(), inbox.socket.try_clone().map_err(|_| "无法监听收信连接")?);
    }
    if binding.cursor.validity != inbox.baseline.validity {
        commit(app, provider, binding, inbox.baseline, None)?;
    }
    loop {
        if !current(&mail.lock(), provider, &binding.key) { return Ok(()); }
        let uids = inbox.new_uids(binding)?;
        for uid in uids {
            let content = inbox.get(uid)?;
            let cursor = Cursor { validity: binding.cursor.validity, last: uid };
            commit(app, provider, binding, cursor, content)?;
        }
        *backoff = 5;
        if inbox.idle && *use_idle {
            if !status(app, provider, &binding.key, "正在监听新邮件") { return Ok(()); }
            if let Err(e) = inbox.wait() {
                if e == inbox::IDLE_REFUSED { *use_idle = false; }
                return Err(e);
            }
        } else {
            if !status(app, provider, &binding.key, "每 30 秒检查新邮件") || !wait_current(app, provider, &binding.key, 30) { return Ok(()); }
        }
    }
}

fn spawn_worker(app: AppHandle, provider: Provider, mut binding: Binding, mut initial: Option<Inbox>) {
    std::thread::spawn(move || {
        let mut backoff = 5;
        let mut use_idle = true;
        loop {
            if !status(&app, provider, &binding.key, "正在连接") { break; }
            let connected = initial.take().map(Ok).unwrap_or_else(|| {
                let secret = entry(&binding.key)?.get_password().map_err(|e| match e {
                    keyring::Error::NoEntry => NO_SECRET, _ => "无法读取 Windows 邮箱凭据",
                })?;
                Inbox::connect(provider, &binding.address, &secret)
            });
            let result = connected.and_then(|inbox| synchronize(&app, provider, &mut binding, inbox, &mut use_idle, &mut backoff));
            let Err(error) = result else { break };
            let mail = app.state::<Mail>();
            {
                let mut r = mail.lock();
                if !current(&r, provider, &binding.key) { break; }
                r.sockets.remove(&binding.key);
            }
            if error == inbox::AUTH || error == NO_SECRET {
                status(&app, provider, &binding.key, error);
                break;
            }
            status(&app, provider, &binding.key, &format!("{error}，{backoff} 秒后重试"));
            if !wait_current(&app, provider, &binding.key, backoff) { break; }
            backoff = (backoff * 2).min(300);
        }
    });
}
