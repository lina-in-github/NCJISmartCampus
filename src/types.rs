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
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    // session token -> user id（内存版，重启后需重新登录）
    pub sessions: Arc<Mutex<HashMap<String, i64>>>,
}

pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    salt     TEXT NOT NULL,
    pwhash   TEXT NOT NULL,
    page     TEXT NOT NULL
);
"#;





// ---------- 表单 / 请求体 ----------

#[derive(Deserialize)]
pub struct RegisterForm {
    pub username: String,
    pub password: String,
    pub password2: String,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SavePage {
    pub source: String,
}

#[derive(Deserialize)]
pub struct ChangePw {
    pub old: String,
    pub new: String,
    pub new2: String,
}
