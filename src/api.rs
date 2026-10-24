use axum::{extract::State, http::HeaderMap, response::Response, Json};
use rand::seq::SliceRandom;
use regex::Regex;
use serde_json::{json, Value};

use crate::db;
use crate::tools::{
    api_bad, api_forbidden, api_not_found, api_ok, api_unauthorized, clean_display_name, fmt_iso,
    invalidate_auth_sessions, normalize_data, now_ts, parse_deadline_value, parse_match_mode,
    role_session, MatchMode,
};
use crate::types::{
    AllSubmissionReq, AppState, AuthCtx, DelKeyReq, DelObjectReq, DownloadReq, NewClassReq,
    NewSubmissionReq, NewUserReq, RenameAuthReq, ReplyReq, SubmissionRow, UploadReq,
};

const MAX_TITLE: usize = 200;
const MAX_DESC: usize = 5_000;
const MAX_SUBMISSION_DESC: usize = 2_000;

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
    match db::submission_class_id(&db, &req.which) {
        Some(c) if c == auth.class_id => {}
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
// 健康检查（便于部署探活）
// ---------------------------------------------------------------------------

pub async fn health() -> Response {
    api_ok(json!({ "ok": true, "service": "smart-campus", "time": now_ts() }))
}
