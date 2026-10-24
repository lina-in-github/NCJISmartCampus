-- 智慧校园数据库结构（SQLite）
-- 设计要点：
--   * 一个 user 可以绑定多个 auth；一个 auth 只属于一个班级、拥有一个角色。
--   * auth.key 同时是 /api 的认证密钥与 /login 的角色登录密钥。
--   * 家长没有独立 auth，使用孩子的学生 auth 登录（家长模式）。

CREATE TABLE IF NOT EXISTS users (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    username     TEXT NOT NULL COLLATE NOCASE UNIQUE,
    salt         TEXT NOT NULL,
    pwhash       TEXT NOT NULL,
    display_name TEXT NOT NULL DEFAULT '',
    prefs        TEXT NOT NULL DEFAULT '{}',
    created_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS classes (
    id         TEXT PRIMARY KEY,
    code       TEXT NOT NULL UNIQUE,
    name       TEXT NOT NULL,
    desc       TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    created_by TEXT
);

CREATE TABLE IF NOT EXISTS auths (
    id           TEXT PRIMARY KEY,
    key          TEXT NOT NULL UNIQUE,
    class_id     TEXT NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
    user_id      INTEGER REFERENCES users(id) ON DELETE SET NULL,
    role         TEXT NOT NULL CHECK (role IN ('student', 'teacher', 'admin')),
    student_id   TEXT,
    short_id     INTEGER,
    display_name TEXT NOT NULL DEFAULT '',
    created_at   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_auths_user ON auths(user_id);
CREATE INDEX IF NOT EXISTS idx_auths_class ON auths(class_id, role);
CREATE UNIQUE INDEX IF NOT EXISTS idx_auths_student
    ON auths(class_id, student_id) WHERE student_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_auths_short
    ON auths(class_id, short_id) WHERE short_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS submissions (
    id           TEXT PRIMARY KEY,
    class_id     TEXT NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    desc         TEXT NOT NULL DEFAULT '',
    type         TEXT NOT NULL DEFAULT 'homework',
    deadline     INTEGER,
    created_at   INTEGER NOT NULL,
    created_by   TEXT,
    created_name TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS idx_submissions_class ON submissions(class_id, created_at);

CREATE TABLE IF NOT EXISTS uploads (
    id            TEXT PRIMARY KEY,
    submission_id TEXT NOT NULL REFERENCES submissions(id) ON DELETE CASCADE,
    class_id      TEXT NOT NULL REFERENCES classes(id) ON DELETE CASCADE,
    auth_id       TEXT,
    user_id       INTEGER,
    student_id    TEXT,
    short_id      INTEGER,
    uploader_name TEXT NOT NULL DEFAULT '',
    role          TEXT NOT NULL,
    title         TEXT NOT NULL,
    desc          TEXT NOT NULL DEFAULT '',
    data          TEXT NOT NULL DEFAULT '[]',
    created_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_uploads_submission ON uploads(submission_id, created_at);
CREATE INDEX IF NOT EXISTS idx_uploads_class ON uploads(class_id, created_at);
CREATE INDEX IF NOT EXISTS idx_uploads_auth ON uploads(auth_id, created_at);
CREATE INDEX IF NOT EXISTS idx_uploads_student ON uploads(class_id, student_id, created_at);

CREATE TABLE IF NOT EXISTS replies (
    id           TEXT PRIMARY KEY,
    upload_id    TEXT NOT NULL REFERENCES uploads(id) ON DELETE CASCADE,
    auth_id      TEXT,
    replier_name TEXT NOT NULL DEFAULT '',
    role         TEXT NOT NULL DEFAULT 'teacher',
    content      TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_replies_upload ON replies(upload_id, created_at);
