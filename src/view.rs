use std::collections::HashSet;
use std::fmt::Write as _;

use axum::response::IntoResponse;
use serde_json::{json, Value};

use crate::db;
use crate::tools::{
    b64_binary_len, deadline_chip, fmt_datetime, fmt_datetime_local, format_size, html_escape,
    js_json, parse_attachment, relative_time, role_label, truncate_chars,
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
        .unwrap_or_else(|| "校".to_string())
}

fn truncate(s: &str, n: usize) -> String {
    esc(&truncate_chars(s, n))
}

fn type_label(kind: &str) -> String {
    match kind.trim().to_ascii_lowercase().as_str() {
        "homework" => "作业".to_string(),
        "open call" | "open_call" | "opencall" => "公开征集".to_string(),
        "leave request" | "leave_request" | "leaverequest" => "请假申请".to_string(),
        "notice" => "通知".to_string(),
        "" => "征集".to_string(),
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

fn upload_status(upload: &UploadRow) -> String {
    if upload.reply_count > 0 {
        format!(
            r#"<span class="chip ok">{}{} 条回复</span>"#,
            icon("check-circle"),
            upload.reply_count
        )
    } else {
        format!(
            r#"<span class="chip warn">{}{}</span>"#,
            icon("clock"),
            "待回复"
        )
    }
}

fn role_badge_text(role: &str) -> String {
    let cls = role_class(role);
    format!(
        r#"<span class="role-badge {cls}">{}</span>"#,
        esc(role_label(role))
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
            title: "账号中心".to_string(),
            mode: prefs.mode.clone(),
            page_role: "student".to_string(),
            role: "student".to_string(),
            active: "user".to_string(),
            class_name: "个人账号".to_string(),
            class_desc: "管理身份、密钥与偏好设置".to_string(),
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

    fn user_label(&self) -> &'static str {
        if self.is_account {
            "账号中心"
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
            label: "账号中心",
            href: "/user".to_string(),
            icon: "user",
        }];
        if !ctx.auth_id.is_empty() {
            items.push(NavItem {
                key: "home",
                label: "返回首页",
                href: "/home".to_string(),
                icon: "home",
            });
        }
        items.push(NavItem {
            key: "login",
            label: if ctx.auth_id.is_empty() {
                "身份登录"
            } else {
                "切换身份"
            },
            href: "/login".to_string(),
            icon: "lock",
        });
        return items;
    }
    let base = ctx.base();
    let uploads_label = match ctx.page_role.as_str() {
        "parent" => "孩子提交",
        "student" => "我的上传",
        _ => "提交管理",
    };
    let mut items = vec![
        NavItem {
            key: "home",
            label: "首页",
            href: base.clone(),
            icon: "home",
        },
        NavItem {
            key: "submissions",
            label: "征集",
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
            label: "账号",
            href: "/user".to_string(),
            icon: "user",
        },
    ];
    if ctx.role == "teacher" || ctx.role == "admin" {
        items.push(NavItem {
            key: "submissions_new",
            label: "发布征集",
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
            r#"<div class="flash ok" role="status">{}{}<button type="button" class="flash-close" data-flash-close aria-label="关闭">×</button></div>"#,
            icon("check-circle"),
            esc(ok)
        );
    }
    if let Some(err) = &flash.err {
        let _ = write!(
            out,
            r#"<div class="flash err" role="alert">{}{}<button type="button" class="flash-close" data-flash-close aria-label="关闭">×</button></div>"#,
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

fn document_head(title: &str, theme: &str, with_app_css: bool) -> String {
    format!(
        r##"<!doctype html>
<html lang="zh-CN" data-pref="{theme}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<meta name="theme-color" content="#4f46e5">
<title>{title} · 智慧校园</title>
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
{theme_boot}
{css}
</head>"##,
        theme = esc(theme),
        title = esc(title),
        theme_boot = THEME_BOOT,
        css = if with_app_css {
            r#"<link rel="stylesheet" href="/static/app.css?v=3">"#
        } else {
            ""
        }
    )
}

fn page_footer(scripts: &str, with_app: bool) -> String {
    format!(
        r#"<div class="toast-wrap" aria-live="polite"></div>
<dialog id="confirm-dialog" class="dialog">
  <form method="dialog" class="dialog-card">
    <h3 class="dialog-title" data-dialog-title>确认操作</h3>
    <p class="dialog-msg" data-dialog-msg></p>
    <div class="dialog-actions">
      <button class="btn outline" value="cancel" formnovalidate>取消</button>
      <button class="btn danger" value="ok" data-dialog-confirm formnovalidate>确定</button>
    </div>
  </form>
</dialog>
<dialog id="rename-dialog" class="dialog">
  <form class="dialog-card" id="rename-form">
    <h3 class="dialog-title">为身份改名</h3>
    <p class="dialog-msg">新名称会显示在该班级的提交、回复与页面身份信息中。</p>
    <div class="field">
      <label class="label" for="rename-input">新名称</label>
      <input class="input" id="rename-input" type="text" maxlength="40" autocomplete="off" required>
      <span class="hint">1–40 个字符，改名会同步刷新该身份的历史显示名称。</span>
    </div>
    <div class="dialog-actions">
      <button class="btn outline" type="button" id="rename-cancel">取消</button>
      <button class="btn primary" type="submit" id="rename-save">保存</button>
    </div>
  </form>
</dialog>
<dialog id="lightbox" class="lightbox">
  <div class="lightbox-bar">
    <span data-lightbox-name>附件预览</span>
    <form method="dialog"><button class="icon-btn" value="close" aria-label="关闭">×</button></form>
  </div>
  <img alt="附件预览">
</dialog>
{}
{}
</body>
</html>"#,
        if with_app {
            r#"<script src="/static/app.js?v=3"></script>"#
        } else {
            ""
        },
        scripts
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
            r#"<div class="side-class"><span class="eyebrow">ACCOUNT</span><strong>{}</strong><span>{}</span></div>"#,
            esc(&ctx.class_name),
            esc(&ctx.class_desc)
        )
    } else {
        format!(
            r#"<div class="side-class"><span class="eyebrow">CURRENT CLASS</span><strong>{}</strong><span>{} · {}</span>{}</div>"#,
            esc(&ctx.class_name),
            esc(ctx.user_label()),
            if ctx.student_label.is_empty() {
                "班级空间"
            } else {
                "学生身份"
            },
            if ctx.student_label.is_empty() {
                String::new()
            } else {
                format!(r#"<div style="margin-top:8px">{}</div>"#, status_bar(ctx))
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

    let mut out = document_head(&ctx.title, &ctx.theme, true);
    let _ = write!(
        out,
        r#"<body class="fade-in">
<div class="app-shell">
  <aside class="sidebar">
    <a class="brand" href="{home}" aria-label="智慧校园首页">
      <span class="brand-mark">{logo}</span>
      <span class="brand-text"><strong>智慧校园</strong><small>Campus OS</small></span>
    </a>
    {class_block}
    <nav class="side-nav" aria-label="主导航">
      <span class="side-nav-label">空间导航</span>
      {side_nav}
    </nav>
    <div class="side-foot">
      <a class="side-user" href="/user">
        <span class="avatar sm{avatar_class}">{initial}</span>
        <span class="who"><strong>{display}</strong><span>{role}</span></span>
      </a>
      <form method="post" action="/logout" class="inline-form">
        <button class="side-logout" type="submit" title="退出登录" aria-label="退出登录">{logout}</button>
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
        <a class="icon-btn" href="/user" title="账号中心" aria-label="账号中心">{user_icon}</a>
        <button class="icon-btn" id="theme-toggle" type="button" title="切换主题" aria-label="切换主题">
          <span class="icon-moon">{moon}</span><span class="icon-sun">{sun}</span>
        </button>
        <form method="post" action="/logout" class="inline-form desktop-only">
          <button class="icon-btn" type="submit" title="退出登录" aria-label="退出登录">{logout}</button>
        </form>
      </div>
    </header>
    <main class="content">
      {flash}
      {body}
    </main>
    <nav class="bottom-nav" aria-label="底部导航">{bottom_nav}</nav>
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
        role = esc(user_role_label),
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
    out.push_str(&page_footer("", true));
    out
}

// ---------------------------------------------------------------------------
// 登录
// ---------------------------------------------------------------------------

fn auth_brand(mobile: bool) -> String {
    format!(
        r#"<a class="auth-brand{auth_mobile}" href="/login">
  <span class="brand-mark">{logo}</span>
  <span class="auth-brand-text"><strong>智慧校园</strong><span>CAMPUS OS</span></span>
</a>"#,
        auth_mobile = if mobile { " auth-mobile-brand" } else { "" },
        logo = icon("graduation")
    )
}

fn mode_switch(mode: &str, role: Option<&str>) -> String {
    let href = |m: &str| match role {
        Some(r) => format!("/login/{m}/{r}"),
        None => format!("/login?mode={m}"),
    };
    format!(
        r#"<div class="mode-switch" role="group" aria-label="视图模式">
  <a class="mode-pill{mob}" href="{mob_href}">{phone}手机版</a>
  <a class="mode-pill{pc}" href="{pc_href}">{desktop}电脑版</a>
</div>"#,
        mob = if mode == "mob" { " active" } else { "" },
        pc = if mode == "pc" { " active" } else { "" },
        mob_href = esc(&href("mob")),
        pc_href = esc(&href("pc")),
        phone = icon("phone"),
        desktop = icon("desktop")
    )
}

fn auth_hero(kind: &str) -> String {
    let (title, desc, features) = match kind {
        "account" => (
            "一个账号，连接<br><em>每一个班级身份</em>",
            "绑定多个班级密钥，随时切换家长、老师或管理员视角。管理偏好设置，查看自己拥有的全部身份。",
            vec![
                ("shield", "HttpOnly 会话"),
                ("key", "多班级密钥"),
                ("settings", "偏好设置"),
            ],
        ),
        _ => (
            "让每一次提交，<br>都被<em>认真看见</em>",
            "面向班级的征集、提交与回复工作台。家长、老师和管理员共享同一套清晰的数据，却拥有各自恰到好处的视角。",
            vec![
                ("megaphone", "班级征集"),
                ("folder", "附件归档"),
                ("reply", "及时回复"),
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
    <span class="hero-eyebrow">{spark} SMART CAMPUS</span>
    <h1>{title}</h1>
    <p>{desc}</p>
    <div class="auth-features">{features}</div>
  </div>
  <div class="auth-hero-foot"><span>智慧校园 · Campus OS</span><span>2026</span></div>
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
  <summary>演示环境 · 一键复制密钥</summary>
  <div class="demo-key-list">
    <div class="demo-key"><span>家长 / 学生</span><code>{stu}</code><button class="btn ghost sm" type="button" data-copy="{stu}">{copy}</button></div>
    <div class="demo-key"><span>老师</span><code>{tea}</code><button class="btn ghost sm" type="button" data-copy="{tea}">{copy}</button></div>
    <div class="demo-key"><span>管理员</span><code>{adm}</code><button class="btn ghost sm" type="button" data-copy="{adm}">{copy}</button></div>
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
        <span class="eyebrow">选择你的身份</span>
        {mode_switch}
      </div>
      {flash}
      <div class="auth-heading">
        <h2>欢迎回到智慧校园</h2>
        <p>请选择进入班级空间的身份，每种身份使用对应的班级密钥登录。</p>
      </div>
      <div class="role-grid">
        {parent}
        {teacher}
        {admin}
      </div>
      <div class="auth-divider">或</div>
      <a class="auth-mini-link" href="/user">{user_icon}进入账号中心（用户名 / 密码登录）</a>
      {demo_keys}
      <p class="hint" style="margin-top:16px;text-align:center">家长与学生共用孩子的学生密钥 · 密钥即身份，请勿转发</p>
    </div>
  </section>
</div>"#,
        hero = auth_hero("login"),
        mobile_brand = auth_brand(true),
        mode_switch = mode_switch(mode, None),
        parent = role_card(
            "parent",
            "家长",
            "查看孩子的提交、回复与班级征集",
            "users",
            ""
        ),
        teacher = role_card(
            "teacher",
            "老师",
            "发布征集、批阅提交、写回复",
            "book",
            "sky"
        ),
        admin = role_card(
            "admin",
            "管理员",
            "管理班级、成员与全部内容",
            "shield",
            "amber"
        ),
        user_icon = icon("user"),
        demo_keys = demo_keys,
        flash = flash_html(flash)
    );

    let mut out = document_head("登录", "system", true);
    out.push_str(&format!(r#"<body>{body}<div class="toast-wrap"></div><script src="/static/app.js?v=3"></script></body></html>"#));
    out
}

pub fn login_page(mode: &str, role: &str, error: Option<&str>) -> String {
    let mode = if mode == "pc" { "pc" } else { "mob" };
    let (title, desc, icon_name, icon_class, demo_key, placeholder) = match role {
        "parent" => (
            "家长登录",
            "使用孩子的学生密钥进入班级，查看征集、提交与老师回复。",
            "users",
            "",
            db::DEMO_STUDENT_KEY,
            "例如 SC-STU-XXXX-XXXX",
        ),
        "teacher" => (
            "老师登录",
            "使用教师密钥登录，发布征集、查看全班提交并写回复。",
            "book",
            "sky",
            db::DEMO_TEACHER_KEY,
            "例如 SC-TEA-XXXX-XXXX",
        ),
        "admin" => (
            "管理员登录",
            "使用管理员密钥登录，管理班级、成员、征集与全部内容。",
            "shield",
            "amber",
            db::DEMO_ADMIN_KEY,
            "例如 SC-ADM-XXXX-XXXX",
        ),
        _ => (
            "学生登录",
            "使用学生密钥进入班级空间，上传作业与查看回复。",
            "graduation",
            "",
            db::DEMO_STUDENT_KEY,
            "例如 SC-STU-XXXX-XXXX",
        ),
    };
    let demo_label = match role {
        "teacher" => "老师演示密钥",
        "admin" => "管理员演示密钥",
        _ => "学生 / 家长演示密钥",
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
        .map(|e| {
            format!(
                r#"<div class="alert" style="margin-bottom:14px">{}{}</div>"#,
                icon("alert"),
                esc(e)
            )
        })
        .unwrap_or_default();

    let body = format!(
        r#"<div class="auth-shell">
  {hero}
  <section class="auth-panel">
    <div class="auth-card">
      {mobile_brand}
      <div class="auth-topline">
        <a class="back-link" href="/login">{arrow}返回身份选择</a>
        {mode_switch}
      </div>
      <div class="auth-heading">
        <span class="role-icon {icon_class}" style="margin-bottom:12px">{role_icon}</span>
        <h2>{title}</h2>
        <p>{desc}</p>
      </div>
      {error_html}
      <form class="login-form" method="post" action="/login/{mode}/{role}">
        <div class="field">
          <label class="label" for="key-input">班级密钥 <span class="req">*</span></label>
          <input class="input mono" id="key-input" name="key" type="text" autocomplete="off"
                 spellcheck="false" placeholder="{placeholder}" required autofocus>
          <span class="hint">每个密钥只对应一个班级。需要切换班级时，请切换到该班级的密钥。</span>
        </div>
        <button class="btn primary block" type="submit">{login_icon}登录</button>
        <details class="demo-keys">
          <summary>{demo_label}</summary>
          <div class="demo-key-list">
            <button class="demo-key" type="button" data-demo-key="{demo_key}">
              <span>{demo_label}</span><code>{demo_key}</code><span>点击填入</span>
            </button>
          </div>
        </details>
      </form>
      <div class="auth-divider">其他身份</div>
      <div class="mode-switch" style="width:100%;justify-content:space-between">{role_links}</div>
      <div class="auth-divider">或</div>
      <a class="auth-mini-link" href="/user">{user_icon}账号中心（用户名 / 密码登录）</a>
      <p class="hint" style="margin-top:16px;text-align:center">登入即表示你已获得该班级密钥的授权。</p>
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

    let mut out = document_head(title, "system", true);
    out.push_str(&format!(r#"<body>{body}<div class="toast-wrap"></div><script src="/static/app.js?v=3"></script></body></html>"#));
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
    let count_label = if staff {
        format!("{} 份提交", row.upload_count)
    } else if row.my_upload_count == 0 {
        if parent {
            "孩子尚未提交".to_string()
        } else {
            "尚未提交".to_string()
        }
    } else if parent {
        format!("孩子已提交 {} 次", row.my_upload_count)
    } else {
        format!("已提交 {} 次", row.my_upload_count)
    };
    let desc = if row.desc.trim().is_empty() {
        "暂无描述".to_string()
    } else {
        truncate(&row.desc, 90)
    };
    format!(
        r#"<article class="card hover submission-card fade-in">
  <a class="card-link" href="{href}" aria-label="打开征集 {name}"></a>
  <div class="submission-top">
    <span class="chip type">{type_label}</span>
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
    let count_label = if staff {
        format!("{} 份提交", row.upload_count)
    } else if row.my_upload_count == 0 {
        if parent {
            "孩子尚未提交".to_string()
        } else {
            "尚未提交".to_string()
        }
    } else if parent {
        format!("孩子已提交 {} 次", row.my_upload_count)
    } else {
        format!("已提交 {} 次", row.my_upload_count)
    };
    let desc = if row.desc.trim().is_empty() {
        "暂无描述".to_string()
    } else {
        truncate(&row.desc, 110)
    };
    let pending = if staff && row.pending_count > 0 {
        format!(
            r#"<span class="chip warn">{}{} 待回复</span>"#,
            icon("clock"),
            row.pending_count
        )
    } else {
        String::new()
    };
    let delete = if admin {
        format!(
            r#"<button class="btn danger sm" type="button" data-del="{id}" data-name="{name}">{trash}删除</button>"#,
            id = esc(&row.id),
            name = esc(&row.name),
            trash = icon("trash")
        )
    } else {
        String::new()
    };
    format!(
        r#"<article class="card submission-card fade-in">
  <div class="submission-top">
    <span class="chip type">{type_label}</span>
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
        action_label = if staff {
            "打开"
        } else if row.my_upload_count == 0 {
            "去提交"
        } else {
            "查看"
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
        r#"<div class="list-row fade-in">
  <a class="card-link" href="{href}" aria-label="查看提交 {title}"></a>
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
            "PARENT SPACE",
            format!("您好，<em>{}</em>的家长", esc(&ctx.display_name)),
            "孩子的每一次提交与老师的每一条回复，都在这里清晰呈现。".to_string(),
        ),
        "teacher" => (
            "TEACHER DESK",
            "把每一个反馈，<br>送到学生手里。".to_string(),
            "发布班级征集、批阅学生提交、写下一条有温度的回复。".to_string(),
        ),
        "admin" => (
            "ADMIN CONSOLE",
            "让班级的每件事，<br>都有迹可循。".to_string(),
            "管理征集与提交，处理成员与班级，掌握班级空间的实时动态。".to_string(),
        ),
        _ => (
            "STUDENT SPACE",
            "把每一次提交，<br>变成成长的刻度。".to_string(),
            "查看班级征集、上传你的作品，并收下老师的每一条回复。".to_string(),
        ),
    };
    let hero_actions = match ctx.page_role.as_str() {
        "teacher" | "admin" => format!(
            r#"<a class="btn light" href="{base}submissions/new">{plus}发布征集</a><a class="btn glass" href="{base}uploads/">{folder}提交管理</a><a class="btn glass" href="/user">{user}账号</a>"#,
            base = base,
            plus = icon("plus"),
            folder = icon("folder"),
            user = icon("user")
        ),
        "parent" => format!(
            r#"<a class="btn light" href="{base}submissions/">{inbox}浏览征集</a><a class="btn glass" href="{base}uploads/">{folder}孩子提交</a><a class="btn glass" href="/user">{user}账号</a>"#,
            base = base,
            inbox = icon("inbox"),
            folder = icon("folder"),
            user = icon("user")
        ),
        _ => format!(
            r#"<a class="btn light" href="{base}submissions/">{inbox}浏览征集</a><a class="btn glass" href="{base}uploads/">{folder}我的上传</a><a class="btn glass" href="/user">{user}账号</a>"#,
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
            r#"<div class="empty"><span class="empty-icon">{icon}</span><div class="empty-title">还没有征集</div><div class="empty-desc">当前班级还没有发布任何征集，稍后再来看看吧。</div></div>"#,
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
            r#"<div class="empty"><span class="empty-icon">{icon}</span><div class="empty-title">暂时没有需要关注的提交</div><div class="empty-desc">{hint}</div></div>"#,
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

    let float_title = match ctx.page_role.as_str() {
        "teacher" => "待回复提交",
        "admin" => "班级动态",
        "parent" => "孩子的最近提交",
        _ => "我的最近提交",
    };
    let float_sub = match ctx.page_role.as_str() {
        "teacher" => "优先处理未回复的提交",
        "admin" => "全部内容尽在掌握",
        _ => "点击即可查看回复",
    };

    let body = format!(
        r#"<section class="hero {role} fade-in">
  <div class="hero-copy">
    <span class="hero-eyebrow">{spark} {eyebrow}</span>
    <h2 class="hero-title">{hero_title}</h2>
    <p class="hero-desc">{hero_desc}</p>
    <div class="hero-actions">{hero_actions}</div>
  </div>
  <div class="hero-art" aria-hidden="true">
    <span class="hero-ring"></span><span class="hero-ring r2"></span><span class="hero-ring r3"></span>
    <div class="float-card">
      <div class="fc-top"><span>{float_title}</span><span>{role_label}</span></div>
      <div class="fc-title">{float_sub}</div>
      <div class="fc-line"></div><div class="fc-line w70"></div><div class="fc-line w45"></div>
      <div class="fc-foot"><span class="fc-dot"></span>{class_name}</div>
    </div>
  </div>
</section>
<div class="stats">{stats_html}</div>
<section class="section">
  <div class="section-head">
    <h2 class="section-title">{inbox}最近的征集</h2>
    <a class="section-link" href="{base}submissions/">查看全部{arrow}</a>
  </div>
  {submissions_html}
</section>
<section class="section">
  <div class="section-head">
    <h2 class="section-title">{folder}{uploads_title}</h2>
    <a class="section-link" href="{base}uploads/">查看全部{arrow}</a>
  </div>
  {uploads_html}
</section>"#,
        role = ctx.page_role,
        spark = icon("sparkles"),
        eyebrow = eyebrow,
        hero_title = hero_title,
        hero_desc = esc(&hero_desc),
        hero_actions = hero_actions,
        float_title = float_title,
        float_sub = float_sub,
        role_label = esc(role_label(&ctx.page_role)),
        class_name = esc(&ctx.class_name),
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
            r#"<a class="btn primary" href="{base}submissions/new">{plus}发布征集</a>"#,
            base = base,
            plus = icon("plus")
        )
    } else {
        String::new()
    };
    let content = if rows.is_empty() {
        format!(
            r#"<div class="empty"><span class="empty-icon">{icon}</span><div class="empty-title">还没有征集</div><div class="empty-desc">老师发布征集后，会在这里展示。你提交过的内容也会自动关联到对应征集。</div>{new_btn}</div>"#,
            icon = icon("inbox"),
            new_btn = if staff {
                format!(r#"<a class="btn primary" href="{base}submissions/new">发布第一个征集</a>"#)
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
    let body = format!(
        r#"<div class="page-head">
  <div>
    <h2 class="page-title">班级征集</h2>
    <p class="page-sub">{count} 个征集 · 单位：{unit}</p>
  </div>
  <div class="page-actions">{new_btn}</div>
</div>
{content}"#,
        count = rows.len(),
        unit = if staff {
            "班级"
        } else if parent {
            "孩子"
        } else {
            "本人"
        },
        new_btn = new_btn,
        content = content
    );
    layout(ctx, &body)
}

pub fn new_submission(ctx: &LayoutCtx) -> String {
    let base = ctx.base();
    let default_deadline = fmt_datetime_local(crate::tools::now_ts() + 3 * 86_400 + 12 * 3600);
    let body = format!(
        r#"<div class="page-head">
  <div>
    <a class="back-link" href="{base}submissions/">{arrow}返回征集列表</a>
    <h2 class="page-title" style="margin-top:6px">发布新征集</h2>
    <p class="page-sub">征集会立即出现在班级空间，学生与家长可以查看并提交。</p>
  </div>
</div>
<div class="detail-grid">
  <form class="form-card" id="submission-form">
    <div class="form-grid">
      <div class="field full">
        <label class="label" for="sub-name">征集名称 <span class="req">*</span></label>
        <input class="input" id="sub-name" name="name" type="text" maxlength="200" required
               placeholder="例如：函数与导数 · 周末练习">
      </div>
      <div class="field">
        <label class="label" for="sub-type">类型</label>
        <select class="select" id="sub-type" name="type">
          <option value="homework">作业</option>
          <option value="open call">公开征集</option>
          <option value="leave request">请假申请</option>
          <option value="notice">通知</option>
          <option value="other">其他</option>
        </select>
      </div>
      <div class="field">
        <label class="label" for="sub-deadline">截止时间</label>
        <input class="input" id="sub-deadline" name="deadline" type="datetime-local" value="{default_deadline}">
        <label class="check"><input type="checkbox" id="no-deadline"> 不设置截止时间</label>
      </div>
      <div class="field full">
        <label class="label" for="sub-desc">说明</label>
        <textarea class="textarea" id="sub-desc" name="desc" maxlength="2000"
                  placeholder="写清楚提交要求、格式与注意事项，学生会在详情页看到这段说明。"></textarea>
        <span class="hint" data-counter="sub-desc" data-max="2000">0 / 2000</span>
      </div>
    </div>
    <div class="form-actions">
      <button class="btn primary" type="submit" id="submission-submit">{plus}发布征集</button>
      <a class="btn ghost" href="{base}submissions/">取消</a>
      <span class="spacer"></span>
      <span class="hint">发布身份：{display}</span>
    </div>
  </form>
  <aside class="stack">
    <div class="panel">
      <div class="panel-head"><h3 class="panel-title">{spark}发布小贴士</h3></div>
      <div class="panel-body">
        <div class="stack text-sm" style="color:var(--text-2)">
          <p>1. 名称尽量具体，包含学科、主题或活动名称。</p>
          <p>2. 截止时间按北京时间显示，学生端会实时提示剩余时间。</p>
          <p>3. 学生可多次上传；老师可以在提交详情中逐条回复。</p>
          <p>4. 发布后管理员可以删除该征集，请确认内容无误。</p>
        </div>
      </div>
    </div>
    <div class="panel">
      <div class="panel-body">
        <div class="kv"><dt>当前班级</dt><dd>{class_name}</dd></div>
        <div class="kv"><dt>发布身份</dt><dd>{role}</dd></div>
        <div class="kv"><dt>数据范围</dt><dd>管理员 / 老师 · 全班</dd></div>
      </div>
    </div>
  </aside>
</div>"#,
        base = base,
        arrow = icon("arrow-left"),
        default_deadline = default_deadline,
        plus = icon("plus"),
        display = esc(&ctx.display_name),
        spark = icon("sparkles"),
        class_name = esc(&ctx.class_name),
        role = esc(role_label(&ctx.role))
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
        "班级提交"
    } else if parent {
        "孩子的提交"
    } else {
        "我的提交"
    };
    let sub = if staff {
        "查看全班提交，及时给每一份作品写下回复。".to_string()
    } else if parent {
        "孩子上传过的全部内容都在这里，可按征集回溯。".to_string()
    } else {
        "你上传过的所有内容都在这里，可按征集回溯。".to_string()
    };
    let tabs = if staff {
        format!(
            r#"<div class="segmented" role="tablist">
  <a class="segment{all}" href="{base}uploads/?filter=all">全部 {c0}</a>
  <a class="segment{pending}" href="{base}uploads/?filter=pending">待回复 {c1}</a>
  <a class="segment{replied}" href="{base}uploads/?filter=replied">已回复 {c2}</a>
</div>"#,
            all = if filter == "all" { " active" } else { "" },
            pending = if filter == "pending" { " active" } else { "" },
            replied = if filter == "replied" { " active" } else { "" },
            base = base,
            c0 = counts.0,
            c1 = counts.1,
            c2 = counts.2
        )
    } else {
        format!(
            r#"<span class="chip soft">{}{} 次提交</span>"#,
            icon("folder"),
            rows.len()
        )
    };
    let content = if rows.is_empty() {
        format!(
            r#"<div class="empty"><span class="empty-icon">{icon}</span><div class="empty-title">{empty_title}</div><div class="empty-desc">{empty_desc}</div><a class="btn primary" href="{base}submissions/">去征集列表看看</a></div>"#,
            icon = icon("folder"),
            empty_title = if filter == "pending" {
                "没有待回复的提交"
            } else if filter == "replied" {
                "还没有已回复的提交"
            } else {
                "还没有提交"
            },
            empty_desc = if staff {
                "学生上传后，这里会实时出现新的提交。"
            } else if parent {
                "孩子尚未提交过内容，可以从征集列表开始。"
            } else {
                "选择一个征集，上传你的作业或作品吧。"
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
            r#"<button class="btn outline" type="button" id="export-btn" data-submission="{id}" data-name="{name}">{download}导出 JSON</button>"#,
            id = esc(&sub.id),
            name = esc(&sub.name),
            download = icon("download")
        )
    } else {
        String::new()
    };
    let delete = if admin {
        format!(
            r#"<button class="btn danger" type="button" data-del="{id}" data-name="{name}" data-redirect="{base}submissions/">{trash}删除征集</button>"#,
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
            r#"<a class="btn primary" href="{base}submission/{id}/upload">{upload}上传作品</a>"#,
            base = base,
            id = esc(&sub.id),
            upload = icon("upload")
        )
    } else {
        String::new()
    };
    let list = if rows.is_empty() {
        format!(
            r#"<div class="empty"><span class="empty-icon">{icon}</span><div class="empty-title">还没有提交</div><div class="empty-desc">{desc}</div>{upload_btn}</div>"#,
            icon = icon("folder"),
            desc = if staff {
                "学生提交后会显示在这里。"
            } else if ctx.page_role == "parent" {
                "孩子还没有为这个征集上传内容。"
            } else {
                "你还没有为这个征集上传内容。"
            },
            upload_btn = if can_upload {
                format!(
                    r#"<a class="btn primary" href="{base}submission/{id}/upload">上传第一份作品</a>"#,
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
        r#"<div class="page-head">
  <div>
    <a class="back-link" href="{base}submissions/">{arrow}返回征集列表</a>
    <h2 class="page-title" style="margin-top:6px">{name}</h2>
    <p class="page-sub">{created_name} 发布 · {created}</p>
  </div>
  <div class="page-actions">{upload_btn}{export}{delete}</div>
</div>
<section class="card pad-lg" style="margin-bottom:18px">
  <div class="submission-top">
    <span class="chip type">{type_label}</span>
    {deadline}
  </div>
  <p style="color:var(--text-2);font-size:13.5px;line-height:1.75;margin-top:4px">{desc}</p>
  <div class="submission-meta">
    <span class="meta-item">{file_icon}<span>{count_label}</span></span>
    <span class="meta-item">{clock_icon}<span>发布于 {created}</span></span>
    {pending}
  </div>
</section>
<section class="section" style="margin-top:0">
  <div class="section-head">
    <h2 class="section-title">{folder}{list_title}<span class="count">{n}</span></h2>
    <span class="hint">{list_hint}</span>
  </div>
  {list}
</section>"#,
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
            "暂无说明".to_string()
        } else {
            esc(&sub.desc)
        },
        file_icon = icon("file"),
        count_label = if staff {
            format!("{} 份提交", sub.upload_count)
        } else {
            format!("已提交 {} 次", sub.my_upload_count)
        },
        clock_icon = icon("clock"),
        pending = if staff && sub.pending_count > 0 {
            format!(
                r#"<span class="chip warn">{}{} 待回复</span>"#,
                icon("clock"),
                sub.pending_count
            )
        } else {
            String::new()
        },
        folder = icon("folder"),
        list_title = if staff {
            "全部提交"
        } else {
            "我的提交"
        },
        n = rows.len(),
        list_hint = if staff {
            "点击任意提交查看附件并回复"
        } else {
            "仅显示你自己的提交"
        },
        list = list
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
            format!("孩子已为这个征集提交过 {existing} 次，本次会作为新的上传保留。")
        } else {
            format!("你已为这个征集提交过 {existing} 次，本次会作为新的上传保留。")
        }
    } else if parent {
        "这是孩子在本次征集的第一次提交。".to_string()
    } else {
        "这是你在本征集的第一次提交。".to_string()
    };
    let last_link = last_id
        .map(|id| {
            format!(
                r#" <a href="{base}upload/{id}/">查看上一次提交</a>"#,
                base = base,
                id = esc(id)
            )
        })
        .unwrap_or_default();
    let body = format!(
        r#"<div class="page-head">
  <div>
    <a class="back-link" href="{base}submission/{id}/">{arrow}返回征集详情</a>
    <h2 class="page-title" style="margin-top:6px">上传到「{name}」</h2>
    <p class="page-sub">支持图片、PDF 与常见文档，单个上传最多 8 个附件、总计约 18MB。</p>
  </div>
</div>
<div class="detail-grid">
  <form class="form-card" id="upload-form" data-submission="{id}">
    <div class="form-grid">
      <div class="field full">
        <label class="label" for="upload-title">标题 <span class="req">*</span></label>
        <input class="input" id="upload-title" type="text" maxlength="200" required value="{default_title}">
        <span class="hint">给这份提交一个清晰的名字，方便老师批阅。</span>
      </div>
      <div class="field full">
        <label class="label" for="upload-desc">说明</label>
        <textarea class="textarea" id="upload-desc" maxlength="5000"
                  placeholder="补充说明、解题思路、遇到的问题……"></textarea>
        <span class="hint" data-counter="upload-desc" data-max="5000">0 / 5000</span>
      </div>
      <div class="field full">
        <label class="label">附件</label>
        <div class="dropzone" id="dropzone">
          <input type="file" id="file-input" multiple>
          <span class="dropzone-icon">{upload}</span>
          <span class="dropzone-text">拖拽文件到这里，或 <span>点击选择文件</span></span>
          <span class="dropzone-hint">图片 / PDF / 文档 · 最多 8 个 · 总计 18MB</span>
        </div>
        <div class="file-list" id="file-list"></div>
      </div>
    </div>
    <div class="form-actions">
      <button class="btn primary" type="submit" id="upload-submit">{send}提交</button>
      <a class="btn ghost" href="{base}submission/{id}/">取消</a>
      <span class="spacer"></span>
      <span class="hint">{existing_hint}{last_link}</span>
    </div>
  </form>
  <aside class="stack">
    <div class="panel">
      <div class="panel-head"><h3 class="panel-title">{folder}征集信息</h3></div>
      <div class="panel-body">
        <div class="stack">
          <div><span class="chip type">{type_label}</span> {deadline}</div>
          <strong style="font-size:15px">{name}</strong>
          <p class="text-sm" style="color:var(--text-2);line-height:1.7">{desc}</p>
        </div>
        <div class="divider"></div>
        <div class="kv"><dt>发布人</dt><dd>{created_name}</dd></div>
        <div class="kv"><dt>发布时间</dt><dd>{created}</dd></div>
        <div class="kv"><dt>截止时间</dt><dd>{deadline_text}</dd></div>
      </div>
    </div>
    <div class="account-note">
      提示：截止后仍可补交，老师会看到每次提交的时间。
    </div>
  </aside>
</div>"#,
        base = base,
        arrow = icon("arrow-left"),
        id = esc(&sub.id),
        name = esc(&sub.name),
        default_title = esc(&default_title),
        upload = icon("upload"),
        send = icon("send"),
        existing_hint = esc(&existing_hint),
        last_link = last_link,
        folder = icon("folder"),
        type_label = esc(&type_label(&sub.kind)),
        deadline = deadline_badge(sub.deadline, now),
        desc = if sub.desc.trim().is_empty() {
            "暂无说明".to_string()
        } else {
            esc(&sub.desc)
        },
        created_name = esc(&sub.created_name),
        created = esc(&fmt_datetime(sub.created_at)),
        deadline_text = sub
            .deadline
            .map(fmt_datetime)
            .unwrap_or_else(|| "不设截止时间".to_string())
    );
    layout(ctx, &body)
}

// ---------------------------------------------------------------------------
// 附件 / 回复
// ---------------------------------------------------------------------------

fn attachment_gallery(data: &Value) -> String {
    let Some(arr) = data.as_array() else {
        return r#"<p class="hint">该上传没有附件。</p>"#.to_string();
    };
    if arr.is_empty() {
        return r#"<p class="hint">该上传没有附件。</p>"#.to_string();
    }
    let mut out = String::from(r#"<div class="attachment-grid">"#);
    for item in arr {
        let Some(att) = parse_attachment(item) else {
            continue;
        };
        let size = format_size(b64_binary_len(&att.b64));
        let name = esc(&att.name);
        let mime = esc(&att.mime);
        if att.mime.starts_with("image/") {
            let src = format!("data:{};base64,{}", att.mime, att.b64);
            let _ = write!(
                out,
                r#"<div class="attachment">
  <div class="attachment-thumb" role="button" tabindex="0" data-lightbox data-name="{name}" aria-label="预览附件 {name}">
    <img src="{src}" alt="{name}">
  </div>
  <div class="attachment-caption">
    <strong><a href="{src}" download="{name}">{name}</a></strong>
    <span>{mime} · {size}</span>
  </div>
</div>"#,
                name = name,
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
            r#"<div class="empty" style="padding:30px 18px"><span class="empty-icon">{icon}</span><div class="empty-title">还没有回复</div><div class="empty-desc">{desc}</div></div>"#,
            icon = icon("reply"),
            desc = if admin {
                "写下第一条回复，让学生收到反馈。"
            } else {
                "老师回复后会显示在这里。"
            }
        );
    }
    let mut out = String::from(r#"<div class="replies">"#);
    for r in replies {
        let delete = if admin {
            format!(
                r#"<button class="btn ghost sm" type="button" data-del="{id}" data-name="回复">{trash}删除</button>"#,
                id = esc(&r.id),
                trash = icon("trash")
            )
        } else {
            String::new()
        };
        let _ = write!(
            out,
            r#"<div class="reply">
  <span class="avatar sm{ac}">{initial}</span>
  <div class="reply-body">
    <div class="reply-head">
      <span class="reply-author">{author}</span>
      {role_badge}
      <span class="reply-time">{time}</span>
      <span style="flex:1"></span>{delete}
    </div>
    <div class="reply-content">{content}</div>
  </div>
</div>"#,
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
            r#"<form class="reply-form" data-reply-form data-upload="{upload_id}">
  <div class="quick-replies">
    <button class="quick-reply" type="button" data-quick-reply="已完成，步骤清晰，继续保持！">已完成</button>
    <button class="quick-reply" type="button" data-quick-reply="请补充解题过程，并标注关键步骤。">请补充过程</button>
    <button class="quick-reply" type="button" data-quick-reply="已收到，稍后为你详细批注。">已收到</button>
  </div>
  <div class="field">
    <textarea class="textarea" name="content" maxlength="5000" placeholder="写下对这份提交的回复……" required></textarea>
  </div>
  <div class="form-actions" style="margin-top:12px;padding-top:12px">
    <button class="btn primary" type="submit">{send}发布回复</button>
    <span class="hint">回复会立即展示给提交人和家长</span>
  </div>
</form>"#,
            upload_id = esc(&upload.id),
            send = icon("send")
        )
    } else {
        format!(
            r#"<div class="account-note">{} 只有老师和管理员可以回复这份提交。</div>"#,
            icon("lock")
        )
    };
    let reupload = format!(
        r#"<a class="btn outline block" href="{base}submission/{sub_id}/upload">{upload}重新上传</a>"#,
        base = base,
        sub_id = esc(&upload.submission_id),
        upload = icon("upload")
    );
    let delete_upload = if admin {
        format!(
            r#"<button class="btn danger block" type="button" data-del="{id}" data-name="{title}" data-redirect="{base}submission/{sub_id}/all">{trash}删除本次提交</button>"#,
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
            .map(|n| format!(" · {n:02} 号"))
            .unwrap_or_default();
        format!(
            r#"<div class="kv"><dt>学号</dt><dd>{}{}</dd></div>"#,
            esc(upload.student_id.as_deref().unwrap_or("")),
            esc(&short)
        )
    } else {
        String::new()
    };
    let body = format!(
        r#"<div class="page-head">
  <div>
    <a class="back-link" href="{base}submission/{sub_id}/all">{arrow}返回「{sub_name}」全部提交</a>
    <h2 class="page-title" style="margin-top:6px">{title}</h2>
    <p class="page-sub">{uploader} · {created}</p>
  </div>
  <div class="page-actions">
    {status}
    <a class="btn outline" href="{base}submission/{sub_id}/all">{folder}查看征集</a>
  </div>
</div>
<div class="detail-grid">
  <div class="detail-main">
    <div class="panel">
      <div class="panel-head">
        <h3 class="panel-title">{paperclip}附件与内容</h3>
        <span class="hint">{time}</span>
      </div>
      <div class="panel-body">
        {desc}
        <div class="divider"></div>
        {gallery}
      </div>
    </div>
    <div class="panel">
      <div class="panel-head">
        <h3 class="panel-title">{reply}回复<span class="count">{reply_count}</span></h3>
        <span class="hint">老师 / 管理员</span>
      </div>
      <div class="panel-body">
        {replies}
        <div class="divider"></div>
        {reply_form}
      </div>
    </div>
  </div>
  <aside class="detail-side">
    <div class="panel">
      <div class="panel-head"><h3 class="panel-title">{user}提交信息</h3></div>
      <div class="panel-body">
        <div class="kv"><dt>提交人</dt><dd>{uploader}</dd></div>
        <div class="kv"><dt>身份</dt><dd>{role_badge}</dd></div>
        {student_row}
        <div class="kv"><dt>征集</dt><dd><a href="{base}submission/{sub_id}/">{sub_name}</a></dd></div>
        <div class="kv"><dt>提交时间</dt><dd>{created}</dd></div>
        {last_reply_row}
        <div class="kv"><dt>回复数量</dt><dd>{reply_count}</dd></div>
      </div>
    </div>
    <div class="panel">
      <div class="panel-body stack">
        {reupload}
        {delete_upload}
        <a class="btn ghost block" href="{base}uploads/">{back}返回提交列表</a>
      </div>
    </div>
    <div class="account-note">
      {note}
    </div>
  </aside>
</div>"#,
        base = base,
        sub_id = esc(&sub.id),
        sub_name = esc(&sub.name),
        arrow = icon("arrow-left"),
        title = esc(&upload.title),
        uploader = esc(&upload.uploader_name),
        created = esc(&fmt_datetime(upload.created_at)),
        status = upload_status(upload),
        folder = icon("folder"),
        paperclip = icon("paperclip"),
        time = esc(&relative_time(upload.created_at, now)),
        desc = if upload.desc.trim().is_empty() {
            r#"<p class="hint">提交人没有填写说明。</p>"#.to_string()
        } else {
            format!(
                r#"<p style="color:var(--text-2);line-height:1.8;white-space:pre-wrap">{}</p>"#,
                esc(&upload.desc)
            )
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
                r#"<div class="kv"><dt>最近回复</dt><dd>{}</dd></div>"#,
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
            "提示：回复发出后不可编辑，请确认内容后提交。"
        } else {
            "提示：你可以在截止后补交，老师会看到每次提交的时间。"
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
                r#"<div class="account-note" style="margin-bottom:14px">{}{}</div>"#,
                icon("key"),
                esc(n)
            )
        })
        .unwrap_or_default();
    let body = format!(
        r#"<div class="auth-shell">
  {hero}
  <section class="auth-panel">
    <div class="auth-card wide">
      {mobile_brand}
      <div class="auth-topline">
        <a class="back-link" href="/login">{arrow}返回角色登录</a>
        {mode_switch}
      </div>
      <div class="auth-heading">
        <h2>账号中心</h2>
        <p>使用用户名与密码登录，管理你绑定的班级身份与个人偏好。</p>
      </div>
      {flash}
      {note}
      <div class="login-pair" style="grid-template-columns:minmax(0,1fr)">
        <form class="login-form" method="post" action="/user/login">
          <div class="field">
            <label class="label" for="username">用户名</label>
            <input class="input" id="username" name="username" type="text" autocomplete="username" required>
          </div>
          <div class="field">
            <label class="label" for="password">密码</label>
            <input class="input" id="password" name="password" type="password" autocomplete="current-password" required>
          </div>
          <button class="btn primary block" type="submit">{lock}登录账号</button>
          <p class="hint" style="margin-top:10px">演示账号：student / student123 · teacher / teacher123 · admin / admin123</p>
        </form>
        <div class="auth-divider">第一次使用？创建账号并绑定班级密钥</div>
        <form class="login-form" method="post" action="/user/register">
          <div class="form-grid">
            <div class="field">
              <label class="label" for="new-username">用户名</label>
              <input class="input" id="new-username" name="username" type="text" autocomplete="username" required>
            </div>
            <div class="field">
              <label class="label" for="new-password">密码</label>
              <input class="input" id="new-password" name="password" type="password" autocomplete="new-password" minlength="6" required>
            </div>
            <div class="field">
              <label class="label" for="new-password2">确认密码</label>
              <input class="input" id="new-password2" name="password2" type="password" autocomplete="new-password" minlength="6" required>
            </div>
            <div class="field">
              <label class="label" for="claim-key">班级密钥</label>
              <input class="input mono" id="claim-key" name="key" type="text" value="{prefill_key}" placeholder="可选，稍后也可在账号内添加">
            </div>
          </div>
          <button class="btn outline block" type="submit" style="margin-top:16px">{key}创建账号并绑定</button>
          <p class="hint" style="margin-top:10px">密钥可以之后在账号中心添加。每个密钥只对应一个班级。</p>
        </form>
      </div>
    </div>
  </section>
</div>"#,
        hero = auth_hero("account"),
        mobile_brand = auth_brand(true),
        arrow = icon("arrow-left"),
        mode_switch = mode_switch(mode, None),
        flash = flash_html(flash),
        note = note,
        lock = icon("lock"),
        key = icon("key"),
        prefill_key = esc(prefill_key)
    );
    let mut out = document_head("账号中心", "system", true);
    out.push_str(&format!(r#"<body>{body}<div class="toast-wrap"></div><script src="/static/app.js?v=3"></script></body></html>"#));
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
                    "light" => "浅色",
                    "dark" => "深色",
                    _ => "跟随系统",
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
                    "手机版"
                } else {
                    "电脑版"
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
            r#"<div class="empty" style="padding:34px 18px"><span class="empty-icon">{icon}</span><div class="empty-title">还没有绑定班级身份</div><div class="empty-desc">使用管理员发放的班级密钥，在下方添加你的第一个身份。</div></div>"#,
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
                    r#"<div class="auth-row">
  <span class="avatar sm{ac}">{initial}</span>
  <div class="auth-row-main">
    <div class="auth-row-title">
      {class_name}
      {role_badge}
      {current}
    </div>
    <div class="auth-row-sub">
      <span class="auth-row-name">{auth_name}</span>
      {student}
      <span class="key" data-full="{full}" data-mask="{masked}">{masked}</span>
    </div>
  </div>
  <div class="key-actions">
    <button class="btn ghost sm" type="button" data-rename="{full}" data-name="{auth_name}" title="为此身份改名" aria-label="为此身份改名">{pencil}改名</button>
    <button class="btn ghost sm" type="button" data-reveal>显示</button>
    <button class="btn ghost sm" type="button" data-copy="{full}" title="复制密钥">{copy}</button>
    <form class="enter-form" method="post" action="/user/enter">
      <input type="hidden" name="auth_id" value="{id}">
      <button class="btn outline sm" type="submit">{arrow}进入</button>
    </form>
    <form method="post" action="/user/auth/{id}/delete" data-confirm="确定删除「{class_name}」的{role_label}身份吗？删除后该密钥将立即失效。" data-confirm-text="删除密钥">
      <button class="btn danger sm" type="submit" title="删除该身份" aria-label="删除该身份">{trash}</button>
    </form>
  </div>
</div>"#,
                    ac = avatar_class(&a.role),
                    initial = esc(&initial(&display)),
                    class_name = esc(&a.class_name),
                    role_badge = role_badge_text(&a.role),
                    current = if current {
                        r#"<span class="current-tag">当前身份</span>"#
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
                    role_label = esc(role_label(&a.role))
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
  <label class="label" for="member-admin">管理员身份</label>
  <select class="select" id="member-admin">{opts}</select>
  <span class="hint">生成密钥时会使用该身份所在班级。</span>
</div>"#,
            opts = opts
        )
    };

    let admin_area = if admin_auths.is_empty() {
        format!(
            r#"<div class="account-note">{} 你目前不是任何班级的管理员，因此不能创建班级或生成成员密钥。请先在某个班级获得管理员密钥。</div>"#,
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
            r#"<div class="stack">
  <form class="stack" method="post" action="/user/class">
    <div class="form-grid">
      <div class="field">
        <label class="label" for="class-name">班级名称</label>
        <input class="input" id="class-name" name="name" type="text" maxlength="40" required placeholder="例如：高一（4）班">
      </div>
      <div class="field">
        <label class="label" for="class-auth">以哪个管理员身份创建</label>
        <select class="select" id="class-auth" name="admin_auth_id">{class_opts}</select>
      </div>
      <div class="field full">
        <label class="label" for="class-desc">班级简介</label>
        <textarea class="textarea" id="class-desc" name="desc" maxlength="500" placeholder="一句话介绍这个班级"></textarea>
      </div>
    </div>
    <div><button class="btn primary" type="submit">{plus}创建班级并成为管理员</button></div>
  </form>
  <div class="divider"></div>
  <form class="stack" id="member-form">
    <div class="form-grid">
      {admin_select}
      <div class="field">
        <label class="label" for="member-kind">成员类型</label>
        <select class="select" id="member-kind">
          <option value="Student">学生</option>
          <option value="Teacher">老师</option>
          <option value="Admin">管理员</option>
        </select>
      </div>
      <div class="field">
        <label class="label" for="member-quantity">生成数量</label>
        <input class="input" id="member-quantity" type="number" min="1" max="100" value="1">
      </div>
    </div>
    <div><button class="btn outline" type="submit">{key}生成成员密钥</button></div>
    <div class="member-result" id="member-result" hidden></div>
    <p class="hint">生成后密钥仅展示一次，请复制后交给对应成员。成员可在账号中心绑定该密钥。</p>
  </form>
</div>"#,
            admin_select = admin_select,
            class_opts = class_opts,
            plus = icon("plus"),
            key = icon("key")
        )
    };

    let body = format!(
        r#"{flash}
<section class="account-hero">
  <span class="avatar lg">{initial}</span>
  <div class="account-meta">
    <h2>{display}</h2>
    <p class="username">@{username}</p>
  </div>
  <div class="account-stats">
    <div class="account-stat"><strong>{n_auths}</strong><span>已绑定身份</span></div>
    <div class="account-stat"><strong>{n_classes}</strong><span>所在班级</span></div>
    <div class="account-stat"><strong>{n_admin}</strong><span>管理员身份</span></div>
  </div>
</section>
<div class="account-grid">
  <section class="panel">
    <div class="panel-head"><h3 class="panel-title">{settings}偏好设置</h3></div>
    <div class="panel-body">
      <form class="stack" method="post" action="/user/prefs" id="prefs-form">
        <div class="field">
          <label class="label" for="display-name">显示名称</label>
          <input class="input" id="display-name" name="display_name" type="text" maxlength="40" value="{display}">
        </div>
        <div class="form-grid">
          <div class="field">
            <label class="label" for="pref-theme">主题</label>
            <select class="select" id="pref-theme" name="theme">{theme_sel}</select>
          </div>
          <div class="field">
            <label class="label" for="pref-mode">默认视图</label>
            <select class="select" id="pref-mode" name="mode">{mode_sel}</select>
          </div>
        </div>
        <div><button class="btn primary" type="submit">保存偏好</button></div>
      </form>
    </div>
  </section>
  <section class="panel">
    <div class="panel-head"><h3 class="panel-title">{lock}登录与安全</h3></div>
    <div class="panel-body">
      <form class="stack" method="post" action="/user/password">
        <div class="field">
          <label class="label" for="old-pw">当前密码</label>
          <input class="input" id="old-pw" name="old" type="password" autocomplete="current-password" required>
        </div>
        <div class="form-grid">
          <div class="field">
            <label class="label" for="new-pw">新密码</label>
            <input class="input" id="new-pw" name="new" type="password" minlength="6" autocomplete="new-password" required>
          </div>
          <div class="field">
            <label class="label" for="new-pw2">确认新密码</label>
            <input class="input" id="new-pw2" name="new2" type="password" minlength="6" autocomplete="new-password" required>
          </div>
        </div>
        <div><button class="btn primary" type="submit">修改密码</button></div>
      </form>
      <div class="divider"></div>
      <form method="post" action="/user/logout">
        <button class="btn danger block" type="submit">{logout}退出账号中心</button>
      </form>
    </div>
  </section>
  <section class="panel account-section">
    <div class="panel-head">
      <h3 class="panel-title">{key}我的班级身份<span class="count">{n_auths}</span></h3>
      <span class="hint">密钥只对你可见</span>
    </div>
    <div class="panel-body">
      {auths}
      <div class="divider"></div>
      <form class="row" method="post" action="/user/claim" style="align-items:flex-end">
        <div class="field grow">
          <label class="label" for="claim-key2">添加已有密钥</label>
          <input class="input mono" id="claim-key2" name="key" type="text" placeholder="输入管理员发放的班级密钥" required>
        </div>
        <button class="btn outline" type="submit">{plus}绑定到账号</button>
      </form>
    </div>
  </section>
  <section class="panel account-section">
    <div class="panel-head">
      <h3 class="panel-title">{shield}班级管理<span class="count">管理员</span></h3>
      <span class="hint">创建班级 / 生成成员密钥</span>
    </div>
    <div class="panel-body">{admin_area}</div>
  </section>
</div>"#,
        flash = flash_html(flash),
        initial = esc(&initial(&display)),
        display = esc(&display),
        username = esc(&user.username),
        n_auths = auths.len(),
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
        r#"<div class="empty" style="margin-top:6vh;padding:60px 22px">
  <span class="empty-icon">{icon}</span>
  <div class="empty-title">页面或内容不存在</div>
  <div class="empty-desc">{msg}</div>
  <a class="btn primary" href="/home">返回首页</a>
</div>"#,
        icon = icon("alert"),
        msg = esc(msg)
    );
    match ctx {
        Some(ctx) => (
            axum::http::StatusCode::NOT_FOUND,
            axum::response::Html(layout(ctx, &body)),
        )
            .into_response(),
        None => crate::tools::plain_error(axum::http::StatusCode::NOT_FOUND, "页面不存在", msg),
    }
}
