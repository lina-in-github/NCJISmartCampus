use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ROLE_STUDENT: &str = "student";
pub const ROLE_TEACHER: &str = "teacher";
pub const ROLE_ADMIN: &str = "admin";

// ---------------------------------------------------------------------------
// 全局状态
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<crate::db::Database>,
    /// 角色会话：sc_sid cookie -> session（包含当前 auth）
    pub sessions: Arc<Mutex<HashMap<String, Session>>>,
    /// 账号会话：sc_uid cookie -> user id（用于 /user 账号中心）
    pub user_sessions: Arc<Mutex<HashMap<String, i64>>>,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub auth_id: String,
    /// 页面角色：parent / student / teacher / admin
    pub page_role: String,
    pub mode: String,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct AuthCtx {
    pub id: String,
    pub key: String,
    pub class_id: String,
    pub class_name: String,
    pub class_desc: String,
    pub user_id: Option<i64>,
    pub role: String,
    pub student_id: Option<String>,
    pub short_id: Option<i64>,
    pub display_name: String,
}

impl AuthCtx {
    pub fn is_staff(&self) -> bool {
        self.role == ROLE_TEACHER || self.role == ROLE_ADMIN
    }

    pub fn is_admin(&self) -> bool {
        self.role == ROLE_ADMIN
    }

    pub fn is_student(&self) -> bool {
        self.role == ROLE_STUDENT
    }

    pub fn identity(&self) -> String {
        if self.display_name.trim().is_empty() {
            match self.role.as_str() {
                ROLE_ADMIN => crate::strings::t("identity_admin"),
                ROLE_TEACHER => crate::strings::t("identity_teacher"),
                _ => match &self.student_id {
                    Some(id) => crate::strings::f1("identity_student_id", id),
                    None => crate::strings::t("identity_student"),
                },
            }
        } else {
            self.display_name.clone()
        }
    }

    pub fn page_role(&self) -> &'static str {
        match self.role.as_str() {
            ROLE_ADMIN => "admin",
            ROLE_TEACHER => "teacher",
            _ => "student",
        }
    }

    pub fn student_no(&self) -> String {
        match (&self.student_id, self.short_id) {
            (Some(id), Some(short)) => {
                crate::strings::f2("student_no_full", id, format!("{short:02}"))
            }
            (Some(id), None) => id.clone(),
            (None, Some(short)) => crate::strings::f1("student_no_short", format!("{short:02}")),
            _ => String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct UserCtx {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub prefs: Prefs,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_theme() -> String {
    "system".to_string()
}
fn default_mode() -> String {
    "mob".to_string()
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            mode: default_mode(),
        }
    }
}

impl Prefs {
    pub fn from_json(raw: &str) -> Self {
        let mut p: Prefs = serde_json::from_str(raw).unwrap_or_default();
        p.sanitize();
        p
    }

    pub fn sanitize(&mut self) {
        self.theme = match self.theme.as_str() {
            "light" | "dark" | "system" => self.theme.clone(),
            _ => "system".to_string(),
        };
        self.mode = match self.mode.as_str() {
            "mob" | "pc" => self.mode.clone(),
            _ => "mob".to_string(),
        };
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

// ---------------------------------------------------------------------------
// 页面 / API 视图
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct SubmissionRow {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub kind: String,
    pub deadline: Option<i64>,
    pub created_at: i64,
    pub created_name: String,
    pub upload_count: i64,
    pub my_upload_count: i64,
    pub pending_count: i64,
    pub last_upload_id: Option<String>,
}

impl SubmissionRow {
    /// 公示版（notice）只展示详情，不开放作业提交。
    pub fn is_notice(&self) -> bool {
        self.kind.trim().eq_ignore_ascii_case("notice")
    }

    /// 问卷模式：简介中可包含表单控件，提交时自动组装 JSON 答案。
    pub fn is_survey(&self) -> bool {
        self.kind.trim().eq_ignore_ascii_case("survey")
    }

    /// 公示版在截止时间前置顶；未设置截止时间的公示长期有效。
    pub fn is_pinned_notice(&self, now: i64) -> bool {
        self.is_notice() && self.deadline.map_or(true, |deadline| deadline > now)
    }
}

#[derive(Clone, Debug, Default)]
pub struct UploadRow {
    pub id: String,
    pub submission_id: String,
    pub submission_name: String,
    pub title: String,
    pub desc: String,
    pub data: Value,
    pub created_at: i64,
    pub uploader_name: String,
    pub student_id: Option<String>,
    pub short_id: Option<i64>,
    pub role: String,
    pub auth_id: Option<String>,
    pub reply_count: i64,
    pub last_reply_at: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct ReplyRow {
    pub id: String,
    pub content: String,
    pub replier_name: String,
    pub role: String,
    pub created_at: i64,
}

// ---------------------------------------------------------------------------
// API 请求体
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct UploadReq {
    pub auth: String,
    pub which: String,
    pub title: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub data: Value,
}

#[derive(Deserialize)]
pub struct DownloadReq {
    pub auth: String,
    pub which: String,
    #[serde(default, rename = "match_mode", alias = "matchMode")]
    pub match_mode: Option<Value>,
}

#[derive(Deserialize)]
pub struct NewSubmissionReq {
    pub auth: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default, rename = "type")]
    pub kind: String,
    #[serde(default, rename = "deadLine", alias = "deadline")]
    pub deadline: Option<Value>,
}

#[derive(Deserialize)]
pub struct AllSubmissionReq {
    pub auth: String,
    #[serde(default, rename = "match_mode", alias = "matchMode")]
    pub match_mode: Option<Value>,
}

#[derive(Deserialize)]
pub struct ReplyReq {
    pub auth: String,
    #[serde(rename = "uploadID", alias = "upload_id", alias = "uploadId")]
    pub upload_id: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct NewClassReq {
    pub auth: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Deserialize)]
pub struct NewUserReq {
    pub auth: String,
    #[serde(rename = "classID", alias = "class_id", alias = "classId")]
    pub class_id: String,
    pub quantity: i64,
    #[serde(default, rename = "type")]
    pub kind: String,
}

#[derive(Deserialize)]
pub struct DelKeyReq {
    pub auth: String,
}

#[derive(Deserialize)]
pub struct RenameAuthReq {
    pub auth: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Deserialize)]
pub struct DelObjectReq {
    /// 官方接口只要求 id；auth 可选（缺省时回退到登录 cookie）。
    #[serde(default)]
    pub auth: Option<String>,
    pub id: String,
}

// ---------------------------------------------------------------------------
// 页面表单
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginForm {
    #[serde(default)]
    pub key: String,
}

#[derive(Deserialize)]
pub struct UserLoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct UserRegisterForm {
    pub username: String,
    pub password: String,
    pub password2: String,
    #[serde(default)]
    pub key: String,
}

#[derive(Deserialize)]
pub struct ClaimForm {
    #[serde(default)]
    pub key: String,
}

#[derive(Deserialize)]
pub struct PrefsForm {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub theme: String,
    #[serde(default)]
    pub mode: String,
}

#[derive(Deserialize)]
pub struct PasswordForm {
    pub old: String,
    pub new: String,
    pub new2: String,
}

#[derive(Deserialize)]
pub struct EnterClassForm {
    pub auth_id: String,
}

#[derive(Deserialize)]
pub struct ClassForm {
    pub admin_auth_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub desc: String,
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_sanitize() {
        let mut p = Prefs {
            theme: "neon".into(),
            mode: "watch".into(),
        };
        p.sanitize();
        assert_eq!(p.theme, "system");
        assert_eq!(p.mode, "mob");
    }

    fn init_test_strings() {
        if crate::strings::stats().0 == 0 {
            let path = std::env::temp_dir().join(format!(
                "smart-campus-types-{}-strings.db",
                std::process::id()
            ));
            let _ = crate::strings::init(
                concat!(env!("CARGO_MANIFEST_DIR"), "/strings"),
                &path.to_string_lossy(),
                "zh-CN",
            );
            let _ = std::fs::remove_file(path);
        }
    }

    #[test]
    fn auth_identity_fallback() {
        init_test_strings();
        let a = AuthCtx {
            id: "auth".into(),
            key: "k".into(),
            class_id: "c".into(),
            class_name: "c".into(),
            class_desc: String::new(),
            user_id: None,
            role: "student".into(),
            student_id: Some("G2603-01".into()),
            short_id: Some(1),
            display_name: String::new(),
        };
        assert_eq!(a.identity(), "学生 G2603-01");
        assert_eq!(a.student_no(), "G2603-01 · 01 号");
        assert!(a.is_student());
    }
}
