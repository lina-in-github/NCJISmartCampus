use std::collections::HashSet;
use std::fmt::Write as _;

use axum::response::IntoResponse;
use serde_json::{json, Value};

use crate::db;
use crate::tools::{
    b64_binary_len, fmt_date, fmt_datetime, fmt_datetime_local, format_size, html_escape, js_json,
    parse_attachment, role_label, truncate_chars,
};
use crate::types::{AuthCtx, Prefs, ReplyRow, Session, SubmissionRow, UploadRow, UserCtx};

// ---------------------------------------------------------------------------
// 图标
// ---------------------------------------------------------------------------

pub fn icon(name: &str) -> &'static str {
    match name {
        "home" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m3 10.5 9-7.5 9 7.5"/><path d="M5 9.5V21h14V9.5"/><path d="M9.5 21v-5.5h5V21"/></svg>"#
        }
        "inbox" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="4" width="17" height="16" rx="3"/><path d="M3.5 13h4l2 3h5l2-3h4"/></svg>"#
        }
        "folder" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2.3h7.5A2.5 2.5 0 0 1 21 9.8v8.7a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 18.5Z"/></svg>"#
        }
        "user" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="4"/><path d="M4.5 21c.6-4 3.8-6 7.5-6s6.9 2 7.5 6"/></svg>"#
        }
        "users" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="9" cy="8" r="3.2"/><path d="M2.8 20c.5-3.4 3-5.1 6.2-5.1S14.7 16.6 15.2 20"/><path d="M16 5.2a3 3 0 0 1 0 5.6"/><path d="M18 14.6c2.1.4 3.4 2.1 3.7 4.6"/></svg>"#
        }
        "plus" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>"#
        }
        "arrow-right" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4 12h15"/><path d="m13 5 7 7-7 7"/></svg>"#
        }
        "arrow-left" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M20 12H5"/><path d="m11 19-7-7 7-7"/></svg>"#
        }
        "chevron-right" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 5 7 7-7 7"/></svg>"#
        }
        "chevron-down" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m5 9 7 7 7-7"/></svg>"#
        }
        "clock" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="8.5"/><path d="M12 7.5V12l3 2"/></svg>"#
        }
        "calendar" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="5" width="17" height="15.5" rx="2.5"/><path d="M8 3v4M16 3v4M3.5 10h17"/></svg>"#
        }
        "file" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14 2.5H6.5A2.5 2.5 0 0 0 4 5v14a2.5 2.5 0 0 0 2.5 2.5h11A2.5 2.5 0 0 0 20 19V8.5Z"/><path d="M14 2.5v6h6"/></svg>"#
        }
        "image" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="4" width="17" height="16" rx="2.5"/><circle cx="8.5" cy="9" r="1.5"/><path d="m21 15-4.8-4.8L6.5 20"/></svg>"#
        }
        "paperclip" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m20.5 11.5-8.4 8.4a5.5 5.5 0 0 1-7.8-7.8l8.4-8.4a3.7 3.7 0 0 1 5.2 5.2l-8.4 8.4a1.9 1.9 0 0 1-2.6-2.6l7.7-7.7"/></svg>"#
        }
        "check" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.1" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>"#
        }
        "check-circle" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="m8 12 2.7 2.7L16.5 9"/></svg>"#
        }
        "reply" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m9 17-6-6 6-6"/><path d="M3 11h10.5A5.5 5.5 0 0 1 19 16.5V21"/></svg>"#
        }
        "send" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="m22 2-7 20-4-9-9-4Z"/><path d="M22 2 11 13"/></svg>"#
        }
        "trash" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16"/><path d="M9 7V4.5h6V7"/><path d="m6.5 7 1 13h9l1-13"/><path d="M10 11v5M14 11v5"/></svg>"#
        }
        "key" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="8" cy="15" r="4.5"/><path d="m11.5 11.5 8-8"/><path d="m16 7 2.5 2.5M18.5 4.5 21 7"/></svg>"#
        }
        "copy" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V5a2 2 0 0 1 2-2h8"/></svg>"#
        }
        "eye" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="3"/></svg>"#
        }
        "download" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3v12"/><path d="m7 10 5 5 5-5"/><path d="M4 20h16"/></svg>"#
        }
        "upload" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 16V4"/><path d="m7 9 5-5 5 5"/><path d="M4 20h16"/></svg>"#
        }
        "phone" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="6.5" y="2.5" width="11" height="19" rx="2.5"/><path d="M10.5 18.5h3"/></svg>"#
        }
        "desktop" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="2.5" y="4" width="19" height="13" rx="2"/><path d="M8 21h8M12 17v4"/></svg>"#
        }
        "logout" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M14 4h4a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-4"/><path d="M10 8l-4 4 4 4"/><path d="M6 12h11"/></svg>"#
        }
        "sun" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/></svg>"#
        }
        "moon" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5Z"/></svg>"#
        }
        "shield" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2.5 20 5v6.5c0 4.6-3.2 8.1-8 10-4.8-1.9-8-5.4-8-10V5Z"/><path d="m8.5 12 2.4 2.4 4.8-4.8"/></svg>"#
        }
        "book" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 4.5A2.5 2.5 0 0 1 6.5 2H20v17.5H6.5A2.5 2.5 0 0 0 4 22Z"/><path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20"/></svg>"#
        }
        "megaphone" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m3 11 14-6v14L3 13Z"/><path d="M3 11v2a2 2 0 0 0 2 2h1l2 5h2l-2-5"/><path d="M20 8a4 4 0 0 1 0 8"/></svg>"#
        }
        "sparkles" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3.5 13.8 9l5.7 1.8-5.7 1.8L12 18l-1.8-5.4L4.5 10.8 10.2 9Z"/><path d="M19 3v3M20.5 4.5h-3M5 17v3M6.5 18.5h-3"/></svg>"#
        }
        "alert" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M12 3 2.5 20h19Z"/><path d="M12 9v5M12 17.5h.01"/></svg>"#
        }
        "lock" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="4.5" y="10" width="15" height="11" rx="2.5"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/><path d="M12 14v3"/></svg>"#
        }
        "settings" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2.8 2.8-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.5V21h-4v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.9.3l-.1.1L4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9 1.7 1.7 0 0 0-1.5-1H3v-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.9L4.2 7 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3 1.7 1.7 0 0 0 1-1.5V3h4v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.9-.3l.1-.1L19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.5 1h.1v4h-.1a1.7 1.7 0 0 0-1.5 1Z"/></svg>"#
        }
        "graduation" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m2.5 8.5 9.5-4 9.5 4-9.5 4Z"/><path d="M6 10.7V16c0 1.7 2.7 3 6 3s6-1.3 6-3v-5.3"/><path d="M21.5 8.5V14"/></svg>"#
        }
        "list" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M8 6h13M8 12h13M8 18h13"/><path d="M3.5 6h.01M3.5 12h.01M3.5 18h.01"/></svg>"#
        }
        "grid" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.5"/></svg>"#
        }
        "x" => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12"/></svg>"#
        }
        _ => {
            r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M12 8v.01M12 11v5"/></svg>"#
        }
    }
}

// ---------------------------------------------------------------------------
// 通用
// ---------------------------------------------------------------------------

fn esc(s: &str) -> String {
    html_escape(s)
}

fn initial(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_else(|| crate::strings::t("avatar_fallback"))
}

fn truncate(s: &str, n: usize) -> String {
    esc(&truncate_chars(s, n))
}

fn relative_time(ts: i64, now: i64) -> String {
    let d = now - ts;
    if d < 0 {
        fmt_datetime(ts)
    } else if d < 60 {
        crate::strings::t("time_just_now")
    } else if d < 3600 {
        crate::strings::f1("time_minutes_ago", d / 60)
    } else if d < 86_400 {
        crate::strings::f1("time_hours_ago", d / 3600)
    } else if d < 7 * 86_400 {
        crate::strings::f1("time_days_ago", d / 86_400)
    } else {
        fmt_date(ts)
    }
}

fn deadline_chip(deadline: Option<i64>, now: i64) -> (String, &'static str) {
    let Some(ts) = deadline else {
        return (crate::strings::t("deadline_none"), "muted");
    };
    let d = ts - now;
    if d <= 0 {
        (crate::strings::t("deadline_expired"), "danger")
    } else if d < 3600 {
        (
            crate::strings::f1("deadline_minutes", (d / 60).max(1)),
            "danger",
        )
    } else if d < 86_400 {
        (crate::strings::f1("deadline_hours", d / 3600), "warn")
    } else {
        (crate::strings::f1("deadline_days", d / 86_400), "ok")
    }
}

fn type_label(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().as_str() {
        "homework" => crate::strings::t("type_homework"),
        "open call" | "open_call" | "opencall" => crate::strings::t("type_open_call"),
        "leave request" | "leave_request" | "leaverequest" => {
            crate::strings::t("type_leave_request")
        }
        "notice" => crate::strings::t("type_notice"),
        "other" => crate::strings::t("type_other"),
        "" => crate::strings::t("type_generic"),
        other => truncate_chars(other, 14),
    }
}

pub fn role_class(role: &str) -> &'static str {
    match role {
        "admin" => "admin",
        "teacher" => "teacher",
        _ => "student",
    }
}

pub fn avatar_class(role: &str) -> &'static str {
    match role {
        "admin" => " admin",
        "teacher" => " teacher",
        _ => "",
    }
}

fn deadline_badge(deadline: Option<i64>, now: i64) -> String {
    let (text, tone) = deadline_chip(deadline, now);
    format!(
        r#"<span class="chip {tone}">{icon}{text}</span>"#,
        icon = icon("clock"),
        text = esc(&text)
    )
}

fn notice_pin_badge(row: &SubmissionRow, now: i64) -> String {
    if row.is_pinned_notice(now) {
        format!(
            r#"<span class="chip pinned">{}{}</span>"#,
            icon("megaphone"),
            esc(&crate::strings::t("submission_notice_pinned"))
        )
    } else {
        String::new()
    }
}

fn upload_status(upload: &UploadRow) -> String {
    if upload.reply_count > 0 {
        format!(
            r#"<span class="chip ok">{}{}</span>"#,
            icon("check-circle"),
            crate::strings::f1("uploads_status_replies", upload.reply_count)
        )
    } else {
        format!(
            r#"<span class="chip warn">{}{}</span>"#,
            icon("clock"),
            "@[strings/uploads_status_pending]"
        )
    }
}

fn role_badge_text(role: &str) -> String {
    let cls = role_class(role);
    format!(
        r#"<span class="role-badge {cls}">{}</span>"#,
        esc(&role_label(role))
    )
}

// ---------------------------------------------------------------------------
// 布局
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
pub struct Flash {
    pub ok: Option<String>,
    pub err: Option<String>,
}

impl Flash {
    pub fn from_map(map: &std::collections::HashMap<String, String>) -> Self {
        fn clean(v: Option<&String>) -> Option<String> {
            v.map(|s| truncate_chars(s.trim(), 220))
                .filter(|s| !s.is_empty())
        }
        Self {
            ok: clean(map.get("ok")),
            err: clean(map.get("err")),
        }
    }
}

pub struct StatItem {
    pub label: String,
    pub value: String,
    pub icon: &'static str,
    pub tone: &'static str,
}

pub struct LayoutCtx {
    pub title: String,
    pub mode: String,
    pub page_role: String,
    pub role: String,
    pub active: String,
    pub class_name: String,
    pub class_desc: String,
    pub display_name: String,
    pub auth_key: String,
    pub auth_id: String,
    pub class_id: String,
    pub student_label: String,
    pub theme: String,
    pub is_account: bool,
    pub user_logged_in: bool,
    pub flash: Flash,
}

impl LayoutCtx {
    pub fn from_auth(
        auth: &AuthCtx,
        session: &Session,
        prefs: &Prefs,
        title: &str,
        active: &str,
        flash: Flash,
    ) -> Self {
        Self {
            title: title.to_string(),
            mode: session.mode.clone(),
            page_role: session.page_role.clone(),
            role: auth.role.clone(),
            active: active.to_string(),
            class_name: auth.class_name.clone(),
            class_desc: auth.class_desc.clone(),
            display_name: auth.identity(),
            auth_key: auth.key.clone(),
            auth_id: auth.id.clone(),
            class_id: auth.class_id.clone(),
            student_label: if auth.role == "student" {
                auth.student_no()
            } else {
                String::new()
            },
            theme: prefs.theme.clone(),
            is_account: false,
            user_logged_in: true,
            flash,
        }
    }

    pub fn account(user: &UserCtx, prefs: &Prefs, flash: Flash) -> Self {
        Self {
            title: "@[strings/role_account]".to_string(),
            mode: prefs.mode.clone(),
            page_role: "student".to_string(),
            role: "student".to_string(),
            active: "user".to_string(),
            class_name: "@[strings/common_personal_account]".to_string(),
            class_desc: "@[strings/common_account_desc]".to_string(),
            display_name: if user.display_name.trim().is_empty() {
                user.username.clone()
            } else {
                user.display_name.clone()
            },
            auth_key: String::new(),
            auth_id: String::new(),
            class_id: String::new(),
            student_label: String::new(),
            theme: prefs.theme.clone(),
            is_account: true,
            user_logged_in: true,
            flash,
        }
    }

    pub fn base(&self) -> String {
        if self.is_account {
            "/user".to_string()
        } else {
            format!("/home/{}/{}/", self.mode, self.page_role)
        }
    }

    fn user_label(&self) -> String {
        if self.is_account {
            crate::strings::t("role_account")
        } else {
            role_label(&self.page_role)
        }
    }
}

struct NavItem {
    key: &'static str,
    label: &'static str,
    href: String,
    icon: &'static str,
}

fn nav_items(ctx: &LayoutCtx) -> Vec<NavItem> {
    if ctx.is_account {
        let mut items = vec![NavItem {
            key: "user",
            label: "@[strings/role_account]",
            href: "/user".to_string(),
            icon: "user",
        }];
        if !ctx.auth_id.is_empty() {
            items.push(NavItem {
                key: "home",
                label: "@[strings/common_back_home]",
                href: "/home".to_string(),
                icon: "home",
            });
        }
        items.push(NavItem {
            key: "login",
            label: if ctx.auth_id.is_empty() {
                "@[strings/common_identity_login]"
            } else {
                "@[strings/common_switch_identity]"
            },
            href: "/login".to_string(),
            icon: "lock",
        });
        return items;
    }
    let base = ctx.base();
    let uploads_label = match ctx.page_role.as_str() {
        "parent" => "@[strings/nav_uploads_parent]",
        "student" => "@[strings/nav_uploads_self]",
        _ => "@[strings/nav_uploads_staff]",
    };
    let mut items = vec![
        NavItem {
            key: "home",
            label: "@[strings/nav_home]",
            href: base.clone(),
            icon: "home",
        },
        NavItem {
            key: "submissions",
            label: "@[strings/type_generic]",
            href: format!("{base}submissions/"),
            icon: "inbox",
        },
        NavItem {
            key: "uploads",
            label: uploads_label,
            href: format!("{base}uploads/"),
            icon: "folder",
        },
        NavItem {
            key: "user",
            label: "@[strings/nav_account]",
            href: "/user".to_string(),
            icon: "user",
        },
    ];
    if ctx.role == "teacher" || ctx.role == "admin" {
        items.push(NavItem {
            key: "submissions_new",
            label: "@[strings/nav_publish]",
            href: format!("{base}submissions/new"),
            icon: "plus",
        });
    }
    items
}

fn flash_html(flash: &Flash) -> String {
    let mut out = String::new();
    if let Some(ok) = &flash.ok {
        let _ = write!(
            out,
            r#"<div class="flash ok" role="status">{}{}<button type="button" class="flash-close" data-flash-close aria-label="@[strings/common_close]">×</button></div>"#,
            icon("check-circle"),
            esc(ok)
        );
    }
    if let Some(err) = &flash.err {
        let _ = write!(
            out,
            r#"<div class="flash err" role="alert">{}{}<button type="button" class="flash-close" data-flash-close aria-label="@[strings/common_close]">×</button></div>"#,
            icon("alert"),
            esc(err)
        );
    }
    out
}

fn status_bar(ctx: &LayoutCtx) -> String {
    if ctx.student_label.is_empty() {
        String::new()
    } else {
        format!(
            r#"<span class="status-line"><span class="status-dot"></span>{}</span>"#,
            esc(&ctx.student_label)
        )
    }
}

const THEME_BOOT: &str = r#"<script>(function(){try{var p=localStorage.getItem('sc-theme')||document.documentElement.getAttribute('data-pref')||'system';var d=window.matchMedia&&window.matchMedia('(prefers-color-scheme: dark)').matches;document.documentElement.dataset.theme=(p==='dark'||(p==='system'&&d))?'dark':'light';}catch(e){}})();</script>"#;

fn document_head(title: &str, theme: &str) -> String {
    format!(
        r##"<!doctype html>
<html lang="{lang}" data-pref="{theme}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="theme-color" content="#0010ff">
<title>{title} · @[strings/title]</title>
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
{theme_boot}
<link rel="stylesheet" href="/static/app.css?v=5">
</head>"##,
        theme = esc(theme),
        title = esc(title),
        lang = esc(&crate::strings::current_lang()),
        theme_boot = THEME_BOOT
    )
}

fn page_footer() -> String {
    format!(
        r#"<div class="toast-wrap" aria-live="polite"></div>
<dialog id="confirm-dialog" class="dialog">
  <form method="dialog" class="dialog-card">
    <h3 class="dialog-title" data-dialog-title>@[strings/common_confirm_title]</h3>
    <p class="dialog-msg" data-dialog-msg></p>
    <div class="dialog-actions">
      <button class="btn outline" value="cancel" formnovalidate>@[strings/common_cancel]</button>
      <button class="btn danger" value="ok" data-dialog-confirm formnovalidate>@[strings/common_confirm]</button>
    </div>
  </form>
</dialog>
<dialog id="rename-dialog" class="dialog">
  <form class="dialog-card" id="rename-form">
    <h3 class="dialog-title">@[strings/account_rename_title]</h3>
    <p class="dialog-msg">@[strings/account_rename_desc]</p>
    <div class="field">
      <label class="label" for="rename-input">@[strings/account_rename_label]</label>
      <input class="input" id="rename-input" type="text" maxlength="40" autocomplete="off" required>
      <span class="hint">@[strings/account_rename_hint]</span>
    </div>
    <div class="dialog-actions">
      <button class="btn outline" type="button" id="rename-cancel">@[strings/common_cancel]</button>
      <button class="btn primary" type="submit" id="rename-save">@[strings/common_save]</button>
    </div>
  </form>
</dialog>
<dialog id="lightbox" class="lightbox">
  <div class="lightbox-bar">
    <span data-lightbox-name>@[strings/common_attachment_preview]</span>
    <form method="dialog"><button class="icon-btn" value="close" aria-label="@[strings/common_close]">×</button></form>
  </div>
  <img alt="@[strings/common_attachment_preview]">
</dialog>
<script src="/static/strings.js?v=5"></script><script src="/static/app.js?v=5"></script>
</body>
</html>"#
    )
}

pub fn layout(ctx: &LayoutCtx, body: &str) -> String {
    let base = ctx.base();
    let nav = nav_items(ctx);
    let side_nav: String = nav
        .iter()
        .map(|item| {
            let active = if ctx.active == item.key
                || (item.key == "submissions" && ctx.active == "submission")
                || (item.key == "uploads" && ctx.active == "upload")
            {
                " active"
            } else {
                ""
            };
            format!(
                r#"<a class="nav-link{active}" href="{href}">{icon}<span>{label}</span></a>"#,
                active = active,
                href = esc(&item.href),
                icon = icon(item.icon),
                label = esc(item.label)
            )
        })
        .collect();

    let bottom_nav: String = nav
        .iter()
        .take(4)
        .map(|item| {
            let active = if ctx.active == item.key
                || (item.key == "submissions" && ctx.active == "submission")
                || (item.key == "uploads" && ctx.active == "upload")
            {
                " active"
            } else {
                ""
            };
            format!(
                r#"<a class="bottom-link{active}" href="{href}">{icon}<span>{label}</span></a>"#,
                active = active,
                href = esc(&item.href),
                icon = icon(item.icon),
                label = esc(item.label)
            )
        })
        .collect();

    let class_block = if ctx.is_account {
        format!(
            r#"<div class="side-class"><span class="eyebrow">@[strings/layout_account]</span><strong>{}</strong><span>{}</span></div>"#,
            esc(&ctx.class_name),
            esc(&ctx.class_desc)
        )
    } else {
        format!(
            r#"<div class="side-class"><span class="eyebrow">@[strings/layout_current_class]</span><strong>{}</strong><span>{} · {}</span>{}</div>"#,
            esc(&ctx.class_name),
            esc(&ctx.user_label()),
            if ctx.student_label.is_empty() {
                "@[strings/layout_class_space]"
            } else {
                "@[strings/layout_student_identity]"
            },
            if ctx.student_label.is_empty() {
                String::new()
            } else {
                format!(r#"<div class="side-status">{}</div>"#, status_bar(ctx))
            }
        )
    };

    let user_role_label = ctx.user_label();
    let sc = js_json(&json!({
        "auth": ctx.auth_key,
        "authID": ctx.auth_id,
        "classID": ctx.class_id,
        "className": ctx.class_name,
        "role": ctx.role,
        "pageRole": ctx.page_role,
        "mode": ctx.mode,
        "home": base,
        "displayName": ctx.display_name,
        "isAccount": ctx.is_account
    }));

    let mut out = document_head(&ctx.title, &ctx.theme);
    let _ = write!(
        out,
        r#"<body>
<div class="app-shell">
  <aside class="sidebar">
    <a class="brand" href="{home}" aria-label="@[strings/layout_brand_home]">
      <span class="brand-mark">{logo}</span>
      <span class="brand-text"><strong>@[strings/title]</strong><small>@[strings/brand_tagline]</small></span>
    </a>
    {class_block}
    <nav class="side-nav" aria-label="@[strings/layout_main_nav]">
      <span class="side-nav-label">@[strings/layout_space_nav]</span>
      {side_nav}
    </nav>
    <div class="side-foot">
      <a class="side-user" href="/user">
        <span class="avatar sm{avatar_class}">{initial}</span>
        <span class="who"><strong>{display}</strong><span>{role}</span></span>
      </a>
      <form method="post" action="/logout" class="inline-form">
        <button class="side-logout" type="submit" title="@[strings/common_logout]" aria-label="@[strings/common_logout]">{logout}</button>
      </form>
    </div>
  </aside>
  <div class="main">
    <header class="topbar">
      <div class="topbar-title">
        <span class="eyebrow">{class_name} · {role}</span>
        <h1>{title}</h1>
      </div>
      <div class="topbar-actions">
        <span class="role-chip"><span class="dot"></span>{role}</span>
        <a class="icon-btn" href="/user" title="@[strings/role_account]" aria-label="@[strings/role_account]">{user_icon}</a>
        <button class="icon-btn" id="theme-toggle" type="button" title="@[strings/common_theme_toggle]" aria-label="@[strings/common_theme_toggle]">
          <span class="icon-moon">{moon}</span><span class="icon-sun">{sun}</span>
        </button>
        <form method="post" action="/logout" class="inline-form desktop-only">
          <button class="icon-btn" type="submit" title="@[strings/common_logout]" aria-label="@[strings/common_logout]">{logout}</button>
        </form>
      </div>
    </header>
    <main class="content">
      {flash}
      {body}
    </main>
    <nav class="bottom-nav" aria-label="@[strings/layout_bottom_nav]">{bottom_nav}</nav>
  </div>
</div>
<script>window.SC = {sc};</script>
"#,
        home = esc(&base),
        logo = icon("graduation"),
        class_block = class_block,
        side_nav = side_nav,
        avatar_class = if ctx.is_account {
            ""
        } else {
            avatar_class(&ctx.role)
        },
        initial = esc(&initial(&ctx.display_name)),
        display = esc(&ctx.display_name),
        role = esc(&user_role_label),
        logout = icon("logout"),
        class_name = esc(&ctx.class_name),
        title = esc(&ctx.title),
        user_icon = icon("user"),
        moon = icon("moon"),
        sun = icon("sun"),
        flash = flash_html(&ctx.flash),
        body = body,
        bottom_nav = bottom_nav,
        sc = sc
    );
    out.push_str(&page_footer());
    out
}

// ---------------------------------------------------------------------------
// 登录
// ---------------------------------------------------------------------------

fn auth_brand(mobile: bool) -> String {
    format!(
        r#"<a class="auth-brand{auth_mobile}" href="/login">
  <span class="brand-mark">{logo}</span>
  <span class="auth-brand-text"><strong>@[strings/title]</strong><span>@[strings/brand_tagline_upper]</span></span>
</a>"#,
        auth_mobile = if mobile { " auth-mobile-brand" } else { "" },
        logo = icon("graduation")
    )
}

fn mode_switch_links<F>(mode: &str, href: F) -> String
where
    F: Fn(&str) -> String,
{
    format!(
        r#"<div class="mode-switch" role="group" aria-label="@[strings/common_view_mode]">
  <a class="mode-pill{mob}" href="{mob_href}">{phone}@[strings/common_mobile]</a>
  <a class="mode-pill{pc}" href="{pc_href}">{desktop}@[strings/common_desktop]</a>
</div>"#,
        mob = if mode == "mob" { " active" } else { "" },
        pc = if mode == "pc" { " active" } else { "" },
        mob_href = esc(&href("mob")),
        pc_href = esc(&href("pc")),
        phone = icon("phone"),
        desktop = icon("desktop")
    )
}

fn mode_switch(mode: &str, role: Option<&str>) -> String {
    mode_switch_links(mode, |m| match role {
        Some(r) => format!("/login/{m}/{r}"),
        None => format!("/login?mode={m}"),
    })
}

fn account_mode_switch(mode: &str) -> String {
    mode_switch_links(mode, |m| format!("/login/account?mode={m}"))
}

fn auth_hero(kind: &str) -> String {
    let (title, desc, features) = match kind {
        "account" => (
            "@[strings/login_hero_account_title]",
            "@[strings/login_hero_account_desc]",
            vec![
                ("shield", "@[strings/login_feature_session]"),
                ("key", "@[strings/login_feature_keys]"),
                ("settings", "@[strings/login_feature_prefs]"),
            ],
        ),
        _ => (
            "@[strings/login_hero_login_title]",
            "@[strings/login_hero_login_desc]",
            vec![
                ("megaphone", "@[strings/login_feature_class]"),
                ("folder", "@[strings/login_feature_files]"),
                ("reply", "@[strings/login_feature_reply]"),
            ],
        ),
    };
    let feature_html: String = features
        .into_iter()
        .map(|(i, label)| {
            format!(
                r#"<span class="auth-feature"><span class="fi">{}</span>{}</span>"#,
                icon(i),
                esc(label)
            )
        })
        .collect();
    format!(
        r#"<section class="auth-hero" aria-hidden="true">
  <div>{brand}</div>
  <div class="auth-hero-main">
    <span class="hero-eyebrow">{spark} @[strings/login_hero_eyebrow]</span>
    <h1>{title}</h1>
    <p>{desc}</p>
    <div class="auth-features">{features}</div>
  </div>
  <div class="auth-hero-foot"><span>@[strings/brand_footer]</span><span>@[strings/login_hero_year]</span></div>
</section>"#,
        brand = auth_brand(false),
        spark = icon("sparkles"),
        title = title,
        desc = desc,
        features = feature_html
    )
}

#[allow(clippy::format_in_format_args)]
pub fn login_index(mode: &str, flash: &Flash) -> String {
    let mode = if mode == "pc" { "pc" } else { "mob" };
    let role_card = |role: &str, title: &str, desc: &str, icon_name: &str, icon_class: &str| {
        format!(
            r#"<a class="role-card" href="{href}">
  <span class="role-icon {icon_class}">{icon}</span>
  <span class="role-copy"><span class="role-title">{title}</span><span class="role-desc">{desc}</span></span>
  <span class="role-arrow">{arrow}</span>
</a>"#,
            href = format!("/login/{mode}/{role}"),
            icon_class = icon_class,
            icon = icon(icon_name),
            title = esc(title),
            desc = esc(desc),
            arrow = icon("arrow-right")
        )
    };

    let demo_keys = format!(
        r#"<details class="demo-keys">
  <summary>@[strings/login_demo_summary]</summary>
  <div class="demo-key-list">
    <div class="demo-key"><span>@[strings/login_demo_parent_student]</span><code>{stu}</code><button class="btn ghost sm" type="button" data-copy="{stu}">{copy}</button></div>
    <div class="demo-key"><span>@[strings/role_teacher]</span><code>{tea}</code><button class="btn ghost sm" type="button" data-copy="{tea}">{copy}</button></div>
    <div class="demo-key"><span>@[strings/role_admin]</span><code>{adm}</code><button class="btn ghost sm" type="button" data-copy="{adm}">{copy}</button></div>
  </div>
</details>"#,
        stu = db::DEMO_STUDENT_KEY,
        tea = db::DEMO_TEACHER_KEY,
        adm = db::DEMO_ADMIN_KEY,
        copy = icon("copy")
    );

    let body = format!(
        r#"<div class="auth-shell">
  {hero}
  <section class="auth-panel">
    <div class="auth-card">
      {mobile_brand}
      <div class="auth-topline">
        <span class="eyebrow">@[strings/login_choose_identity]</span>
        {mode_switch}
      </div>
      {flash}
      <div class="auth-heading">
        <h2>@[strings/login_welcome]</h2>
        <p>@[strings/login_welcome_desc]</p>
      </div>
      <div class="role-grid">
        {parent}
        {teacher}
        {admin}
      </div>
      <div class="auth-divider">@[strings/common_or]</div>
      <a class="auth-mini-link" href="/login/account">{user_icon}@[strings/login_account_center]</a>
      {demo_keys}
      <p class="hint auth-footnote">@[strings/login_footer_note]</p>
    </div>
  </section>
</div>"#,
        hero = auth_hero("login"),
        mobile_brand = auth_brand(true),
        mode_switch = mode_switch(mode, None),
        parent = role_card(
            "parent",
            "@[strings/role_parent]",
            "@[strings/login_parent_desc]",
            "users",
            ""
        ),
        teacher = role_card(
            "teacher",
            "@[strings/role_teacher]",
            "@[strings/login_teacher_desc]",
            "book",
            "sky"
        ),
        admin = role_card(
            "admin",
            "@[strings/role_admin]",
            "@[strings/login_admin_desc]",
            "shield",
            "amber"
        ),
        user_icon = icon("user"),
        demo_keys = demo_keys,
        flash = flash_html(flash)
    );

    let mut out = document_head("@[strings/login_submit]", "system");
    out.push_str(&format!(r#"<body>{body}<div class="toast-wrap"></div><script src="/static/strings.js?v=5"></script><script src="/static/app.js?v=5"></script></body></html>"#));
    out
}

pub fn login_page(mode: &str, role: &str, error: Option<&str>) -> String {
    let mode = if mode == "pc" { "pc" } else { "mob" };
    let (title, desc, icon_name, icon_class, demo_key, placeholder) = match role {
        "parent" => (
            "@[strings/login_parent_page_title]",
            "@[strings/login_parent_page_desc]",
            "users",
            "",
            db::DEMO_STUDENT_KEY,
            "@[strings/login_placeholder]",
        ),
        "teacher" => (
            "@[strings/login_teacher_page_title]",
            "@[strings/login_teacher_page_desc]",
            "book",
            "sky",
            db::DEMO_TEACHER_KEY,
            "@[strings/login_placeholder_teacher]",
        ),
        "admin" => (
            "@[strings/login_admin_page_title]",
            "@[strings/login_admin_page_desc]",
            "shield",
            "amber",
            db::DEMO_ADMIN_KEY,
            "@[strings/login_placeholder_admin]",
        ),
        _ => (
            "@[strings/login_student_page_title]",
            "@[strings/login_student_page_desc]",
            "graduation",
            "",
            db::DEMO_STUDENT_KEY,
            "@[strings/login_placeholder]",
        ),
    };
    let demo_label = match role {
        "teacher" => "@[strings/login_demo_teacher]",
        "admin" => "@[strings/login_demo_admin]",
        _ => "@[strings/login_demo_parent]",
    };

    let role_links = ["parent", "teacher", "admin"]
        .iter()
        .map(|r| {
            let active = if *r == role { " active" } else { "" };
            format!(
                r#"<a class="mode-pill{active}" href="/login/{mode}/{r}">{label}</a>"#,
                active = active,
                mode = mode,
                r = r,
                label = role_label(r)
            )
        })
        .collect::<String>();

    let error_html = error
        .map(|e| format!(r#"<div class="alert">{}{}</div>"#, icon("alert"), esc(e)))
        .unwrap_or_default();

    let body = format!(
        r#"<div class="auth-shell">
  {hero}
  <section class="auth-panel">
    <div class="auth-card">
      {mobile_brand}
      <div class="auth-topline">
        <a class="back-link" href="/login">{arrow}@[strings/login_back_identity]</a>
        {mode_switch}
      </div>
      <div class="auth-heading">
        <span class="role-icon {icon_class}">{role_icon}</span>
        <h2>{title}</h2>
        <p>{desc}</p>
      </div>
      {error_html}
      <form class="login-form" method="post" action="/login/{mode}/{role}">
        <div class="field">
          <label class="label" for="key-input">@[strings/login_key_label] <span class="req">*</span></label>
          <input class="input mono" id="key-input" name="key" type="text" autocomplete="off"
                 spellcheck="false" placeholder="{placeholder}" required autofocus>
          <span class="hint">@[strings/login_key_hint]</span>
        </div>
        <button class="btn primary block" type="submit">{login_icon}@[strings/login_submit]</button>
        <details class="demo-keys">
          <summary>{demo_label}</summary>
          <div class="demo-key-list">
            <button class="demo-key" type="button" data-demo-key="{demo_key}">
              <span>{demo_label}</span><code>{demo_key}</code><span>@[strings/common_click_fill]</span>
            </button>
          </div>
        </details>
      </form>
      <div class="auth-divider">@[strings/login_other_identity]</div>
      <div class="mode-switch fill">{role_links}</div>
      <div class="auth-divider">@[strings/common_or]</div>
      <a class="auth-mini-link" href="/login/account">{user_icon}@[strings/login_account_center_link]</a>
      <p class="hint auth-footnote">@[strings/login_authorized_note]</p>
    </div>
  </section>
</div>"#,
        hero = auth_hero("login"),
        mobile_brand = auth_brand(true),
        arrow = icon("arrow-left"),
        mode_switch = mode_switch(mode, Some(role)),
        icon_class = icon_class,
        role_icon = icon(icon_name),
        title = esc(title),
        desc = esc(desc),
        error_html = error_html,
        mode = mode,
        role = esc(role),
        placeholder = esc(placeholder),
        login_icon = icon("arrow-right"),
        demo_label = demo_label,
        demo_key = demo_key,
        role_links = role_links,
        user_icon = icon("user")
    );

    let mut out = document_head(title, "system");
    out.push_str(&format!(r#"<body>{body}<div class="toast-wrap"></div><script src="/static/strings.js?v=5"></script><script src="/static/app.js?v=5"></script></body></html>"#));
    out
}

// ---------------------------------------------------------------------------
// 首页 / 征集 / 提交
// ---------------------------------------------------------------------------

fn submission_card_link(
    row: &SubmissionRow,
    now: i64,
    base: &str,
    staff: bool,
    parent: bool,
) -> String {
    let href = format!("{base}submission/{}/", row.id);
    let count_label = if row.is_notice() {
        crate::strings::t("submission_notice_detail")
    } else if staff {
        crate::strings::f1("submissions_count_staff", row.upload_count)
    } else if row.my_upload_count == 0 {
        if parent {
            crate::strings::t("submissions_child_not_submitted")
        } else {
            crate::strings::t("submissions_not_submitted")
        }
    } else if parent {
        crate::strings::f1("submissions_count_child", row.my_upload_count)
    } else {
        crate::strings::f1("submissions_count_own", row.my_upload_count)
    };
    let desc = if row.desc.trim().is_empty() {
        crate::strings::t("common_no_description")
    } else {
        truncate(&row.desc, 90)
    };
    format!(
        r#"<article class="card hover submission-card">
  <a class="card-link" href="{href}" aria-label="@[strings/common_open]@[strings/type_generic] {name}"></a>
  <div class="submission-top">
    <span class="chip type">{type_label}</span>
    {pin}
    {deadline}
  </div>
  <h3 class="submission-title">{name}</h3>
  <p class="submission-desc">{desc}</p>
  <div class="submission-meta">
    <span class="meta-item">{file_icon}<span>{count}</span></span>
    <span class="meta-item">{clock_icon}<span>{created}</span></span>
  </div>
</article>"#,
        href = esc(&href),
        name = esc(&row.name),
        type_label = esc(&type_label(&row.kind)),
        pin = notice_pin_badge(row, now),
        deadline = deadline_badge(row.deadline, now),
        desc = desc,
        file_icon = icon("file"),
        count = esc(&count_label),
        clock_icon = icon("clock"),
        created = esc(&relative_time(row.created_at, now))
    )
}

fn submission_card_manage(
    row: &SubmissionRow,
    now: i64,
    base: &str,
    staff: bool,
    admin: bool,
    parent: bool,
) -> String {
    let href = format!("{base}submission/{}/", row.id);
    let count_label = if row.is_notice() {
        crate::strings::t("submission_notice_detail")
    } else if staff {
        crate::strings::f1("submissions_count_staff", row.upload_count)
    } else if row.my_upload_count == 0 {
        if parent {
            crate::strings::t("submissions_child_not_submitted")
        } else {
            crate::strings::t("submissions_not_submitted")
        }
    } else if parent {
        crate::strings::f1("submissions_count_child", row.my_upload_count)
    } else {
        crate::strings::f1("submissions_count_own", row.my_upload_count)
    };
    let desc = if row.desc.trim().is_empty() {
        crate::strings::t("common_no_description")
    } else {
        truncate(&row.desc, 110)
    };
    let pending = if staff && row.pending_count > 0 {
        format!(
            r#"<span class="chip warn">{}{} @[strings/uploads_status_pending]</span>"#,
            icon("clock"),
            row.pending_count
        )
    } else {
        String::new()
    };
    let delete = if admin {
        format!(
            r#"<button class="btn danger sm" type="button" data-del="{id}" data-name="{name}">{trash}@[strings/common_delete]</button>"#,
            id = esc(&row.id),
            name = esc(&row.name),
            trash = icon("trash")
        )
    } else {
        String::new()
    };
    format!(
        r#"<article class="card submission-card">
  <div class="submission-top">
    <span class="chip type">{type_label}</span>
    {pin}
    {deadline}
  </div>
  <h3 class="submission-title"><a href="{href}">{name}</a></h3>
  <p class="submission-desc">{desc}</p>
  <div class="submission-meta">
    <span class="meta-item">{file_icon}<span>{count}</span></span>
    <span class="meta-item">{clock_icon}<span>{created}</span></span>
  </div>
  <div class="card-actions">
    {pending}
    <a class="btn outline sm" href="{href}">{action_label}{arrow}</a>
    {delete}
  </div>
</article>"#,
        type_label = esc(&type_label(&row.kind)),
        pin = notice_pin_badge(row, now),
        deadline = deadline_badge(row.deadline, now),
        href = esc(&href),
        name = esc(&row.name),
        desc = desc,
        file_icon = icon("file"),
        count = esc(&count_label),
        clock_icon = icon("clock"),
        created = esc(&relative_time(row.created_at, now)),
        pending = pending,
        arrow = icon("arrow-right"),
        action_label = if row.is_notice() {
            "@[strings/common_view]"
        } else if staff {
            "@[strings/common_open]"
        } else if row.my_upload_count == 0 {
            "@[strings/common_go_submit]"
        } else {
            "@[strings/common_view]"
        },
        delete = delete
    )
}

fn upload_row_html(upload: &UploadRow, now: i64, base: &str, staff: bool) -> String {
    let href = format!("{base}upload/{}/", upload.id);
    let student = if staff {
        if let Some(id) = &upload.student_id {
            format!(r#"<span>{}</span><span class="dot-sep"></span>"#, esc(id))
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    let uploader = if staff {
        format!(
            r#"<span class="meta-item">{}{}</span>"#,
            icon("user"),
            esc(&upload.uploader_name)
        )
    } else {
        String::new()
    };
    format!(
        r#"<div class="list-row">
  <a class="card-link" href="{href}" aria-label="@[strings/common_view]@[strings/page_uploads] {title}"></a>
  <span class="avatar sm{avatar_class}">{initial}</span>
  <div class="row-main">
    <div class="row-title">{title}</div>
    <div class="row-sub">
      {student}
      <span>{submission}</span><span class="dot-sep"></span>
      {uploader}
      {uploader_sep}
      <span>{time}</span>
    </div>
  </div>
  <div class="row-side">{status}{arrow}</div>
</div>"#,
        href = esc(&href),
        avatar_class = avatar_class(&upload.role),
        initial = esc(&initial(&upload.uploader_name)),
        title = esc(&upload.title),
        student = student,
        submission = esc(&truncate_chars(&upload.submission_name, 26)),
        uploader = uploader,
        uploader_sep = if staff {
            r#"<span class="dot-sep"></span>"#
        } else {
            ""
        },
        time = esc(&relative_time(upload.created_at, now)),
        status = upload_status(upload),
        arrow = icon("chevron-right")
    )
}

pub fn home(
    ctx: &LayoutCtx,
    stats: &[StatItem],
    submissions: &[SubmissionRow],
    uploads: &[UploadRow],
    uploads_title: &str,
    uploads_hint: &str,
) -> String {
    let now = crate::tools::now_ts();
    let base = ctx.base();
    let (eyebrow, hero_title, hero_desc) = match ctx.page_role.as_str() {
        "parent" => (
            "@[strings/home_eyebrow_parent]",
            crate::strings::f1("home_title_parent", esc(&ctx.display_name)),
            "@[strings/home_desc_parent]".to_string(),
        ),
        "teacher" => (
            "@[strings/home_eyebrow_teacher]",
            "@[strings/home_title_teacher]".to_string(),
            "@[strings/home_desc_teacher]".to_string(),
        ),
        "admin" => (
            "@[strings/home_eyebrow_admin]",
            "@[strings/home_title_admin]".to_string(),
            "@[strings/home_desc_admin]".to_string(),
        ),
        _ => (
            "@[strings/home_eyebrow_student]",
            "@[strings/home_title_student]".to_string(),
            "@[strings/home_desc_student]".to_string(),
        ),
    };
    let hero_actions = match ctx.page_role.as_str() {
        "teacher" | "admin" => format!(
            r#"<a class="btn light" href="{base}submissions/new">{plus}@[strings/nav_publish]</a><a class="btn glass" href="{base}uploads/">{folder}@[strings/nav_uploads_staff]</a><a class="btn glass" href="/user">{user}@[strings/nav_account]</a>"#,
            base = base,
            plus = icon("plus"),
            folder = icon("folder"),
            user = icon("user")
        ),
        "parent" => format!(
            r#"<a class="btn light" href="{base}submissions/">{inbox}@[strings/home_action_browse]</a><a class="btn glass" href="{base}uploads/">{folder}@[strings/nav_uploads_parent]</a><a class="btn glass" href="/user">{user}@[strings/nav_account]</a>"#,
            base = base,
            inbox = icon("inbox"),
            folder = icon("folder"),
            user = icon("user")
        ),
        _ => format!(
            r#"<a class="btn light" href="{base}submissions/">{inbox}@[strings/home_action_browse]</a><a class="btn glass" href="{base}uploads/">{folder}@[strings/nav_uploads_self]</a><a class="btn glass" href="/user">{user}@[strings/nav_account]</a>"#,
            base = base,
            inbox = icon("inbox"),
            folder = icon("folder"),
            user = icon("user")
        ),
    };

    let stats_html: String = stats
        .iter()
        .map(|s| {
            format!(
                r#"<div class="stat"><span class="stat-icon {tone}">{icon}</span><div><div class="stat-value">{value}</div><div class="stat-label">{label}</div></div></div>"#,
                tone = s.tone,
                icon = icon(s.icon),
                value = esc(&s.value),
                label = esc(&s.label)
            )
        })
        .collect();

    let submissions_html = if submissions.is_empty() {
        format!(
            r#"<div class="empty"><span class="empty-icon">{icon}</span><div class="empty-title">@[strings/home_empty_submissions_title]</div><div class="empty-desc">@[strings/home_empty_submissions_desc]</div></div>"#,
            icon = icon("inbox")
        )
    } else {
        let cards: String = submissions
            .iter()
            .map(|row| {
                submission_card_link(
                    row,
                    now,
                    &base,
                    ctx.role != "student",
                    ctx.page_role == "parent",
                )
            })
            .collect();
        format!(r#"<div class="card-grid">{cards}</div>"#)
    };

    let uploads_html = if uploads.is_empty() {
        format!(
            r#"<div class="empty"><span class="empty-icon">{icon}</span><div class="empty-title">@[strings/home_empty_uploads_title]</div><div class="empty-desc">{hint}</div></div>"#,
            icon = icon("folder"),
            hint = esc(uploads_hint)
        )
    } else {
        let rows: String = uploads
            .iter()
            .map(|u| upload_row_html(u, now, &base, ctx.role != "student"))
            .collect();
        format!(r#"<div class="list">{rows}</div>"#)
    };

    let body = format!(
        r#"<section class="hero">
  <div class="hero-copy">
    <span class="hero-eyebrow">{spark} {eyebrow}</span>
    <h2 class="hero-title">{hero_title}</h2>
    <p class="hero-desc">{hero_desc}</p>
    <div class="hero-actions">{hero_actions}</div>
  </div>
</section>
<div class="stats">{stats_html}</div>
<section class="section">
  <div class="section-head">
    <h2 class="section-title">{inbox}@[strings/home_recent_submissions]</h2>
    <a class="section-link" href="{base}submissions/">@[strings/common_view_all]{arrow}</a>
  </div>
  {submissions_html}
</section>
<section class="section">
  <div class="section-head">
    <h2 class="section-title">{folder}{uploads_title}</h2>
    <a class="section-link" href="{base}uploads/">@[strings/common_view_all]{arrow}</a>
  </div>
  {uploads_html}
</section>"#,
        spark = icon("sparkles"),
        eyebrow = eyebrow,
        hero_title = hero_title,
        hero_desc = esc(&hero_desc),
        hero_actions = hero_actions,
        stats_html = stats_html,
        inbox = icon("inbox"),
        arrow = icon("arrow-right"),
        base = base,
        submissions_html = submissions_html,
        folder = icon("folder"),
        uploads_title = esc(uploads_title),
        uploads_html = uploads_html
    );

    layout(ctx, &body)
}

pub fn submissions(
    ctx: &LayoutCtx,
    rows: &[SubmissionRow],
    staff: bool,
    admin: bool,
    parent: bool,
) -> String {
    let now = crate::tools::now_ts();
    let base = ctx.base();
    let new_btn = if staff {
        format!(
            r#"<a class="btn primary" href="{base}submissions/new">{plus}@[strings/nav_publish]</a>"#,
            base = base,
            plus = icon("plus")
        )
    } else {
        String::new()
    };
    let content = if rows.is_empty() {
        format!(
            r#"<div class="empty"><span class="empty-icon">{icon}</span><div class="empty-title">@[strings/home_empty_submissions_title]</div><div class="empty-desc">@[strings/submissions_empty_desc]</div>{new_btn}</div>"#,
            icon = icon("inbox"),
            new_btn = if staff {
                format!(
                    r#"<a class="btn primary" href="{base}submissions/new">@[strings/submissions_publish_first]</a>"#
                )
            } else {
                String::new()
            }
        )
    } else {
        let cards: String = rows
            .iter()
            .map(|row| {
                submission_card_manage(row, now, &base, staff, admin, ctx.page_role == "parent")
            })
            .collect();
        format!(r#"<div class="card-grid">{cards}</div>"#)
    };
    let unit = if staff {
        crate::strings::t("submissions_unit_class")
    } else if parent {
        crate::strings::t("submissions_unit_child")
    } else {
        crate::strings::t("submissions_unit_person")
    };
    let page_sub = crate::strings::f2("submissions_page_sub", rows.len(), unit);
    let body = format!(
        r#"<div class="page-head">
  <div>
    <h2 class="page-title">@[strings/submissions_page_title]</h2>
    <p class="page-sub">{page_sub}</p>
  </div>
  <div class="page-actions">{new_btn}</div>
</div>
{content}"#,
        page_sub = page_sub,
        new_btn = new_btn,
        content = content
    );
    layout(ctx, &body)
}

pub fn new_submission(ctx: &LayoutCtx) -> String {
    let base = ctx.base();
    let default_deadline = fmt_datetime_local(crate::tools::now_ts() + 3 * 86_400 + 12 * 3600);
    let body = format!(
        include_str!("./views/new_submission.html"),
        base = base,
        arrow = icon("arrow-left"),
        default_deadline = default_deadline,
        plus = icon("plus"),
        display = esc(&ctx.display_name),
        spark = icon("sparkles"),
        class_name = esc(&ctx.class_name),
        role = esc(&role_label(&ctx.role))
    );
    layout(ctx, &body)
}

pub fn uploads(
    ctx: &LayoutCtx,
    rows: &[UploadRow],
    staff: bool,
    filter: &str,
    counts: (i64, i64, i64),
) -> String {
    let now = crate::tools::now_ts();
    let base = ctx.base();
    let parent = ctx.page_role == "parent";
    let title = if staff {
        "@[strings/uploads_title_staff]"
    } else if parent {
        "@[strings/uploads_title_parent]"
    } else {
        "@[strings/stat_my_uploads]"
    };
    let sub = if staff {
        "@[strings/uploads_sub_staff]".to_string()
    } else if parent {
        "@[strings/uploads_sub_parent]".to_string()
    } else {
        "@[strings/uploads_sub_self]".to_string()
    };
    let tabs = if staff {
        format!(
            r#"<div class="segmented" role="tablist">
  <a class="segment{all}" href="{base}uploads/?filter=all">{all_label}</a>
  <a class="segment{pending}" href="{base}uploads/?filter=pending">{pending_label}</a>
  <a class="segment{replied}" href="{base}uploads/?filter=replied">{replied_label}</a>
</div>"#,
            all = if filter == "all" { " active" } else { "" },
            pending = if filter == "pending" { " active" } else { "" },
            replied = if filter == "replied" { " active" } else { "" },
            base = base,
            all_label = crate::strings::f1("uploads_tab_all", counts.0),
            pending_label = crate::strings::f1("uploads_tab_pending", counts.1),
            replied_label = crate::strings::f1("uploads_tab_replied", counts.2)
        )
    } else {
        format!(
            r#"<span class="chip soft">{}{}</span>"#,
            icon("folder"),
            crate::strings::f1("uploads_count_own", rows.len())
        )
    };
    let content = if rows.is_empty() {
        format!(
            r#"<div class="empty"><span class="empty-icon">{icon}</span><div class="empty-title">{empty_title}</div><div class="empty-desc">{empty_desc}</div><a class="btn primary" href="{base}submissions/">@[strings/uploads_go]</a></div>"#,
            icon = icon("folder"),
            empty_title = if filter == "pending" {
                "@[strings/uploads_empty_pending]"
            } else if filter == "replied" {
                "@[strings/uploads_empty_replied]"
            } else {
                "@[strings/uploads_empty_all]"
            },
            empty_desc = if staff {
                "@[strings/uploads_empty_desc_staff]"
            } else if parent {
                "@[strings/uploads_empty_desc_parent]"
            } else {
                "@[strings/uploads_empty_desc_self]"
            },
            base = base
        )
    } else {
        let list: String = rows
            .iter()
            .map(|u| upload_row_html(u, now, &base, staff))
            .collect();
        format!(r#"<div class="list">{list}</div>"#)
    };
    let body = format!(
        r#"<div class="page-head">
  <div><h2 class="page-title">{title}</h2><p class="page-sub">{sub}</p></div>
  <div class="page-actions">{tabs}</div>
</div>
{content}"#,
        title = title,
        sub = esc(&sub),
        tabs = tabs,
        content = content
    );
    layout(ctx, &body)
}

pub fn submission_all(
    ctx: &LayoutCtx,
    sub: &SubmissionRow,
    rows: &[UploadRow],
    staff: bool,
    admin: bool,
) -> String {
    let now = crate::tools::now_ts();
    let base = ctx.base();
    let can_upload = true;
    let export = if staff {
        format!(
            r#"<button class="btn outline" type="button" id="export-btn" data-submission="{id}" data-name="{name}">{download}@[strings/submission_all_export]</button>"#,
            id = esc(&sub.id),
            name = esc(&sub.name),
            download = icon("download")
        )
    } else {
        String::new()
    };
    let delete = if admin {
        format!(
            r#"<button class="btn danger" type="button" data-del="{id}" data-name="{name}" data-redirect="{base}submissions/">{trash}@[strings/submission_all_delete]</button>"#,
            id = esc(&sub.id),
            name = esc(&sub.name),
            base = base,
            trash = icon("trash")
        )
    } else {
        String::new()
    };
    let upload_btn = if can_upload {
        format!(
            r#"<a class="btn primary" href="{base}submission/{id}/upload">{upload}@[strings/page_upload]</a>"#,
            base = base,
            id = esc(&sub.id),
            upload = icon("upload")
        )
    } else {
        String::new()
    };
    let list = if rows.is_empty() {
        format!(
            r#"<div class="empty"><span class="empty-icon">{icon}</span><div class="empty-title">@[strings/uploads_empty_all]</div><div class="empty-desc">{desc}</div>{upload_btn}</div>"#,
            icon = icon("folder"),
            desc = if staff {
                "@[strings/submission_all_empty_staff]"
            } else if ctx.page_role == "parent" {
                "@[strings/submission_all_empty_parent]"
            } else {
                "@[strings/submission_all_empty_self]"
            },
            upload_btn = if can_upload {
                format!(
                    r#"<a class="btn primary" href="{base}submission/{id}/upload">@[strings/submission_all_upload_first]</a>"#,
                    base = base,
                    id = esc(&sub.id)
                )
            } else {
                String::new()
            }
        )
    } else {
        let rows_html: String = rows
            .iter()
            .map(|u| upload_row_html(u, now, &base, staff))
            .collect();
        format!(r#"<div class="list">{rows_html}</div>"#)
    };
    let body = format!(
        include_str!("./views/submission_all.html"),
        base = base,
        arrow = icon("arrow-left"),
        name = esc(&sub.name),
        created_name = esc(&sub.created_name),
        created = esc(&fmt_datetime(sub.created_at)),
        upload_btn = upload_btn,
        export = export,
        delete = delete,
        type_label = esc(&type_label(&sub.kind)),
        deadline = deadline_badge(sub.deadline, now),
        desc = if sub.desc.trim().is_empty() {
            "@[strings/submission_all_no_desc]".to_string()
        } else {
            esc(&sub.desc)
        },
        file_icon = icon("file"),
        count_label = if staff {
            crate::strings::f1("submissions_count_staff", sub.upload_count)
        } else {
            crate::strings::f1("submissions_count_own", sub.my_upload_count)
        },
        clock_icon = icon("clock"),
        published = crate::strings::f1("submission_all_published", fmt_datetime(sub.created_at)),
        pending = if staff && sub.pending_count > 0 {
            format!(
                r#"<span class="chip warn">{}{} @[strings/uploads_status_pending]</span>"#,
                icon("clock"),
                sub.pending_count
            )
        } else {
            String::new()
        },
        folder = icon("folder"),
        list_title = if staff {
            "@[strings/submission_all_list_staff]"
        } else if ctx.page_role == "parent" {
            "@[strings/submission_all_list_parent]"
        } else {
            "@[strings/submission_all_list_self]"
        },
        n = rows.len(),
        list_hint = if staff {
            "@[strings/submission_all_hint_staff]"
        } else if ctx.page_role == "parent" {
            "@[strings/submission_all_hint_parent]"
        } else {
            "@[strings/submission_all_hint_self]"
        },
        list = list
    );
    layout(ctx, &body)
}

pub fn notice_detail(ctx: &LayoutCtx, sub: &SubmissionRow, admin: bool) -> String {
    let now = crate::tools::now_ts();
    let base = ctx.base();
    let delete = if admin {
        format!(
            r#"<button class="btn danger" type="button" data-del="{id}" data-name="{name}" data-redirect="{base}submissions/">{trash}@[strings/submission_all_delete]</button>"#,
            id = esc(&sub.id),
            name = esc(&sub.name),
            base = base,
            trash = icon("trash")
        )
    } else {
        String::new()
    };
    let body = format!(
        include_str!("./views/notice_detail.html"),
        base = base,
        arrow = icon("arrow-left"),
        name = esc(&sub.name),
        created_name = esc(&sub.created_name),
        created = esc(&fmt_datetime(sub.created_at)),
        type_label = esc(&type_label(&sub.kind)),
        pinned = if sub.is_pinned_notice(now) {
            format!(
                r#"<span class="chip pinned">{}{}</span>"#,
                icon("megaphone"),
                esc(&crate::strings::t("submission_notice_pinned"))
            )
        } else {
            String::new()
        },
        deadline = deadline_badge(sub.deadline, now),
        desc = if sub.desc.trim().is_empty() {
            "@[strings/submission_all_no_desc]".to_string()
        } else {
            esc(&sub.desc)
        },
        clock_icon = icon("clock"),
        published = crate::strings::f1("submission_all_published", fmt_datetime(sub.created_at)),
        megaphone = icon("megaphone"),
        delete = delete
    );
    layout(ctx, &body)
}

pub fn upload_form(
    ctx: &LayoutCtx,
    sub: &SubmissionRow,
    existing: i64,
    last_id: Option<&str>,
) -> String {
    let now = crate::tools::now_ts();
    let base = ctx.base();
    let default_title = truncate_chars(&format!("{} · {}", ctx.display_name, sub.name), 160);
    let parent = ctx.page_role == "parent";
    let existing_hint = if existing > 0 {
        if parent {
            crate::strings::f1("upload_form_existing_parent", existing)
        } else {
            crate::strings::f1("upload_form_existing", existing)
        }
    } else if parent {
        crate::strings::t("upload_form_first_parent")
    } else {
        crate::strings::t("upload_form_first")
    };
    let last_link = last_id
        .map(|id| {
            format!(
                r#" <a href="{base}upload/{id}/">@[strings/upload_form_last_link]</a>"#,
                base = base,
                id = esc(id)
            )
        })
        .unwrap_or_default();
    let body = format!(
        include_str!("./views/upload_form.html"),
        base = base,
        arrow = icon("arrow-left"),
        id = esc(&sub.id),
        name = esc(&sub.name),
        page_title = esc(&crate::strings::f1("upload_form_page_title", &sub.name)),
        default_title = esc(&default_title),
        upload = icon("upload"),
        send = icon("send"),
        existing_hint = esc(&existing_hint),
        last_link = last_link,
        folder = icon("folder"),
        type_label = esc(&type_label(&sub.kind)),
        deadline = deadline_badge(sub.deadline, now),
        desc = if sub.desc.trim().is_empty() {
            "@[strings/submission_all_no_desc]".to_string()
        } else {
            esc(&sub.desc)
        },
        created_name = esc(&sub.created_name),
        created = esc(&fmt_datetime(sub.created_at)),
        deadline_text = sub
            .deadline
            .map(fmt_datetime)
            .unwrap_or_else(|| "@[strings/time_no_deadline]".to_string())
    );
    layout(ctx, &body)
}

// ---------------------------------------------------------------------------
// 附件 / 回复
// ---------------------------------------------------------------------------

fn attachment_gallery(data: &Value) -> String {
    let Some(arr) = data.as_array() else {
        return r#"<p class="hint">@[strings/upload_no_attachments]</p>"#.to_string();
    };
    if arr.is_empty() {
        return r#"<p class="hint">@[strings/upload_no_attachments]</p>"#.to_string();
    }
    let mut out = String::from(r#"<div class="attachment-grid">"#);
    for item in arr {
        let Some(att) = parse_attachment(item) else {
            continue;
        };
        let size = format_size(b64_binary_len(&att.b64));
        let display_name = if att.name == "Attachment" {
            crate::strings::t("common_attachment")
        } else if let Some(rest) = att.name.strip_prefix("Attachment ") {
            crate::strings::f1("common_attachment_n", rest)
        } else {
            att.name.clone()
        };
        let name = esc(&display_name);
        let mime = esc(&att.mime);
        if att.mime.starts_with("image/") {
            let src = format!("data:{};base64,{}", att.mime, att.b64);
            let _ = write!(
                out,
                r#"<div class="attachment">
  <div class="attachment-thumb" role="button" tabindex="0" data-lightbox data-name="{name}" aria-label="{preview_aria}">
    <img src="{src}" alt="{name}">
  </div>
  <div class="attachment-caption">
    <strong><a href="{src}" download="{name}">{name}</a></strong>
    <span>{mime} · {size}</span>
  </div>
</div>"#,
                name = name,
                preview_aria = esc(&crate::strings::f1("upload_preview_aria", &display_name)),
                src = esc(&src),
                mime = mime,
                size = size
            );
        } else {
            let src = format!("data:{};base64,{}", att.mime, att.b64);
            let file_icon = if att.mime.contains("pdf") || att.mime.starts_with("text/") {
                icon("file")
            } else {
                icon("paperclip")
            };
            let _ = write!(
                out,
                r#"<a class="attachment plain" href="{src}" download="{name}">
  <span class="attachment-icon">{file_icon}</span>
  <span class="attachment-caption"><strong>{name}</strong><span>{mime} · {size}</span></span>
  {download}
</a>"#,
                src = esc(&src),
                name = name,
                file_icon = file_icon,
                mime = mime,
                size = size,
                download = icon("download")
            );
        }
    }
    out.push_str("</div>");
    out
}

fn replies_html(replies: &[ReplyRow], admin: bool, now: i64) -> String {
    if replies.is_empty() {
        return format!(
            r#"<div class="empty compact"><span class="empty-icon">{icon}</span><div class="empty-title">@[strings/upload_reply_empty_title]</div><div class="empty-desc">{desc}</div></div>"#,
            icon = icon("reply"),
            desc = if admin {
                "@[strings/upload_reply_empty_staff]"
            } else {
                "@[strings/upload_reply_empty_student]"
            }
        );
    }
    let mut out = String::from(r#"<div class="replies">"#);
    for r in replies {
        let delete = if admin {
            format!(
                r#"<button class="btn ghost sm" type="button" data-del="{id}" data-name="@[strings/upload_replies]">{trash}@[strings/common_delete]</button>"#,
                id = esc(&r.id),
                trash = icon("trash")
            )
        } else {
            String::new()
        };
        let _ = write!(
            out,
            include_str!("./views/replies_item.html"),
            ac = avatar_class(&r.role),
            initial = esc(&initial(&r.replier_name)),
            author = esc(&r.replier_name),
            role_badge = role_badge_text(&r.role),
            time = esc(&relative_time(r.created_at, now)),
            delete = delete,
            content = esc(&r.content)
        );
    }
    out.push_str("</div>");
    out
}

pub fn upload_detail(
    ctx: &LayoutCtx,
    upload: &UploadRow,
    sub: &SubmissionRow,
    replies: &[ReplyRow],
    staff: bool,
    admin: bool,
) -> String {
    let now = crate::tools::now_ts();
    let base = ctx.base();
    let reply_form = if staff {
        format!(
            include_str!("./views/upload_detail_staff.html"),
            upload_id = esc(&upload.id),
            send = icon("send")
        )
    } else {
        format!(
            r#"<div class="account-note">{} @[strings/upload_reply_only_staff]</div>"#,
            icon("lock")
        )
    };
    let reupload = format!(
        r#"<a class="btn outline block" href="{base}submission/{sub_id}/upload">{upload}@[strings/upload_reupload]</a>"#,
        base = base,
        sub_id = esc(&upload.submission_id),
        upload = icon("upload")
    );
    let delete_upload = if admin {
        format!(
            r#"<button class="btn danger block" type="button" data-del="{id}" data-name="{title}" data-redirect="{base}submission/{sub_id}/all">{trash}@[strings/upload_delete]</button>"#,
            id = esc(&upload.id),
            title = esc(&upload.title),
            base = base,
            sub_id = esc(&upload.submission_id),
            trash = icon("trash")
        )
    } else {
        String::new()
    };
    let student_row = if upload.student_id.is_some() {
        let short = upload
            .short_id
            .map(|n| crate::strings::f1("common_student_no_suffix", format!("{n:02}")))
            .unwrap_or_default();
        format!(
            r#"<div class="kv"><dt>@[strings/upload_info_student_no]</dt><dd>{}{}</dd></div>"#,
            esc(upload.student_id.as_deref().unwrap_or("")),
            esc(&short)
        )
    } else {
        String::new()
    };
    let body = format!(
        include_str!("./views/upload_detail.html"),
        base = base,
        sub_id = esc(&sub.id),
        sub_name = esc(&sub.name),
        back_label = esc(&crate::strings::f1("upload_back_all", &sub.name)),
        arrow = icon("arrow-left"),
        title = esc(&upload.title),
        uploader = esc(&upload.uploader_name),
        created = esc(&fmt_datetime(upload.created_at)),
        status = upload_status(upload),
        folder = icon("folder"),
        paperclip = icon("paperclip"),
        time = esc(&relative_time(upload.created_at, now)),
        desc = if upload.desc.trim().is_empty() {
            r#"<p class="hint">@[strings/upload_no_desc]</p>"#.to_string()
        } else {
            format!(r#"<p class="desc-text">{}</p>"#, esc(&upload.desc))
        },
        gallery = attachment_gallery(&upload.data),
        reply = icon("reply"),
        reply_count = replies.len(),
        replies = replies_html(replies, admin, now),
        reply_form = reply_form,
        user = icon("user"),
        role_badge = role_badge_text(&upload.role),
        student_row = student_row,
        last_reply_row = upload
            .last_reply_at
            .map(|ts| format!(
                r#"<div class="kv"><dt>@[strings/upload_info_last_reply]</dt><dd>{}</dd></div>"#,
                esc(&relative_time(ts, now))
            ))
            .unwrap_or_default(),
        reupload = if ctx.page_role != "student" || upload.auth_id.as_deref() == Some(&ctx.auth_id)
        {
            reupload
        } else {
            String::new()
        },
        delete_upload = delete_upload,
        back = icon("arrow-left"),
        note = if staff {
            "@[strings/upload_tip_staff]"
        } else {
            "@[strings/upload_tip_student]"
        }
    );
    layout(ctx, &body)
}

// ---------------------------------------------------------------------------
// 账号中心
// ---------------------------------------------------------------------------

pub fn user_guest(
    mode: &str,
    flash: &Flash,
    prefill_key: &str,
    session_note: Option<&str>,
) -> String {
    let mode = if mode == "pc" { "pc" } else { "mob" };
    let note = session_note
        .map(|n| {
            format!(
                r#"<div class="account-note spaced">{}{}</div>"#,
                icon("key"),
                esc(n)
            )
        })
        .unwrap_or_default();
    let body = format!(
        include_str!("./views/user_guest.html"),
        hero = auth_hero("account"),
        mobile_brand = auth_brand(true),
        arrow = icon("arrow-left"),
        mode_switch = account_mode_switch(mode),
        flash = flash_html(flash),
        note = note,
        lock = icon("lock"),
        key = icon("key"),
        prefill_key = esc(prefill_key)
    );
    let mut out = document_head("@[strings/role_account]", "system");
    out.push_str(&format!(r#"<body>{body}<div class="toast-wrap"></div><script src="/static/strings.js?v=5"></script><script src="/static/app.js?v=5"></script></body></html>"#));
    out
}

pub fn user_account(
    ctx: &LayoutCtx,
    user: &UserCtx,
    auths: &[AuthCtx],
    admin_auths: &[AuthCtx],
    flash: &Flash,
) -> String {
    let classes: HashSet<&str> = auths.iter().map(|a| a.class_id.as_str()).collect();
    let role_options = |selected: &str| {
        ["system", "light", "dark"]
            .iter()
            .map(|v| {
                let label = match *v {
                    "light" => "@[strings/account_theme_light]",
                    "dark" => "@[strings/account_theme_dark]",
                    _ => "@[strings/account_theme_system]",
                };
                format!(
                    r#"<option value="{v}"{sel}>{label}</option>"#,
                    v = v,
                    sel = if selected == *v { " selected" } else { "" },
                    label = label
                )
            })
            .collect::<String>()
    };
    let mode_options = |selected: &str| {
        ["mob", "pc"]
            .iter()
            .map(|v| {
                let label = if *v == "mob" {
                    "@[strings/common_mobile]"
                } else {
                    "@[strings/common_desktop]"
                };
                format!(
                    r#"<option value="{v}"{sel}>{label}</option>"#,
                    v = v,
                    sel = if selected == *v { " selected" } else { "" },
                    label = label
                )
            })
            .collect::<String>()
    };
    let theme_sel = role_options(&user.prefs.theme);
    let mode_sel = mode_options(&user.prefs.mode);
    let display = if user.display_name.trim().is_empty() {
        user.username.clone()
    } else {
        user.display_name.clone()
    };

    let auths_html = if auths.is_empty() {
        format!(
            r#"<div class="empty compact"><span class="empty-icon">{icon}</span><div class="empty-title">@[strings/account_no_auths_title]</div><div class="empty-desc">@[strings/account_no_auths_desc]</div></div>"#,
            icon = icon("key")
        )
    } else {
        let rows: String = auths
            .iter()
            .map(|a| {
                let current = !ctx.auth_id.is_empty() && a.id == ctx.auth_id;
                let head: String = a.key.chars().take(6).collect();
                let tail: String = a
                    .key
                    .chars()
                    .rev()
                    .take(4)
                    .collect::<Vec<char>>()
                    .into_iter()
                    .rev()
                    .collect();
                let masked = format!("{head}••••{tail}");
                let auth_name = a.identity();
                let student = if a.role == "student" {
                    format!(
                        r#"<span>{}</span>"#,
                        esc(&a.student_no())
                    )
                } else {
                    String::new()
                };
                format!(
                    include_str!("./views/user_account.html"),
                    ac = avatar_class(&a.role),
                    initial = esc(&initial(&display)),
                    class_name = esc(&a.class_name),
                    role_badge = role_badge_text(&a.role),
                    current = if current {
                        r#"<span class="current-tag">@[strings/common_current_identity]</span>"#
                    } else {
                        ""
                    },
                    student = student,
                    auth_name = esc(&auth_name),
                    pencil = icon("pencil"),
                    full = esc(&a.key),
                    masked = esc(&masked),
                    copy = icon("copy"),
                    id = esc(&a.id),
                    arrow = icon("arrow-right"),
                    trash = icon("trash"),
                    delete_confirm = esc(&crate::strings::f2(
                        "account_delete_confirm",
                        &a.class_name,
                        role_label(&a.role),
                    ))
                )
            })
            .collect();
        format!(r#"<div class="auth-list">{rows}</div>"#)
    };

    let admin_select = if admin_auths.is_empty() {
        String::new()
    } else {
        let opts: String = admin_auths
            .iter()
            .map(|a| {
                format!(
                    r#"<option value="{key}" data-class="{class}">{class_name}</option>"#,
                    key = esc(&a.key),
                    class = esc(&a.class_id),
                    class_name = esc(&a.class_name)
                )
            })
            .collect();
        format!(
            r#"<div class="field">
  <label class="label" for="member-admin">@[strings/account_stats_admin]</label>
  <select class="select" id="member-admin">{opts}</select>
  <span class="hint">@[strings/account_member_hint]</span>
</div>"#,
            opts = opts
        )
    };

    let admin_area = if admin_auths.is_empty() {
        format!(
            r#"<div class="account-note">{} @[strings/account_not_admin_note]</div>"#,
            icon("shield")
        )
    } else {
        let class_opts: String = admin_auths
            .iter()
            .map(|a| {
                format!(
                    r#"<option value="{id}">{name}</option>"#,
                    id = esc(&a.id),
                    name = esc(&a.class_name)
                )
            })
            .collect();
        format!(
            include_str!("./views/user_account_admin.html"),
            admin_select = admin_select,
            class_opts = class_opts,
            plus = icon("plus"),
            key = icon("key")
        )
    };

    let body = format!(
        include_str!("./views/user_account_body.html"),
        flash = flash_html(flash),
        initial = esc(&initial(&display)),
        display = esc(&display),
        username = esc(&user.username),
        n_auths = auths.len(),
        mode_sel=mode_sel,
        theme_sel=theme_sel,
        n_classes = classes.len(),
        n_admin = admin_auths.len(),
        settings = icon("settings"),
        lock = icon("lock"),
        key = icon("key"),
        shield = icon("shield"),
        logout = icon("logout"),
        plus = icon("plus"),

        auths = auths_html,
        admin_area = admin_area
    );
    layout(ctx, &body)
}

pub fn not_found(ctx: Option<&LayoutCtx>, msg: &str) -> axum::response::Response {
    let body = format!(
        r#"<div class="empty page-empty">
  <span class="empty-icon">{icon}</span>
  <div class="empty-title">@[strings/error_not_found_title]</div>
  <div class="empty-desc">{msg}</div>
  <a class="btn primary" href="/home">@[strings/common_back_home]</a>
</div>"#,
        icon = icon("alert"),
        msg = esc(msg)
    );
    match ctx {
        Some(ctx) => (
            axum::http::StatusCode::NOT_FOUND,
            axum::response::Html(crate::strings::apply(&layout(ctx, &body))),
        )
            .into_response(),
        None => crate::tools::plain_error(
            axum::http::StatusCode::NOT_FOUND,
            "@[strings/error_page_not_found]",
            msg,
        ),
    }
}
