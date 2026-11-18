use dom_query::Document;
use mailparse::{DispositionType, MailHeaderMap, ParsedMail};
use regex::Regex;
use reqwest::Url;
use serde::Serialize;
use std::sync::OnceLock;

#[derive(Clone, Serialize)]
pub(super) struct Link {
    label: String,
    domain: String,
    #[serde(skip)]
    pub url: String,
}

#[derive(Clone, Default, Serialize)]
pub(super) struct Content {
    sender: String,
    subject: String,
    pub code: Option<String>,
    pub links: Vec<Link>,
}

fn short(text: &str, limit: usize) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(limit).collect()
}

pub(super) fn web_url(raw: &str) -> Option<Url> {
    let raw = raw.trim();
    if raw.len() > 8192 || raw.contains('\\') || raw.chars().any(char::is_control) {
        return None;
    }
    let (scheme, _) = raw.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("https") && !scheme.eq_ignore_ascii_case("http") {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    (url.host().is_some() && url.username().is_empty() && url.password().is_none()).then_some(url)
}

const CODE_HINT: &str = r"(?i:验证码|校验码|认证码|动态码|动态密码|一次性密码|安全码|登录代码|verification\s+code|security\s+code|(?:login|sign[- ]in|launch|confirmation|authentication)\s+code|one[- ]time\s+(?:code|password|passcode)|(?:Google|ChatGPT|OpenAI)\s+code|passcode|\bOTP\b)";

fn code_in(text: &str) -> Option<String> {
    static PATTERNS: OnceLock<[Regex; 2]> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        let token = r"(?-u:\b)([a-zA-Z0-9]{4,8})(?-u:\b)";
        [
            Regex::new(&format!(r"{CODE_HINT}[\s:：为是]*(?:(?i:is)\s*)?[\s:：-]*{token}")).unwrap(),
            Regex::new(&format!(r"{token}\s*(?:(?i:is\s+(?:your|the))|是您的|是你的|为您的)\s*{CODE_HINT}")).unwrap(),
        ]
    });
    patterns.iter().flat_map(|re| re.captures_iter(text))
        .filter_map(|c| c.get(1))
        .map(|m| m.as_str())
        .find(|s| s.bytes().any(|b| b.is_ascii_digit()))
        .map(str::to_string)
}

fn body_code(subject: &str, text: &str) -> Option<String> {
    if let Some(code) = code_in(text) { return Some(code); }
    static HINT: OnceLock<Regex> = OnceLock::new();
    let hint = HINT.get_or_init(|| Regex::new(CODE_HINT).unwrap());
    if !hint.is_match(subject) && !hint.is_match(text) { return None; }
    // 常见模板把验证码单独放在粗体段落或表格。多个互异候选不猜选。
    let mut tokens = text.lines().map(str::trim).filter(|s| {
        (4..=8).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric())
            && s.bytes().any(|b| b.is_ascii_digit())
    });
    let first = tokens.next()?;
    tokens.all(|s| s == first).then(|| first.to_owned())
}

fn add_link(links: &mut Vec<Link>, raw: &str, label: &str) {
    let Some(url) = web_url(raw) else { return };
    if let Some(link) = links.iter_mut().find(|link| link.url == url.as_str()) {
        if link.label == "网页链接" && !label.is_empty() && web_url(label).is_none() {
            link.label = short(label, 64);
        }
        return;
    }
    if links.len() >= 24 { return; }
    let label = if label.trim().is_empty() || web_url(label).is_some() { "网页链接" } else { label };
    links.push(Link {
        label: short(label, 64),
        domain: url.host_str().unwrap_or_default().to_owned(),
        url: url.to_string(),
    });
}

fn scan_text(content: &mut Content, text: &str) {
    if content.code.is_none() {
        content.code = body_code(&content.subject, text);
    }
    static URLS: OnceLock<Regex> = OnceLock::new();
    let urls = URLS.get_or_init(|| Regex::new(r#"(?i)https?://[^\s<>"'，。；【】]+"#).unwrap());
    for m in urls.find_iter(text) {
        let mut raw = m.as_str().trim_end_matches(['.', ',', ';', '!', '）', '】']);
        if raw.ends_with(')') && raw.matches(')').count() > raw.matches('(').count() {
            raw = raw.trim_end_matches(')');
        }
        add_link(&mut content.links, raw, "");
    }
}

fn scan_part(content: &mut Content, part: &ParsedMail<'_>, complete: bool) {
    let disposition = part.get_content_disposition();
    if disposition.disposition == DispositionType::Attachment
        || disposition.params.contains_key("filename") || part.ctype.params.contains_key("name") {
        return;
    }
    if !part.subparts.is_empty() {
        for child in &part.subparts {
            let child_complete = complete || child.raw_bytes.as_ptr_range().end != part.raw_bytes.as_ptr_range().end;
            scan_part(content, child, child_complete);
        }
        return;
    }
    // 只使用完整的 MIME 部分，避免把截断的验证链接作为可点击地址。
    if !complete { return; }
    match part.ctype.mimetype.as_str() {
        "text/plain" => {
            if let Ok(text) = part.get_body() { scan_text(content, &text); }
        }
        "text/html" => {
            let Ok(html) = part.get_body() else { return };
            let doc = Document::from(html.as_str());
            doc.select("head, script, style, template, [hidden]").remove();
            for a in doc.select("a[href]").iter() {
                if let Some(href) = a.attr("href") { add_link(&mut content.links, &href, &a.text()); }
            }
            // 链接文字可能伪装成另一个地址。只提取真实 href，正文仍用于识别验证码。
            let visible = doc.formatted_text();
            if content.code.is_none() { content.code = body_code(&content.subject, &visible); }
            doc.select("a").remove();
            scan_text(content, &doc.formatted_text());
        }
        _ => {}
    }
}

pub(super) fn extract(bytes: &[u8], complete: bool) -> Content {
    let mut content = Content::default();
    if let Ok((headers, _)) = mailparse::parse_headers(bytes) {
        content.subject = short(&headers.get_first_value("Subject").unwrap_or_default(), 200);
        content.sender = short(&headers.get_first_value("From").unwrap_or_default(), 160);
        content.code = code_in(&content.subject);
    }
    if let Ok(mail) = mailparse::parse_mail(bytes) { scan_part(&mut content, &mail, complete); }
    content
}
