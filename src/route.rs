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

use crate::tools::{bad, cookie_token, create_session, current_user_id, error_page, hash_password, html_escape, rand_string, render_me, unauthorized_json, with_session_cookie};
use crate::types::{AppState, ChangePw, LoginForm, RegisterForm, SCHEMA, SavePage};
pub async fn index(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if current_user_id(&st, &headers).is_some() {
        Redirect::to("/me").into_response()
    } else {
        let login_html=read_to_string("assets/login.html").expect("login.html not found");
        Html(login_html).into_response()
    }
}

pub async fn register(State(st): State<AppState>, Form(f): Form<RegisterForm>) -> Response {
    let username = f.username.trim().to_string();
    if username.is_empty() || f.password.is_empty() {
        return error_page("用户名和密码不能为空");
    }
    if f.password != f.password2 {
        return error_page("两次输入的密码不一致");
    }

    let salt = rand_string(16);
    let pwhash = hash_password(&salt, &f.password);
    let default_page = format!(
        r#"<!doctype html><html><body style="font-family:sans-serif;padding:24px">
             <h1>{}</h1>
             <p>这是你的个人主页，点击顶部的「编辑页面」即可修改这段 HTML。</p>
           </body></html>"#,
        html_escape(&username)
    );

    let res = {
        let db = st.db.lock().unwrap();
        db.execute(
            "INSERT INTO users (username, salt, pwhash, page) VALUES (?1, ?2, ?3, ?4)",
            params![username, salt, pwhash, default_page],
        )
        .map(|_| db.last_insert_rowid())
    };

    match res {
        Ok(uid) => {
            let token = create_session(&st, uid);
            with_session_cookie(Redirect::to("/me").into_response(), &token)
        }
        Err(e) => {
            let text = e.to_string();
            if text.contains("UNIQUE") {
                error_page("这个用户名已经被注册了")
            } else {
                error_page(&format!("注册失败：{text}"))
            }
        }
    }
}

pub async fn login(State(st): State<AppState>, Form(f): Form<LoginForm>) -> Response {
    let username = f.username.trim().to_string();

    let row = {
        let db = st.db.lock().unwrap();
        db.query_row(
            "SELECT id, salt, pwhash FROM users WHERE username = ?1",
            params![username],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )
        .optional()
    };

    match row {
        Ok(Some((uid, salt, hash))) if hash_password(&salt, &f.password) == hash => {
            let token = create_session(&st, uid);
            with_session_cookie(Redirect::to("/me").into_response(), &token)
        }
        _ => error_page("用户名或密码错误"),
    }
}

pub async fn logout(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if let Some(token) = cookie_token(&headers) {
        st.sessions.lock().unwrap().remove(&token);
    }
    let mut res = Redirect::to("/").into_response();
    res.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static("sid=; HttpOnly; Path=/; SameSite=Lax; Max-Age=0"),
    );
    res
}

pub async fn me_page(State(st): State<AppState>, headers: HeaderMap) -> Response {
    let Some(uid) = current_user_id(&st, &headers) else {
        return Redirect::to("/").into_response();
    };

    let name = {
        let db = st.db.lock().unwrap();
        db.query_row("SELECT username FROM users WHERE id = ?1", params![uid], |r| {
            r.get::<_, String>(0)
        })
        .optional()
    };

    match name {
        Ok(Some(name)) => Html(render_me(&name)).into_response(),
        _ => Redirect::to("/").into_response(),
    }
}

/// iframe 里加载的原始个人页面
pub async fn raw_page(State(st): State<AppState>, headers: HeaderMap) -> Response {
    let Some(uid) = current_user_id(&st, &headers) else {
        return (StatusCode::UNAUTHORIZED, Html("<p>未登录</p>")).into_response();
    };

    let page = {
        let db = st.db.lock().unwrap();
        db.query_row("SELECT page FROM users WHERE id = ?1", params![uid], |r| {
            r.get::<_, String>(0)
        })
        .optional()
    };

    match page {
        Ok(Some(html)) => {
            let mut res = Html(html).into_response();
            res.headers_mut()
                .insert("x-content-type-options", HeaderValue::from_static("nosniff"));
            res.headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            res
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, Html("读取失败")).into_response(),
    }
}

/// 读取页面源码（供编辑器用）
pub async fn get_page(State(st): State<AppState>, headers: HeaderMap) -> Response {
    let Some(uid) = current_user_id(&st, &headers) else {
        return unauthorized_json();
    };

    let page = {
        let db = st.db.lock().unwrap();
        db.query_row("SELECT page FROM users WHERE id = ?1", params![uid], |r| {
            r.get::<_, String>(0)
        })
        .optional()
    };

    match page {
        Ok(Some(source)) => Json(json!({"ok": true, "source": source})).into_response(),
        Ok(None) => unauthorized_json(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "msg": e.to_string()})),
        )
            .into_response(),
    }
}

/// 保存页面源码
pub async fn save_page(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<SavePage>,
) -> Response {
    let Some(uid) = current_user_id(&st, &headers) else {
        return unauthorized_json();
    };

    let res = {
        let db = st.db.lock().unwrap();
        db.execute(
            "UPDATE users SET page = ?1 WHERE id = ?2",
            params![body.source, uid],
        )
    };

    match res {
        Ok(_) => Json(json!({"ok": true, "msg": "已保存"})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "msg": e.to_string()})),
        )
            .into_response(),
    }
}

/// 修改密码
pub async fn change_password(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ChangePw>,
) -> Response {
    let Some(uid) = current_user_id(&st, &headers) else {
        return unauthorized_json();
    };

    if body.new.is_empty() {
        return bad("新密码不能为空");
    }
    if body.new != body.new2 {
        return bad("两次输入的新密码不一致");
    }

    let row = {
        let db = st.db.lock().unwrap();
        db.query_row(
            "SELECT salt, pwhash FROM users WHERE id = ?1",
            params![uid],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()
    };

    let Ok(Some((salt, hash))) = row else {
        return bad("用户不存在");
    };
    if hash_password(&salt, &body.old) != hash {
        return bad("原密码不正确");
    }

    let new_salt = rand_string(16);
    let new_hash = hash_password(&new_salt, &body.new);

    let res = {
        let db = st.db.lock().unwrap();
        db.execute(
            "UPDATE users SET salt = ?1, pwhash = ?2 WHERE id = ?3",
            params![new_salt, new_hash, uid],
        )
    };

    match res {
        Ok(_) => Json(json!({"ok": true, "msg": "密码已修改"})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"ok": false, "msg": e.to_string()})),
        )
            .into_response(),
    }
}
