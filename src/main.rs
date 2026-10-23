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

use crate::route::{change_password, get_page, index, login, logout, me_page, raw_page, register, save_page};
use crate::tools::{bad, cookie_token, create_session, current_user_id, error_page, hash_password, html_escape, rand_string, render_me, unauthorized_json, with_session_cookie};
use crate::types::{AppState, ChangePw, LoginForm, RegisterForm, SCHEMA, SavePage};
mod types;
mod tools;
mod route;



// ---------- Handlers ----------


// ---------- main ----------

#[tokio::main]
async fn main() {
    let conn = Connection::open("app.db").expect("无法打开数据库");
    conn.execute_batch(SCHEMA).expect("初始化数据表失败");

    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
        sessions: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/login", post(login))
        .route("/register", post(register))
        .route("/logout", post(logout))
        .route("/me", get(me_page))
        .route("/page", get(raw_page))
        .route("/api/page", get(get_page).post(save_page))
        .route("/api/password", post(change_password))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("[::]:3000")
        .await
        .expect("端口绑定失败");

    println!("服务器已启动: http://[::]:3000");
    axum::serve(listener, app).await.expect("服务器运行失败");
}

