use std::collections::HashMap;
use std::fs::read_to_string;
use std::sync::{Arc, Mutex};
use axum::{
    extract::{Form, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Json, Router,
};
use rand::{distributions::Alphanumeric, Rng};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::types::AppState;
pub fn rand_string(n: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

pub fn hash_password(salt: &str, password: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b":");
    h.update(password.as_bytes());
    format!("{:x}", h.finalize())
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie
        .split(';')
        .filter_map(|p| p.trim().strip_prefix("sid="))
        .next()
        .map(|s| s.to_string())
}

pub fn current_user_id(st: &AppState, headers: &HeaderMap) -> Option<i64> {
    let token = cookie_token(headers)?;
    st.sessions.lock().unwrap().get(&token).copied()
}

pub fn create_session(st: &AppState, user_id: i64) -> String {
    let token = rand_string(32);
    st.sessions.lock().unwrap().insert(token.clone(), user_id);
    token
}

pub fn with_session_cookie(mut res: Response, token: &str) -> Response {
    let cookie = format!("sid={token}; HttpOnly; Path=/; SameSite=Lax; Max-Age=86400");
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        res.headers_mut().insert(header::SET_COOKIE, v);
    }
    res
}

pub fn error_page(msg: &str) -> Response {
    Html(format!(
        r#"<div style="font-family:system-ui,sans-serif;padding:24px">
             <p style="color:#dc2626">{}</p>
             <p><a href="/">返回</a></p>
           </div>"#,
        html_escape(msg)
    ))
    .into_response()
}

pub fn bad(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "msg": msg}))).into_response()
}

pub fn unauthorized_json() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({"ok": false, "msg": "未登录"}))).into_response()
}



pub fn render_me(username: &str) -> String {
    read_to_string("assets/me.html").expect("me.html not found").replace("{{USERNAME}}", &html_escape(username))
}