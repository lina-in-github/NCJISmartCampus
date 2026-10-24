use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    extract::DefaultBodyLimit,
    http::{header, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};

use crate::types::AppState;

mod api;
mod db;
mod pages;
mod strings;
mod survey;
mod tools;
mod types;
mod view;

const FAVICON: &str = include_str!("./assets/favicon.csv"); 

async fn static_css() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("assets/app.css"),
    )
        .into_response()
}

async fn static_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        crate::strings::apply(include_str!("assets/app.js")),
    )
        .into_response()
}

async fn static_strings() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        strings::js_object(),
    )
        .into_response()
}

async fn favicon() -> Response {
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        FAVICON,
    )
        .into_response()
}

async fn fallback(uri: Uri) -> Response {
    if uri.path().starts_with("/api/") {
        return tools::api_not_found("endpoint not found");
    }
    view::not_found(None, &strings::t("error_not_found_desc"))
}

#[tokio::main]
async fn main() {
    let strings_dir = std::env::var("SC_STRINGS_DIR").unwrap_or_else(|_| "strings".to_string());
    let strings_db = std::env::var("SC_STRINGS_DB").unwrap_or_else(|_| "strings.db".to_string());
    let lang = std::env::var("SC_LANG").unwrap_or_else(|_| "zh-CN".to_string());
    match strings::init(&strings_dir, &strings_db, &lang) {
        Ok(Some(warning)) => eprintln!("{warning}"),
        Ok(None) => {}
        Err(e) => {
            eprintln!("failed to load UI strings: {e}");
            std::process::exit(1);
        }
    }

    let db_path = std::env::var("SC_DB").unwrap_or_else(|_| "smartcampus.db".to_string());
    let conn = db::open(&db_path).expect("database initialization failed");

    let state = AppState {
        db: Arc::new(db::Database::new(conn)),
        sessions: Arc::new(Mutex::new(HashMap::new())),
        user_sessions: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/", get(pages::root))
        .route("/login", get(pages::login_index))
        .route("/login/account", get(pages::login_account_page))
        .route(
            "/login/{mode}/{role}",
            get(pages::login_page).post(pages::login_submit),
        )
        .route("/logout", post(pages::logout))
        .route("/home", get(pages::home_entry))
        .route("/home/{mode}/{role}", get(pages::home_index))
        .route("/home/{mode}/{role}/", get(pages::home_slash))
        .route("/home/{mode}/{role}/{*rest}", get(pages::home_wild))
        .route("/user", get(pages::user_page))
        .route("/user/login", post(pages::user_login))
        .route("/user/register", post(pages::user_register))
        .route("/user/claim", post(pages::user_claim))
        .route("/user/enter", post(pages::user_enter))
        .route("/user/auth/{id}/delete", post(pages::user_delete_auth))
        .route("/user/prefs", post(pages::user_prefs))
        .route("/user/password", post(pages::user_password))
        .route("/user/class", post(pages::user_create_class))
        .route("/user/logout", post(pages::user_logout))
        .route("/api/upload", post(api::upload))
        .route("/api/download", post(api::download))
        .route("/api/new-submission", post(api::new_submission))
        .route("/api/all-submission", post(api::all_submission))
        .route("/api/reply", post(api::reply))
        .route("/api/new-class", post(api::new_class))
        .route("/api/new-user", post(api::new_user))
        .route("/api/del-key", post(api::del_key))
        .route("/api/rename-auth", post(api::rename_auth))
        .route("/api/del-object", post(api::del_object))
        .route("/api/health", get(api::health))
        .route("/static/app.css", get(static_css))
        .route("/static/app.js", get(static_js))
        .route("/static/strings.js", get(static_strings))
        .route("/favicon.svg", get(favicon))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .fallback(fallback)
        .with_state(state);

    let port: u16 = std::env::var("SC_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let bind = std::env::var("SC_BIND").unwrap_or_else(|_| format!("[::]:{port}"));
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .expect("failed to bind listener");
    let display = if let Some(rest) = bind.strip_prefix("0.0.0.0:") {
        format!("localhost:{rest}")
    } else if let Some(rest) = bind.strip_prefix("[::]:") {
        format!("localhost:{rest}")
    } else {
        bind.clone()
    };

    let (string_count, string_lang) = strings::stats();
    println!("{} · Campus OS", strings::t("title"));
    println!("  Address : http://{display}");
    println!("  Strings : {string_count} ({string_lang})");
    println!("  Database: {db_path}");
    println!(
        "  Demo    : keys {} / {} / {}",
        db::DEMO_ADMIN_KEY,
        db::DEMO_TEACHER_KEY,
        db::DEMO_STUDENT_KEY
    );
    println!("            accounts admin/admin123 · teacher/teacher123 · student/student123");
    axum::serve(listener, app).await.expect("server failed");
}
