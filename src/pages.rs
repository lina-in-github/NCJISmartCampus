use std::collections::HashMap;

use axum::{
    extract::{Form, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use rusqlite::{params, OptionalExtension};
use serde_json::Value;

use crate::db;
use crate::tools::{
    clear_cookie, constant_time_eq, cookie_value, create_role_session, create_user_session,
    current_user, hash_password, invalidate_auth_sessions, now_ts, rand_string, role_session,
    url_encode, user_session_id, SID_COOKIE, USID_COOKIE,
};
use crate::types::{
    AppState, AuthCtx, ClaimForm, ClassForm, EnterClassForm, LoginForm, PasswordForm, Prefs,
    PrefsForm, ReplyRow, Session, SubmissionRow, UploadRow, UserLoginForm, UserRegisterForm,
};
use crate::view::{self, Flash, LayoutCtx, StatItem};

type Q = Query<HashMap<String, String>>;

// ---------------------------------------------------------------------------
// 通用
// ---------------------------------------------------------------------------

fn redirect(location: &str) -> Response {
    Redirect::to(location).into_response()
}

fn with_cookies(location: &str, cookies: Vec<String>) -> Response {
    let mut res = Redirect::to(location).into_response();
    for c in cookies {
        crate::tools::append_cookie(&mut res, c);
    }
    res
}

fn msg_redirect(location: &str, key: &str, msg: &str) -> Response {
    let sep = if location.contains('?') { "&" } else { "?" };
    redirect(&format!("{location}{sep}{key}={}", url_encode(msg)))
}

fn flash_from(q: &HashMap<String, String>) -> Flash {
    Flash::from_map(q)
}

fn no_store(mut res: Response) -> Response {
    res.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, max-age=0"),
    );
    res.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    res
}

fn html(s: String) -> Response {
    no_store(Html(s).into_response())
}

fn html_status(status: StatusCode, s: String) -> Response {
    no_store((status, Html(s)).into_response())
}

fn detect_mode(headers: &HeaderMap) -> String {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ua.contains("mobile")
        || ua.contains("android")
        || ua.contains("iphone")
        || ua.contains("ipad")
        || ua.contains("ipod")
    {
        "mob".to_string()
    } else {
        "pc".to_string()
    }
}

fn normalize_mode(mode: &str) -> Option<&'static str> {
    match mode {
        "mob" => Some("mob"),
        "pc" => Some("pc"),
        _ => None,
    }
}

fn normalize_role(role: &str) -> Option<&'static str> {
    crate::tools::normalize_page_role(role)
}

fn clear_role_session(st: &AppState, headers: &HeaderMap) -> Option<String> {
    let token = cookie_value(headers, SID_COOKIE)?;
    st.sessions.lock().unwrap().remove(&token);
    Some(token)
}

fn clear_user_session(st: &AppState, headers: &HeaderMap) -> Option<String> {
    let token = cookie_value(headers, USID_COOKIE)?;
    st.user_sessions.lock().unwrap().remove(&token);
    Some(token)
}

fn logout_all(st: &AppState, headers: &HeaderMap) -> Response {
    clear_role_session(st, headers);
    clear_user_session(st, headers);
    with_cookies(
        "/login?ok=%E5%B7%B2%E9%80%80%E5%87%BA%E7%99%BB%E5%BD%95",
        vec![clear_cookie(SID_COOKIE), clear_cookie(USID_COOKIE)],
    )
}

// ---------------------------------------------------------------------------
// 数据库读取：列表 / 详情
// ---------------------------------------------------------------------------

fn map_submission(r: &rusqlite::Row<'_>) -> rusqlite::Result<SubmissionRow> {
    Ok(SubmissionRow {
        id: r.get(0)?,
        name: r.get(1)?,
        desc: r.get(2)?,
        kind: r.get(3)?,
        deadline: r.get(4)?,
        created_at: r.get(5)?,
        created_name: r.get(6)?,
        upload_count: r.get(7)?,
        my_upload_count: r.get(8)?,
        pending_count: r.get(9)?,
        last_upload_id: r.get(10)?,
    })
}

fn map_upload(r: &rusqlite::Row<'_>) -> rusqlite::Result<UploadRow> {
    let data_raw: String = r.get(5)?;
    let data = serde_json::from_str::<Value>(&data_raw).unwrap_or(Value::Null);
    Ok(UploadRow {
        id: r.get(0)?,
        submission_id: r.get(1)?,
        submission_name: r.get(2)?,
        title: r.get(3)?,
        desc: r.get(4)?,
        data,
        created_at: r.get(6)?,
        uploader_name: r.get(7)?,
        student_id: r.get(8)?,
        short_id: r.get(9)?,
        role: r.get(10)?,
        auth_id: r.get(11)?,
        reply_count: r.get(12)?,
        last_reply_at: r.get(13)?,
    })
}

fn submission_sql(staff: bool, with_limit: bool) -> String {
    let tail = if with_limit {
        if staff {
            " LIMIT ?2"
        } else {
            " LIMIT ?3"
        }
    } else {
        ""
    };
    if staff {
        format!(
            "SELECT s.id, s.name, s.desc, s.type, s.deadline, s.created_at, s.created_name,
                (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id),
                0,
                (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id
                    AND NOT EXISTS (SELECT 1 FROM replies r WHERE r.upload_id = u.id)),
                (SELECT u.id FROM uploads u WHERE u.submission_id = s.id
                    ORDER BY u.created_at DESC, u.id DESC LIMIT 1)
             FROM submissions s WHERE s.class_id = ?1
             ORDER BY CASE WHEN s.deadline IS NOT NULL AND s.deadline < CAST(strftime('%s','now') AS INTEGER) THEN 1 ELSE 0 END, CASE WHEN s.deadline IS NULL THEN 1 ELSE 0 END, s.deadline ASC, s.created_at DESC{tail}"
        )
    } else {
        format!(
            "SELECT s.id, s.name, s.desc, s.type, s.deadline, s.created_at, s.created_name,
                (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?2),
                (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?2),
                (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?2
                    AND NOT EXISTS (SELECT 1 FROM replies r WHERE r.upload_id = u.id)),
                (SELECT u.id FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?2
                    ORDER BY u.created_at DESC, u.id DESC LIMIT 1)
             FROM submissions s WHERE s.class_id = ?1
             ORDER BY CASE WHEN s.deadline IS NOT NULL AND s.deadline < CAST(strftime('%s','now') AS INTEGER) THEN 1 ELSE 0 END, CASE WHEN s.deadline IS NULL THEN 1 ELSE 0 END, s.deadline ASC, s.created_at DESC{tail}"
        )
    }
}

fn list_submissions(db: &rusqlite::Connection, auth: &AuthCtx, limit: i64) -> Vec<SubmissionRow> {
    let staff = auth.is_staff();
    let sql = submission_sql(staff, true);
    let mut stmt = match db.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = if staff {
        stmt.query_map(params![auth.class_id, limit], map_submission)
    } else {
        stmt.query_map(params![auth.class_id, auth.id, limit], map_submission)
    };
    match rows {
        Ok(iter) => iter.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

fn load_submission(
    db: &rusqlite::Connection,
    auth: &AuthCtx,
    sub_id: &str,
) -> Option<SubmissionRow> {
    let staff = auth.is_staff();
    let sql = if staff {
        "SELECT s.id, s.name, s.desc, s.type, s.deadline, s.created_at, s.created_name,
            (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id),
            0,
            (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id
                AND NOT EXISTS (SELECT 1 FROM replies r WHERE r.upload_id = u.id)),
            (SELECT u.id FROM uploads u WHERE u.submission_id = s.id
                ORDER BY u.created_at DESC, u.id DESC LIMIT 1)
         FROM submissions s WHERE s.id = ?1 AND s.class_id = ?2"
    } else {
        "SELECT s.id, s.name, s.desc, s.type, s.deadline, s.created_at, s.created_name,
            (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?3),
            (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?3),
            (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?3
                AND NOT EXISTS (SELECT 1 FROM replies r WHERE r.upload_id = u.id)),
            (SELECT u.id FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?3
                ORDER BY u.created_at DESC, u.id DESC LIMIT 1)
         FROM submissions s WHERE s.id = ?1 AND s.class_id = ?2"
    };
    let row = if staff {
        db.query_row(sql, params![sub_id, auth.class_id], map_submission)
    } else {
        db.query_row(sql, params![sub_id, auth.class_id, auth.id], map_submission)
    };
    row.optional().ok().flatten()
}

fn list_uploads(
    db: &rusqlite::Connection,
    auth: &AuthCtx,
    filter: Option<&str>,
    limit: i64,
    include_data: bool,
) -> Vec<UploadRow> {
    let staff = auth.is_staff();
    let data_col = if include_data { "u.data" } else { "'[]'" };
    let reply_filter = if staff {
        match filter {
            Some("pending") => "AND NOT EXISTS (SELECT 1 FROM replies r WHERE r.upload_id = u.id)",
            Some("replied") => "AND EXISTS (SELECT 1 FROM replies r WHERE r.upload_id = u.id)",
            _ => "",
        }
    } else {
        ""
    };
    let own_filter = if staff { "" } else { "AND u.auth_id = ?2" };
    let sql = format!(
        "SELECT u.id, u.submission_id, s.name, u.title, u.desc, {data_col}, u.created_at,
                u.uploader_name, u.student_id, u.short_id, u.role, u.auth_id,
                (SELECT COUNT(*) FROM replies r WHERE r.upload_id = u.id),
                (SELECT MAX(r.created_at) FROM replies r WHERE r.upload_id = u.id)
         FROM uploads u JOIN submissions s ON s.id = u.submission_id
         WHERE u.class_id = ?1 {own_filter} {reply_filter}
         ORDER BY u.created_at DESC, u.id DESC LIMIT {limit}"
    );
    let mut stmt = match db.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = if staff {
        stmt.query_map(params![auth.class_id], map_upload)
    } else {
        stmt.query_map(params![auth.class_id, auth.id], map_upload)
    };
    match rows {
        Ok(iter) => iter.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

fn load_upload(db: &rusqlite::Connection, auth: &AuthCtx, upload_id: &str) -> Option<UploadRow> {
    let row = db
        .query_row(
            "SELECT u.id, u.submission_id, s.name, u.title, u.desc, u.data, u.created_at,
                    u.uploader_name, u.student_id, u.short_id, u.role, u.auth_id,
                    (SELECT COUNT(*) FROM replies r WHERE r.upload_id = u.id),
                    (SELECT MAX(r.created_at) FROM replies r WHERE r.upload_id = u.id)
             FROM uploads u JOIN submissions s ON s.id = u.submission_id
             WHERE u.id = ?1 AND u.class_id = ?2",
            params![upload_id, auth.class_id],
            map_upload,
        )
        .optional()
        .ok()
        .flatten()?;
    if auth.is_student() && row.auth_id.as_deref() != Some(auth.id.as_str()) {
        return None;
    }
    Some(row)
}

fn load_replies(db: &rusqlite::Connection, upload_id: &str) -> Vec<ReplyRow> {
    let mut stmt = match db.prepare(
        "SELECT id, content, replier_name, role, created_at
         FROM replies WHERE upload_id = ?1 ORDER BY created_at ASC, id ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map(params![upload_id], |r| {
        Ok(ReplyRow {
            id: r.get(0)?,
            content: r.get(1)?,
            replier_name: r.get(2)?,
            role: r.get(3)?,
            created_at: r.get(4)?,
        })
    });
    match rows {
        Ok(iter) => iter.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

fn latest_upload_id(db: &rusqlite::Connection, auth: &AuthCtx, sub_id: &str) -> Option<String> {
    let sql = if auth.is_staff() {
        "SELECT id FROM uploads WHERE submission_id = ?1 ORDER BY created_at DESC, id DESC LIMIT 1"
    } else {
        "SELECT id FROM uploads WHERE submission_id = ?1 AND auth_id = ?2
         ORDER BY created_at DESC, id DESC LIMIT 1"
    };
    let row = if auth.is_staff() {
        db.query_row(sql, params![sub_id], |r| r.get::<_, String>(0))
    } else {
        db.query_row(sql, params![sub_id, auth.id], |r| r.get::<_, String>(0))
    };
    row.optional().ok().flatten()
}

fn own_upload_info(
    db: &rusqlite::Connection,
    auth: &AuthCtx,
    sub_id: &str,
) -> (i64, Option<String>) {
    let count: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM uploads WHERE submission_id = ?1 AND auth_id = ?2",
            params![sub_id, auth.id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let last = db
        .query_row(
            "SELECT id FROM uploads WHERE submission_id = ?1 AND auth_id = ?2
             ORDER BY created_at DESC, id DESC LIMIT 1",
            params![sub_id, auth.id],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten();
    (count, last)
}

fn upload_counts(db: &rusqlite::Connection, auth: &AuthCtx) -> (i64, i64, i64) {
    let own = if auth.is_staff() {
        String::new()
    } else {
        "AND u.auth_id = 'OWN'".to_string()
    };
    let _ = own;
    let total: i64 = if auth.is_staff() {
        db.query_row(
            "SELECT COUNT(*) FROM uploads u WHERE u.class_id = ?1",
            params![auth.class_id],
            |r| r.get(0),
        )
    } else {
        db.query_row(
            "SELECT COUNT(*) FROM uploads u WHERE u.class_id = ?1 AND u.auth_id = ?2",
            params![auth.class_id, auth.id],
            |r| r.get(0),
        )
    }
    .unwrap_or(0);
    let pending: i64 = if auth.is_staff() {
        db.query_row(
            "SELECT COUNT(*) FROM uploads u WHERE u.class_id = ?1
             AND NOT EXISTS (SELECT 1 FROM replies r WHERE r.upload_id = u.id)",
            params![auth.class_id],
            |r| r.get(0),
        )
    } else {
        db.query_row(
            "SELECT COUNT(*) FROM uploads u WHERE u.class_id = ?1 AND u.auth_id = ?2
             AND NOT EXISTS (SELECT 1 FROM replies r WHERE r.upload_id = u.id)",
            params![auth.class_id, auth.id],
            |r| r.get(0),
        )
    }
    .unwrap_or(0);
    (total, pending, total - pending)
}

fn home_stats(
    db: &rusqlite::Connection,
    auth: &AuthCtx,
    now: i64,
    page_role: &str,
) -> Vec<StatItem> {
    let members: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM auths WHERE class_id = ?1",
            params![auth.class_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let subs: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM submissions WHERE class_id = ?1",
            params![auth.class_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let (uploads, pending, replied) = upload_counts(db, auth);
    if auth.is_staff() {
        let active: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM submissions
                 WHERE class_id = ?1 AND (deadline IS NULL OR deadline > ?2)",
                params![auth.class_id, now],
                |r| r.get(0),
            )
            .unwrap_or(0);
        vec![
            StatItem {
                label: "班级成员".to_string(),
                value: members.to_string(),
                icon: "users",
                tone: "",
            },
            StatItem {
                label: "进行中征集".to_string(),
                value: active.to_string(),
                icon: "inbox",
                tone: "sky",
            },
            StatItem {
                label: "待回复提交".to_string(),
                value: pending.to_string(),
                icon: "clock",
                tone: "amber",
            },
            StatItem {
                label: "已回复提交".to_string(),
                value: replied.to_string(),
                icon: "check-circle",
                tone: "green",
            },
        ]
    } else {
        let parent = page_role == "parent";
        let replies: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM replies r JOIN uploads u ON u.id = r.upload_id
                 WHERE u.auth_id = ?1",
                params![auth.id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        vec![
            StatItem {
                label: "班级征集".to_string(),
                value: subs.to_string(),
                icon: "inbox",
                tone: "",
            },
            StatItem {
                label: if parent {
                    "孩子提交"
                } else {
                    "我的提交"
                }
                .to_string(),
                value: uploads.to_string(),
                icon: "folder",
                tone: "sky",
            },
            StatItem {
                label: "收到回复".to_string(),
                value: replies.to_string(),
                icon: "reply",
                tone: "green",
            },
            StatItem {
                label: "待回复提交".to_string(),
                value: pending.to_string(),
                icon: "clock",
                tone: "amber",
            },
        ]
    }
}

// ---------------------------------------------------------------------------
// 会话守卫
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)]
fn guard_home(
    st: &AppState,
    headers: &HeaderMap,
    mode: &str,
    role: &str,
) -> Result<(Session, AuthCtx, String), Response> {
    let Some(mode) = normalize_mode(mode) else {
        return Err(redirect("/login"));
    };
    let Some(req_role) = normalize_role(role) else {
        return Err(redirect(&format!("/login/{mode}/parent")));
    };
    let Some((mut session, auth)) = role_session(st, headers) else {
        return Err(redirect(&format!("/login/{mode}/{req_role}")));
    };
    let allowed = match req_role {
        "parent" | "student" => auth.role == "student",
        "teacher" => auth.role == "teacher",
        "admin" => auth.role == "admin",
        _ => false,
    };
    if !allowed || session.page_role != req_role {
        return Err(redirect(&format!("/home/{mode}/{}/", session.page_role)));
    }
    session.mode = mode.to_string();
    Ok((session, auth, mode.to_string()))
}

#[allow(clippy::too_many_arguments)]
fn page_context(
    st: &AppState,
    headers: &HeaderMap,
    session: &Session,
    auth: &AuthCtx,
    prefs: &Prefs,
    title: &str,
    active: &str,
    flash: Flash,
) -> LayoutCtx {
    let mut ctx = LayoutCtx::from_auth(auth, session, prefs, title, active, flash);
    ctx.user_logged_in = current_user(st, headers).is_some();
    ctx
}

fn not_found_in(ctx: &LayoutCtx, msg: &str) -> Response {
    no_store(view::not_found(Some(ctx), msg))
}

// ---------------------------------------------------------------------------
// / /login /home /logout
// ---------------------------------------------------------------------------

pub async fn root(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if role_session(&st, &headers).is_some() {
        redirect("/home")
    } else if user_session_id(&st, &headers).is_some() {
        redirect("/user")
    } else {
        redirect("/login")
    }
}

pub async fn login_index(headers: HeaderMap, Query(q): Q) -> Response {
    // 已登录时仍展示身份选择页，便于通过另一个班级密钥切换 auth。
    // `/` 会把已登录用户送回 /home；这里刻意不做重定向。
    let mode = q
        .get("mode")
        .and_then(|m| normalize_mode(m))
        .map(str::to_string)
        .unwrap_or_else(|| detect_mode(&headers));
    html(view::login_index(&mode, &flash_from(&q)))
}

pub async fn login_page(
    State(st): State<AppState>,
    Path((mode, role)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let Some(mode) = normalize_mode(&mode) else {
        return redirect("/login");
    };
    let Some(role) = normalize_role(&role) else {
        return redirect("/login");
    };
    let _ = headers;
    let _ = st;
    html(view::login_page(mode, role, None))
}

pub async fn login_submit(
    State(st): State<AppState>,
    Path((mode, role)): Path<(String, String)>,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    let Some(mode) = normalize_mode(&mode) else {
        return html_status(
            StatusCode::BAD_REQUEST,
            view::login_page("mob", "parent", Some("登录地址不正确")),
        );
    };
    let Some(req_role) = normalize_role(&role) else {
        return html_status(
            StatusCode::NOT_FOUND,
            view::login_page(mode, "parent", Some("未知的登录身份")),
        );
    };
    let key = form.key.trim();
    if key.is_empty() {
        return html_status(
            StatusCode::BAD_REQUEST,
            view::login_page(mode, req_role, Some("请输入班级密钥")),
        );
    }
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_key(&db, key)
    };
    let Some(auth) = auth else {
        return html_status(
            StatusCode::UNAUTHORIZED,
            view::login_page(mode, req_role, Some("密钥无效或已被删除，请检查后重试")),
        );
    };
    let allowed = match req_role {
        "parent" | "student" => auth.role == "student",
        "teacher" => auth.role == "teacher",
        "admin" => auth.role == "admin",
        _ => false,
    };
    if !allowed {
        let msg = format!(
            "该密钥属于{}身份，不能以{}身份登录",
            crate::tools::role_label(&auth.role),
            crate::tools::role_label(req_role)
        );
        return html_status(
            StatusCode::FORBIDDEN,
            view::login_page(mode, req_role, Some(&msg)),
        );
    }
    clear_role_session(&st, &headers);
    let target = format!("/home/{mode}/{req_role}/", mode = mode, req_role = req_role);
    let mut cookies = vec![crate::tools::session_cookie(
        SID_COOKIE,
        &create_role_session(&st, &auth, req_role, mode),
    )];
    // 密钥已绑定用户时顺带建立账号会话，账号中心与身份删除后的体验更连贯。
    if let Some(uid) = auth.user_id {
        clear_user_session(&st, &headers);
        cookies.push(crate::tools::session_cookie(
            USID_COOKIE,
            &create_user_session(&st, uid),
        ));
    }
    with_cookies(&target, cookies)
}

pub async fn home_entry(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if let Some((session, _)) = role_session(&st, &headers) {
        return redirect(&format!("/home/{}/{}/", session.mode, session.page_role));
    }
    if user_session_id(&st, &headers).is_some() {
        return redirect("/user");
    }
    redirect("/login")
}

pub async fn home_index(
    State(st): State<AppState>,
    Path((mode, role)): Path<(String, String)>,
    headers: HeaderMap,
    Query(q): Q,
) -> Response {
    render_home_route(st, headers, mode, role, String::new(), q).await
}

pub async fn home_slash(
    State(st): State<AppState>,
    Path((mode, role)): Path<(String, String)>,
    headers: HeaderMap,
    Query(q): Q,
) -> Response {
    render_home_route(st, headers, mode, role, String::new(), q).await
}

pub async fn home_wild(
    State(st): State<AppState>,
    Path((mode, role, rest)): Path<(String, String, String)>,
    headers: HeaderMap,
    Query(q): Q,
) -> Response {
    render_home_route(st, headers, mode, role, rest, q).await
}

async fn render_home_route(
    st: AppState,
    headers: HeaderMap,
    mode: String,
    role: String,
    rest: String,
    q: HashMap<String, String>,
) -> Response {
    let (session, auth, mode) = match guard_home(&st, &headers, &mode, &role) {
        Ok(x) => x,
        Err(r) => return r,
    };
    let prefs = crate::tools::user_prefs(&st, &headers);
    let flash = flash_from(&q);
    let active_role = session.page_role.clone();
    let base = format!("/home/{mode}/{active_role}/");
    let segments: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    let now = now_ts();
    let db = st.db.lock().unwrap();

    match segments.as_slice() {
        [] => {
            let stats = home_stats(&db, &auth, now, &active_role);
            let submissions = list_submissions(&db, &auth, 4);
            let (filter, title, hint) = match active_role.as_str() {
                "teacher" => (
                    Some("pending"),
                    "待回复的提交",
                    "所有提交都已回复，辛苦了。新的提交会实时出现在这里。",
                ),
                "admin" => (None, "最近提交", "班级还没有任何提交。"),
                "parent" => (
                    None,
                    "孩子的最近提交",
                    "孩子还没有提交过内容，可以先去征集列表看看。",
                ),
                _ => (
                    None,
                    "我的最近提交",
                    "你还没有提交过内容，可以先去征集列表看看。",
                ),
            };
            let uploads = list_uploads(&db, &auth, filter, 4, false);
            drop(db);
            let ctx = page_context(
                &st, &headers, &session, &auth, &prefs, "首页", "home", flash,
            );
            html(view::home(
                &ctx,
                &stats,
                &submissions,
                &uploads,
                title,
                hint,
            ))
        }
        ["submissions"] => {
            let rows = list_submissions(&db, &auth, 500);
            drop(db);
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                "征集",
                "submissions",
                flash,
            );
            html(view::submissions(
                &ctx,
                &rows,
                auth.is_staff(),
                auth.is_admin(),
                ctx.page_role == "parent",
            ))
        }
        ["submissions", "new"] => {
            drop(db);
            if !auth.is_staff() {
                return msg_redirect(
                    &format!("{base}submissions/"),
                    "err",
                    "仅老师或管理员可以发布征集",
                );
            }
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                "发布",
                "submissions_new",
                flash,
            );
            html(view::new_submission(&ctx))
        }
        ["uploads"] => {
            let filter = q.get("filter").map(String::as_str).unwrap_or("all");
            let filter = if auth.is_staff() {
                match filter {
                    "pending" | "replied" => Some(filter),
                    _ => None,
                }
            } else {
                None
            };
            let counts = upload_counts(&db, &auth);
            let rows = list_uploads(&db, &auth, filter, 300, false);
            let staff = auth.is_staff();
            drop(db);
            let ctx = page_context(
                &st, &headers, &session, &auth, &prefs, "提交", "uploads", flash,
            );
            html(view::uploads(
                &ctx,
                &rows,
                staff,
                filter.unwrap_or("all"),
                counts,
            ))
        }
        ["submission", sub_id] => {
            let Some(sub) = load_submission(&db, &auth, sub_id) else {
                drop(db);
                let ctx = page_context(
                    &st,
                    &headers,
                    &session,
                    &auth,
                    &prefs,
                    "未找到",
                    "submissions",
                    flash,
                );
                return not_found_in(&ctx, "该征集不存在，或不属于当前班级。");
            };
            let next = latest_upload_id(&db, &auth, &sub.id);
            drop(db);
            let target = match next {
                Some(id) => format!("{base}upload/{id}/"),
                None => format!("{base}submission/{}/upload", sub.id),
            };
            redirect(&target)
        }
        ["submission", sub_id, "all"] => {
            let Some(sub) = load_submission(&db, &auth, sub_id) else {
                drop(db);
                let ctx = page_context(
                    &st,
                    &headers,
                    &session,
                    &auth,
                    &prefs,
                    "未找到",
                    "submissions",
                    flash,
                );
                return not_found_in(&ctx, "该征集不存在，或不属于当前班级。");
            };
            let rows = if auth.is_staff() {
                list_uploads(&db, &auth, None, 500, false)
                    .into_iter()
                    .filter(|u| u.submission_id == sub.id)
                    .collect::<Vec<_>>()
            } else {
                list_uploads(&db, &auth, None, 500, false)
                    .into_iter()
                    .filter(|u| u.submission_id == sub.id)
                    .collect::<Vec<_>>()
            };
            let staff = auth.is_staff();
            let admin = auth.is_admin();
            drop(db);
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                "征集详情",
                "submissions",
                flash,
            );
            html(view::submission_all(&ctx, &sub, &rows, staff, admin))
        }
        ["submission", sub_id, "upload"] => {
            let Some(sub) = load_submission(&db, &auth, sub_id) else {
                drop(db);
                let ctx = page_context(
                    &st,
                    &headers,
                    &session,
                    &auth,
                    &prefs,
                    "未找到",
                    "submissions",
                    flash,
                );
                return not_found_in(&ctx, "该征集不存在，或不属于当前班级。");
            };
            let (count, last) = own_upload_info(&db, &auth, &sub.id);
            drop(db);
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                "上传作品",
                "submissions",
                flash,
            );
            html(view::upload_form(&ctx, &sub, count, last.as_deref()))
        }
        ["upload", upload_id] => {
            let Some(upload) = load_upload(&db, &auth, upload_id) else {
                drop(db);
                let ctx = page_context(
                    &st,
                    &headers,
                    &session,
                    &auth,
                    &prefs,
                    "未找到",
                    "uploads",
                    flash,
                );
                return not_found_in(&ctx, "该提交不存在，或你没有权限查看。");
            };
            let sub = load_submission(&db, &auth, &upload.submission_id);
            let Some(sub) = sub else {
                drop(db);
                let ctx = page_context(
                    &st,
                    &headers,
                    &session,
                    &auth,
                    &prefs,
                    "未找到",
                    "uploads",
                    flash,
                );
                return not_found_in(&ctx, "关联的征集不存在。");
            };
            let replies = load_replies(&db, &upload.id);
            let staff = auth.is_staff();
            let admin = auth.is_admin();
            drop(db);
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                "提交详情",
                "uploads",
                flash,
            );
            html(view::upload_detail(
                &ctx, &upload, &sub, &replies, staff, admin,
            ))
        }
        ["user"] => {
            drop(db);
            redirect("/user")
        }
        _ => {
            drop(db);
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                "未找到",
                "home",
                flash,
            );
            not_found_in(&ctx, "页面不存在。")
        }
    }
}

pub async fn logout(State(st): State<AppState>, headers: HeaderMap) -> Response {
    logout_all(&st, &headers)
}

// ---------------------------------------------------------------------------
// /user
// ---------------------------------------------------------------------------

pub async fn user_page(State(st): State<AppState>, headers: HeaderMap, Query(q): Q) -> Response {
    let flash = flash_from(&q);
    let current = current_user(&st, &headers);
    if let Some(user) = current {
        let current_auth_id = role_session(&st, &headers)
            .map(|(_, a)| a.id)
            .unwrap_or_default();
        let (auths, admin_auths) = {
            let db = st.db.lock().unwrap();
            let auths = db::user_auths(&db, user.id);
            let admin_auths = auths
                .iter()
                .filter(|a| a.role == "admin")
                .cloned()
                .collect::<Vec<_>>();
            (auths, admin_auths)
        };
        let prefs = user.prefs.clone();
        let mut ctx = LayoutCtx::account(&user, &prefs, flash);
        ctx.auth_id = current_auth_id;
        return html(view::user_account(
            &ctx,
            &user,
            &auths,
            &admin_auths,
            &Flash::default(),
        ));
    }

    // 未登录：若当前有未绑定账号的角色会话，则预填该密钥并提示绑定。
    let (mode, prefill, note) = if let Some((session, auth)) = role_session(&st, &headers) {
        let note = format!(
            "当前以「{}」的{}身份访问，班级密钥尚未绑定账号。创建账号并绑定后，即可管理多个班级身份。",
            auth.class_name,
            crate::tools::role_label(&auth.role)
        );
        let prefill = if auth.user_id.is_none() {
            auth.key.clone()
        } else {
            String::new()
        };
        (session.mode, prefill, Some(note))
    } else {
        (detect_mode(&headers), String::new(), None)
    };
    html(view::user_guest(&mode, &flash, &prefill, note.as_deref()))
}

pub async fn user_login(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<UserLoginForm>,
) -> Response {
    let username = form.username.trim();
    if username.is_empty() || form.password.is_empty() {
        return msg_redirect("/user", "err", "请输入用户名和密码");
    }
    let found = {
        let db = st.db.lock().unwrap();
        db::load_user_with_secret(&db, username)
    };
    let Some(found) = found else {
        return msg_redirect("/user", "err", "用户名或密码错误");
    };
    if !constant_time_eq(&hash_password(&found.salt, &form.password), &found.pwhash) {
        return msg_redirect("/user", "err", "用户名或密码错误");
    }
    clear_user_session(&st, &headers);
    let token = create_user_session(&st, found.user.id);
    with_cookies(
        "/user?ok=%E6%AC%A2%E8%BF%8E%E5%9B%9E%E6%9D%A5",
        vec![crate::tools::session_cookie(USID_COOKIE, &token)],
    )
}

fn valid_username(name: &str) -> bool {
    let n = name.chars().count();
    (2..=32).contains(&n)
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

pub async fn user_register(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<UserRegisterForm>,
) -> Response {
    let username = form.username.trim();
    if !valid_username(username) {
        return msg_redirect(
            "/user",
            "err",
            "用户名需为 2–32 位，只能包含中英文、数字、下划线或短横线",
        );
    }
    if form.password.chars().count() < 6 {
        return msg_redirect("/user", "err", "密码至少需要 6 位");
    }
    if form.password != form.password2 {
        return msg_redirect("/user", "err", "两次输入的密码不一致");
    }
    if form.password.chars().count() > 128 {
        return msg_redirect("/user", "err", "密码过长");
    }

    let key = form.key.trim();
    let claim = if key.is_empty() {
        None
    } else {
        let found = {
            let db = st.db.lock().unwrap();
            db::load_auth_by_key(&db, key)
        };
        match found {
            Some(a) if a.user_id.is_none() => Some(a),
            Some(_) => return msg_redirect("/user", "err", "该密钥已被其他账号绑定"),
            None => return msg_redirect("/user", "err", "密钥无效或已被删除"),
        }
    };

    let salt = rand_string(16);
    let pwhash = hash_password(&salt, &form.password);
    let created = {
        let db = st.db.lock().unwrap();
        let exists: Option<i64> = db
            .query_row(
                "SELECT id FROM users WHERE username = ?1 COLLATE NOCASE",
                params![username],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        if exists.is_some() {
            None
        } else {
            let res = db.execute(
                "INSERT INTO users (username, salt, pwhash, display_name, prefs, created_at)
                 VALUES (?1, ?2, ?3, ?1, '{}', ?4)",
                params![username, salt, pwhash, now_ts()],
            );
            res.ok().map(|_| db.last_insert_rowid())
        }
    };
    let Some(user_id) = created else {
        return msg_redirect("/user", "err", "用户名已被使用，请换一个");
    };
    let mut cookies = Vec::new();
    let mut target = "/user?ok=%E8%B4%A6%E5%8F%B7%E5%B7%B2%E5%88%9B%E5%BB%BA";
    if let Some(auth) = claim {
        {
            let db = st.db.lock().unwrap();
            let _ = db.execute(
                "UPDATE auths SET user_id = ?1 WHERE id = ?2",
                params![user_id, auth.id],
            );
        }
        clear_role_session(&st, &headers);
        let token = create_role_session(&st, &auth, auth.page_role(), "mob");
        cookies.push(crate::tools::session_cookie(SID_COOKIE, &token));
        target = "/user?ok=%E8%B4%A6%E5%8F%B7%E5%B7%B2%E5%88%9B%E5%BB%BA%EF%BC%8C%E5%AF%86%E9%92%A5%E5%B7%B2%E7%BB%91%E5%AE%9A";
    }
    clear_user_session(&st, &headers);
    let usid = create_user_session(&st, user_id);
    cookies.push(crate::tools::session_cookie(USID_COOKIE, &usid));
    with_cookies(target, cookies)
}

pub async fn user_claim(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ClaimForm>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect("/user", "err", "请先登录账号");
    };
    let key = form.key.trim();
    if key.is_empty() {
        return msg_redirect("/user", "err", "请输入班级密钥");
    }
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_key(&db, key)
    };
    let Some(auth) = auth else {
        return msg_redirect("/user", "err", "密钥无效或已被删除");
    };
    match auth.user_id {
        Some(uid) if uid == user.id => {
            return msg_redirect("/user", "ok", "该密钥已经绑定在你的账号上");
        }
        Some(_) => return msg_redirect("/user", "err", "该密钥已被其他账号绑定"),
        None => {}
    }
    let result = {
        let db = st.db.lock().unwrap();
        db.execute(
            "UPDATE auths SET user_id = ?1 WHERE id = ?2 AND user_id IS NULL",
            params![user.id, auth.id],
        )
    };
    match result {
        Ok(1) => msg_redirect("/user", "ok", "密钥已绑定到账号"),
        _ => msg_redirect("/user", "err", "绑定失败，请稍后重试"),
    }
}

pub async fn user_enter(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<EnterClassForm>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect("/user", "err", "请先登录账号");
    };
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_id(&db, &form.auth_id)
    };
    let Some(auth) = auth else {
        return msg_redirect("/user", "err", "该身份不存在或已被删除");
    };
    if auth.user_id != Some(user.id) {
        return msg_redirect("/user", "err", "该身份不属于当前账号");
    }
    clear_role_session(&st, &headers);
    let mode = user.prefs.mode.as_str();
    let token = create_role_session(&st, &auth, auth.page_role(), mode);
    let session_cookie = crate::tools::session_cookie(SID_COOKIE, &token);
    let mut cookies = vec![session_cookie];
    if cookie_value(&headers, USID_COOKIE).is_none() {
        let usid = create_user_session(&st, user.id);
        cookies.push(crate::tools::session_cookie(USID_COOKIE, &usid));
    }
    let target = format!("/home/{mode}/{}/", auth.page_role());
    with_cookies(&target, cookies)
}

pub async fn user_delete_auth(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(auth_id): Path<String>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect("/user", "err", "请先登录账号");
    };
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_id(&db, &auth_id)
    };
    let Some(auth) = auth else {
        return msg_redirect("/user", "err", "该身份不存在");
    };
    if auth.user_id != Some(user.id) {
        return msg_redirect("/user", "err", "只能删除自己账号下的身份");
    }
    let currently_using = role_session(&st, &headers)
        .map(|(_, a)| a.id == auth_id)
        .unwrap_or(false);
    {
        let db = st.db.lock().unwrap();
        if let Err(e) = db.execute("DELETE FROM auths WHERE id = ?1", params![auth_id]) {
            return msg_redirect("/user", "err", &format!("删除失败：{e}"));
        }
    }
    invalidate_auth_sessions(&st, &auth_id);
    let mut cookies = Vec::new();
    if currently_using {
        if let Some(token) = cookie_value(&headers, SID_COOKIE) {
            st.sessions.lock().unwrap().remove(&token);
        }
        cookies.push(clear_cookie(SID_COOKIE));
    }
    let target = "/user?ok=%E8%BA%AB%E4%BB%BD%E5%B7%B2%E5%88%A0%E9%99%A4";
    if cookies.is_empty() {
        redirect(target)
    } else {
        with_cookies(target, cookies)
    }
}

pub async fn user_prefs(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PrefsForm>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect("/user", "err", "请先登录账号");
    };
    let display = form.display_name.trim();
    let display = if display.is_empty() {
        user.username.clone()
    } else {
        display.to_string()
    };
    if display.chars().count() > 40 {
        return msg_redirect("/user", "err", "显示名称最多 40 个字");
    }
    let mut prefs = Prefs {
        theme: form.theme,
        mode: form.mode,
    };
    prefs.sanitize();
    let res = {
        let db = st.db.lock().unwrap();
        db.execute(
            "UPDATE users SET display_name = ?1, prefs = ?2 WHERE id = ?3",
            params![display, prefs.to_json(), user.id],
        )
    };
    match res {
        Ok(_) => msg_redirect("/user", "ok", "偏好设置已保存"),
        Err(e) => msg_redirect("/user", "err", &format!("保存失败：{e}")),
    }
}

pub async fn user_password(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PasswordForm>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect("/user", "err", "请先登录账号");
    };
    if form.new.chars().count() < 6 {
        return msg_redirect("/user", "err", "新密码至少需要 6 位");
    }
    if form.new != form.new2 {
        return msg_redirect("/user", "err", "两次输入的新密码不一致");
    }
    let secret = {
        let db = st.db.lock().unwrap();
        db.query_row(
            "SELECT salt, pwhash FROM users WHERE id = ?1",
            params![user.id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
        .ok()
        .flatten()
    };
    let Some((salt, hash)) = secret else {
        return msg_redirect("/user", "err", "账号不存在");
    };
    if !constant_time_eq(&hash_password(&salt, &form.old), &hash) {
        return msg_redirect("/user", "err", "当前密码不正确");
    }
    let new_salt = rand_string(16);
    let new_hash = hash_password(&new_salt, &form.new);
    let res = {
        let db = st.db.lock().unwrap();
        db.execute(
            "UPDATE users SET salt = ?1, pwhash = ?2 WHERE id = ?3",
            params![new_salt, new_hash, user.id],
        )
    };
    match res {
        Ok(_) => {
            // 保留当前会话，清除该用户的其他账号会话。
            let current_token = cookie_value(&headers, USID_COOKIE);
            st.user_sessions
                .lock()
                .unwrap()
                .retain(|t, uid| *uid != user.id || Some(t.as_str()) == current_token.as_deref());
            msg_redirect("/user", "ok", "密码已修改")
        }
        Err(e) => msg_redirect("/user", "err", &format!("修改失败：{e}")),
    }
}

pub async fn user_create_class(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ClassForm>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect("/user", "err", "请先登录账号");
    };
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_id(&db, &form.admin_auth_id)
    };
    let Some(auth) = auth else {
        return msg_redirect("/user", "err", "管理员身份不存在");
    };
    if auth.user_id != Some(user.id) {
        return msg_redirect("/user", "err", "该管理员身份不属于当前账号");
    }
    if !auth.is_admin() {
        return msg_redirect("/user", "err", "只有管理员身份可以创建班级");
    }
    let db = st.db.lock().unwrap();
    match db::create_class(&db, &auth, &form.name, &form.desc) {
        Ok(_c) => msg_redirect(
            "/user",
            "ok",
            "班级已创建，新的管理员密钥已加入你的身份列表",
        ),
        Err(e) => msg_redirect("/user", "err", &e),
    }
}

pub async fn user_logout(State(st): State<AppState>, headers: HeaderMap) -> Response {
    logout_all(&st, &headers)
}
