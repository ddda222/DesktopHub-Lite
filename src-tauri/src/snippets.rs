//! 常用短语与轻量动态模板（Snippets），内置「账密」安全类别。
//!
//! 约束：
//! - 独立持久化在 `snippets.json`，先原子写盘再更新内存并广播，写盘失败绝不污染内存。
//! - 复制短语只写入系统剪贴板并发送 `"copied"` 状态反馈，绝不模拟粘贴或自动发送；系统剪贴板轮询已排除本程序自写序列号，因此复制密码不会回流进普通剪贴板历史。
//! - 「账密」条目的密码在 Windows 下通过系统 DPAPI 加密后落盘，列表广播结构绝不包含密码字段。
//! - 动态占位符采用单趟扫描替换，避免剪贴板文本中的 `{{...}}` 被二次展开，保留未知花括号语法；账号与密码不经模板展开。
//! - 严禁在任何日志或错误消息中输出短语正文、账号、密码或剪贴板正文。

use crate::AppState;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Manager, State};

pub const SNIPPETS_FILE: &str = "snippets.json";
pub const SNIPPETS_EVENT: &str = "snippets-changed";
pub const SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_GROUP: &str = "未分组";
pub const CREDENTIAL_GROUP: &str = "账密";
pub const MAX_SNIPPETS: usize = 1000;
pub const MAX_GROUPS: usize = 100;
pub const MAX_TITLE_CHARS: usize = 80;
pub const MAX_CONTENT_CHARS: usize = 20_000;
pub const MAX_ACCOUNT_CHARS: usize = 300;
pub const MAX_PASSWORD_CHARS: usize = 1000;
pub const MAX_CREDENTIAL_NOTE_CHARS: usize = 500;
pub const MAX_CLIPBOARD_INJECT_CHARS: usize = 50_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnippetKind {
    #[default]
    Text,
    Credential,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetEntry {
    pub id: String,
    #[serde(default)]
    pub kind: SnippetKind,
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// 磁盘密文（`dpapi:v1:<base64>`），绝不通过 `SnippetPayload` 下发给前端列表。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub group: String,
    #[serde(default)]
    pub pinned: bool,
    pub updated_at: i64,
}

/// 面向前端展示的安全视图，绝不包含密码明文或密文。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetView {
    pub id: String,
    pub kind: SnippetKind,
    pub title: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub has_password: bool,
    pub group: String,
    pub pinned: bool,
    pub updated_at: i64,
}

impl From<&SnippetEntry> for SnippetView {
    fn from(entry: &SnippetEntry) -> Self {
        Self {
            id: entry.id.clone(),
            kind: entry.kind,
            title: entry.title.clone(),
            content: entry.content.clone(),
            account: entry.account.clone(),
            has_password: entry.password.as_deref().is_some_and(|pw| !pw.is_empty()),
            group: entry.group.clone(),
            pinned: entry.pinned,
            updated_at: entry.updated_at,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetIndex {
    pub schema_version: u32,
    #[serde(default = "default_groups")]
    pub groups: Vec<String>,
    #[serde(default)]
    pub entries: Vec<SnippetEntry>,
}

impl Default for SnippetIndex {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            groups: default_groups(),
            entries: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetPayload {
    pub groups: Vec<String>,
    pub entries: Vec<SnippetView>,
}

impl From<&SnippetIndex> for SnippetPayload {
    fn from(index: &SnippetIndex) -> Self {
        Self {
            groups: index.groups.clone(),
            entries: index.entries.iter().map(SnippetView::from).collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnippetDraft {
    pub id: Option<String>,
    #[serde(default)]
    pub kind: SnippetKind,
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    pub group: String,
    #[serde(default)]
    pub pinned: bool,
}

fn default_groups() -> Vec<String> {
    vec![DEFAULT_GROUP.into(), CREDENTIAL_GROUP.into()]
}

#[cfg(windows)]
mod dpapi {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use std::ptr::{null, null_mut};

    const DPAPI_PREFIX: &str = "dpapi:v1:";
    const DPAPI_ENTROPY: &[u8] = b"DesktopHub.Lite.Snippets.Credential.v1";
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x01;

    #[repr(C)]
    struct CryptoApiBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            data_in: *const CryptoApiBlob,
            data_descr: *const u16,
            optional_entropy: *const CryptoApiBlob,
            reserved: *const core::ffi::c_void,
            prompt_struct: *const core::ffi::c_void,
            flags: u32,
            data_out: *mut CryptoApiBlob,
        ) -> i32;

        fn CryptUnprotectData(
            data_in: *const CryptoApiBlob,
            data_descr: *mut *mut u16,
            optional_entropy: *const CryptoApiBlob,
            reserved: *const core::ffi::c_void,
            prompt_struct: *const core::ffi::c_void,
            flags: u32,
            data_out: *mut CryptoApiBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(hmem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    }

    pub fn protect(plain: &str) -> Result<String, String> {
        let mut raw_bytes = plain.as_bytes().to_vec();
        let mut entropy_bytes = DPAPI_ENTROPY.to_vec();
        let data_in = CryptoApiBlob {
            cb_data: u32::try_from(raw_bytes.len()).map_err(|_| "密码过长，无法加密")?,
            pb_data: raw_bytes.as_mut_ptr(),
        };
        let entropy_in = CryptoApiBlob {
            cb_data: entropy_bytes.len() as u32,
            pb_data: entropy_bytes.as_mut_ptr(),
        };
        let mut data_out = CryptoApiBlob {
            cb_data: 0,
            pb_data: null_mut(),
        };

        let ok = unsafe {
            CryptProtectData(
                &data_in,
                null(),
                &entropy_in,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut data_out,
            )
        };
        raw_bytes.fill(0);

        if ok == 0 || data_out.pb_data.is_null() {
            return Err("无法在本机加密保存密码".into());
        }

        let encoded = unsafe {
            let slice = std::slice::from_raw_parts(data_out.pb_data, data_out.cb_data as usize);
            let b64 = STANDARD.encode(slice);
            LocalFree(data_out.pb_data.cast());
            b64
        };
        Ok(format!("{DPAPI_PREFIX}{encoded}"))
    }

    pub fn unprotect(stored: &str) -> Result<String, String> {
        let Some(encoded) = stored.strip_prefix(DPAPI_PREFIX) else {
            return Ok(stored.to_owned());
        };
        let mut cipher = STANDARD
            .decode(encoded.trim())
            .map_err(|_| "保存的密码格式损坏，请重新编辑保存".to_string())?;
        let mut entropy_bytes = DPAPI_ENTROPY.to_vec();
        let data_in = CryptoApiBlob {
            cb_data: u32::try_from(cipher.len()).map_err(|_| "保存的密码格式无效".to_string())?,
            pb_data: cipher.as_mut_ptr(),
        };
        let entropy_in = CryptoApiBlob {
            cb_data: entropy_bytes.len() as u32,
            pb_data: entropy_bytes.as_mut_ptr(),
        };
        let mut data_out = CryptoApiBlob {
            cb_data: 0,
            pb_data: null_mut(),
        };

        let ok = unsafe {
            CryptUnprotectData(
                &data_in,
                null_mut(),
                &entropy_in,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut data_out,
            )
        };
        if ok == 0 || data_out.pb_data.is_null() {
            return Err("本机无法解密这条密码（可能来自其他 Windows 账户），请重新编辑保存".into());
        }

        let mut plain_bytes = unsafe {
            let slice = std::slice::from_raw_parts(data_out.pb_data, data_out.cb_data as usize);
            let copied = slice.to_vec();
            std::ptr::write_bytes(data_out.pb_data, 0, data_out.cb_data as usize);
            LocalFree(data_out.pb_data.cast());
            copied
        };
        match String::from_utf8(std::mem::take(&mut plain_bytes)) {
            Ok(text) => Ok(text),
            Err(err) => {
                err.into_bytes().fill(0);
                Err("解密后的密码格式无效，请重新编辑保存".into())
            }
        }
    }
}

#[cfg(not(windows))]
mod dpapi {
    pub fn protect(plain: &str) -> Result<String, String> {
        Ok(plain.to_owned())
    }

    pub fn unprotect(stored: &str) -> Result<String, String> {
        Ok(stored.to_owned())
    }
}

pub fn protect_password(plain: &str) -> Result<String, String> {
    dpapi::protect(plain)
}

pub fn unprotect_password(stored: &str) -> Result<String, String> {
    dpapi::unprotect(stored)
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn next_id() -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "snip-{}-{:x}",
        now_millis(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn validate_id(id: &str) -> Result<String, String> {
    let trimmed = id.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > 128
        || trimmed.chars().any(char::is_control)
    {
        return Err("短语标识无效".into());
    }
    Ok(trimmed.into())
}

fn validate_group_name(group: &str) -> Result<String, String> {
    let trimmed = group.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_GROUP.into());
    }
    if trimmed == "全部"
        || trimmed.chars().count() > 40
        || trimmed.chars().any(char::is_control)
    {
        return Err("分组名需为 1–40 个可显示字符，且不能使用“全部”".into());
    }
    Ok(trimmed.into())
}

fn validate_title(title: &str) -> Result<String, String> {
    let normalized: String = title
        .trim()
        .chars()
        .map(|ch| if ch == '\r' || ch == '\n' || ch == '\t' { ' ' } else { ch })
        .collect();
    if normalized.trim().is_empty() {
        return Err("请填写短语标题".into());
    }
    if normalized.chars().count() > MAX_TITLE_CHARS || normalized.chars().any(char::is_control) {
        return Err(format!("短语标题需为 1–{MAX_TITLE_CHARS} 个可显示字符"));
    }
    Ok(normalized)
}

fn validate_content(content: &str) -> Result<String, String> {
    if content.trim().is_empty() {
        return Err("短语内容不能为空".into());
    }
    if content.contains('\0') {
        return Err("短语内容包含不支持的空字符".into());
    }
    if content.chars().count() > MAX_CONTENT_CHARS {
        return Err(format!("短语内容不能超过 {MAX_CONTENT_CHARS} 个字符"));
    }
    Ok(content.to_owned())
}

fn validate_account(account: Option<&str>) -> Result<String, String> {
    let normalized: String = account
        .unwrap_or_default()
        .trim()
        .chars()
        .map(|ch| if ch == '\r' || ch == '\n' || ch == '\t' { ' ' } else { ch })
        .collect();
    if normalized.trim().is_empty() {
        return Err("请填写账号".into());
    }
    if normalized.chars().count() > MAX_ACCOUNT_CHARS || normalized.chars().any(char::is_control) {
        return Err(format!("账号需为 1–{MAX_ACCOUNT_CHARS} 个可显示字符"));
    }
    Ok(normalized)
}

/// 校验密码（保留原始首尾空格与特殊字符，不进行 trim 或模板替换）。
fn validate_password(password: &str) -> Result<(), String> {
    if password.is_empty() {
        return Err("请填写密码".into());
    }
    if password.contains('\0') {
        return Err("密码包含不支持的空字符".into());
    }
    if password.chars().count() > MAX_PASSWORD_CHARS {
        return Err(format!("密码不能超过 {MAX_PASSWORD_CHARS} 个字符"));
    }
    Ok(())
}

fn validate_credential_note(note: &str) -> Result<String, String> {
    let trimmed = note.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.contains('\0') {
        return Err("备注包含不支持的空字符".into());
    }
    if trimmed.chars().count() > MAX_CREDENTIAL_NOTE_CHARS {
        return Err(format!("备注不能超过 {MAX_CREDENTIAL_NOTE_CHARS} 个字符"));
    }
    Ok(trimmed.to_owned())
}

/// 从多行正文中提取可读默认标题（取首个非空行前 28 个字符）。
pub fn suggest_title(content: &str, note: &str) -> String {
    let preferred = note.trim();
    let candidate = if !preferred.is_empty() {
        preferred
    } else {
        content
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("常用短语")
    };
    let clean: String = candidate
        .chars()
        .filter(|ch| !ch.is_control())
        .take(28)
        .collect();
    let trimmed = clean.trim();
    if trimmed.is_empty() {
        "常用短语".into()
    } else if candidate.chars().filter(|ch| !ch.is_control()).count() > 28 {
        format!("{trimmed}…")
    } else {
        trimmed.into()
    }
}

impl SnippetIndex {
    pub fn ensure_invariants(&mut self) {
        if !self.groups.iter().any(|group| group == DEFAULT_GROUP) {
            self.groups.insert(0, DEFAULT_GROUP.into());
        }
        if !self.groups.iter().any(|group| group == CREDENTIAL_GROUP) {
            let pos = self
                .groups
                .iter()
                .position(|group| group == DEFAULT_GROUP)
                .map_or(1, |idx| idx + 1)
                .min(self.groups.len());
            self.groups.insert(pos, CREDENTIAL_GROUP.into());
        }
        for entry in &mut self.entries {
            if entry.kind == SnippetKind::Credential {
                entry.group = CREDENTIAL_GROUP.into();
            } else if entry.group == CREDENTIAL_GROUP
                && entry
                    .account
                    .as_deref()
                    .is_some_and(|account| !account.trim().is_empty())
            {
                entry.kind = SnippetKind::Credential;
            }
        }
        self.normalize_order();
    }

    /// 保证置顶短语始终位于普通短语之前，同组内保留原有相对顺序。
    pub fn normalize_order(&mut self) {
        let mut pinned = Vec::with_capacity(self.entries.len());
        let mut ordinary = Vec::with_capacity(self.entries.len());
        for entry in self.entries.drain(..) {
            if entry.pinned {
                pinned.push(entry);
            } else {
                ordinary.push(entry);
            }
        }
        pinned.extend(ordinary);
        self.entries = pinned;
    }

    pub fn upsert(&mut self, draft: SnippetDraft, timestamp: i64) -> Result<(), String> {
        let title = validate_title(&draft.title)?;
        let requested_group = validate_group_name(&draft.group)?;
        let is_credential =
            draft.kind == SnippetKind::Credential || requested_group == CREDENTIAL_GROUP;
        let kind = if is_credential {
            SnippetKind::Credential
        } else {
            SnippetKind::Text
        };
        let group = if is_credential {
            CREDENTIAL_GROUP.to_string()
        } else {
            requested_group
        };

        let existing_id = draft
            .id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .map(validate_id)
            .transpose()?;

        let (content, account, encrypted_password) = if is_credential {
            let acc = validate_account(draft.account.as_deref())?;
            let note = validate_credential_note(&draft.content)?;
            let existing_pw = existing_id.as_deref().and_then(|target_id| {
                self.entries
                    .iter()
                    .find(|item| item.id == target_id)
                    .and_then(|item| item.password.clone())
                    .filter(|pw| !pw.is_empty())
            });
            let pw = match draft.password {
                Some(raw) => {
                    validate_password(&raw)?;
                    protect_password(&raw)?
                }
                None => existing_pw.ok_or_else(|| "请填写密码".to_string())?,
            };
            (note, Some(acc), Some(pw))
        } else {
            (validate_content(&draft.content)?, None, None)
        };

        if !self.groups.contains(&group) {
            if self.groups.len() >= MAX_GROUPS {
                return Err(format!("短语分组不能超过 {MAX_GROUPS} 个"));
            }
            self.groups.push(group.clone());
        }

        if let Some(ref id) = existing_id {
            if let Some(existing) = self.entries.iter_mut().find(|item| &item.id == id) {
                existing.kind = kind;
                existing.title = title;
                existing.content = content;
                existing.account = account;
                existing.password = encrypted_password;
                existing.group = group;
                existing.pinned = draft.pinned;
                existing.updated_at = timestamp;
                self.ensure_invariants();
                return Ok(());
            }
        }

        if self.entries.len() >= MAX_SNIPPETS {
            return Err(format!("最多保存 {MAX_SNIPPETS} 条常用短语"));
        }

        let id = existing_id.unwrap_or_else(next_id);
        self.entries.insert(
            0,
            SnippetEntry {
                id,
                kind,
                title,
                content,
                account,
                password: encrypted_password,
                group,
                pinned: draft.pinned,
                updated_at: timestamp,
            },
        );
        self.ensure_invariants();
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        let before = self.entries.len();
        self.entries.retain(|item| item.id != id);
        if self.entries.len() == before {
            return Err("这条短语已不存在".into());
        }
        Ok(())
    }

    pub fn reorder(&mut self, ids: &[String]) -> Result<(), String> {
        if ids.len() != self.entries.len() {
            return Err("短语列表已变化，请刷新后重试".into());
        }
        let unique: HashSet<&str> = ids.iter().map(String::as_str).collect();
        if unique.len() != self.entries.len() {
            return Err("排序包含重复短语标识".into());
        }
        let mut reordered = Vec::with_capacity(self.entries.len());
        for id in ids {
            let item = self
                .entries
                .iter()
                .find(|entry| &entry.id == id)
                .cloned()
                .ok_or_else(|| "排序包含不存在的短语".to_string())?;
            reordered.push(item);
        }
        self.entries = reordered;
        self.normalize_order();
        Ok(())
    }

    pub fn create_group(&mut self, group: &str) -> Result<(), String> {
        let name = validate_group_name(group)?;
        if !self.groups.contains(&name) {
            if self.groups.len() >= MAX_GROUPS {
                return Err(format!("短语分组不能超过 {MAX_GROUPS} 个"));
            }
            self.groups.push(name);
        }
        Ok(())
    }

    pub fn remove_group(&mut self, group: &str) -> Result<(), String> {
        let target = group.trim();
        if target == DEFAULT_GROUP || target == CREDENTIAL_GROUP {
            return Err("默认分组和账密类别不能删除".into());
        }
        self.groups.retain(|name| name != target);
        for entry in &mut self.entries {
            if entry.group == target {
                entry.group = DEFAULT_GROUP.into();
            }
        }
        self.ensure_invariants();
        Ok(())
    }
}

/// 本地日历时钟快照，便于无外部依赖格式化并做确定性测试。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalClock {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    /// 0 = 周日，1 = 周一，…，6 = 周六
    pub weekday: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

impl LocalClock {
    pub fn now() -> Self {
        #[cfg(windows)]
        {
            #[repr(C)]
            #[derive(Default)]
            struct WinSystemTime {
                year: u16,
                month: u16,
                day_of_week: u16,
                day: u16,
                hour: u16,
                minute: u16,
                second: u16,
                milliseconds: u16,
            }
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn GetLocalTime(time: *mut WinSystemTime);
            }
            let mut st = WinSystemTime::default();
            unsafe { GetLocalTime(&mut st) };
            Self {
                year: st.year,
                month: st.month,
                day: st.day,
                weekday: st.day_of_week,
                hour: st.hour,
                minute: st.minute,
                second: st.second,
            }
        }
        #[cfg(not(windows))]
        {
            let secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let days = secs.div_euclid(86_400);
            let sod = secs.rem_euclid(86_400) as u32;
            let z = days + 719_468;
            let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
            let doe = (z - era * 146_097) as u32;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
            let y = yoe as i64 + era * 400;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let year = (y + if m <= 2 { 1 } else { 0 }) as u16;
            let weekday = (days + 4).rem_euclid(7) as u16;
            Self {
                year,
                month: m as u16,
                day: d as u16,
                weekday,
                hour: (sod / 3600) as u16,
                minute: ((sod % 3600) / 60) as u16,
                second: (sod % 60) as u16,
            }
        }
    }

    fn date_str(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    fn time_str(self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }

    fn datetime_str(self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    fn weekday_str(self) -> &'static str {
        match self.weekday % 7 {
            0 => "周日",
            1 => "周一",
            2 => "周二",
            3 => "周三",
            4 => "周四",
            5 => "周五",
            _ => "周六",
        }
    }
}

/// 检测模板中是否包含 `{{clipboard}}`，避免无此占位符时无谓读取系统剪贴板。
pub fn template_uses_clipboard(template: &str) -> bool {
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let mut after_open = &rest[start + 2..];
        while let Some(stripped) = after_open.strip_prefix('{') {
            after_open = stripped;
        }
        let Some(end) = after_open.find("}}") else {
            break;
        };
        if after_open[..end].contains("{{") {
            rest = after_open;
            continue;
        }
        if after_open[..end].trim().eq_ignore_ascii_case("clipboard") {
            return true;
        }
        rest = &after_open[end + 2..];
    }
    false
}

/// 单趟扫描展开已知占位符，未知语法（如代码里的 `{{ item.name }}`）与替换结果本身均不二次解析。
pub fn expand_template(
    template: &str,
    clock: LocalClock,
    clipboard_text: Option<&str>,
) -> String {
    if !template.contains("{{") {
        return template.to_owned();
    }
    let clipped_board: String;
    let board = match clipboard_text {
        Some(raw) if raw.chars().count() > MAX_CLIPBOARD_INJECT_CHARS => {
            clipped_board = raw.chars().take(MAX_CLIPBOARD_INJECT_CHARS).collect();
            clipped_board.as_str()
        }
        Some(raw) => raw,
        None => "",
    };

    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let mut open_offset = start;
        let mut after_open = &rest[start + 2..];
        while let Some(stripped) = after_open.strip_prefix('{') {
            out.push('{');
            open_offset += 1;
            after_open = stripped;
        }
        let Some(end) = after_open.find("}}") else {
            out.push_str(&rest[open_offset..]);
            return out;
        };
        if after_open[..end].contains("{{") {
            out.push_str("{{");
            rest = after_open;
            continue;
        }
        let token = after_open[..end].trim();
        if token.eq_ignore_ascii_case("date") {
            out.push_str(&clock.date_str());
            rest = &after_open[end + 2..];
        } else if token.eq_ignore_ascii_case("time") {
            out.push_str(&clock.time_str());
            rest = &after_open[end + 2..];
        } else if token.eq_ignore_ascii_case("datetime") {
            out.push_str(&clock.datetime_str());
            rest = &after_open[end + 2..];
        } else if token.eq_ignore_ascii_case("weekday") {
            out.push_str(clock.weekday_str());
            rest = &after_open[end + 2..];
        } else if token.eq_ignore_ascii_case("clipboard") {
            out.push_str(board);
            rest = &after_open[end + 2..];
        } else {
            out.push_str(&rest[open_offset..open_offset + 2 + end + 2]);
            rest = &after_open[end + 2..];
        }
    }
    out.push_str(rest);
    out
}

fn mutate_snippets(
    app: &AppHandle,
    change: impl FnOnce(&mut SnippetIndex) -> Result<(), String>,
) -> Result<SnippetPayload, String> {
    let state = app.state::<AppState>();
    let mut guard = state.snippets.lock().unwrap_or_else(|e| e.into_inner());
    let mut next = guard.clone();
    change(&mut next)?;
    state.store.write(SNIPPETS_FILE, &next)?;
    *guard = next.clone();
    let payload = SnippetPayload::from(&next);
    drop(guard);
    let _ = app.emit(SNIPPETS_EVENT, &payload);
    Ok(payload)
}

#[tauri::command]
pub fn list_snippets(state: State<'_, AppState>) -> SnippetPayload {
    let guard = state.snippets.lock().unwrap_or_else(|e| e.into_inner());
    SnippetPayload::from(&*guard)
}

#[tauri::command]
pub fn save_snippet(app: AppHandle, snippet: SnippetDraft) -> Result<SnippetPayload, String> {
    let timestamp = now_millis();
    mutate_snippets(&app, |index| index.upsert(snippet, timestamp))
}

#[tauri::command]
pub fn delete_snippet(app: AppHandle, id: String) -> Result<SnippetPayload, String> {
    mutate_snippets(&app, |index| index.delete(&id))
}

#[tauri::command]
pub fn reorder_snippets(app: AppHandle, ids: Vec<String>) -> Result<SnippetPayload, String> {
    mutate_snippets(&app, |index| index.reorder(&ids))
}

#[tauri::command]
pub fn create_snippet_group(app: AppHandle, group: String) -> Result<SnippetPayload, String> {
    mutate_snippets(&app, |index| index.create_group(&group))
}

#[tauri::command]
pub fn remove_snippet_group(app: AppHandle, group: String) -> Result<SnippetPayload, String> {
    mutate_snippets(&app, |index| index.remove_group(&group))
}

#[tauri::command]
pub fn save_clip_as_snippet(
    app: AppHandle,
    state: State<'_, AppState>,
    clip_id: String,
    title: Option<String>,
    group: Option<String>,
) -> Result<SnippetPayload, String> {
    let (content, fallback_title, clip_group) = {
        let clip_index = state.clipboard.lock().unwrap_or_else(|e| e.into_inner());
        let entry = clip_index
            .entries
            .iter()
            .find(|item| item.id == clip_id)
            .ok_or("剪贴板记录已不存在")?;
        if entry.kind != "text" {
            return Err("只有文本记录可以存为常用短语".into());
        }
        let text = entry.text.as_deref().ok_or("文本记录内容为空")?;
        (
            text.to_owned(),
            suggest_title(text, &entry.note),
            entry.group.clone(),
        )
    };
    let final_title = match title.as_deref().map(str::trim) {
        Some(custom) if !custom.is_empty() => custom.to_owned(),
        _ => fallback_title,
    };
    let final_group = {
        let candidate = group
            .as_deref()
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .unwrap_or(&clip_group);
        if candidate == CREDENTIAL_GROUP {
            DEFAULT_GROUP.to_owned()
        } else {
            candidate.to_owned()
        }
    };
    let timestamp = now_millis();
    mutate_snippets(&app, |index| {
        index.upsert(
            SnippetDraft {
                id: None,
                kind: SnippetKind::Text,
                title: final_title,
                content,
                account: None,
                password: None,
                group: final_group,
                pinned: false,
            },
            timestamp,
        )
    })
}

/// 解析待复制文本；账密条目绝不走模板占位符展开，保持账号与密码原始字面量。
pub fn resolve_copy_text(
    entry: &SnippetEntry,
    raw: bool,
    field: Option<&str>,
    clock: LocalClock,
    read_board: impl FnOnce() -> Option<String>,
) -> Result<String, String> {
    if entry.kind == SnippetKind::Credential {
        return match field.unwrap_or("account") {
            "account" => entry
                .account
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| "这条账密尚未填写账号".to_string()),
            "password" => {
                let stored = entry
                    .password
                    .as_deref()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| "这条账密尚未设置密码".to_string())?;
                unprotect_password(stored)
            }
            _ => Err("不支持的复制字段".into()),
        };
    }

    if raw {
        return Ok(entry.content.clone());
    }
    let board = if template_uses_clipboard(&entry.content) {
        read_board()
    } else {
        None
    };
    Ok(expand_template(&entry.content, clock, board.as_deref()))
}

#[tauri::command]
pub fn copy_snippet(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    raw: Option<bool>,
    field: Option<String>,
) -> Result<(), String> {
    let entry = {
        let guard = state.snippets.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .entries
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or("这条常用短语已不存在")?
    };
    let final_text = resolve_copy_text(
        &entry,
        raw.unwrap_or(false),
        field.as_deref(),
        LocalClock::now(),
        crate::clipboard_service::read_current_text,
    )?;
    crate::clipboard_service::copy_plain_text(&app, &final_text)
}

#[tauri::command]
pub fn reveal_snippet_password(state: State<'_, AppState>, id: String) -> Result<String, String> {
    let stored = {
        let guard = state.snippets.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard
            .entries
            .iter()
            .find(|item| item.id == id)
            .ok_or("这条账密已不存在")?;
        if entry.kind != SnippetKind::Credential {
            return Err("只有账密条目支持查看密码".into());
        }
        entry
            .password
            .as_deref()
            .filter(|pw| !pw.is_empty())
            .map(str::to_owned)
            .ok_or("这条账密尚未设置密码")?
    };
    unprotect_password(&stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_draft(
        id: Option<&str>,
        title: &str,
        content: &str,
        group: &str,
        pinned: bool,
    ) -> SnippetDraft {
        SnippetDraft {
            id: id.map(str::to_owned),
            kind: SnippetKind::Text,
            title: title.into(),
            content: content.into(),
            account: None,
            password: None,
            group: group.into(),
            pinned,
        }
    }

    fn credential_draft(
        id: Option<&str>,
        title: &str,
        account: &str,
        password: Option<&str>,
        note: &str,
        pinned: bool,
    ) -> SnippetDraft {
        SnippetDraft {
            id: id.map(str::to_owned),
            kind: SnippetKind::Credential,
            title: title.into(),
            content: note.into(),
            account: Some(account.into()),
            password: password.map(str::to_owned),
            group: CREDENTIAL_GROUP.into(),
            pinned,
        }
    }

    fn sample_clock() -> LocalClock {
        LocalClock {
            year: 2026,
            month: 11,
            day: 18,
            weekday: 3,
            hour: 14,
            minute: 5,
            second: 9,
        }
    }

    #[test]
    fn expands_known_placeholders_in_single_pass_without_reexpanding_clipboard() {
        let clock = sample_clock();
        let injected_clip = "原剪贴板含 {{date}} 与 {{clipboard}} 字面量";
        let rendered = expand_template(
            "[{{date}} {{time}} {{weekday}}] 摘录：{{ clipboard }} | 完整：{{datetime}}",
            clock,
            Some(injected_clip),
        );
        assert_eq!(
            rendered,
            "[2026-11-18 14:05:09 周三] 摘录：原剪贴板含 {{date}} 与 {{clipboard}} 字面量 | 完整：2026-11-18 14:05:09"
        );
    }

    #[test]
    fn preserves_unknown_mustache_and_unclosed_braces() {
        let clock = sample_clock();
        let template = "const tpl = '{{ user.name }}'; // {{{date}}} {{unclosed";
        let rendered = expand_template(template, clock, None);
        assert_eq!(
            rendered,
            "const tpl = '{{ user.name }}'; // {2026-11-18} {{unclosed"
        );
        assert!(!template_uses_clipboard(template));
        assert!(template_uses_clipboard("引用：{{ clipboard }}"));
    }

    #[test]
    fn upsert_keeps_pinned_first_and_manages_groups() {
        let mut index = SnippetIndex::default();
        index
            .upsert(text_draft(Some("a"), "普通短语", "第一条内容", "工作", false), 100)
            .unwrap();
        index
            .upsert(text_draft(Some("b"), "置顶模板", "第二条 {{date}}", "代码", true), 200)
            .unwrap();
        assert_eq!(index.groups, vec!["未分组", "账密", "工作", "代码"]);
        assert_eq!(
            index.entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["b", "a"]
        );

        // 删除分组后，该组短语自动回到“未分组”，默认分组和账密类别不可删除
        assert!(index.remove_group("未分组").is_err());
        assert!(index.remove_group("账密").is_err());
        index.remove_group("工作").unwrap();
        assert_eq!(index.entries[1].group, "未分组");
    }

    #[test]
    fn reorder_preserves_pinned_partition_and_rejects_mismatch() {
        let mut index = SnippetIndex::default();
        for (id, pinned) in [("p1", true), ("p2", true), ("n1", false), ("n2", false)] {
            index
                .upsert(text_draft(Some(id), id, id, "未分组", pinned), 1)
                .unwrap();
        }
        index
            .reorder(&["n2".into(), "p2".into(), "n1".into(), "p1".into()])
            .unwrap();
        assert_eq!(
            index.entries.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            vec!["p2", "p1", "n2", "n1"]
        );
        assert!(index.reorder(&["p2".into()]).is_err());
    }

    #[test]
    fn validates_boundaries_and_suggests_clean_titles() {
        let mut index = SnippetIndex::default();
        assert!(index
            .upsert(text_draft(None, "   ", "正文", "未分组", false), 1)
            .is_err());
        assert!(index
            .upsert(text_draft(None, "标题", "   ", "全部", false), 1)
            .is_err());
        assert_eq!(
            suggest_title("\n  第一行很长的内容摘要用于生成默认标题测试超过二十八个字自动省略号  \n第二行", ""),
            "第一行很长的内容摘要用于生成默认标题测试超过二十八个字自…"
        );
        assert_eq!(suggest_title("正文", "已有备注"), "已有备注");
    }

    #[test]
    fn legacy_snippets_json_loads_and_adds_credential_group() {
        let legacy_json = r#"{
            "schemaVersion": 1,
            "groups": ["未分组", "工作"],
            "entries": [
                {
                    "id": "legacy-1",
                    "title": "旧版模板",
                    "content": "每日汇报 {{date}}",
                    "group": "工作",
                    "pinned": false,
                    "updatedAt": 123456
                }
            ]
        }"#;
        let mut index: SnippetIndex = serde_json::from_str(legacy_json).unwrap();
        index.ensure_invariants();
        assert_eq!(index.groups, vec!["未分组", "账密", "工作"]);
        assert_eq!(index.entries[0].kind, SnippetKind::Text);
        assert_eq!(index.entries[0].account, None);
        assert_eq!(index.entries[0].password, None);
    }

    #[test]
    fn credential_dpapi_roundtrip_upsert_copy_and_view_redaction() {
        let secret = "  P@ss#{{date}}中文空格!  ";
        let mut index = SnippetIndex::default();

        // 缺少密码或账号时拒绝新建
        assert!(index
            .upsert(
                credential_draft(Some("bad"), "缺密码", "user@test.com", None, "", false),
                1
            )
            .is_err());
        assert!(index
            .upsert(
                credential_draft(Some("bad2"), "缺账号", "   ", Some("abc"), "", false),
                1
            )
            .is_err());

        // 新建账密并验证 DPAPI 密文、列表脱敏、原样复制账号/密码（不展开 {{date}}、不裁剪空格）
        index
            .upsert(
                credential_draft(
                    Some("cred-1"),
                    "测试后台",
                    "admin@example.com",
                    Some(secret),
                    "联调环境",
                    true,
                ),
                100,
            )
            .unwrap();
        let entry = &index.entries[0];
        assert_eq!(entry.kind, SnippetKind::Credential);
        assert_eq!(entry.group, "账密");
        assert_eq!(entry.account.as_deref(), Some("admin@example.com"));
        let stored_cipher = entry.password.as_deref().unwrap().to_owned();
        assert!(stored_cipher.starts_with("dpapi:v1:"));
        assert!(!stored_cipher.contains("P@ss"));
        assert!(!stored_cipher.contains("中文空格"));

        let payload = SnippetPayload::from(&index);
        let payload_json = serde_json::to_string(&payload).unwrap();
        assert!(payload.entries[0].has_password);
        assert!(!payload_json.contains("dpapi:v1:"));
        assert!(!payload_json.contains("P@ss"));

        let clock = sample_clock();
        assert_eq!(
            resolve_copy_text(entry, false, None, clock, || None).unwrap(),
            "admin@example.com"
        );
        assert_eq!(
            resolve_copy_text(entry, false, Some("account"), clock, || None).unwrap(),
            "admin@example.com"
        );
        assert_eq!(
            resolve_copy_text(entry, false, Some("password"), clock, || None).unwrap(),
            secret
        );

        // 置顶/编辑不传 password 时保留原有密码密文
        index
            .upsert(
                credential_draft(
                    Some("cred-1"),
                    "测试后台",
                    "admin@example.com",
                    None,
                    "联调环境",
                    false,
                ),
                200,
            )
            .unwrap();
        assert_eq!(index.entries[0].password.as_deref(), Some(stored_cipher.as_str()));
        assert_eq!(
            unprotect_password(index.entries[0].password.as_deref().unwrap()).unwrap(),
            secret
        );

        // 切回普通短语时彻底清除原密码与账号字段
        index
            .upsert(text_draft(Some("cred-1"), "普通短语", "新正文", "未分组", false), 300)
            .unwrap();
        assert_eq!(index.entries[0].kind, SnippetKind::Text);
        assert_eq!(index.entries[0].account, None);
        assert_eq!(index.entries[0].password, None);
    }
}
