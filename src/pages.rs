use std::collections::HashMap;

use axum::{
    extract::{Form, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};

use crate::db;
use crate::strings::{f2, t};
use crate::tools::{
    clear_cookie, constant_time_eq, cookie_value, create_role_session, create_user_session,
    current_user, hash_password, invalidate_auth_sessions, now_ts, rand_string, role_session,
    url_encode, user_session_id, SID_COOKIE, USID_COOKIE,
};
use crate::types::{
    AppState, AuthCtx, ClaimForm, ClassForm, EnterClassForm, LoginForm, PasswordForm, Prefs,
    PrefsForm, Session, UserLoginForm, UserRegisterForm,
};
use crate::view::{self, Flash, LayoutCtx, StatItem};

type Q = Query<HashMap<String, String>>;

const ACCOUNT_LOGIN: &str = "/login/account";

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
    no_store(Html(crate::strings::apply(&s)).into_response())
}

fn html_status(status: StatusCode, s: String) -> Response {
    no_store((status, Html(crate::strings::apply(&s))).into_response())
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
    let target = format!("/login?ok={}", url_encode(&t("flash_logged_out")));
    with_cookies(
        &target,
        vec![clear_cookie(SID_COOKIE), clear_cookie(USID_COOKIE)],
    )
}

// ---------------------------------------------------------------------------
// 首页统计卡片（查询逻辑在 db.rs）
// ---------------------------------------------------------------------------

fn home_stats(counts: db::HomeCounts, page_role: &str) -> Vec<StatItem> {
    if page_role == "teacher" || page_role == "admin" {
        vec![
            StatItem {
                label: t("stat_members"),
                value: counts.members.to_string(),
                icon: "users",
                tone: "",
            },
            StatItem {
                label: t("stat_active_submissions"),
                value: counts.active.to_string(),
                icon: "inbox",
                tone: "sky",
            },
            StatItem {
                label: t("stat_pending"),
                value: counts.pending.to_string(),
                icon: "clock",
                tone: "amber",
            },
            StatItem {
                label: t("stat_replied"),
                value: counts.replied.to_string(),
                icon: "check-circle",
                tone: "green",
            },
        ]
    } else {
        let parent = page_role == "parent";
        vec![
            StatItem {
                label: t("stat_class_submissions"),
                value: counts.submissions.to_string(),
                icon: "inbox",
                tone: "",
            },
            StatItem {
                label: if parent {
                    t("stat_child_uploads")
                } else {
                    t("stat_my_uploads")
                },
                value: counts.uploads.to_string(),
                icon: "folder",
                tone: "sky",
            },
            StatItem {
                label: t("stat_replies_received"),
                value: counts.replies.to_string(),
                icon: "reply",
                tone: "green",
            },
            StatItem {
                label: t("stat_pending"),
                value: counts.pending.to_string(),
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
            view::login_page("mob", "parent", Some(&t("login_error_wrong_address"))),
        );
    };
    let Some(req_role) = normalize_role(&role) else {
        return html_status(
            StatusCode::NOT_FOUND,
            view::login_page(mode, "parent", Some(&t("login_error_unknown_identity"))),
        );
    };
    let key = form.key.trim();
    if key.is_empty() {
        return html_status(
            StatusCode::BAD_REQUEST,
            view::login_page(mode, req_role, Some(&t("login_error_enter_key"))),
        );
    }
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_key(&db, key)
    };
    let Some(auth) = auth else {
        return html_status(
            StatusCode::UNAUTHORIZED,
            view::login_page(mode, req_role, Some(&t("login_error_invalid_key"))),
        );
    };
    let allowed = match req_role {
        "parent" | "student" => auth.role == "student",
        "teacher" => auth.role == "teacher",
        "admin" => auth.role == "admin",
        _ => false,
    };
    if !allowed {
        let msg = f2(
            "login_error_role_mismatch",
            crate::tools::role_label(&auth.role),
            crate::tools::role_label(req_role),
        );
        return html_status(
            StatusCode::FORBIDDEN,
            view::login_page(mode, req_role, Some(&msg)),
        );
    }
    clear_role_session(&st, &headers);
    let target = format!("/home/{mode}/{req_role}/", mode = mode, req_role = req_role);
    let cookies = vec![crate::tools::session_cookie(
        SID_COOKIE,
        &create_role_session(&st, &auth, req_role, mode),
    )];
    // 角色登录只建立 sc_sid；账号中心必须通过用户名/密码另行建立 sc_uid。
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
            let stats = home_stats(db::home_counts(&db, &auth, now), &active_role);
            let submissions = db::list_submissions(&db, &auth, 4);
            let (filter, title_key, hint_key) = match active_role.as_str() {
                "teacher" => (Some("pending"), "home_recent_teacher", "home_pending_hint"),
                "admin" => (None, "home_recent_admin", "home_admin_hint"),
                "parent" => (None, "home_recent_parent", "home_parent_hint"),
                _ => (None, "home_recent_self", "home_self_hint"),
            };
            let uploads_title = t(title_key);
            let uploads_hint = t(hint_key);
            let uploads = db::list_uploads(&db, &auth, filter, 4, false);
            drop(db);
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                &t("page_home"),
                "home",
                flash,
            );
            html(view::home(
                &ctx,
                &stats,
                &submissions,
                &uploads,
                &uploads_title,
                &uploads_hint,
            ))
        }
        ["submissions"] => {
            let rows = db::list_submissions(&db, &auth, 500);
            drop(db);
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                &t("page_submissions"),
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
                    &t("flash_teacher_only_publish"),
                );
            }
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                &t("page_publish"),
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
            let counts = db::upload_counts(&db, &auth);
            let rows = db::list_uploads(&db, &auth, filter, 300, false);
            let staff = auth.is_staff();
            drop(db);
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                &t("page_uploads"),
                "uploads",
                flash,
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
            let Some(sub) = db::load_submission(&db, &auth, sub_id) else {
                drop(db);
                let ctx = page_context(
                    &st,
                    &headers,
                    &session,
                    &auth,
                    &prefs,
                    &t("common_not_found"),
                    "submissions",
                    flash,
                );
                return not_found_in(&ctx, &t("error_submission_missing"));
            };
            let next = db::latest_upload_id(&db, &auth, &sub.id);
            drop(db);
            let target = match next {
                Some(id) => format!("{base}upload/{id}/"),
                None => format!("{base}submission/{}/upload", sub.id),
            };
            redirect(&target)
        }
        ["submission", sub_id, "all"] => {
            let Some(sub) = db::load_submission(&db, &auth, sub_id) else {
                drop(db);
                let ctx = page_context(
                    &st,
                    &headers,
                    &session,
                    &auth,
                    &prefs,
                    &t("common_not_found"),
                    "submissions",
                    flash,
                );
                return not_found_in(&ctx, &t("error_submission_missing"));
            };
            let rows = if auth.is_staff() {
                db::list_uploads(&db, &auth, None, 500, false)
                    .into_iter()
                    .filter(|u| u.submission_id == sub.id)
                    .collect::<Vec<_>>()
            } else {
                db::list_uploads(&db, &auth, None, 500, false)
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
                &t("page_submission_detail"),
                "submissions",
                flash,
            );
            html(view::submission_all(&ctx, &sub, &rows, staff, admin))
        }
        ["submission", sub_id, "upload"] => {
            let Some(sub) = db::load_submission(&db, &auth, sub_id) else {
                drop(db);
                let ctx = page_context(
                    &st,
                    &headers,
                    &session,
                    &auth,
                    &prefs,
                    &t("common_not_found"),
                    "submissions",
                    flash,
                );
                return not_found_in(&ctx, &t("error_submission_missing"));
            };
            let (count, last) = db::own_upload_info(&db, &auth, &sub.id);
            drop(db);
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                &t("page_upload"),
                "submissions",
                flash,
            );
            html(view::upload_form(&ctx, &sub, count, last.as_deref()))
        }
        ["upload", upload_id] => {
            let Some(upload) = db::load_upload(&db, &auth, upload_id) else {
                drop(db);
                let ctx = page_context(
                    &st,
                    &headers,
                    &session,
                    &auth,
                    &prefs,
                    &t("common_not_found"),
                    "uploads",
                    flash,
                );
                return not_found_in(&ctx, &t("error_upload_missing"));
            };
            let sub = db::load_submission(&db, &auth, &upload.submission_id);
            let Some(sub) = sub else {
                drop(db);
                let ctx = page_context(
                    &st,
                    &headers,
                    &session,
                    &auth,
                    &prefs,
                    &t("common_not_found"),
                    "uploads",
                    flash,
                );
                return not_found_in(&ctx, &t("error_submission_related_missing"));
            };
            let replies = db::load_replies(&db, &upload.id);
            let staff = auth.is_staff();
            let admin = auth.is_admin();
            drop(db);
            let ctx = page_context(
                &st,
                &headers,
                &session,
                &auth,
                &prefs,
                &t("page_upload_detail"),
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
                &t("common_not_found"),
                "home",
                flash,
            );
            not_found_in(&ctx, &t("error_page_missing"))
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
    let Some(user) = current_user(&st, &headers) else {
        // 未登录账号中心：跳转到专门的账号登录页。
        return redirect(ACCOUNT_LOGIN);
    };
    let flash = flash_from(&q);
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
    html(view::user_account(
        &ctx,
        &user,
        &auths,
        &admin_auths,
        &Flash::default(),
    ))
}

pub async fn login_account_page(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Q,
) -> Response {
    if current_user(&st, &headers).is_some() {
        return redirect("/user");
    }
    let mode = q
        .get("mode")
        .and_then(|m| normalize_mode(m))
        .map(str::to_string)
        .unwrap_or_else(|| detect_mode(&headers));
    html(view::user_guest(&mode, &flash_from(&q), "", None))
}

pub async fn user_login(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<UserLoginForm>,
) -> Response {
    let username = form.username.trim();
    if username.is_empty() || form.password.is_empty() {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_enter_username_password"));
    }
    let found = {
        let db = st.db.lock().unwrap();
        db::load_user_with_secret(&db, username)
    };
    let Some(found) = found else {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_wrong_credentials"));
    };
    if !constant_time_eq(&hash_password(&found.salt, &form.password), &found.pwhash) {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_wrong_credentials"));
    }
    clear_user_session(&st, &headers);
    let token = create_user_session(&st, found.user.id);
    let target = format!("/user?ok={}", url_encode(&t("flash_welcome")));
    with_cookies(
        &target,
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
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_username_invalid"));
    }
    if form.password.chars().count() < 6 {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_password_min"));
    }
    if form.password != form.password2 {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_password_mismatch"));
    }
    if form.password.chars().count() > 128 {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_password_too_long"));
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
            Some(_) => return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_key_bound")),
            None => return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_key_invalid")),
        }
    };

    let salt = rand_string(16);
    let pwhash = hash_password(&salt, &form.password);
    let created = {
        let db = st.db.lock().unwrap();
        db::register_user(&db, username, &salt, &pwhash)
    };
    let Ok(user_id) = created else {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_username_taken"));
    };
    let mut cookies = Vec::new();
    let mut target = format!("/user?ok={}", url_encode(&t("flash_account_created")));
    if let Some(auth) = claim {
        {
            let db = st.db.lock().unwrap();
            let _ = db::bind_auth_if_unclaimed(&db, &auth.id, user_id);
        }
        clear_role_session(&st, &headers);
        let token = create_role_session(&st, &auth, auth.page_role(), "mob");
        cookies.push(crate::tools::session_cookie(SID_COOKIE, &token));
        target = format!("/user?ok={}", url_encode(&t("flash_account_created_bound")));
    }
    clear_user_session(&st, &headers);
    let usid = create_user_session(&st, user_id);
    cookies.push(crate::tools::session_cookie(USID_COOKIE, &usid));
    with_cookies(&target, cookies)
}

pub async fn user_claim(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ClaimForm>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_need_login"));
    };
    let key = form.key.trim();
    if key.is_empty() {
        return msg_redirect("/user", "err", &t("flash_enter_key"));
    }
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_key(&db, key)
    };
    let Some(auth) = auth else {
        return msg_redirect("/user", "err", &t("flash_key_invalid"));
    };
    match auth.user_id {
        Some(uid) if uid == user.id => {
            return msg_redirect("/user", "ok", &t("flash_already_bound"));
        }
        Some(_) => return msg_redirect("/user", "err", &t("flash_key_bound")),
        None => {}
    }
    let result = {
        let db = st.db.lock().unwrap();
        db::bind_auth_if_unclaimed(&db, &auth.id, user.id)
    };
    match result {
        Ok(true) => msg_redirect("/user", "ok", &t("flash_bound")),
        _ => msg_redirect("/user", "err", &t("flash_bind_failed")),
    }
}

pub async fn user_enter(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<EnterClassForm>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_need_login"));
    };
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_id(&db, &form.auth_id)
    };
    let Some(auth) = auth else {
        return msg_redirect("/user", "err", &t("flash_identity_missing"));
    };
    if auth.user_id != Some(user.id) {
        return msg_redirect("/user", "err", &t("flash_not_your_identity"));
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
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_need_login"));
    };
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_id(&db, &auth_id)
    };
    let Some(auth) = auth else {
        return msg_redirect("/user", "err", &t("flash_identity_missing"));
    };
    if auth.user_id != Some(user.id) {
        return msg_redirect("/user", "err", &t("flash_only_own_auth"));
    }
    let currently_using = role_session(&st, &headers)
        .map(|(_, a)| a.id == auth_id)
        .unwrap_or(false);
    {
        let db = st.db.lock().unwrap();
        if let Err(e) = db::delete_auth(&db, &auth_id) {
            eprintln!("delete auth failed: {e}");
            return msg_redirect("/user", "err", &t("flash_delete_failed"));
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
    let target = format!("/user?ok={}", url_encode(&t("flash_identity_deleted")));
    if cookies.is_empty() {
        redirect(&target)
    } else {
        with_cookies(&target, cookies)
    }
}

pub async fn user_prefs(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PrefsForm>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_need_login"));
    };
    let display = form.display_name.trim();
    let display = if display.is_empty() {
        user.username.clone()
    } else {
        display.to_string()
    };
    if display.chars().count() > 40 {
        return msg_redirect("/user", "err", &t("flash_display_name_too_long"));
    }
    let mut prefs = Prefs {
        theme: form.theme,
        mode: form.mode,
    };
    prefs.sanitize();
    let res = {
        let db = st.db.lock().unwrap();
        db::update_user_prefs(&db, user.id, &display, &prefs)
    };
    match res {
        Ok(_) => msg_redirect("/user", "ok", &t("flash_prefs_saved")),
        Err(e) => {
            eprintln!("save prefs failed: {e}");
            msg_redirect("/user", "err", &t("flash_save_failed"))
        }
    }
}

pub async fn user_password(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<PasswordForm>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_need_login"));
    };
    if form.new.chars().count() < 6 {
        return msg_redirect("/user", "err", &t("flash_password_min"));
    }
    if form.new != form.new2 {
        return msg_redirect("/user", "err", &t("flash_password_mismatch"));
    }
    let secret = {
        let db = st.db.lock().unwrap();
        db::user_secret(&db, user.id)
    };
    let Some((salt, hash)) = secret else {
        return msg_redirect("/user", "err", &t("flash_account_missing"));
    };
    if !constant_time_eq(&hash_password(&salt, &form.old), &hash) {
        return msg_redirect("/user", "err", &t("flash_current_password_wrong"));
    }
    let new_salt = rand_string(16);
    let new_hash = hash_password(&new_salt, &form.new);
    let res = {
        let db = st.db.lock().unwrap();
        db::update_user_password(&db, user.id, &new_salt, &new_hash)
    };
    match res {
        Ok(_) => {
            // 保留当前会话，清除该用户的其他账号会话。
            let current_token = cookie_value(&headers, USID_COOKIE);
            st.user_sessions
                .lock()
                .unwrap()
                .retain(|t, uid| *uid != user.id || Some(t.as_str()) == current_token.as_deref());
            msg_redirect("/user", "ok", &t("flash_password_changed"))
        }
        Err(e) => {
            eprintln!("change password failed: {e}");
            msg_redirect("/user", "err", &t("flash_change_failed"))
        }
    }
}

pub async fn user_create_class(
    State(st): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<ClassForm>,
) -> Response {
    let Some(user) = current_user(&st, &headers) else {
        return msg_redirect(ACCOUNT_LOGIN, "err", &t("flash_need_login"));
    };
    let auth = {
        let db = st.db.lock().unwrap();
        db::load_auth_by_id(&db, &form.admin_auth_id)
    };
    let Some(auth) = auth else {
        return msg_redirect("/user", "err", &t("flash_admin_identity_missing"));
    };
    if auth.user_id != Some(user.id) {
        return msg_redirect("/user", "err", &t("flash_not_your_admin_identity"));
    }
    if !auth.is_admin() {
        return msg_redirect("/user", "err", &t("flash_admin_only_create_class"));
    }
    let db = st.db.lock().unwrap();
    match db::create_class(&db, &auth, &form.name, &form.desc) {
        Ok(_c) => msg_redirect("/user", "ok", &t("flash_class_created")),
        Err(e) => {
            eprintln!("create class failed: {e}");
            msg_redirect("/user", "err", &t("flash_class_create_failed"))
        }
    }
}

pub async fn user_logout(State(st): State<AppState>, headers: HeaderMap) -> Response {
    logout_all(&st, &headers)
}
