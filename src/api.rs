use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use rand::seq::SliceRandom;
use regex::Regex;
use serde_json::{json, Value};

use crate::crypto;
use crate::db;
use crate::tools::{
    api_bad, api_forbidden, api_not_found, api_ok, api_unauthorized, clean_display_name,
    constant_time_eq, cookie_value, create_user_session, fmt_iso, hash_password,
    invalidate_auth_sessions, normalize_data, now_ts, parse_deadline_value, parse_match_mode,
    role_session, session_cookie, valid_username, MatchMode, USID_COOKIE,
};
use crate::types::{
    AllSubmissionReq, AppState, AuthCtx, ByteInput, DelKeyReq, DelObjectReq, DownloadReq,
    GetAuthsReq, LoginSession, LoginTryReq, NewClassReq, NewSubmissionReq, NewUserReq,
    RegisterReq, RenameAuthReq, ReplyReq, ResetPasswordReq, SubmissionRow, UpgradePubkeyReq,
    UploadReq,
};

const MAX_TITLE: usize = 200;
const MAX_DESC: usize = 5_000;
const MAX_SUBMISSION_DESC: usize = 2_000;
/// 登录挑战有效期（秒）。
const LOGIN_SESSION_TTL: i64 = 300;
/// 单个登录会话允许的错误尝试次数。
const MAX_LOGIN_ATTEMPTS: u8 = 5;

// ---------------------------------------------------------------------------
// 公共辅助
// ---------------------------------------------------------------------------

#[allow(clippy::result_large_err)]
fn require_auth(st: &AppState, key: &str) -> Result<AuthCtx, Response> {
    if key.trim().is_empty() {
        return Err(api_unauthorized("missing auth parameter"));
    }
    let db = st.db.lock().unwrap();
    db::load_auth_by_key(&db, key).ok_or_else(|| api_unauthorized("invalid or expired auth"))
}

// ---------------------------------------------------------------------------
// /api/upload
// ---------------------------------------------------------------------------

pub async fn upload(State(st): State<AppState>, Json(req): Json<UploadReq>) -> Response {
    let auth = match require_auth(&st, &req.auth) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let title = req.title.trim().to_string();
    let desc = req.desc.trim().to_string();
    if title.is_empty() {
        return api_bad("title must not be empty");
    }
    if title.chars().count() > MAX_TITLE {
        return api_bad(format!("title is too long (max {MAX_TITLE} characters)"));
    }
    if desc.chars().count() > MAX_DESC {
        return api_bad(format!("desc is too long (max {MAX_DESC} characters)"));
    }
    let data = match normalize_data(req.data) {
        Ok(d) => d,
        Err(e) => return api_bad(e),
    };

    let db = st.db.lock().unwrap();
    match db::submission_meta(&db, &req.which) {
        Some((class_id, kind, form_desc)) if class_id == auth.class_id => {
            if kind.trim().eq_ignore_ascii_case("notice") {
                return api_forbidden("uploads are disabled for notice submissions");
            }
            // 问卷 / 答题卡：简介里的必填项在服务端再校验一次（answer 为 JSON 对象时）。
            // 答题卡模式下所有题目都必填。
            if let Some(err) = crate::survey::validate_submission(&kind, &form_desc, &desc) {
                return api_bad(err);
            }
        }
        Some(_) => return api_not_found("submission does not belong to the current class"),
        None => return api_not_found("submission not found"),
    }
    match db::insert_upload(&db, &auth, &req.which, &title, &desc, &data) {
        Ok(id) => api_ok(json!({ "ok": true, "id": id })),
        Err(e) => crate::tools::api_err(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("upload failed: {e}"),
        ),
    }
}

// ---------------------------------------------------------------------------
// /api/download
// ---------------------------------------------------------------------------

pub async fn download(State(st): State<AppState>, Json(req): Json<DownloadReq>) -> Response {
    let auth = match require_auth(&st, &req.auth) {
        Ok(a) => a,
        Err(r) => return r,
    };
    if !auth.is_staff() {
        return api_forbidden("only teachers or admins can download class submissions");
    }
    let mode = match parse_match_mode(req.match_mode.as_ref()) {
        Ok(m) => m,
        Err(e) => return api_bad(e),
    };

    let db = st.db.lock().unwrap();
    match db::submission_class_id(&db, &req.which) {
        Some(c) if c == auth.class_id => {}
        Some(_) => return api_not_found("submission does not belong to the current class"),
        None => return api_not_found("submission not found"),
    }

    let mut rows = db::list_download_uploads(&db, &req.which);

    match mode {
        MatchMode::All => {}
        MatchMode::Random(n) => {
            let mut idx: Vec<usize> = (0..rows.len()).collect();
            idx.shuffle(&mut rand::thread_rng());
            idx.truncate(n);
            idx.sort_unstable();
            let selected: Vec<db::DownloadUploadRow> =
                idx.into_iter().map(|i| rows[i].clone()).collect();
            rows = selected;
        }
        MatchMode::StudentId(id) => {
            rows.retain(|r| r.student_id.as_deref() == Some(id.as_str()));
        }
        MatchMode::ShortStudentId(id) => {
            rows.retain(|r| r.short_id == Some(id));
        }
        MatchMode::TitleRegex(re) => {
            let regex = match Regex::new(&re) {
                Ok(r) => r,
                Err(e) => return api_bad(format!("invalid regular expression: {e}")),
            };
            rows.retain(|r| regex.is_match(&r.title));
        }
        MatchMode::Before(ts) => {
            rows.retain(|r| r.created_at < ts);
        }
    }

    let mut datas = Vec::with_capacity(rows.len());
    for row in rows {
        let replies = db::reply_contents(&db, &row.id);
        datas.push(json!({
            "id": row.id,
            "studentID": row.student_id.clone().unwrap_or_default(),
            "shortStudentID": row.short_id.unwrap_or(0),
            "title": row.title,
            "desc": row.desc,
            "data": row.data,
            "reply": replies,
            "uploader": row.uploader_name,
            "role": row.role,
            "createdAt": fmt_iso(row.created_at),
        }));
    }

    api_ok(json!({ "ok": true, "datas": datas }))
}

// ---------------------------------------------------------------------------
// /api/all-submission
// ---------------------------------------------------------------------------

pub async fn all_submission(
    State(st): State<AppState>,
    Json(req): Json<AllSubmissionReq>,
) -> Response {
    let auth = match require_auth(&st, &req.auth) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let mode = match parse_match_mode(req.match_mode.as_ref()) {
        Ok(m) => m,
        Err(e) => return api_bad(e),
    };
    let db = st.db.lock().unwrap();
    let mut rows: Vec<SubmissionRow> = db::list_submissions(&db, &auth, i64::MAX);

    match mode {
        MatchMode::All => {}
        MatchMode::Random(n) => {
            let mut idx: Vec<usize> = (0..rows.len()).collect();
            idx.shuffle(&mut rand::thread_rng());
            idx.truncate(n);
            idx.sort_unstable();
            let selected: Vec<SubmissionRow> = idx.into_iter().map(|i| rows[i].clone()).collect();
            rows = selected;
        }
        MatchMode::StudentId(id) => {
            let set = db::submission_ids_by_student(&db, &auth.class_id, &id);
            rows.retain(|r| set.iter().any(|sid| sid == &r.id));
        }
        MatchMode::ShortStudentId(id) => {
            let set = db::submission_ids_by_short(&db, &auth.class_id, id);
            rows.retain(|r| set.iter().any(|sid| sid == &r.id));
        }
        MatchMode::TitleRegex(re) => {
            let regex = match Regex::new(&re) {
                Ok(r) => r,
                Err(e) => return api_bad(format!("invalid regular expression: {e}")),
            };
            rows.retain(|r| regex.is_match(&r.name));
        }
        MatchMode::Before(ts) => {
            rows.retain(|r| r.deadline.is_some_and(|d| d < ts));
        }
    }

    let datas: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "name": r.name,
                "desc": r.desc,
                "deadLine": r.deadline.map(fmt_iso),
                "type": r.kind,
                "createdAt": fmt_iso(r.created_at),
                "createdBy": r.created_name,
                "uploadCount": r.upload_count,
                "myUploadCount": r.my_upload_count,
                "pendingCount": r.pending_count,
                "latestUploadID": r.last_upload_id,
            })
        })
        .collect();
    api_ok(json!({ "ok": true, "datas": datas }))
}

// ---------------------------------------------------------------------------
// /api/new-submission
// ---------------------------------------------------------------------------

pub async fn new_submission(
    State(st): State<AppState>,
    Json(req): Json<NewSubmissionReq>,
) -> Response {
    let auth = match require_auth(&st, &req.auth) {
        Ok(a) => a,
        Err(r) => return r,
    };
    if !auth.is_staff() {
        return api_forbidden("only teachers or admins can create submissions");
    }
    let name = req.name.trim().to_string();
    let desc = req.desc.trim().to_string();
    let kind = if req.kind.trim().is_empty() {
        "homework".to_string()
    } else {
        req.kind.trim().to_string()
    };
    if name.is_empty() {
        return api_bad("name must not be empty");
    }
    if name.chars().count() > MAX_TITLE {
        return api_bad(format!("name is too long (max {MAX_TITLE} characters)"));
    }
    if desc.chars().count() > MAX_SUBMISSION_DESC {
        return api_bad(format!(
            "desc is too long (max {MAX_SUBMISSION_DESC} characters)"
        ));
    }
    if kind.chars().count() > 60 {
        return api_bad("type is too long (max 60 characters)");
    }
    let deadline = match parse_deadline_value(req.deadline.as_ref()) {
        Ok(d) => d,
        Err(e) => return api_bad(e),
    };
    // 答题卡：简介里必须至少有一道带 id / data-answer / data-score 的题目。
    if let Some(err) = crate::survey::validate_definition(&kind, &desc) {
        return api_bad(err);
    }
    let db = st.db.lock().unwrap();
    match db::insert_submission(&db, &auth, &name, &desc, &kind, deadline) {
        Ok(id) => api_ok(json!({ "ok": true, "id": id })),
        Err(e) => crate::tools::api_err(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("create failed: {e}"),
        ),
    }
}

// ---------------------------------------------------------------------------
// /api/reply
// ---------------------------------------------------------------------------

pub async fn reply(State(st): State<AppState>, Json(req): Json<ReplyReq>) -> Response {
    let auth = match require_auth(&st, &req.auth) {
        Ok(a) => a,
        Err(r) => return r,
    };
    if !auth.is_staff() {
        return api_forbidden("only teachers or admins can reply");
    }
    let content = req.content.trim().to_string();
    if content.is_empty() {
        return api_bad("content must not be empty");
    }
    if content.chars().count() > MAX_DESC {
        return api_bad(format!("content is too long (max {MAX_DESC} characters)"));
    }
    let db = st.db.lock().unwrap();
    match db::upload_class_id(&db, &req.upload_id) {
        Some(c) if c == auth.class_id => {}
        Some(_) => return api_not_found("upload does not belong to the current class"),
        None => return api_not_found("upload not found"),
    }
    match db::insert_reply(&db, &auth, &req.upload_id, &content) {
        Ok(id) => api_ok(json!({ "ok": true, "id": id })),
        Err(e) => crate::tools::api_err(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("reply failed: {e}"),
        ),
    }
}

// ---------------------------------------------------------------------------
// /api/new-class
// ---------------------------------------------------------------------------

pub async fn new_class(State(st): State<AppState>, Json(req): Json<NewClassReq>) -> Response {
    let auth = match require_auth(&st, &req.auth) {
        Ok(a) => a,
        Err(r) => return r,
    };
    if !auth.is_admin() {
        return api_forbidden("only admins can create classes");
    }
    let desc = req.desc.trim();
    let name = req.name.trim();
    if name.is_empty() && desc.is_empty() {
        return api_bad("desc (class description) must not be empty");
    }
    let db = st.db.lock().unwrap();
    match db::create_class(&db, &auth, name, desc) {
        Ok(c) => api_ok(json!({
            "ok": true,
            "id": c.id,
            "code": c.code,
            "auth": c.key,
            "authID": c.auth_id,
        })),
        Err(e) => api_bad(e),
    }
}

// ---------------------------------------------------------------------------
// /api/new-user
// ---------------------------------------------------------------------------

pub async fn new_user(State(st): State<AppState>, Json(req): Json<NewUserReq>) -> Response {
    let auth = match require_auth(&st, &req.auth) {
        Ok(a) => a,
        Err(r) => return r,
    };
    if !auth.is_admin() {
        return api_forbidden("only admins can add members");
    }
    let db = st.db.lock().unwrap();
    match db::create_members(&db, &auth, &req.class_id, &req.kind, req.quantity) {
        Ok(members) => {
            let ids: Vec<String> = members.iter().map(|m| m.id.clone()).collect();
            let keys: Vec<String> = members.iter().map(|m| m.key.clone()).collect();
            let details: Vec<Value> = members
                .iter()
                .map(|m| {
                    json!({
                        "id": m.id,
                        "key": m.key,
                        "role": m.role,
                        "studentID": m.student_id,
                        "displayName": m.display_name,
                    })
                })
                .collect();
            api_ok(json!({
                "ok": true,
                "IDs": ids,
                "keys": keys,
                "members": details,
            }))
        }
        Err(e) => api_bad(e),
    }
}

// ---------------------------------------------------------------------------
// /api/del-key
// ---------------------------------------------------------------------------

pub async fn del_key(State(st): State<AppState>, Json(req): Json<DelKeyReq>) -> Response {
    let auth = match require_auth(&st, &req.auth) {
        Ok(a) => a,
        Err(r) => return r,
    };
    {
        let db = st.db.lock().unwrap();
        if let Err(e) = db::delete_auth(&db, &auth.id) {
            return api_bad(format!("delete failed: {e}"));
        }
    }
    invalidate_auth_sessions(&st, &auth.id);
    api_ok(json!({ "ok": true, "msg": "key deleted" }))
}

// ---------------------------------------------------------------------------
// /api/rename-auth
// ---------------------------------------------------------------------------

pub async fn rename_auth(State(st): State<AppState>, Json(req): Json<RenameAuthReq>) -> Response {
    let auth = match require_auth(&st, &req.auth) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let name = match clean_display_name(&req.name) {
        Ok(n) => n,
        Err(e) => return api_bad(e),
    };

    // 同步刷新历史快照，让改名立即反映在已有的提交、回复和征集上。
    let db = st.db.lock().unwrap();
    match db::rename_auth(&db, &auth.id, &name) {
        Ok(()) => api_ok(json!({ "ok": true, "name": name, "msg": "identity name updated" })),
        Err(e) => crate::tools::api_err(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("rename failed: {e}"),
        ),
    }
}

// ---------------------------------------------------------------------------
// /api/del-object
// ---------------------------------------------------------------------------

pub async fn del_object(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DelObjectReq>,
) -> Response {
    // 官方接口只传 id；此处支持 body.auth（推荐）或登录 cookie。
    let auth = match req.auth.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(key) => match require_auth(&st, key) {
            Ok(a) => a,
            Err(r) => return r,
        },
        None => match role_session(&st, &headers) {
            Some((_, a)) => a,
            None => return api_unauthorized("missing auth and not logged in"),
        },
    };
    if !auth.is_admin() {
        return api_forbidden("only admins can delete objects");
    }
    let id = req.id.trim().to_string();
    if id.is_empty() {
        return api_bad("id must not be empty");
    }

    if let Some(class_id) = id.strip_prefix("cls_") {
        let class_id = format!("cls_{class_id}");
        if class_id != auth.class_id {
            return api_forbidden("you can only delete a class you manage");
        }
        let members = {
            let db = st.db.lock().unwrap();
            let members = db::auth_ids_in_class(&db, &class_id);
            if let Err(e) = db::delete_class(&db, &class_id) {
                return api_bad(format!("delete failed: {e}"));
            }
            members
        };
        for m in members {
            invalidate_auth_sessions(&st, &m);
        }
        return api_ok(json!({ "ok": true, "msg": "class deleted" }));
    }

    if id.starts_with("auth_") {
        let target = {
            let db = st.db.lock().unwrap();
            db::load_auth_by_id(&db, &id)
        };
        let Some(target) = target else {
            return api_not_found("auth not found");
        };
        if target.class_id != auth.class_id {
            return api_forbidden("you can only delete an auth in the current class");
        }
        {
            let db = st.db.lock().unwrap();
            if let Err(e) = db::delete_auth(&db, &id) {
                return api_bad(format!("delete failed: {e}"));
            }
        }
        invalidate_auth_sessions(&st, &id);
        return api_ok(json!({ "ok": true, "msg": "auth deleted" }));
    }

    let class = {
        let db = st.db.lock().unwrap();
        if id.starts_with("sub_") {
            db::submission_class_id(&db, &id)
        } else if id.starts_with("up_") {
            db::upload_class_id(&db, &id)
        } else if id.starts_with("rep_") {
            db::reply_class_id(&db, &id)
        } else {
            None
        }
    };
    let Some(class) = class else {
        return api_not_found("object not found or unsupported id type");
    };
    if class != auth.class_id {
        return api_forbidden("you can only delete objects in the current class");
    }
    {
        let db = st.db.lock().unwrap();
        let result = if id.starts_with("sub_") {
            db::delete_submission(&db, &id)
        } else if id.starts_with("up_") {
            db::delete_upload(&db, &id)
        } else {
            db::delete_reply(&db, &id)
        };
        if let Err(e) = result {
            return api_bad(format!("delete failed: {e}"));
        }
    }
    api_ok(json!({ "ok": true, "msg": "deleted" }))
}

// ---------------------------------------------------------------------------
// 用户登录（非对称挑战 / API）
// ---------------------------------------------------------------------------

fn api_error_code(status: StatusCode, msg: impl Into<String>, code: &str) -> Response {
    (
        status,
        Json(json!({ "ok": false, "msg": msg.into(), "code": code })),
    )
        .into_response()
}

fn with_cookie(mut res: Response, cookie: String) -> Response {
    crate::tools::append_cookie(&mut res, cookie);
    res
}

fn decode_pubkey(input: &ByteInput) -> Result<String, String> {
    let bytes = input.decode()?;
    if bytes.len() != crypto::KEY_LEN {
        return Err("public key must be 32 bytes".to_string());
    }
    crypto::validate_public_key(&bytes)?;
    Ok(crypto::b64_encode(&bytes))
}

/// 校验 loginSessionID + encryptedAnswer；成功时返回会话快照。
#[allow(clippy::result_large_err)]
fn verify_login_answer(
    st: &AppState,
    session_id: &str,
    answer: &ByteInput,
) -> Result<LoginSession, Response> {
    let now = now_ts();
    let mut sessions = st.login_sessions.lock().unwrap();
    let Some(session) = sessions.get(session_id).cloned() else {
        return Err(api_unauthorized("invalid or expired login session"));
    };
    if now - session.created_at > LOGIN_SESSION_TTL {
        sessions.remove(session_id);
        return Err(api_unauthorized("login session expired"));
    }
    let decoded = match answer.decode() {
        Ok(bytes) => bytes,
        Err(e) => return Err(api_bad(e)),
    };
    if decoded.len() != crypto::ANSWER_CIPHERTEXT_LEN {
        let attempts = match sessions.get_mut(session_id) {
            Some(s) => {
                s.attempts = s.attempts.saturating_add(1);
                s.attempts
            }
            None => MAX_LOGIN_ATTEMPTS,
        };
        if attempts >= MAX_LOGIN_ATTEMPTS {
            sessions.remove(session_id);
        }
        return Err(api_bad("encryptedAnswer has an invalid length"));
    }
    let ok = crypto::aes_gcm_decrypt(&session.answer_key, &decoded)
        .map(|plain| crypto::constant_time_eq(&plain, &session.challenge))
        .unwrap_or(false);
    if !ok {
        let attempts = match sessions.get_mut(session_id) {
            Some(s) => {
                s.attempts = s.attempts.saturating_add(1);
                s.attempts
            }
            None => MAX_LOGIN_ATTEMPTS,
        };
        if attempts >= MAX_LOGIN_ATTEMPTS {
            sessions.remove(session_id);
        }
        return Err(api_unauthorized("encrypted answer verification failed"));
    }
    if let Some(s) = sessions.get_mut(session_id) {
        s.verified = true;
    }
    let mut verified = session;
    verified.verified = true;
    Ok(verified)
}

/// `/api/logintry`：给用户公钥加密一个随机挑战，并下发答案加密密钥。
pub async fn logintry(State(st): State<AppState>, Json(req): Json<LoginTryReq>) -> Response {
    let username = req.username.trim();
    if username.is_empty() {
        return api_bad("username must not be empty");
    }
    let user = {
        let db = st.db.lock().unwrap();
        db::load_login_user(&db, username)
    };
    let Some(user) = user else {
        return api_unauthorized("invalid username or account does not exist");
    };
    let Some(pubkey_b64) = user
        .pubkey
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return api_error_code(
            StatusCode::UNAUTHORIZED,
            "this account has no public key yet; upgrade the login first",
            "pubkey_missing",
        );
    };
    let pubkey = match crypto::b64_decode(pubkey_b64) {
        Ok(bytes) if bytes.len() == crypto::KEY_LEN => bytes,
        _ => {
            return crate::tools::api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "stored public key is invalid",
            )
        }
    };
    if crypto::validate_public_key(&pubkey).is_err() {
        return crate::tools::api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "stored public key is invalid",
        );
    }
    let challenge = match crypto::random_bytes(crypto::CHALLENGE_LEN) {
        Ok(bytes) => bytes,
        Err(e) => return crate::tools::api_err(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let challenge_encrypt = match crypto::encrypt_challenge(&pubkey, &challenge) {
        Ok(bytes) => bytes,
        Err(e) => return crate::tools::api_err(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let answer_key = match crypto::random_bytes(crypto::KEY_LEN) {
        Ok(bytes) => bytes,
        Err(e) => return crate::tools::api_err(StatusCode::INTERNAL_SERVER_ERROR, e),
    };
    let session_id = format!("login_{}", crate::tools::rand_string(32));
    let now = now_ts();
    {
        let mut sessions = st.login_sessions.lock().unwrap();
        sessions.retain(|_, s| now - s.created_at <= LOGIN_SESSION_TTL);
        sessions.insert(
            session_id.clone(),
            LoginSession {
                user_id: user.id,
                challenge,
                answer_key: answer_key.clone(),
                created_at: now,
                verified: false,
                attempts: 0,
            },
        );
    }
    api_ok(json!({
        "ok": true,
        "loginSessionID": session_id,
        "challengeEncrypt": crypto::b64_encode(&challenge_encrypt),
        "answerEncryptingKey": crypto::b64_encode(&answer_key),
        "expiresIn": LOGIN_SESSION_TTL,
        "username": user.username,
        "displayName": user.display_name,
        "kdf": {
            "name": "PBKDF2-HMAC-SHA256",
            "iterations": crypto::KDF_ITERATIONS,
            "salt": crypto::kdf_salt(&user.username),
            "privateKey": "X25519",
            "challengeEncryption": "X25519-HKDF-SHA256-AES-256-GCM",
            "answerEncryption": "AES-256-GCM"
        }
    }))
}

/// `/api/getAuths`：验证答案后返回该账号的全部班级密钥，并建立账号会话。
pub async fn get_auths(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<GetAuthsReq>,
) -> Response {
    let session = match verify_login_answer(&st, &req.login_session_id, &req.encrypted_answer) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let (user, auths) = {
        let db = st.db.lock().unwrap();
        (
            db::load_user_by_id(&db, session.user_id),
            db::user_auths(&db, session.user_id),
        )
    };
    let Some(user) = user else {
        return api_unauthorized("account no longer exists");
    };
    let keys: Vec<String> = auths.iter().map(|a| a.key.clone()).collect();
    let ids: Vec<String> = auths.iter().map(|a| a.id.clone()).collect();
    let members: Vec<Value> = auths
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "key": a.key,
                "classID": a.class_id,
                "className": a.class_name,
                "role": a.role,
                "studentID": a.student_id,
                "shortStudentID": a.short_id,
                "displayName": a.display_name,
            })
        })
        .collect();

    // 登录成功后给浏览器建立账号会话；API 客户端可以只使用返回的 auths。
    if let Some(old) = cookie_value(&headers, USID_COOKIE) {
        st.user_sessions.lock().unwrap().remove(&old);
    }
    let token = create_user_session(&st, user.id);
    let cookie = session_cookie(USID_COOKIE, &token);
    let res = api_ok(json!({
        "ok": true,
        "auths": keys,
        "authIDs": ids,
        "members": members,
        "user": {
            "id": user.id,
            "username": user.username,
            "displayName": user.display_name,
        }
    }));
    with_cookie(res, cookie)
}

/// `/api/register`：提交用户名 + 公钥创建账号（可选绑定一个未认领的班级密钥）。
pub async fn register(State(st): State<AppState>, Json(req): Json<RegisterReq>) -> Response {
    let username = req.username.trim();
    if !valid_username(username) {
        return api_bad("username must be 2-32 characters and contain only letters, digits, _ or -");
    }
    let pubkey = match decode_pubkey(&req.pubkey) {
        Ok(key) => key,
        Err(e) => return api_bad(e),
    };
    let display_name = if req.display_name.trim().is_empty() {
        username.to_string()
    } else {
        match clean_display_name(&req.display_name) {
            Ok(name) => name,
            Err(e) => return api_bad(e),
        }
    };
    let key = req.key.trim();
    let claim = if key.is_empty() {
        None
    } else {
        let db = st.db.lock().unwrap();
        match db::load_auth_by_key(&db, key) {
            Some(auth) if auth.user_id.is_none() => Some(auth),
            Some(_) => return api_bad("this key is already bound to another account"),
            None => return api_bad("invalid key"),
        }
    };
    let created = {
        let db = st.db.lock().unwrap();
        db::register_user(&db, username, &pubkey, &display_name)
    };
    let user_id = match created {
        Ok(id) => id,
        Err(e) if e.contains("already exists") => {
            return api_error_code(
                StatusCode::BAD_REQUEST,
                "username already exists",
                "username_taken",
            )
        }
        Err(e) => {
            return crate::tools::api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("register failed: {e}"),
            )
        }
    };
    let mut bound_key = None;
    if let Some(auth) = claim {
        let result = {
            let db = st.db.lock().unwrap();
            db::bind_auth_if_unclaimed(&db, &auth.id, user_id)
        };
        match result {
            Ok(true) => bound_key = Some(auth.key.clone()),
            Ok(false) => {
                let db = st.db.lock().unwrap();
                let _ = db::delete_user(&db, user_id);
                return api_bad("this key is already bound to another account");
            }
            Err(e) => {
                let db = st.db.lock().unwrap();
                let _ = db::delete_user(&db, user_id);
                return crate::tools::api_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("bind key failed: {e}"),
                );
            }
        }
    }
    api_ok(json!({
        "ok": true,
        "id": user_id,
        "userID": user_id,
        "username": username,
        "displayName": display_name,
        "boundAuth": bound_key,
    }))
}

/// `/api/resetPassword`：验证挑战后替换公钥（即修改密码）。
pub async fn reset_password(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ResetPasswordReq>,
) -> Response {
    let session = match verify_login_answer(&st, &req.login_session_id, &req.encrypted_answer) {
        Ok(s) => s,
        Err(r) => return r,
    };
    let new_pubkey = match decode_pubkey(&req.new_password_pub_key) {
        Ok(key) => key,
        Err(e) => return api_bad(e),
    };
    let auth_ids = {
        let db = st.db.lock().unwrap();
        if let Err(e) = db::update_user_pubkey(&db, session.user_id, &new_pubkey) {
            return crate::tools::api_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("reset password failed: {e}"),
            );
        }
        db::user_auths(&db, session.user_id)
            .into_iter()
            .map(|a| a.id)
            .collect::<Vec<_>>()
    };
    // 旧密码、旧挑战、旧账号会话与班级角色会话全部作废；给当前浏览器发一个新会话。
    for auth_id in auth_ids {
        invalidate_auth_sessions(&st, &auth_id);
    }
    st.login_sessions
        .lock()
        .unwrap()
        .retain(|_, s| s.user_id != session.user_id);
    if let Some(old) = cookie_value(&headers, USID_COOKIE) {
        st.user_sessions.lock().unwrap().remove(&old);
    }
    st.user_sessions
        .lock()
        .unwrap()
        .retain(|_, uid| *uid != session.user_id);
    let token = create_user_session(&st, session.user_id);
    let cookie = session_cookie(USID_COOKIE, &token);
    with_cookie(
        api_ok(json!({ "ok": true, "msg": "password reset" })),
        cookie,
    )
}

/// `/api/upgrade-pubkey`：老账号的一次性迁移接口（仍校验旧密码哈希）。
pub async fn upgrade_pubkey(
    State(st): State<AppState>,
    Json(req): Json<UpgradePubkeyReq>,
) -> Response {
    let username = req.username.trim();
    if username.is_empty() || req.password.is_empty() {
        return api_bad("username and password are required");
    }
    let pubkey = match decode_pubkey(&req.pubkey) {
        Ok(key) => key,
        Err(e) => return api_bad(e),
    };
    let found = {
        let db = st.db.lock().unwrap();
        db::load_user_with_secret(&db, username)
    };
    let Some(found) = found else {
        return api_unauthorized("invalid username or password");
    };
    if found.pwhash.is_empty()
        || !constant_time_eq(&hash_password(&found.salt, &req.password), &found.pwhash)
    {
        return api_unauthorized("invalid username or password");
    }
    let changed = {
        let db = st.db.lock().unwrap();
        db::upgrade_user_pubkey(&db, found.user.id, &pubkey)
    };
    match changed {
        Ok(true) => api_ok(json!({
            "ok": true,
            "msg": "public key upgraded",
            "username": found.user.username,
        })),
        Ok(false) => api_bad("this account already has a public key"),
        Err(e) => crate::tools::api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("upgrade failed: {e}"),
        ),
    }
}

// ---------------------------------------------------------------------------
// 健康检查（便于部署探活）
// ---------------------------------------------------------------------------

pub async fn health() -> Response {
    api_ok(json!({ "ok": true, "service": "smart-campus", "time": now_ts() }))
}
