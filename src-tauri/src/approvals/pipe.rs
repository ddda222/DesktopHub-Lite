use super::{enqueue, withdraw, Answer, Request, MAX_BYTES, WAIT};
use sha2::{Digest, Sha256};
use std::{io, os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle}, sync::Arc, time::Duration};
use tauri::AppHandle;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions},
    sync::Semaphore,
    time::timeout,
};
use windows::{
    core::PWSTR,
    Win32::{
        Foundation::{HANDLE, WAIT_TIMEOUT},
        Security::{GetTokenInformation, TokenStatistics, TOKEN_QUERY, TOKEN_STATISTICS},
        System::{
            Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
            Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, WaitForSingleObject,
                PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE},
        },
    },
};

// 两端都运行同一个便携 EXE，并用系统提供的进程身份核对路径与登录会话。
// 只知道管道名的进程无法充当灵动岛，向等待中的真实任务返回决定。
#[derive(PartialEq)]
struct Identity {
    image: Vec<u16>,
    logon: (i32, u32),
}

impl Identity {
    fn read(process: HANDLE) -> windows::core::Result<Self> {
        let mut image = vec![0u16; 32768];
        let mut len = image.len() as u32;
        let mut token = HANDLE::default();
        let mut stats = TOKEN_STATISTICS::default();
        unsafe {
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(image.as_mut_ptr()), &mut len)?;
            image.truncate(len as usize);
            OpenProcessToken(process, TOKEN_QUERY, &mut token)?;
            let _token = OwnedHandle::from_raw_handle(token.0);
            let mut used = 0;
            GetTokenInformation(token, TokenStatistics, Some(std::ptr::from_mut(&mut stats).cast()),
                std::mem::size_of_val(&stats) as u32, &mut used)?;
        }
        Ok(Self { image, logon: (stats.AuthenticationId.HighPart, stats.AuthenticationId.LowPart) })
    }

    fn current() -> io::Result<Self> {
        Self::read(unsafe { GetCurrentProcess() }).map_err(io::Error::other)
    }

    fn name(&self) -> String {
        let mut hash = Sha256::new();
        for ch in &self.image { hash.update(ch.to_le_bytes()); }
        hash.update(self.logon.0.to_le_bytes());
        hash.update(self.logon.1.to_le_bytes());
        format!(r"\\.\pipe\DesktopHub-approvals-{:x}", hash.finalize())
    }

    fn check_peer(&self, pipe: &impl AsRawHandle, server: bool) -> io::Result<()> {
        let handle = HANDLE(pipe.as_raw_handle());
        let mut pid = 0;
        let peer = unsafe {
            if server { GetNamedPipeServerProcessId(handle, &mut pid) }
            else { GetNamedPipeClientProcessId(handle, &mut pid) }
            .map_err(io::Error::other)?;
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(io::Error::other)?;
            let _process = OwnedHandle::from_raw_handle(process.0);
            Self::read(process).map_err(io::Error::other)?
        };
        if self != &peer { return Err(io::ErrorKind::PermissionDenied.into()); }
        Ok(())
    }
}

async fn connect(name: &str) -> io::Result<NamedPipeClient> {
    for attempt in 0..6 {
        match ClientOptions::new().open(name) {
            Err(error) if error.raw_os_error() == Some(231) && attempt < 5 => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            result => return result,
        }
    }
    Err(io::ErrorKind::TimedOut.into())
}

async fn supervisor_exit(pid: u32) -> io::Result<()> {
    let process = unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, false, pid).map_err(io::Error::other)?;
        OwnedHandle::from_raw_handle(handle.0)
    };
    while unsafe { WaitForSingleObject(HANDLE(process.as_raw_handle()), 0) } == WAIT_TIMEOUT {
        tokio::time::sleep(Duration::from_millis(350)).await;
    }
    Ok(())
}

pub(super) async fn ask(request: Request, supervisor: u32) -> io::Result<Answer> {
    let message = serde_json::to_vec(&request)?;
    if message.len() > MAX_BYTES { return Ok(Answer::Delegate); }
    let identity = Identity::current()?;
    let mut pipe = connect(&identity.name()).await?;
    identity.check_peer(&pipe, true)?;
    timeout(WAIT + Duration::from_secs(5), async {
        pipe.write_u32(message.len() as u32).await?;
        pipe.write_all(&message).await?;
        tokio::select! {
            biased;
            _ = supervisor_exit(supervisor) => Ok(Answer::Delegate),
            answer = async {
                let answer = match pipe.read_u8().await? {
                    b'A' => Answer::Allow,
                    b'D' => Answer::Deny,
                    b'Q' if request.kind == super::Kind::Questions => {
                        let size = pipe.read_u32().await? as usize;
                        if size == 0 || size > MAX_BYTES { return Err(io::ErrorKind::InvalidData.into()); }
                        let mut bytes = vec![0; size];
                        pipe.read_exact(&mut bytes).await?;
                        Answer::Answers(serde_json::from_slice(&bytes)?)
                    }
                    _ => Answer::Delegate,
                };
                // 已完整读到应答。记录连接状态失败不应改变交给智能体的决定。
                let _ = pipe.write_u8(b'K').await;
                Ok(answer)
            } => answer,
        }
    }).await?
}

async fn read_request(pipe: &mut NamedPipeServer) -> io::Result<Request> {
    let size = pipe.read_u32().await? as usize;
    if size == 0 || size > MAX_BYTES { return Err(io::ErrorKind::InvalidData.into()); }
    let mut bytes = vec![0; size];
    pipe.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn handle(app: AppHandle, mut pipe: NamedPipeServer) -> io::Result<()> {
    let request = timeout(Duration::from_secs(3), read_request(&mut pipe)).await??;
    let source = request.source;
    let kind = request.kind;
    let Some((id, reply)) = enqueue(&app, request) else { return Ok(()) };
    // 每个客户端只发一帧。取消、进程退出或额外输入均撤回这张卡。
    let answer = tokio::select! {
        biased;
        _ = pipe.read_u8() => None,
        result = timeout(WAIT, reply) => result.ok().and_then(Result::ok),
    };
    withdraw(&app, id);
    let handled = answer.as_ref().is_some_and(|answer| completed_on_island(kind, answer));
    match answer {
        Some(Answer::Answers(values)) => {
            let bytes = serde_json::to_vec(&values)?;
            pipe.write_u8(b'Q').await?;
            pipe.write_u32(bytes.len() as u32).await?;
            pipe.write_all(&bytes).await?;
        }
        other => pipe.write_u8(match other {
            Some(Answer::Allow) => b'A',
            Some(Answer::Deny) => b'D',
            _ => b'P',
        }).await?,
    }
    if matches!(timeout(Duration::from_secs(2), pipe.read_u8()).await, Ok(Ok(b'K'))) && handled {
        super::setup::remember_connection(&app, source);
    }
    Ok(())
}

fn completed_on_island(kind: super::Kind, answer: &Answer) -> bool {
    use super::Kind;
    matches!((kind, answer),
        (Kind::Approval, Answer::Allow | Answer::Deny) |
        (Kind::Questions, Answer::Answers(_)) |
        (Kind::Notice, Answer::Delegate)) // 用户在提醒上点了「知道了」。
}

#[cfg(test)]
mod tests {
    use super::{completed_on_island, Answer};
    use crate::approvals::Kind;

    #[test]
    fn completed_actions_include_denials_and_acknowledged_notices() {
        assert!(completed_on_island(Kind::Approval, &Answer::Allow));
        assert!(completed_on_island(Kind::Approval, &Answer::Deny));
        assert!(completed_on_island(Kind::Questions, &Answer::Answers(Vec::new())));
        assert!(completed_on_island(Kind::Notice, &Answer::Delegate));
    }

    #[test]
    fn handing_control_back_does_not_confirm_an_approval_or_answer() {
        assert!(!completed_on_island(Kind::Approval, &Answer::Delegate));
        assert!(!completed_on_island(Kind::Questions, &Answer::Delegate));
    }
}

pub(super) async fn serve(app: AppHandle) -> io::Result<()> {
    let identity = Identity::current()?;
    let name = identity.name();
    let mut options = ServerOptions::new();
    options.reject_remote_clients(true);
    let mut listener = options.first_pipe_instance(true).create(&name)?;
    options.first_pipe_instance(false);
    let capacity = Arc::new(Semaphore::new(40));
    loop {
        if let Err(error) = listener.connect().await {
            if matches!(error.raw_os_error(), Some(109 | 232 | 233)) {
                listener = options.create(&name)?;
                continue;
            }
            return Err(error);
        }
        let next = options.create(&name)?;
        let connected = std::mem::replace(&mut listener, next);
        let Ok(slot) = capacity.clone().try_acquire_owned() else { continue; };
        if identity.check_peer(&connected, false).is_err() { continue; }
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _slot = slot;
            let _ = handle(app, connected).await;
        });
    }
}
