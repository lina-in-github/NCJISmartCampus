use std::collections::HashMap;
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

// ---------- 状态 ----------

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
    // session token -> user id（内存版，重启后需重新登录）
    sessions: Arc<Mutex<HashMap<String, i64>>>,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    salt     TEXT NOT NULL,
    pwhash   TEXT NOT NULL,
    page     TEXT NOT NULL
);
"#;

// ---------- 工具函数 ----------

fn rand_string(n: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(n)
        .map(char::from)
        .collect()
}

fn hash_password(salt: &str, password: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b":");
    h.update(password.as_bytes());
    format!("{:x}", h.finalize())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn cookie_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie
        .split(';')
        .filter_map(|p| p.trim().strip_prefix("sid="))
        .next()
        .map(|s| s.to_string())
}

fn current_user_id(st: &AppState, headers: &HeaderMap) -> Option<i64> {
    let token = cookie_token(headers)?;
    st.sessions.lock().unwrap().get(&token).copied()
}

fn create_session(st: &AppState, user_id: i64) -> String {
    let token = rand_string(32);
    st.sessions.lock().unwrap().insert(token.clone(), user_id);
    token
}

fn with_session_cookie(mut res: Response, token: &str) -> Response {
    let cookie = format!("sid={token}; HttpOnly; Path=/; SameSite=Lax; Max-Age=86400");
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        res.headers_mut().insert(header::SET_COOKIE, v);
    }
    res
}

fn error_page(msg: &str) -> Response {
    Html(format!(
        r#"<div style="font-family:system-ui,sans-serif;padding:24px">
             <p style="color:#dc2626">{}</p>
             <p><a href="/">返回</a></p>
           </div>"#,
        html_escape(msg)
    ))
    .into_response()
}

fn bad(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "msg": msg}))).into_response()
}

fn unauthorized_json() -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({"ok": false, "msg": "未登录"}))).into_response()
}

// ---------- 页面模板 ----------

const LOGIN_HTML: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>登录 / 注册</title>
<style>
  body{font-family:system-ui,sans-serif;background:#f4f5f7;display:flex;
       justify-content:center;align-items:center;height:100vh;margin:0}
  .box{background:#fff;padding:24px 32px;border-radius:10px;
       box-shadow:0 4px 16px rgba(0,0,0,.1);width:320px}
  h1{font-size:20px;margin:0 0 12px}
  input{width:100%;padding:8px;margin:6px 0;box-sizing:border-box;border:1px solid #ccc;border-radius:6px}
  button{width:100%;padding:9px;margin-top:8px;background:#2563eb;color:#fff;
         border:0;border-radius:6px;cursor:pointer}
  .tabs{display:flex;gap:8px;margin-bottom:12px}
  .tabs button{background:#e5e7eb;color:#111}
  .tabs button.active{background:#2563eb;color:#fff}
</style>
</head>
<body>
<div class="box">
  <div class="tabs">
    <button class="active" onclick="show('login',this)">登录</button>
    <button onclick="show('reg',this)">注册</button>
  </div>

  <form id="login" method="post" action="/login">
    <h1>登录</h1>
    <input name="username" placeholder="用户名" required>
    <input name="password" type="password" placeholder="密码" required>
    <button type="submit">登录</button>
  </form>

  <form id="reg" method="post" action="/register" style="display:none">
    <h1>注册</h1>
    <input name="username" placeholder="用户名" required>
    <input name="password" type="password" placeholder="密码" required>
    <input name="password2" type="password" placeholder="重复密码" required>
    <button type="submit">注册并进入</button>
  </form>
</div>
<script>
function show(id, btn){
  document.getElementById('login').style.display = id === 'login' ? '' : 'none';
  document.getElementById('reg').style.display   = id === 'reg'   ? '' : 'none';
  document.querySelectorAll('.tabs button').forEach(b => b.classList.remove('active'));
  btn.classList.add('active');
}
</script>
</body>
</html>"#;

const ME_HTML: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>我的主页</title>
<style>
  html,body{margin:0;height:100%;font-family:system-ui,sans-serif}
  body{display:flex;flex-direction:column}
  header{display:flex;gap:8px;align-items:center;padding:8px 12px;background:#1f2937;color:#fff}
  header .spacer{flex:1}
  button{cursor:pointer;padding:6px 12px;border:0;border-radius:6px;background:#2563eb;color:#fff}
  button.gray{background:#4b5563}
  iframe{flex:1;width:100%;border:0;background:#fff}
  .modal{position:fixed;top:0;left:0;right:0;bottom:0;background:rgba(0,0,0,.5);
         display:flex;justify-content:center;align-items:center}
  .modal.hidden{display:none}
  .panel{background:#fff;padding:16px;border-radius:10px;width:min(860px,92vw);
         max-height:88vh;display:flex;flex-direction:column;gap:10px}
  textarea{width:100%;height:50vh;font-family:ui-monospace,monospace;font-size:13px;
           padding:8px;box-sizing:border-box}
  .row{display:flex;gap:8px;align-items:center}
  input{padding:7px;border:1px solid #ccc;border-radius:6px;width:100%;box-sizing:border-box}
  .msg{color:#16a34a;font-size:13px}
  .msg.err{color:#dc2626}
</style>
</head>
<body>
<header>
  <strong>{{USERNAME}}</strong>
  <span class="spacer"></span>
  <button id="btn-edit">编辑页面</button>
  <button id="btn-pw">修改密码</button>
  <form method="post" action="/logout"><button class="gray">退出登录</button></form>
</header>

<!-- 个人页面预览（sandbox 限制用户脚本访问父页面/cookie） -->
<iframe id="preview" src="/page" sandbox="allow-scripts"></iframe>

<!-- 编辑源码弹窗 -->
<div class="modal hidden" id="editor">
  <div class="panel">
    <h3 style="margin:0">编辑页面源码（HTML）</h3>
    <textarea id="source" spellcheck="false"></textarea>
    <div class="row">
      <button id="btn-save">保存</button>
      <button class="gray" id="btn-cancel">取消</button>
      <span class="msg" id="save-msg"></span>
    </div>
  </div>
</div>

<!-- 修改密码弹窗 -->
<div class="modal hidden" id="pwbox">
  <div class="panel" style="width:360px">
    <h3 style="margin:0">修改密码</h3>
    <input id="old"  type="password" placeholder="原密码">
    <input id="new1" type="password" placeholder="新密码">
    <input id="new2" type="password" placeholder="重复新密码">
    <div class="row">
      <button id="btn-pw-save">确定</button>
      <button class="gray" id="btn-pw-cancel">取消</button>
      <span class="msg" id="pw-msg"></span>
    </div>
  </div>
</div>

<script>
const $ = id => document.getElementById(id);
const preview = $('preview');

function toggle(el, on){ el.classList.toggle('hidden', !on); }
function setMsg(el, text, isErr){
  el.textContent = text;
  el.className = 'msg' + (isErr ? ' err' : '');
}
function refreshPreview(){ preview.src = '/page?_=' + Date.now(); }

$('btn-edit').onclick = async () => {
  const r = await fetch('/api/page');
  if (!r.ok) return;
  const d = await r.json();
  $('source').value = d.source;
  setMsg($('save-msg'), '');
  toggle($('editor'), true);
};

$('btn-cancel').onclick = () => toggle($('editor'), false);

$('btn-save').onclick = async () => {
  const r = await fetch('/api/page', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ source: $('source').value })
  });
  const d = await r.json().catch(() => ({ ok: false, msg: '请求失败' }));
  if (r.ok && d.ok) {
    setMsg($('save-msg'), '已保存');
    refreshPreview();
  } else {
    setMsg($('save-msg'), d.msg || '保存失败', true);
  }
};

$('btn-pw').onclick = () => {
  ['old', 'new1', 'new2'].forEach(i => $(i).value = '');
  setMsg($('pw-msg'), '');
  toggle($('pwbox'), true);
};

$('btn-pw-cancel').onclick = () => toggle($('pwbox'), false);

$('btn-pw-save').onclick = async () => {
  const body = { old: $('old').value, new: $('new1').value, new2: $('new2').value };
  const r = await fetch('/api/password', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body)
  });
  const d = await r.json().catch(() => ({ ok: false, msg: '请求失败' }));
  if (r.ok && d.ok) {
    setMsg($('pw-msg'), '密码已修改');
  } else {
    setMsg($('pw-msg'), d.msg || '修改失败', true);
  }
};
</script>
</body>
</html>"#;

fn render_me(username: &str) -> String {
    ME_HTML.replace("{{USERNAME}}", &html_escape(username))
}

// ---------- 表单 / 请求体 ----------

#[derive(Deserialize)]
struct RegisterForm {
    username: String,
    password: String,
    password2: String,
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct SavePage {
    source: String,
}

#[derive(Deserialize)]
struct ChangePw {
    old: String,
    new: String,
    new2: String,
}

// ---------- Handlers ----------

async fn index(State(st): State<AppState>, headers: HeaderMap) -> Response {
    if current_user_id(&st, &headers).is_some() {
        Redirect::to("/me").into_response()
    } else {
        Html(LOGIN_HTML).into_response()
    }
}

async fn register(State(st): State<AppState>, Form(f): Form<RegisterForm>) -> Response {
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

async fn login(State(st): State<AppState>, Form(f): Form<LoginForm>) -> Response {
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

async fn logout(State(st): State<AppState>, headers: HeaderMap) -> Response {
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

async fn me_page(State(st): State<AppState>, headers: HeaderMap) -> Response {
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
async fn raw_page(State(st): State<AppState>, headers: HeaderMap) -> Response {
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
async fn get_page(State(st): State<AppState>, headers: HeaderMap) -> Response {
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
async fn save_page(
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
async fn change_password(
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

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("端口绑定失败");

    println!("服务器已启动: http://127.0.0.1:3000");
    axum::serve(listener, app).await.expect("服务器运行失败");
}

