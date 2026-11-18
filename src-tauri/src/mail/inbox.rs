use super::{content, Binding, Cursor, Provider};
use imap::{extensions::idle::SetReadTimeout, Session};
use native_tls::{TlsConnector, TlsStream};
use std::{
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

const IO_TIMEOUT: Duration = Duration::from_secs(15);
pub(super) const AUTH: &str = "邮箱拒绝登录，请检查地址、第三方收信设置和专用密码";
pub(super) const IDLE_REFUSED: &str = "邮箱未接受实时监听";
const NETWORK: &str = "无法连接邮箱服务器，请检查网络";
const READ: &str = "收信连接中断";

// IDLE 结束时库会清除读取期限。恢复普通命令的期限，避免断网后一直卡在 DONE。
struct Transport(TlsStream<TcpStream>);
impl Read for Transport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> { self.0.read(buf) }
}
impl Write for Transport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> { self.0.write(buf) }
    fn flush(&mut self) -> io::Result<()> { self.0.flush() }
}
impl SetReadTimeout for Transport {
    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> imap::error::Result<()> {
        Ok(self.0.get_ref().set_read_timeout(Some(timeout.unwrap_or(IO_TIMEOUT)))?)
    }
}

pub(super) struct Inbox {
    session: Session<Transport>,
    pub socket: TcpStream,
    pub baseline: Cursor,
    pub idle: bool,
}

impl Inbox {
    pub fn connect(provider: Provider, address: &str, secret: &str) -> Result<Self, &'static str> {
        let host = match provider {
            Provider::Qq => "imap.qq.com",
            Provider::Gmail => "imap.gmail.com",
        };
        let mut addresses = (host, 993).to_socket_addrs().map_err(|_| NETWORK)?;
        let tcp = addresses.find_map(|addr| TcpStream::connect_timeout(&addr, IO_TIMEOUT).ok()).ok_or(NETWORK)?;
        tcp.set_read_timeout(Some(IO_TIMEOUT)).map_err(|_| NETWORK)?;
        tcp.set_write_timeout(Some(IO_TIMEOUT)).map_err(|_| NETWORK)?;
        let socket = tcp.try_clone().map_err(|_| NETWORK)?;
        let connector = TlsConnector::new().map_err(|_| "无法初始化系统证书验证")?;
        let tls = connector.connect(host, tcp).map_err(|_| "无法与邮箱建立可信的加密连接")?;
        let mut client = imap::Client::new(Transport(tls));
        client.read_greeting().map_err(|_| NETWORK)?;
        let mut session = client.login(address, secret).map_err(|(error, _)| match error {
            imap::Error::No(_) | imap::Error::Bad(_) => AUTH,
            _ => NETWORK,
        })?;
        let capabilities = session.capabilities().map_err(|_| "已登录邮箱，但未能读取收信服务能力，请重试")?;
        let idle = capabilities.has_str("IDLE");
        // ID 是可选扩展，当前 IMAP 库不能解析 QQ 返回的未标记 ID 应答。
        let mailbox = session.examine("INBOX").map_err(|_| "无法只读打开收件箱，请检查邮箱服务设置")?;
        let validity = mailbox.uid_validity.ok_or("邮箱未返回收件箱标识")?;
        let last = match mailbox.uid_next {
            Some(next) if next > 0 => next - 1,
            _ if mailbox.exists == 0 => 0,
            _ => session.fetch(mailbox.exists.to_string(), "UID").map_err(|_| "已打开收件箱，但未能读取最近一封邮件的编号，请重试")?
                .iter().find_map(|m| m.uid).ok_or("无法取得收件起点")?,
        };
        Ok(Self { session, socket, baseline: Cursor { validity, last }, idle })
    }

    pub fn new_uids(&mut self, binding: &Binding) -> Result<Vec<u32>, &'static str> {
        let last = binding.cursor.last;
        let Some(first) = last.checked_add(1) else { return Ok(Vec::new()) };
        let query = format!("UID {first}:*");
        let mut uids: Vec<_> = self.session.uid_search(query).map_err(|_| READ)?
            .into_iter().filter(|&uid| uid > last).collect();
        // IMAP 区间允许逆向，空增量时服务端也可能返回最后一封旧邮件。
        uids.sort_unstable();
        for _ in self.session.unsolicited_responses.try_iter() {}
        Ok(uids)
    }

    pub fn get(&mut self, uid: u32) -> Result<Option<content::Content>, &'static str> {
        // 限制单封下载量。有大附件的邮件仍可从前段提取标题、发件人和正文。
        let rows = self.session.uid_fetch(uid.to_string(), "(UID RFC822.SIZE BODY.PEEK[]<0.524288>)").map_err(|_| READ)?;
        let Some(row) = rows.iter().find(|row| row.uid == Some(uid)) else { return Ok(None) };
        let bytes = row.body().ok_or("邮箱未返回邮件内容")?;
        let complete = row.size.map_or(bytes.len() < 524288, |size| bytes.len() >= size as usize);
        Ok(Some(content::extract(bytes, complete)))
    }

    pub fn wait(&mut self) -> Result<(), &'static str> {
        let handle = self.session.idle().map_err(|e| match e {
            imap::Error::No(_) | imap::Error::Bad(_) => IDLE_REFUSED,
            _ => READ,
        })?;
        handle.wait_with_timeout(Duration::from_secs(90)).map(|_| ()).map_err(|_| READ)
    }
}
