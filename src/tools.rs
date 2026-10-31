use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use rand::{distributions::Alphanumeric, Rng};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::db;
use crate::types::{AppState, AuthCtx, Prefs, Session, UserCtx};

pub const SID_COOKIE: &str = "sc_sid";
pub const USID_COOKIE: &str = "sc_uid";
pub const SESSION_TTL: i64 = 7 * 24 * 3600;
pub const MAX_ATTACHMENTS: usize = 8;
/// 附件原始数据总大小上限（18MB）
pub const MAX_TOTAL_BYTES: usize = 18 * 1024 * 1024;
/// 时间统一按北京时间（UTC+8）展示、解析
pub const TZ_OFFSET: i64 = 8 * 3600;

// ---------------------------------------------------------------------------
// 随机串 / ID / 哈希
// ---------------------------------------------------------------------------

pub fn rand_string(n: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

pub fn new_id(prefix: &str) -> String {
    format!("{}_{}", prefix, rand_string(12).to_lowercase())
}

pub fn new_auth_key(role: &str) -> String {
    let tag = match role {
        "admin" => "ADM",
        "teacher" => "TEA",
        _ => "STU",
    };
    format!("SC-{}-{}", tag, rand_string(8).to_uppercase())
}

pub fn new_class_code() -> String {
    format!("C{}", rand_string(5).to_uppercase())
}

pub fn hash_password(salt: &str, password: &str) -> String {
    let mut first = Sha256::new();
    first.update(salt.as_bytes());
    first.update(b":");
    first.update(password.as_bytes());
    let mut digest = first.finalize();
    // 简单的迭代哈希，避免单轮 SHA-256 过于轻量。
    for _ in 0..15_000 {
        let mut h = Sha256::new();
        h.update(digest);
        h.update(salt.as_bytes());
        digest = h.finalize();
    }
    let mut out = String::with_capacity(64);
    for b in digest {
        let _ = write!(out, "{:02x}", b);
    }
    out
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ---------------------------------------------------------------------------
// HTML / JS 转义
// ---------------------------------------------------------------------------

pub fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

pub fn js_json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v)
        .unwrap_or_else(|_| "null".to_string())
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace("@[", "\\u0040[")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

/// 账号用户名：2–32 个字符，只允许中英文、数字、下划线、短横线。
pub fn valid_username(name: &str) -> bool {
    let n = name.trim().chars().count();
    (2..=32).contains(&n)
        && name
            .trim()
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// 校验 auth / 账号的显示名称。
pub fn clean_display_name(raw: &str) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err("name must not be empty".to_string());
    }
    if name.chars().count() > 40 {
        return Err("name is too long (max 40 characters)".to_string());
    }
    if name.chars().any(char::is_control) {
        return Err("name must not contain control characters".to_string());
    }
    Ok(name.to_string())
}

pub fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let mut out: String = s.chars().take(n).collect();
    out.push('…');
    out
}

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            _ => {
                let _ = write!(out, "%{:02X}", b);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// base64 / 附件
// ---------------------------------------------------------------------------

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn b64_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_TABLE[((n >> 18) & 63) as usize] as char);
        out.push(B64_TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn is_base64(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let mut pad = 0usize;
    for (i, b) in bytes.iter().enumerate() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' => {
                if pad > 0 {
                    return false;
                }
                let _ = i;
            }
            b'=' => {
                pad += 1;
                if pad > 2 || i + 1 < bytes.len() && bytes[i + 1] != b'=' {
                    return false;
                }
            }
            _ => return false,
        }
    }
    if pad > 0 && !s.len().is_multiple_of(4) {
        return false;
    }
    true
}

pub fn b64_binary_len(s: &str) -> usize {
    let pad = s.bytes().rev().take_while(|b| *b == b'=').count();
    (s.len() * 3 / 4).saturating_sub(pad)
}

pub fn valid_mime(m: &str) -> bool {
    if m.len() > 120 {
        return false;
    }
    let mut parts = m.split('/');
    let (Some(a), Some(b), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    if a.is_empty() || b.is_empty() {
        return false;
    }
    m.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '+' | '-' | '.' | '_'))
}

/// 解析安全的 data URL（只允许 base64）
pub fn data_url_parts(s: &str) -> Option<(String, String)> {
    let rest = s.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?;
    if !valid_mime(mime) || !is_base64(data) {
        return None;
    }
    Some((mime.to_string(), data.to_string()))
}

#[derive(Clone, Debug)]
pub struct Attachment {
    pub name: String,
    pub mime: String,
    pub b64: String,
}

pub fn parse_attachment(v: &Value) -> Option<Attachment> {
    match v {
        Value::String(s) => {
            let (mime, b64) = data_url_parts(s)?;
            Some(Attachment {
                name: "Attachment".to_string(),
                mime,
                b64,
            })
        }
        Value::Object(map) => {
            let mime = map
                .get("mime")
                .or_else(|| map.get("type"))
                .and_then(Value::as_str)
                .unwrap_or("application/octet-stream");
            let b64 = map.get("data").and_then(Value::as_str)?;
            if !valid_mime(mime) || !is_base64(b64) {
                return None;
            }
            let name = map
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Attachment")
                .trim();
            let name: String = if name.is_empty() {
                "Attachment".to_string()
            } else {
                truncate_chars(name, 120)
            };
            Some(Attachment {
                name,
                mime: mime.to_string(),
                b64: b64.trim().to_string(),
            })
        }
        _ => None,
    }
}

/// `/api/upload` 的 data 数组归一化：
/// * `{"name","mime","data"}` -> 原样规范化
/// * `"data:image/png;base64,..."` -> 保留 data URL
/// * 裸 base64 -> 包装为二进制附件
pub fn normalize_data(v: Value) -> Result<Value, String> {
    let arr = match v {
        Value::Null => Vec::new(),
        Value::Array(arr) => arr,
        _ => return Err("data must be an array".to_string()),
    };
    if arr.len() > MAX_ATTACHMENTS {
        return Err(format!("too many attachments (max {MAX_ATTACHMENTS})"));
    }
    let mut out = Vec::new();
    let mut total = 0usize;
    for (idx, item) in arr.iter().enumerate() {
        match item {
            Value::String(raw) => {
                let raw = raw.trim();
                if raw.is_empty() {
                    continue;
                }
                if let Some((mime, data)) = data_url_parts(raw) {
                    total += b64_binary_len(&data);
                    if total > MAX_TOTAL_BYTES {
                        return Err("attachments exceed the size limit (about 18MB)".to_string());
                    }
                    out.push(Value::String(format!("data:{mime};base64,{data}")));
                } else if is_base64(raw) {
                    total += b64_binary_len(raw);
                    if total > MAX_TOTAL_BYTES {
                        return Err("attachments exceed the size limit (about 18MB)".to_string());
                    }
                    out.push(json!({
                        "name": format!("Attachment {}", idx + 1),
                        "mime": "application/octet-stream",
                        "data": raw
                    }));
                } else {
                    return Err("attachment must be base64 or a base64 data URL".to_string());
                }
            }
            Value::Object(map) => {
                let data = map
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "attachment object is missing the data field".to_string())?
                    .trim();
                if !is_base64(data) {
                    return Err("attachment data must be base64".to_string());
                }
                let mime = map
                    .get("mime")
                    .or_else(|| map.get("type"))
                    .and_then(Value::as_str)
                    .unwrap_or("application/octet-stream");
                if !valid_mime(mime) {
                    return Err("invalid attachment mime type".to_string());
                }
                let name = map
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("Attachment")
                    .trim();
                let name = if name.is_empty() {
                    format!("Attachment {}", idx + 1)
                } else {
                    truncate_chars(name, 120)
                };
                total += b64_binary_len(data);
                if total > MAX_TOTAL_BYTES {
                    return Err("attachments exceed the size limit (about 18MB)".to_string());
                }
                out.push(json!({ "name": name, "mime": mime, "data": data }));
            }
            _ => return Err("unsupported attachment format".to_string()),
        }
    }
    Ok(Value::Array(out))
}

pub fn format_size(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

// ---------------------------------------------------------------------------
// 时间（无 chrono 依赖，统一北京时间）
// ---------------------------------------------------------------------------

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

/// Howard Hinnant's civil date algorithms.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

fn local_parts(ts: i64) -> (i64, u32, u32, i64, i64, i64) {
    let t = ts + TZ_OFFSET;
    let days = t.div_euclid(86_400);
    let secs = t.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    (y, m, d, secs / 3600, (secs % 3600) / 60, secs % 60)
}

pub fn fmt_datetime(ts: i64) -> String {
    let (y, m, d, h, mi, _) = local_parts(ts);
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}")
}

pub fn fmt_date(ts: i64) -> String {
    let (y, m, d, _, _, _) = local_parts(ts);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn fmt_datetime_local(ts: i64) -> String {
    let (y, m, d, h, mi, _) = local_parts(ts);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}")
}

/// ISO-8601 风格，带 +08:00 时区
pub fn fmt_iso(ts: i64) -> String {
    let (y, m, d, h, mi, s) = local_parts(ts);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}+08:00")
}

fn parse_tz_offset(tz: &str) -> Option<i64> {
    let bytes = tz.as_bytes();
    if bytes.len() < 3 {
        return None;
    }
    let sign = match bytes[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: String = tz[1..].chars().filter(|c| *c != ':').collect();
    if digits.len() != 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let h: i64 = digits[0..2].parse().ok()?;
    let m: i64 = digits[2..4].parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    Some(sign * (h * 3600 + m * 60))
}

fn split_tz(s: &str) -> (&str, Option<i64>) {
    if let Some(t) = s.strip_suffix('Z').or_else(|| s.strip_suffix('z')) {
        return (t, Some(0));
    }
    if s.len() > 10 {
        let tail = &s[10..];
        if let Some(i) = tail.rfind('+').or_else(|| tail.rfind('-')) {
            let pos = 10 + i;
            if let Some(off) = parse_tz_offset(&s[pos..]) {
                return (&s[..pos], Some(off));
            }
        }
    }
    (s, None)
}

pub fn parse_time(input: &str) -> Option<i64> {
    let raw = input.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.chars().all(|c| c.is_ascii_digit()) {
        return raw.parse::<i64>().ok();
    }
    if !raw.is_ascii() {
        return None;
    }
    let (datetime, offset) = split_tz(raw);
    let (date_part, time_part) = match datetime.find(['T', ' ']) {
        Some(i) => (&datetime[..i], datetime[i + 1..].trim()),
        None => (datetime, ""),
    };
    let bits: Vec<&str> = date_part.split('-').collect();
    if bits.len() != 3 {
        return None;
    }
    let y: i64 = bits[0].parse().ok()?;
    let m: u32 = bits[1].parse().ok()?;
    let d: u32 = bits[2].parse().ok()?;
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    let (h, mi, sec) = if time_part.is_empty() {
        (0i64, 0i64, 0i64)
    } else {
        let parts: Vec<&str> = time_part.split(':').collect();
        if parts.len() < 2 || parts.len() > 3 {
            return None;
        }
        let h: i64 = parts[0].parse().ok()?;
        let mi: i64 = parts[1].parse().ok()?;
        let sec: i64 = if parts.len() == 3 {
            parts[2].split('.').next()?.parse().ok()?
        } else {
            0
        };
        if !(0..=23).contains(&h) || !(0..=59).contains(&mi) || !(0..=59).contains(&sec) {
            return None;
        }
        (h, mi, sec)
    };
    let local = days_from_civil(y, m, d) * 86_400 + h * 3600 + mi * 60 + sec;
    Some(local - offset.unwrap_or(TZ_OFFSET))
}

pub fn parse_duration(input: &str) -> Option<i64> {
    let s = input.trim().to_ascii_lowercase();
    if s.len() < 2 {
        return None;
    }
    if !s.is_ascii() {
        return None;
    }
    let unit = s.chars().last()?;
    let num = &s[..s.len() - unit.len_utf8()];
    if !num.chars().all(|c| c.is_ascii_digit()) || num.is_empty() {
        return None;
    }
    let n: i64 = num.parse().ok()?;
    let mult = match unit {
        's' => 1,
        'm' => 60,
        'h' => 3600,
        'd' => 86_400,
        'w' => 7 * 86_400,
        _ => return None,
    };
    n.checked_mul(mult)
}

/// 兼容 `Before(time)`：数字时间戳、ISO 时间、或者 7d/12h/30m 这样的相对时长。
pub fn parse_before(v: &Value) -> Result<i64, String> {
    match v {
        Value::Number(n) => n
            .as_i64()
            .ok_or_else(|| "invalid Before timestamp".to_string()),
        Value::String(s) => {
            if let Some(secs) = parse_duration(s) {
                Ok(now_ts() - secs)
            } else {
                parse_time(s).ok_or_else(|| "invalid Before time format".to_string())
            }
        }
        Value::Object(map) => {
            let v = map
                .get("time")
                .or_else(|| map.get("value"))
                .or_else(|| map.get("ts"))
                .ok_or_else(|| "Before requires a time field".to_string())?;
            parse_before(v)
        }
        _ => Err("invalid Before time format".to_string()),
    }
}

pub fn parse_deadline_value(v: Option<&Value>) -> Result<Option<i64>, String> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let n = n.as_i64().ok_or_else(|| "invalid deadLine".to_string())?;
            if n <= 0 {
                Ok(None)
            } else {
                Ok(Some(n))
            }
        }
        Some(Value::String(s)) => {
            let s = s.trim();
            if s.is_empty() {
                Ok(None)
            } else {
                parse_time(s).map(Some).ok_or_else(|| {
                    "invalid deadLine format (e.g. 2026-11-01T20:00 or 2026-11-01)".to_string()
                })
            }
        }
        Some(_) => Err("deadLine must be a time string or timestamp".to_string()),
    }
}

// ---------------------------------------------------------------------------
// 角色
// ---------------------------------------------------------------------------

pub fn role_label(role: &str) -> String {
    crate::strings::t(match role {
        "parent" => "role_parent",
        "teacher" => "role_teacher",
        "admin" => "role_admin",
        _ => "role_student",
    })
}

pub fn normalize_page_role(role: &str) -> Option<&'static str> {
    match role {
        "parent" => Some("parent"),
        "student" => Some("student"),
        "teacher" | "teachers" => Some("teacher"),
        "admin" => Some("admin"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// MatchMode
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum MatchMode {
    All,
    Random(usize),
    StudentId(String),
    ShortStudentId(i64),
    TitleRegex(String),
    Before(i64),
}

pub fn parse_match_mode(v: Option<&Value>) -> Result<MatchMode, String> {
    let Some(v) = v else {
        return Ok(MatchMode::All);
    };
    match v {
        Value::Null => Ok(MatchMode::All),
        Value::String(name) => variant_from(name, None),
        Value::Object(map) => {
            if let Some(name) = map
                .get("mode")
                .or_else(|| map.get("type"))
                .or_else(|| map.get("kind"))
                .and_then(Value::as_str)
            {
                variant_from(name, Some(map))
            } else if map.len() == 1 {
                let (name, value) = map.iter().next().unwrap();
                let mut single = Map::new();
                single.insert(name.clone(), value.clone());
                variant_from(name, Some(&single))
            } else {
                Err("invalid match_mode format".to_string())
            }
        }
        _ => Err("invalid match_mode format".to_string()),
    }
}

fn canonical_variant(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase()
}

fn payload_of<'a>(map: &'a Map<String, Value>, mode: &str) -> Option<&'a Value> {
    if let Some(v) = map.get(mode) {
        return Some(v);
    }
    let lower = mode.to_ascii_lowercase();
    for (k, v) in map {
        if canonical_variant(k) == canonical_variant(mode)
            || canonical_variant(k) == canonical_variant(&lower)
        {
            return Some(v);
        }
    }
    None
}

fn variant_from(name: &str, map: Option<&Map<String, Value>>) -> Result<MatchMode, String> {
    let key = canonical_variant(name);
    let payload = map.and_then(|m| payload_of(m, name));
    match key.as_str() {
        "all" => Ok(MatchMode::All),
        "random" => {
            let q = match payload {
                Some(Value::Number(n)) => n.as_i64(),
                Some(Value::Object(o)) => o
                    .get("quantity")
                    .or_else(|| o.get("count"))
                    .or_else(|| o.get("n"))
                    .and_then(Value::as_i64),
                Some(Value::String(s)) => s.parse::<i64>().ok(),
                _ => map.and_then(|m| {
                    m.get("quantity")
                        .or_else(|| m.get("count"))
                        .or_else(|| m.get("n"))
                        .and_then(Value::as_i64)
                }),
            }
            .ok_or_else(|| "Random requires a quantity".to_string())?;
            if q < 0 {
                return Err("Random quantity must not be negative".to_string());
            }
            Ok(MatchMode::Random(q.min(10_000) as usize))
        }
        "studentid" => {
            let id = match payload {
                Some(Value::String(s)) => s.trim().to_string(),
                Some(Value::Object(o)) => o
                    .get("id")
                    .or_else(|| o.get("studentID"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
                _ => map
                    .and_then(|m| m.get("id").or_else(|| m.get("studentID")))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            };
            if id.is_empty() {
                Err("StudentID requires an id".to_string())
            } else {
                Ok(MatchMode::StudentId(id))
            }
        }
        "shortstudentid" => {
            let id = match payload {
                Some(Value::Number(n)) => n.as_i64(),
                Some(Value::String(s)) => s.parse::<i64>().ok(),
                Some(Value::Object(o)) => {
                    o.get("id")
                        .or_else(|| o.get("shortStudentID"))
                        .and_then(|v| match v {
                            Value::Number(n) => n.as_i64(),
                            Value::String(s) => s.parse::<i64>().ok(),
                            _ => None,
                        })
                }
                _ => map
                    .and_then(|m| m.get("id").or_else(|| m.get("shortStudentID")))
                    .and_then(|v| match v {
                        Value::Number(n) => n.as_i64(),
                        Value::String(s) => s.parse::<i64>().ok(),
                        _ => None,
                    }),
            }
            .ok_or_else(|| "ShortStudentID requires an integer id".to_string())?;
            Ok(MatchMode::ShortStudentId(id))
        }
        "titleregex" => {
            let re = match payload {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Object(o)) => o
                    .get("re")
                    .or_else(|| o.get("regex"))
                    .or_else(|| o.get("pattern"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                _ => map
                    .and_then(|m| {
                        m.get("re")
                            .or_else(|| m.get("regex"))
                            .or_else(|| m.get("pattern"))
                    })
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            };
            if re.is_empty() {
                Err("TitleReGex requires a regular expression".to_string())
            } else {
                Ok(MatchMode::TitleRegex(re))
            }
        }
        "before" => {
            let time = match payload {
                Some(v) => v,
                None => map
                    .and_then(|m| {
                        m.get("time")
                            .or_else(|| m.get("value"))
                            .or_else(|| m.get("ts"))
                    })
                    .ok_or_else(|| "Before requires a time".to_string())?,
            };
            Ok(MatchMode::Before(parse_before(time)?))
        }
        _ => Err(format!("unknown match_mode: {name}")),
    }
}

// ---------------------------------------------------------------------------
// Cookie / 会话
// ---------------------------------------------------------------------------

pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in raw.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            if k == name && !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

pub fn session_cookie(name: &str, value: &str) -> String {
    format!("{name}={value}; HttpOnly; Path=/; SameSite=Lax; Max-Age={SESSION_TTL}")
}

pub fn clear_cookie(name: &str) -> String {
    format!("{name}=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0")
}

pub fn append_cookie(res: &mut Response, cookie: String) {
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        res.headers_mut().append(header::SET_COOKIE, v);
    }
}

pub fn create_role_session(st: &AppState, auth: &AuthCtx, page_role: &str, mode: &str) -> String {
    let token = rand_string(40);
    let session = Session {
        auth_id: auth.id.clone(),
        page_role: page_role.to_string(),
        mode: mode.to_string(),
        created_at: now_ts(),
    };
    st.sessions.lock().unwrap().insert(token.clone(), session);
    token
}

pub fn create_user_session(st: &AppState, user_id: i64) -> String {
    let token = rand_string(40);
    st.user_sessions
        .lock()
        .unwrap()
        .insert(token.clone(), user_id);
    token
}

pub fn role_session(st: &AppState, headers: &HeaderMap) -> Option<(Session, AuthCtx)> {
    let token = cookie_value(headers, SID_COOKIE)?;
    let session = {
        let map = st.sessions.lock().unwrap();
        map.get(&token).cloned()
    }?;
    if now_ts() - session.created_at > SESSION_TTL {
        st.sessions.lock().unwrap().remove(&token);
        return None;
    }
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_id(&db, &session.auth_id)
    };
    match auth {
        Some(auth) => Some((session, auth)),
        None => {
            st.sessions.lock().unwrap().remove(&token);
            None
        }
    }
}

pub fn user_session_id(st: &AppState, headers: &HeaderMap) -> Option<i64> {
    let token = cookie_value(headers, USID_COOKIE)?;
    let uid = {
        let map = st.user_sessions.lock().unwrap();
        map.get(&token).copied()
    }?;
    let exists = {
        let db = st.db.lock().unwrap();
        db::load_user_by_id(&db, uid).is_some()
    };
    if !exists {
        st.user_sessions.lock().unwrap().remove(&token);
        return None;
    }
    Some(uid)
}

pub fn current_user(st: &AppState, headers: &HeaderMap) -> Option<UserCtx> {
    // 账号会话（sc_uid）是唯一的 user 来源。auth 只代表班级身份，
    // 不能用来反查/伪造已登录账号。
    let uid = user_session_id(st, headers)?;
    let db = st.db.lock().unwrap();
    db::load_user_by_id(&db, uid)
}

pub fn user_prefs(st: &AppState, headers: &HeaderMap) -> Prefs {
    current_user(st, headers)
        .map(|u| u.prefs)
        .unwrap_or_default()
}

/// 更新当前角色会话的页面角色（学生 / 家长视角互换）。返回是否命中会话。
pub fn update_session_page_role(st: &AppState, headers: &HeaderMap, page_role: &str) -> bool {
    let Some(token) = cookie_value(headers, SID_COOKIE) else {
        return false;
    };
    let mut map = st.sessions.lock().unwrap();
    match map.get_mut(&token) {
        Some(session) => {
            session.page_role = page_role.to_string();
            true
        }
        None => false,
    }
}

/// 记住「学生视角 / 家长视角」偏好：纯前端展示偏好，只影响页面文案。
/// 未登录账号中心（没有 sc_uid）时静默跳过。
pub fn remember_view(st: &AppState, headers: &HeaderMap, view: &str) {
    let Some(user) = current_user(st, headers) else {
        return;
    };
    let mut prefs = user.prefs.clone();
    if prefs.view == view {
        return;
    }
    prefs.view = view.to_string();
    prefs.sanitize();
    let display = if user.display_name.trim().is_empty() {
        user.username.clone()
    } else {
        user.display_name.clone()
    };
    let db = st.db.lock().unwrap();
    if let Err(e) = db::update_user_prefs(&db, user.id, &display, &prefs) {
        eprintln!("remember view preference failed: {e}");
    }
}

pub fn invalidate_auth_sessions(st: &AppState, auth_id: &str) {
    st.sessions
        .lock()
        .unwrap()
        .retain(|_, s| s.auth_id != auth_id);
}

// ---------------------------------------------------------------------------
// API / 页面响应
// ---------------------------------------------------------------------------

pub fn api_ok(value: Value) -> Response {
    Json(value).into_response()
}

pub fn api_err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(json!({ "ok": false, "msg": msg.into() }))).into_response()
}

pub fn api_bad(msg: impl Into<String>) -> Response {
    api_err(StatusCode::BAD_REQUEST, msg)
}

pub fn api_unauthorized(msg: impl Into<String>) -> Response {
    api_err(StatusCode::UNAUTHORIZED, msg)
}

pub fn api_forbidden(msg: impl Into<String>) -> Response {
    api_err(StatusCode::FORBIDDEN, msg)
}

pub fn api_not_found(msg: impl Into<String>) -> Response {
    api_err(StatusCode::NOT_FOUND, msg)
}

pub fn plain_error(status: StatusCode, title: &str, msg: &str) -> Response {
    let html = format!(
        r#"<!doctype html><html lang="zh-CN"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>
<link rel="stylesheet" href="/static/app.css?v=5">
</head><body class="error-page"><div class="error-box"><h1>{}</h1><p>{}</p><a class="btn primary" href="/">@[strings/error_back_home]</a></div></body></html>"#,
        html_escape(title),
        html_escape(msg),
        title = html_escape(title)
    );
    (status, Html(crate::strings::apply(&html))).into_response()
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_html() {
        assert_eq!(
            html_escape("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn base64_round_trip_known_values() {
        assert_eq!(b64_encode(b""), "");
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_encode(b"hello"), "aGVsbG8=");
        assert!(is_base64("aGVsbG8="));
        assert!(!is_base64("not base64!"));
        assert_eq!(b64_binary_len("aGVsbG8="), 5);
    }

    #[test]
    fn time_parse_beijing() {
        // 2026-10-24 12:00 北京时间 = 04:00 UTC
        let ts = parse_time("2026-10-24T12:00").unwrap();
        assert_eq!(ts, 1_792_814_400);
        assert_eq!(fmt_datetime(ts), "2026-10-24 12:00");
        assert_eq!(fmt_datetime_local(ts), "2026-10-24T12:00");
        assert_eq!(fmt_iso(ts), "2026-10-24T12:00:00+08:00");
        assert_eq!(parse_time("2026-10-24T12:00:00+08:00"), Some(ts));
        assert_eq!(parse_time("2026-10-24T04:00:00Z"), Some(ts));
        assert_eq!(parse_time("2026-10-24"), Some(ts - 12 * 3600));
        assert!(parse_time("2026-02-30").is_none());
    }

    #[test]
    fn match_modes() {
        assert_eq!(parse_match_mode(None).unwrap(), MatchMode::All);
        assert_eq!(
            parse_match_mode(Some(&json!("All"))).unwrap(),
            MatchMode::All
        );
        assert_eq!(
            parse_match_mode(Some(&json!({"Random": 3}))).unwrap(),
            MatchMode::Random(3)
        );
        assert_eq!(
            parse_match_mode(Some(&json!({"type":"Random","quantity":2}))).unwrap(),
            MatchMode::Random(2)
        );
        assert_eq!(
            parse_match_mode(Some(&json!({"StudentID": "G2603-01"}))).unwrap(),
            MatchMode::StudentId("G2603-01".into())
        );
        assert_eq!(
            parse_match_mode(Some(&json!({"ShortStudentID": 2}))).unwrap(),
            MatchMode::ShortStudentId(2)
        );
        assert_eq!(
            parse_match_mode(Some(&json!({"TitleReGex": "^数学"}))).unwrap(),
            MatchMode::TitleRegex("^数学".into())
        );
        let t = parse_match_mode(Some(&json!({"Before": "2026-10-24T12:00"}))).unwrap();
        assert!(matches!(t, MatchMode::Before(_)));
    }

    #[test]
    fn normalize_attachment_data() {
        let v = json!([
            {"name":"a.txt","mime":"text/plain","data":"aGVsbG8="},
            "data:image/png;base64,aGVsbG8=",
            "aGVsbG8="
        ]);
        let out = normalize_data(v).unwrap();
        let arr = out.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[2]["mime"], "application/octet-stream");
        assert!(normalize_data(json!("not-array")).is_err());
        assert!(normalize_data(json!([{"data":"!!"}])).is_err());
    }

    #[test]
    fn display_name_validation() {
        assert_eq!(clean_display_name("  林老师  ").unwrap(), "林老师");
        assert!(clean_display_name("").is_err());
        assert!(clean_display_name("   ").is_err());
        assert!(clean_display_name("a\nb").is_err());
        assert!(clean_display_name(&"名".repeat(40)).is_ok());
        assert!(clean_display_name(&"名".repeat(41)).is_err());
    }

    #[test]
    fn url_encoding() {
        assert_eq!(
            url_encode("已提交 成功"),
            "%E5%B7%B2%E6%8F%90%E4%BA%A4%20%E6%88%90%E5%8A%9F"
        );
    }
}
