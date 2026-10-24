use axum::{extract::State, http::HeaderMap, response::Response, Json};
use rand::seq::SliceRandom;
use regex::Regex;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};

use crate::db;
use crate::tools::{
    api_bad, api_forbidden, api_not_found, api_ok, api_unauthorized, clean_display_name,
    deadline_chip, fmt_iso, invalidate_auth_sessions, normalize_data, normalize_page_role, now_ts,
    parse_deadline_value, parse_match_mode, role_session, MatchMode,
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
        return Err(api_unauthorized("缺少 auth 参数"));
    }
    let db = st.db.lock().unwrap();
    db::load_auth_by_key(&db, key).ok_or_else(|| api_unauthorized("auth 无效或已失效"))
}

fn submission_class(db: &rusqlite::Connection, id: &str) -> Option<String> {
    db.query_row(
        "SELECT class_id FROM submissions WHERE id = ?1",
        params![id],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

fn upload_class(db: &rusqlite::Connection, id: &str) -> Option<String> {
    db.query_row(
        "SELECT class_id FROM uploads WHERE id = ?1",
        params![id],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

fn reply_class(db: &rusqlite::Connection, id: &str) -> Option<String> {
    db.query_row(
        "SELECT u.class_id FROM replies r JOIN uploads u ON u.id = r.upload_id WHERE r.id = ?1",
        params![id],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
}

fn get_data(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| json!([]))
}

fn mask_student_id(s: Option<&str>) -> String {
    s.unwrap_or("").to_string()
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
        return api_bad("title 不能为空");
    }
    if title.chars().count() > MAX_TITLE {
        return api_bad(format!("title 过长（最多 {MAX_TITLE} 字）"));
    }
    if desc.chars().count() > MAX_DESC {
        return api_bad(format!("desc 过长（最多 {MAX_DESC} 字）"));
    }
    let data = match normalize_data(req.data) {
        Ok(d) => d,
        Err(e) => return api_bad(e),
    };

    let db = st.db.lock().unwrap();
    match submission_class(&db, &req.which) {
        Some(c) if c == auth.class_id => {}
        Some(_) => return api_not_found("该征集不属于当前班级"),
        None => return api_not_found("征集不存在"),
    }
    let id = crate::tools::new_id("up");
    let now = now_ts();
    let res = db.execute(
        "INSERT INTO uploads
         (id, submission_id, class_id, auth_id, user_id, student_id, short_id,
          uploader_name, role, title, desc, data, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            req.which,
            auth.class_id,
            auth.id,
            auth.user_id,
            auth.student_id,
            auth.short_id,
            auth.identity(),
            auth.role,
            title,
            desc,
            data.to_string(),
            now
        ],
    );
    match res {
        Ok(_) => api_ok(json!({ "ok": true, "id": id })),
        Err(e) => crate::tools::api_err(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("上传失败：{e}"),
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
        return api_forbidden("仅老师或管理员可以下载 / 查看班级提交");
    }
    let mode = match parse_match_mode(req.match_mode.as_ref()) {
        Ok(m) => m,
        Err(e) => return api_bad(e),
    };

    let db = st.db.lock().unwrap();
    match submission_class(&db, &req.which) {
        Some(c) if c == auth.class_id => {}
        Some(_) => return api_not_found("该征集不属于当前班级"),
        None => return api_not_found("征集不存在"),
    }

    #[derive(Clone)]
    struct Row {
        id: String,
        student_id: Option<String>,
        short_id: Option<i64>,
        title: String,
        desc: String,
        data: Value,
        created_at: i64,
        uploader_name: String,
        role: String,
    }

    let mut rows: Vec<Row> = {
        let mut stmt = match db.prepare(
            "SELECT id, student_id, short_id, title, desc, data, created_at, uploader_name, role
             FROM uploads WHERE submission_id = ?1 ORDER BY created_at ASC, id ASC",
        ) {
            Ok(s) => s,
            Err(e) => {
                return crate::tools::api_err(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    e.to_string(),
                )
            }
        };
        let iter = stmt.query_map(params![req.which], |r| {
            Ok(Row {
                id: r.get(0)?,
                student_id: r.get(1)?,
                short_id: r.get(2)?,
                title: r.get(3)?,
                desc: r.get(4)?,
                data: get_data(&r.get::<_, String>(5)?),
                created_at: r.get(6)?,
                uploader_name: r.get(7)?,
                role: r.get(8)?,
            })
        });
        match iter {
            Ok(iter) => iter.filter_map(Result::ok).collect(),
            Err(e) => {
                return crate::tools::api_err(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    e.to_string(),
                )
            }
        }
    };

    match mode {
        MatchMode::All => {}
        MatchMode::Random(n) => {
            let mut idx: Vec<usize> = (0..rows.len()).collect();
            idx.shuffle(&mut rand::thread_rng());
            idx.truncate(n);
            idx.sort_unstable();
            let selected: Vec<Row> = idx.into_iter().map(|i| rows[i].clone()).collect();
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
                Err(e) => return api_bad(format!("正则表达式不合法：{e}")),
            };
            rows.retain(|r| regex.is_match(&r.title));
        }
        MatchMode::Before(ts) => {
            rows.retain(|r| r.created_at < ts);
        }
    }

    let mut datas = Vec::with_capacity(rows.len());
    for row in rows {
        let replies: Vec<String> = {
            let mut stmt = match db
                .prepare("SELECT content FROM replies WHERE upload_id = ?1 ORDER BY created_at ASC")
            {
                Ok(s) => s,
                Err(_) => {
                    return crate::tools::api_err(
                        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                        "读取回复失败",
                    )
                }
            };
            let iter = stmt.query_map(params![row.id], |r| r.get::<_, String>(0));
            match iter {
                Ok(iter) => iter.filter_map(Result::ok).collect(),
                Err(_) => Vec::new(),
            }
        };
        datas.push(json!({
            "id": row.id,
            "studentID": mask_student_id(row.student_id.as_deref()),
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
    let staff = auth.is_staff();

    let mut rows: Vec<SubmissionRow> = {
        let sql = if staff {
            "SELECT s.id, s.name, s.desc, s.type, s.deadline, s.created_at, s.created_name,
                (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id),
                0,
                (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id
                    AND NOT EXISTS (SELECT 1 FROM replies r WHERE r.upload_id = u.id)),
                (SELECT u.id FROM uploads u WHERE u.submission_id = s.id
                    ORDER BY u.created_at DESC, u.id DESC LIMIT 1)
             FROM submissions s WHERE s.class_id = ?1
             ORDER BY CASE WHEN s.deadline IS NOT NULL AND s.deadline < CAST(strftime('%s','now') AS INTEGER) THEN 1 ELSE 0 END, CASE WHEN s.deadline IS NULL THEN 1 ELSE 0 END, s.deadline ASC, s.created_at DESC"
        } else {
            "SELECT s.id, s.name, s.desc, s.type, s.deadline, s.created_at, s.created_name,
                (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?2),
                (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?2),
                (SELECT COUNT(*) FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?2
                    AND NOT EXISTS (SELECT 1 FROM replies r WHERE r.upload_id = u.id)),
                (SELECT u.id FROM uploads u WHERE u.submission_id = s.id AND u.auth_id = ?2
                    ORDER BY u.created_at DESC, u.id DESC LIMIT 1)
             FROM submissions s WHERE s.class_id = ?1
             ORDER BY CASE WHEN s.deadline IS NOT NULL AND s.deadline < CAST(strftime('%s','now') AS INTEGER) THEN 1 ELSE 0 END, CASE WHEN s.deadline IS NULL THEN 1 ELSE 0 END, s.deadline ASC, s.created_at DESC"
        };
        let mut stmt = match db.prepare(sql) {
            Ok(s) => s,
            Err(e) => {
                return crate::tools::api_err(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    e.to_string(),
                )
            }
        };
        let map_row = |r: &rusqlite::Row<'_>| {
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
        };
        let iter = if staff {
            stmt.query_map(params![auth.class_id], map_row)
        } else {
            stmt.query_map(params![auth.class_id, auth.id], map_row)
        };
        match iter {
            Ok(iter) => iter.filter_map(Result::ok).collect(),
            Err(e) => {
                return crate::tools::api_err(
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    e.to_string(),
                )
            }
        }
    };

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
            let set: Vec<String> = {
                let mut stmt = match db.prepare(
                    "SELECT DISTINCT submission_id FROM uploads
                     WHERE class_id = ?1 AND student_id = ?2",
                ) {
                    Ok(s) => s,
                    Err(e) => return api_bad(e.to_string()),
                };
                let iter = stmt.query_map(params![auth.class_id, id], |r| r.get::<_, String>(0));
                match iter {
                    Ok(iter) => iter.filter_map(Result::ok).collect(),
                    Err(_) => Vec::new(),
                }
            };
            rows.retain(|r| set.iter().any(|id| id == &r.id));
        }
        MatchMode::ShortStudentId(id) => {
            let set: Vec<String> = {
                let mut stmt = match db.prepare(
                    "SELECT DISTINCT submission_id FROM uploads
                     WHERE class_id = ?1 AND short_id = ?2",
                ) {
                    Ok(s) => s,
                    Err(e) => return api_bad(e.to_string()),
                };
                let iter = stmt.query_map(params![auth.class_id, id], |r| r.get::<_, String>(0));
                match iter {
                    Ok(iter) => iter.filter_map(Result::ok).collect(),
                    Err(_) => Vec::new(),
                }
            };
            rows.retain(|r| set.iter().any(|id| id == &r.id));
        }
        MatchMode::TitleRegex(re) => {
            let regex = match Regex::new(&re) {
                Ok(r) => r,
                Err(e) => return api_bad(format!("正则表达式不合法：{e}")),
            };
            rows.retain(|r| regex.is_match(&r.name));
        }
        MatchMode::Before(ts) => {
            rows.retain(|r| r.deadline.is_some_and(|d| d < ts));
        }
    }

    let now = now_ts();
    let datas: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            let (chip, _) = deadline_chip(r.deadline, now);
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
                "status": chip,
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
        return api_forbidden("仅老师或管理员可以发布征集");
    }
    let name = req.name.trim().to_string();
    let desc = req.desc.trim().to_string();
    let kind = if req.kind.trim().is_empty() {
        "homework".to_string()
    } else {
        req.kind.trim().to_string()
    };
    if name.is_empty() {
        return api_bad("name 不能为空");
    }
    if name.chars().count() > MAX_TITLE {
        return api_bad(format!("name 过长（最多 {MAX_TITLE} 字）"));
    }
    if desc.chars().count() > MAX_SUBMISSION_DESC {
        return api_bad(format!("desc 过长（最多 {MAX_SUBMISSION_DESC} 字）"));
    }
    if kind.chars().count() > 60 {
        return api_bad("type 过长（最多 60 字）");
    }
    let deadline = match parse_deadline_value(req.deadline.as_ref()) {
        Ok(d) => d,
        Err(e) => return api_bad(e),
    };
    let id = crate::tools::new_id("sub");
    let now = now_ts();
    let db = st.db.lock().unwrap();
    let res = db.execute(
        "INSERT INTO submissions
         (id, class_id, name, desc, type, deadline, created_at, created_by, created_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            auth.class_id,
            name,
            desc,
            kind,
            deadline,
            now,
            auth.id,
            auth.identity()
        ],
    );
    match res {
        Ok(_) => api_ok(json!({ "ok": true, "id": id })),
        Err(e) => crate::tools::api_err(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("创建失败：{e}"),
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
        return api_forbidden("仅老师或管理员可以回复");
    }
    let content = req.content.trim().to_string();
    if content.is_empty() {
        return api_bad("content 不能为空");
    }
    if content.chars().count() > MAX_DESC {
        return api_bad(format!("content 过长（最多 {MAX_DESC} 字）"));
    }
    let db = st.db.lock().unwrap();
    match upload_class(&db, &req.upload_id) {
        Some(c) if c == auth.class_id => {}
        Some(_) => return api_not_found("该上传不属于当前班级"),
        None => return api_not_found("上传不存在"),
    }
    let id = crate::tools::new_id("rep");
    let res = db.execute(
        "INSERT INTO replies (id, upload_id, auth_id, replier_name, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            req.upload_id,
            auth.id,
            auth.identity(),
            auth.role,
            content,
            now_ts()
        ],
    );
    match res {
        Ok(_) => api_ok(json!({ "ok": true, "id": id })),
        Err(e) => crate::tools::api_err(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("回复失败：{e}"),
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
        return api_forbidden("仅管理员可以创建班级");
    }
    let desc = req.desc.trim();
    let name = req.name.trim();
    if name.is_empty() && desc.is_empty() {
        return api_bad("desc（班级描述）不能为空");
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
        return api_forbidden("仅管理员可以新增成员");
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
        let res = db.execute("DELETE FROM auths WHERE id = ?1", params![auth.id]);
        if let Err(e) = res {
            return api_bad(format!("删除失败：{e}"));
        }
    }
    invalidate_auth_sessions(&st, &auth.id);
    api_ok(json!({ "ok": true, "msg": "密钥已删除" }))
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
    let mut db = st.db.lock().unwrap();
    let tx = match db.transaction() {
        Ok(t) => t,
        Err(e) => {
            return crate::tools::api_err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("改名失败：{e}"),
            )
        }
    };
    let result = (|| -> rusqlite::Result<()> {
        tx.execute(
            "UPDATE auths SET display_name = ?1 WHERE id = ?2",
            params![name, auth.id],
        )?;
        tx.execute(
            "UPDATE uploads SET uploader_name = ?1 WHERE auth_id = ?2",
            params![name, auth.id],
        )?;
        tx.execute(
            "UPDATE replies SET replier_name = ?1 WHERE auth_id = ?2",
            params![name, auth.id],
        )?;
        tx.execute(
            "UPDATE submissions SET created_name = ?1 WHERE created_by = ?2",
            params![name, auth.id],
        )?;
        Ok(())
    })();
    match result {
        Ok(()) => match tx.commit() {
            Ok(()) => api_ok(json!({ "ok": true, "name": name, "msg": "身份名称已更新" })),
            Err(e) => crate::tools::api_err(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("改名失败：{e}"),
            ),
        },
        Err(e) => crate::tools::api_err(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("改名失败：{e}"),
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
            None => return api_unauthorized("缺少 auth，且未登录"),
        },
    };
    if !auth.is_admin() {
        return api_forbidden("仅管理员可以删除对象");
    }
    let id = req.id.trim().to_string();
    if id.is_empty() {
        return api_bad("id 不能为空");
    }

    if let Some(class_id) = id.strip_prefix("cls_") {
        let class_id = format!("cls_{class_id}");
        if class_id != auth.class_id {
            return api_forbidden("只能删除自己管理的班级");
        }
        let members: Vec<String> = {
            let db = st.db.lock().unwrap();
            let mut stmt = match db.prepare("SELECT id FROM auths WHERE class_id = ?1") {
                Ok(s) => s,
                Err(e) => return api_bad(e.to_string()),
            };
            let iter = stmt.query_map(params![class_id], |r| r.get::<_, String>(0));
            match iter {
                Ok(iter) => iter.filter_map(Result::ok).collect(),
                Err(_) => Vec::new(),
            }
        };
        {
            let db = st.db.lock().unwrap();
            if let Err(e) = db.execute("DELETE FROM classes WHERE id = ?1", params![class_id]) {
                return api_bad(format!("删除失败：{e}"));
            }
        }
        for m in members {
            invalidate_auth_sessions(&st, &m);
        }
        return api_ok(json!({ "ok": true, "msg": "班级已删除" }));
    }

    if id.starts_with("auth_") {
        let target = {
            let db = st.db.lock().unwrap();
            db::load_auth_by_id(&db, &id)
        };
        let Some(target) = target else {
            return api_not_found("auth 不存在");
        };
        if target.class_id != auth.class_id {
            return api_forbidden("只能删除当前班级的 auth");
        }
        {
            let db = st.db.lock().unwrap();
            if let Err(e) = db.execute("DELETE FROM auths WHERE id = ?1", params![id]) {
                return api_bad(format!("删除失败：{e}"));
            }
        }
        invalidate_auth_sessions(&st, &id);
        return api_ok(json!({ "ok": true, "msg": "auth 已删除" }));
    }

    let class = {
        let db = st.db.lock().unwrap();
        if id.starts_with("sub_") {
            submission_class(&db, &id)
        } else if id.starts_with("up_") {
            upload_class(&db, &id)
        } else if id.starts_with("rep_") {
            reply_class(&db, &id)
        } else {
            None
        }
    };
    let Some(class) = class else {
        return api_not_found("对象不存在，或 id 类型不支持");
    };
    if class != auth.class_id {
        return api_forbidden("只能删除当前班级的对象");
    }
    let table = if id.starts_with("sub_") {
        "submissions"
    } else if id.starts_with("up_") {
        "uploads"
    } else {
        "replies"
    };
    {
        let db = st.db.lock().unwrap();
        let sql = format!("DELETE FROM {table} WHERE id = ?1");
        if let Err(e) = db.execute(&sql, params![id]) {
            return api_bad(format!("删除失败：{e}"));
        }
    }
    api_ok(json!({ "ok": true, "msg": "已删除" }))
}

// ---------------------------------------------------------------------------
// 健康检查（便于部署探活）
// ---------------------------------------------------------------------------

pub async fn health() -> Response {
    api_ok(json!({ "ok": true, "service": "smart-campus", "time": now_ts() }))
}

// 避免未使用告警：保留给未来的 admin-only 页面校验
#[allow(dead_code)]
fn _page_role_check(role: &str) -> bool {
    normalize_page_role(role).is_some()
}
