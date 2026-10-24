use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::tools::{
    b64_encode, hash_password, new_auth_key, new_class_code, new_id, now_ts, rand_string,
};
use crate::types::{AuthCtx, Prefs, UserCtx, ROLE_ADMIN, ROLE_STUDENT, ROLE_TEACHER};

pub const SCHEMA: &str = include_str!("schema.sql");

pub const DEMO_ADMIN_KEY: &str = "SC-ADM-DEMO-2026";
pub const DEMO_TEACHER_KEY: &str = "SC-TEA-DEMO-2026";
pub const DEMO_STUDENT_KEY: &str = "SC-STU-DEMO-2026";
pub const DEMO_STUDENT2_KEY: &str = "SC-STU-DEMO-2026-02";
pub const DEMO_STUDENT3_KEY: &str = "SC-STU-DEMO-2026-03";
pub const DEMO_ADMIN2_KEY: &str = "SC-ADM-DEMO-2027";
pub const DEMO_TEACHER2_KEY: &str = "SC-TEA-DEMO-2027";
pub const DEMO_STUDENT_C2_KEY: &str = "SC-STU-DEMO-2027";

pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA synchronous=NORMAL;",
    )?;
    conn.execute_batch(SCHEMA)?;
    seed_if_empty(&conn)?;
    Ok(conn)
}

// ---------------------------------------------------------------------------
// 查询
// ---------------------------------------------------------------------------

const AUTH_SQL: &str = "SELECT a.id, a.key, a.class_id, c.name, c.desc, a.user_id, a.role,
                               a.student_id, a.short_id, a.display_name
                        FROM auths a JOIN classes c ON c.id = a.class_id";

fn auth_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AuthCtx> {
    Ok(AuthCtx {
        id: r.get(0)?,
        key: r.get(1)?,
        class_id: r.get(2)?,
        class_name: r.get(3)?,
        class_desc: r.get(4)?,
        user_id: r.get(5)?,
        role: r.get(6)?,
        student_id: r.get(7)?,
        short_id: r.get(8)?,
        display_name: r.get(9)?,
    })
}

pub fn load_auth_by_id(db: &Connection, id: &str) -> Option<AuthCtx> {
    db.query_row(
        &format!("{AUTH_SQL} WHERE a.id = ?1"),
        params![id],
        auth_from_row,
    )
    .optional()
    .ok()
    .flatten()
}

pub fn load_auth_by_key(db: &Connection, key: &str) -> Option<AuthCtx> {
    let key = key.trim();
    let found = db
        .query_row(
            &format!("{AUTH_SQL} WHERE a.key = ?1"),
            params![key],
            auth_from_row,
        )
        .optional()
        .ok()
        .flatten();
    if found.is_some() {
        return found;
    }
    let upper = key.to_uppercase();
    if upper != key {
        return db
            .query_row(
                &format!("{AUTH_SQL} WHERE a.key = ?1"),
                params![upper],
                auth_from_row,
            )
            .optional()
            .ok()
            .flatten();
    }
    None
}

pub fn auth_key_exists(db: &Connection, key: &str) -> bool {
    db.query_row("SELECT 1 FROM auths WHERE key = ?1", params![key], |_| {
        Ok(())
    })
    .optional()
    .ok()
    .flatten()
    .is_some()
}

pub fn load_user_by_id(db: &Connection, id: i64) -> Option<UserCtx> {
    db.query_row(
        "SELECT id, username, display_name, prefs FROM users WHERE id = ?1",
        params![id],
        |r| {
            Ok(UserCtx {
                id: r.get(0)?,
                username: r.get(1)?,
                display_name: r.get(2)?,
                prefs: Prefs::from_json(&r.get::<_, String>(3)?),
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

pub struct UserWithSecret {
    pub user: UserCtx,
    pub salt: String,
    pub pwhash: String,
}

pub fn load_user_with_secret(db: &Connection, username: &str) -> Option<UserWithSecret> {
    db.query_row(
        "SELECT id, username, display_name, prefs, salt, pwhash
         FROM users WHERE username = ?1 COLLATE NOCASE",
        params![username.trim()],
        |r| {
            Ok(UserWithSecret {
                user: UserCtx {
                    id: r.get(0)?,
                    username: r.get(1)?,
                    display_name: r.get(2)?,
                    prefs: Prefs::from_json(&r.get::<_, String>(3)?),
                },
                salt: r.get(4)?,
                pwhash: r.get(5)?,
            })
        },
    )
    .optional()
    .ok()
    .flatten()
}

pub fn user_auths(db: &Connection, user_id: i64) -> Vec<AuthCtx> {
    let sql = format!(
        "{AUTH_SQL} WHERE a.user_id = ?1 ORDER BY c.name COLLATE NOCASE, a.role, a.short_id"
    );
    let mut stmt = match db.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt.query_map(params![user_id], auth_from_row);
    match rows {
        Ok(iter) => iter.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

pub fn class_code(db: &Connection, class_id: &str) -> String {
    db.query_row(
        "SELECT code FROM classes WHERE id = ?1",
        params![class_id],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|_| "CLASS".to_string())
}

// ---------------------------------------------------------------------------
// 写入
// ---------------------------------------------------------------------------

pub struct CreatedClass {
    pub id: String,
    pub code: String,
    pub auth_id: String,
    pub key: String,
}

pub fn create_class(
    db: &Connection,
    creator: &AuthCtx,
    name: &str,
    desc: &str,
) -> Result<CreatedClass, String> {
    let name = name.trim();
    let desc = desc.trim();
    let class_name = if !name.is_empty() {
        name.to_string()
    } else if !desc.is_empty() {
        crate::tools::truncate_chars(desc.lines().next().unwrap_or(desc), 30)
    } else {
        "未命名班级".to_string()
    };
    let name_chars = class_name.chars().count();
    if name_chars > 40 {
        return Err("class name is too long (max 40 characters)".to_string());
    }
    if desc.chars().count() > 500 {
        return Err("class description is too long (max 500 characters)".to_string());
    }

    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    let class_id = new_id("cls");
    let mut code = new_class_code();
    for _ in 0..20 {
        let exists: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM classes WHERE code = ?1",
                params![code],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if exists.is_none() {
            break;
        }
        code = new_class_code();
    }
    let now = now_ts();
    tx.execute(
        "INSERT INTO classes (id, code, name, desc, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![class_id, code, class_name, desc, now, creator.id],
    )
    .map_err(|e| e.to_string())?;

    let auth_id = new_id("auth");
    let mut key = new_auth_key(ROLE_ADMIN);
    for _ in 0..20 {
        if !auth_key_exists(&tx, &key) {
            break;
        }
        key = new_auth_key(ROLE_ADMIN);
    }
    tx.execute(
        "INSERT INTO auths (id, key, class_id, user_id, role, display_name, created_at)
         VALUES (?1, ?2, ?3, ?4, 'admin', ?5, ?6)",
        params![
            auth_id,
            key,
            class_id,
            creator.user_id,
            creator.identity(),
            now
        ],
    )
    .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;
    Ok(CreatedClass {
        id: class_id,
        code,
        auth_id,
        key,
    })
}

#[derive(Debug, Clone)]
pub struct CreatedMember {
    pub id: String,
    pub key: String,
    pub role: String,
    pub student_id: Option<String>,
    pub display_name: String,
}

pub fn normalize_role(kind: &str) -> Option<&'static str> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "student" | "学生" => Some(ROLE_STUDENT),
        "teacher" | "老师" => Some(ROLE_TEACHER),
        "admin" | "管理员" => Some(ROLE_ADMIN),
        _ => None,
    }
}

pub fn create_members(
    db: &Connection,
    admin: &AuthCtx,
    class_id: &str,
    kind: &str,
    quantity: i64,
) -> Result<Vec<CreatedMember>, String> {
    if class_id != admin.class_id {
        return Err("members can only be added to the class of the current admin auth".to_string());
    }
    if !(1..=100).contains(&quantity) {
        return Err("quantity must be between 1 and 100".to_string());
    }
    let role = normalize_role(kind)
        .ok_or_else(|| "type must be one of Student / Teacher / Admin".to_string())?;
    let code = class_code(db, class_id);
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    let max_short: i64 = tx
        .query_row(
            "SELECT COALESCE(MAX(short_id), 0) FROM auths WHERE class_id = ?1",
            params![class_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let now = now_ts();
    let mut out = Vec::new();
    for i in 0..quantity {
        let short = if role == ROLE_STUDENT {
            Some(max_short + i + 1)
        } else {
            None
        };
        let student_id = short.map(|s| format!("{code}-{s:02}"));
        let display_name = match role {
            ROLE_STUDENT => format!("学生 {:02}", short.unwrap_or(0)),
            ROLE_TEACHER => format!("教师 {:02}", i + 1),
            _ => format!("管理员 {:02}", i + 1),
        };
        let id = new_id("auth");
        let mut key = new_auth_key(role);
        for _ in 0..20 {
            if !auth_key_exists(&tx, &key) {
                break;
            }
            key = new_auth_key(role);
        }
        tx.execute(
            "INSERT INTO auths
             (id, key, class_id, user_id, role, student_id, short_id, display_name, created_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                key,
                class_id,
                role,
                student_id,
                short,
                display_name,
                now
            ],
        )
        .map_err(|e| e.to_string())?;
        out.push(CreatedMember {
            id,
            key,
            role: role.to_string(),
            student_id,
            display_name,
        });
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// 演示数据
// ---------------------------------------------------------------------------

fn seed_if_empty(conn: &Connection) -> rusqlite::Result<()> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM classes", [], |r| r.get(0))?;
    if n > 0 {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    let now = now_ts();

    // 用户
    let admin = seed_user(&tx, "admin", "admin123", "赵明远")?;
    let teacher = seed_user(&tx, "teacher", "teacher123", "林书言")?;
    let student = seed_user(&tx, "student", "student123", "陈小满")?;

    // 班级
    let c1 = new_id("cls");
    let c2 = new_id("cls");
    tx.execute(
        "INSERT INTO classes (id, code, name, desc, created_at, created_by)
         VALUES (?1, 'G2603', '高一（3）班', '2026 级 · 智慧校园示范班', ?2, NULL)",
        params![c1, now - 30 * 86_400],
    )?;
    tx.execute(
        "INSERT INTO classes (id, code, name, desc, created_at, created_by)
         VALUES (?1, 'G2601', '高二（1）班', '2026 级 · 理科实验班', ?2, NULL)",
        params![c2, now - 25 * 86_400],
    )?;

    // 身份 / auth
    let admin1 = new_id("auth");
    let admin2 = new_id("auth");
    let teacher1 = new_id("auth");
    let teacher2 = new_id("auth");
    let stu1 = new_id("auth");
    let stu2 = new_id("auth");
    let stu3 = new_id("auth");
    let stu_c2 = new_id("auth");

    seed_auth(
        &tx,
        &admin1,
        DEMO_ADMIN_KEY,
        &c1,
        Some(admin),
        ROLE_ADMIN,
        None,
        None,
        "赵明远",
    )?;
    seed_auth(
        &tx,
        &admin2,
        DEMO_ADMIN2_KEY,
        &c2,
        Some(admin),
        ROLE_ADMIN,
        None,
        None,
        "赵明远",
    )?;
    seed_auth(
        &tx,
        &teacher1,
        DEMO_TEACHER_KEY,
        &c1,
        Some(teacher),
        ROLE_TEACHER,
        None,
        None,
        "林书言",
    )?;
    seed_auth(
        &tx,
        &teacher2,
        DEMO_TEACHER2_KEY,
        &c2,
        None,
        ROLE_TEACHER,
        None,
        None,
        "待认领教师",
    )?;
    seed_auth(
        &tx,
        &stu1,
        DEMO_STUDENT_KEY,
        &c1,
        Some(student),
        ROLE_STUDENT,
        Some("G2603-01"),
        Some(1),
        "陈小满",
    )?;
    seed_auth(
        &tx,
        &stu2,
        DEMO_STUDENT2_KEY,
        &c1,
        None,
        ROLE_STUDENT,
        Some("G2603-02"),
        Some(2),
        "李知遥",
    )?;
    seed_auth(
        &tx,
        &stu3,
        DEMO_STUDENT3_KEY,
        &c1,
        None,
        ROLE_STUDENT,
        Some("G2603-03"),
        Some(3),
        "王浩然",
    )?;
    seed_auth(
        &tx,
        &stu_c2,
        DEMO_STUDENT_C2_KEY,
        &c2,
        None,
        ROLE_STUDENT,
        Some("G2601-01"),
        Some(1),
        "周雨桐",
    )?;

    // 征集
    let sub_homework = new_id("sub");
    let sub_sports = new_id("sub");
    let sub_leave = new_id("sub");
    let sub_photo = new_id("sub");
    let sub_english = new_id("sub");
    let sub_physics = new_id("sub");

    seed_submission(
        &tx,
        &sub_homework,
        &c1,
        "函数与导数 · 周末练习",
        "完成练习册 P42–P45，拍照上传完整的解题过程。重点复习导数的几何意义与链式法则。",
        "homework",
        Some(now + 2 * 86_400 + 4 * 3600),
        now - 2 * 86_400,
        &teacher1,
        "林书言",
    )?;
    seed_submission(
        &tx,
        &sub_sports,
        &c1,
        "秋季运动会报名",
        "10 月 30 日校园秋季运动会开始报名。请上传报名的项目与联系方式，每人最多报名两项。",
        "open call",
        Some(now + 4 * 86_400),
        now - 20 * 3600,
        &teacher1,
        "林书言",
    )?;
    seed_submission(
        &tx,
        &sub_leave,
        &c1,
        "研学活动请假申请",
        "11 月 5 日城市研学活动，无法参加的同学请提交请假申请并说明原因，需附家长签字。",
        "leave request",
        Some(now + 6 * 86_400),
        now - 5 * 3600,
        &teacher1,
        "林书言",
    )?;
    seed_submission(
        &tx,
        &sub_photo,
        &c1,
        "校园摄影展作品征集",
        "主题：秋日校园。上传一张原创摄影作品，并附 100 字以内的作品说明。",
        "open call",
        Some(now - 7 * 3600),
        now - 8 * 86_400,
        &teacher1,
        "林书言",
    )?;
    seed_submission(
        &tx,
        &sub_english,
        &c1,
        "英语单元测验订正",
        "Unit 3 单元测验订正，请在错题旁写出正确选项与解析，周一前提交。",
        "homework",
        Some(now + 30 * 3600),
        now - 3 * 86_400,
        &teacher1,
        "林书言",
    )?;
    seed_submission(
        &tx,
        &sub_physics,
        &c2,
        "物理实验报告 · 伏安特性曲线",
        "完成测量小灯泡伏安特性曲线的实验报告，附上数据表与图像。",
        "homework",
        Some(now + 3 * 86_400),
        now - 86_400,
        &admin2,
        "赵明远",
    )?;

    // 上传
    let up1 = new_id("up");
    seed_upload(
        &tx,
        &up1,
        &sub_homework,
        &c1,
        &stu1,
        Some(student),
        Some("G2603-01"),
        Some(1),
        "陈小满",
        ROLE_STUDENT,
        "周末练习 · 第 1 页",
        "完成练习册 P42–P45，第 3 题不太确定。",
        json!([
            demo_homework_svg("函数与导数 · 周末练习", "陈小满  G2603-01"),
            demo_text(
                "解题过程.txt",
                "1. 导数定义：f'(x)=lim...\n2. 第 3 题使用链式法则：...\n"
            )
        ]),
        now - 3 * 3600,
    )?;
    let up2 = new_id("up");
    seed_upload(
        &tx,
        &up2,
        &sub_homework,
        &c1,
        &stu2,
        None,
        Some("G2603-02"),
        Some(2),
        "李知遥",
        ROLE_STUDENT,
        "李知遥的周末练习",
        "已完成，请老师检查。",
        json!([demo_homework_svg(
            "函数与导数 · 周末练习",
            "李知遥  G2603-02"
        )]),
        now - 70 * 60,
    )?;
    let up3 = new_id("up");
    seed_upload(
        &tx,
        &up3,
        &sub_sports,
        &c1,
        &stu3,
        None,
        Some("G2603-03"),
        Some(3),
        "王浩然",
        ROLE_STUDENT,
        "运动会报名：100m / 跳远",
        "报名 100 米和跳远，联系方式：138****6721。",
        json!([demo_text(
            "运动会报名.txt",
            "姓名：王浩然\n学号：G2603-03\n项目：100m、跳远\n联系方式：138****6721\n"
        )]),
        now - 40 * 60,
    )?;
    let up4 = new_id("up");
    seed_upload(
        &tx,
        &up4,
        &sub_leave,
        &c1,
        &stu1,
        Some(student),
        Some("G2603-01"),
        Some(1),
        "陈小满",
        ROLE_STUDENT,
        "研学活动请假申请",
        "因家庭安排无法参加 11 月 5 日研学活动，家长已签字。",
        json!([demo_leave_svg("研学活动请假申请", "陈小满  G2603-01")]),
        now - 25 * 60,
    )?;
    let up5 = new_id("up");
    seed_upload(
        &tx,
        &up5,
        &sub_photo,
        &c1,
        &stu2,
        None,
        Some("G2603-02"),
        Some(2),
        "李知遥",
        ROLE_STUDENT,
        "秋日校园 · 银杏大道",
        "拍摄于周三傍晚的银杏大道，逆光下的落叶很有层次。",
        json!([demo_photo_svg("秋日校园 · 银杏大道", "摄影：李知遥")]),
        now - 10 * 3600,
    )?;
    let up6 = new_id("up");
    seed_upload(
        &tx,
        &up6,
        &sub_physics,
        &c2,
        &stu_c2,
        None,
        Some("G2601-01"),
        Some(1),
        "周雨桐",
        ROLE_STUDENT,
        "伏安特性曲线实验报告",
        "数据表和图像已整理，误差分析还需要补充。",
        json!([demo_homework_svg("物理实验报告", "周雨桐  G2601-01")]),
        now - 2 * 3600,
    )?;

    // 回复
    seed_reply(
        &tx,
        &new_id("rep"),
        &up1,
        &teacher1,
        "林书言",
        ROLE_TEACHER,
        "整体完成得很好：第 1、2 题步骤完整。第 3 题注意链式法则，已在上传第 2 页标出。",
        now - 2 * 3600,
    )?;
    seed_reply(
        &tx,
        &new_id("rep"),
        &up4,
        &admin1,
        "赵明远",
        ROLE_ADMIN,
        "已收到，课程笔记和研学手册请于返校后到教务处补领。",
        now - 10 * 60,
    )?;
    seed_reply(
        &tx,
        &new_id("rep"),
        &up5,
        &teacher1,
        "林书言",
        ROLE_TEACHER,
        "构图和光影都不错，建议投稿到本次摄影展。",
        now - 9 * 3600,
    )?;

    tx.commit()
}

fn seed_user(
    conn: &Connection,
    username: &str,
    password: &str,
    display_name: &str,
) -> rusqlite::Result<i64> {
    let salt = rand_string(16);
    let pwhash = hash_password(&salt, password);
    conn.execute(
        "INSERT INTO users (username, salt, pwhash, display_name, prefs, created_at)
         VALUES (?1, ?2, ?3, ?4, '{}', ?5)",
        params![username, salt, pwhash, display_name, now_ts()],
    )?;
    Ok(conn.last_insert_rowid())
}

#[allow(clippy::too_many_arguments)]
fn seed_auth(
    conn: &Connection,
    id: &str,
    key: &str,
    class_id: &str,
    user_id: Option<i64>,
    role: &str,
    student_id: Option<&str>,
    short_id: Option<i64>,
    display_name: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO auths
         (id, key, class_id, user_id, role, student_id, short_id, display_name, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            key,
            class_id,
            user_id,
            role,
            student_id,
            short_id,
            display_name,
            now_ts()
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn seed_submission(
    conn: &Connection,
    id: &str,
    class_id: &str,
    name: &str,
    desc: &str,
    kind: &str,
    deadline: Option<i64>,
    created_at: i64,
    created_by: &str,
    created_name: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO submissions
         (id, class_id, name, desc, type, deadline, created_at, created_by, created_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            class_id,
            name,
            desc,
            kind,
            deadline,
            created_at,
            created_by,
            created_name
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn seed_upload(
    conn: &Connection,
    id: &str,
    submission_id: &str,
    class_id: &str,
    auth_id: &str,
    user_id: Option<i64>,
    student_id: Option<&str>,
    short_id: Option<i64>,
    uploader_name: &str,
    role: &str,
    title: &str,
    desc: &str,
    data: Value,
    created_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO uploads
         (id, submission_id, class_id, auth_id, user_id, student_id, short_id,
          uploader_name, role, title, desc, data, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            submission_id,
            class_id,
            auth_id,
            user_id,
            student_id,
            short_id,
            uploader_name,
            role,
            title,
            desc,
            data.to_string(),
            created_at
        ],
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn seed_reply(
    conn: &Connection,
    id: &str,
    upload_id: &str,
    auth_id: &str,
    replier_name: &str,
    role: &str,
    content: &str,
    created_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO replies (id, upload_id, auth_id, replier_name, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id,
            upload_id,
            auth_id,
            replier_name,
            role,
            content,
            created_at
        ],
    )?;
    Ok(())
}

fn demo_attachment(name: &str, svg: &str) -> Value {
    json!({
        "name": name,
        "mime": "image/svg+xml",
        "data": b64_encode(svg.as_bytes())
    })
}

fn demo_text(name: &str, content: &str) -> Value {
    json!({
        "name": name,
        "mime": "text/plain",
        "data": b64_encode(content.as_bytes())
    })
}

fn demo_homework_svg(title: &str, subtitle: &str) -> Value {
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="880" height="620" viewBox="0 0 880 620">
<defs>
  <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="#f8fafc"/><stop offset="1" stop-color="#eef2ff"/>
  </linearGradient>
  <pattern id="grid" width="28" height="28" patternUnits="userSpaceOnUse">
    <path d="M28 0H0V28" fill="none" stroke="#dbe3f4" stroke-width="1"/>
  </pattern>
</defs>
<rect width="880" height="620" rx="20" fill="url(#bg)"/>
<rect x="54" y="40" width="772" height="540" rx="14" fill="#ffffff" stroke="#dbe3f4"/>
<rect x="54" y="40" width="772" height="76" rx="14" fill="#eef2ff"/>
<rect x="54" y="102" width="772" height="14" fill="#eef2ff"/>
<text x="86" y="76" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="22" font-weight="700" fill="#312e81">{title}</text>
<text x="86" y="102" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="14" fill="#64748b">{subtitle}</text>
<rect x="86" y="140" width="708" height="408" fill="url(#grid)" rx="8"/>
<text x="108" y="180" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="16" fill="#334155">1. 求函数 f(x)=x³-3x 的单调区间与极值。</text>
<text x="108" y="214" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="15" fill="#475569">解：f'(x)=3x² - 3 = 3(x-1)(x+1)</text>
<text x="108" y="246" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="15" fill="#475569">令 f'(x)=0，得 x=±1；列表讨论符号变化……</text>
<line x1="108" y1="268" x2="760" y2="268" stroke="#e2e8f0"/>
<text x="108" y="306" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="16" fill="#334155">2. 已知曲线 y=x²+2x，求过点 (1,3) 的切线方程。</text>
<text x="108" y="340" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="15" fill="#475569">解：y'=2x+2，在 x=1 处斜率为 4，切线方程为 y-3=4(x-1)。</text>
<line x1="108" y1="362" x2="760" y2="362" stroke="#e2e8f0"/>
<circle cx="560" cy="430" r="60" fill="#f0fdf4" stroke="#86efac" stroke-width="2"/>
<circle cx="560" cy="430" r="46" fill="none" stroke="#22c55e" stroke-width="4" stroke-dasharray="220 80"/>
<text x="560" y="438" text-anchor="middle" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="20" fill="#15803d">已检查</text>
<text x="108" y="500" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="14" fill="#94a3b8">第 3 题：注意链式法则，先对外层求导再乘内层导数。</text>
<text x="760" y="552" text-anchor="end" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="13" fill="#94a3b8">智慧校园 · 作业归档</text>
</svg>"##,
        title = title,
        subtitle = subtitle
    );
    demo_attachment("作业详情.svg", &svg)
}

fn demo_leave_svg(title: &str, subtitle: &str) -> Value {
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="880" height="620" viewBox="0 0 880 620">
<defs>
  <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
    <stop offset="0" stop-color="#fef7ed"/><stop offset="1" stop-color="#fff1f2"/>
  </linearGradient>
</defs>
<rect width="880" height="620" rx="20" fill="url(#bg)"/>
<rect x="70" y="48" width="740" height="524" rx="16" fill="#ffffff" stroke="#fde2e4"/>
<text x="440" y="116" text-anchor="middle" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="30" font-weight="700" fill="#9f1239">{title}</text>
<text x="440" y="146" text-anchor="middle" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="14" fill="#a16207">SMART CAMPUS · LEAVE REQUEST</text>
<line x1="120" y1="174" x2="760" y2="174" stroke="#fecdd3"/>
<text x="120" y="224" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="17" fill="#3f3f46">申请人：{subtitle}</text>
<text x="120" y="268" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="17" fill="#3f3f46">活动名称：城市研学 · 11 月 5 日</text>
<text x="120" y="312" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="17" fill="#3f3f46">请假原因：因家庭行程安排，无法参加本次研学活动。</text>
<text x="120" y="356" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="17" fill="#3f3f46">补课安排：返校后领取研学手册，并补齐课程笔记。</text>
<text x="120" y="406" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="14" fill="#71717a">本人已了解活动安排，并对请假期间的安全与学习负责。</text>
<rect x="120" y="440" width="240" height="58" rx="12" fill="#fff7ed" stroke="#fdba74"/>
<text x="240" y="466" text-anchor="middle" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="13" fill="#c2410c">家长签字</text>
<text x="240" y="488" text-anchor="middle" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="13" fill="#9a3412">陈女士 · 已确认</text>
<circle cx="672" cy="452" r="52" fill="none" stroke="#fb7185" stroke-width="3" opacity=".7"/>
<text x="672" y="459" text-anchor="middle" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="16" fill="#e11d48">已提交</text>
<text x="760" y="540" text-anchor="end" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="13" fill="#a1a1aa">高一（3）班 · 智慧校园</text>
</svg>"##,
        title = title,
        subtitle = subtitle
    );
    demo_attachment("家长签字请假单.svg", &svg)
}

fn demo_photo_svg(title: &str, subtitle: &str) -> Value {
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="960" height="640" viewBox="0 0 960 640">
<defs>
  <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#7dd3fc"/><stop offset=".55" stop-color="#fde68a"/>
    <stop offset="1" stop-color="#fbbf24"/>
  </linearGradient>
  <linearGradient id="ground" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#92400e"/><stop offset="1" stop-color="#451a03"/>
  </linearGradient>
  <radialGradient id="sun" cx="50%" cy="50%" r="50%">
    <stop offset="0" stop-color="#fffbeb"/><stop offset="1" stop-color="#fbbf24"/>
  </radialGradient>
</defs>
<rect width="960" height="640" fill="url(#sky)"/>
<circle cx="740" cy="150" r="68" fill="url(#sun)"/>
<path d="M0 430 Q180 350 360 420 T720 400 T960 430 V640 H0 Z" fill="url(#ground)"/>
<g fill="#78350f" opacity=".9">
  <rect x="120" y="220" width="20" height="250" rx="8"/>
  <circle cx="130" cy="210" r="90" fill="#f59e0b"/>
  <circle cx="86" cy="250" r="70" fill="#fbbf24"/>
  <circle cx="178" cy="258" r="62" fill="#d97706"/>
</g>
<g fill="#a16207" opacity=".85">
  <rect x="420" y="260" width="16" height="220" rx="7"/>
  <circle cx="428" cy="250" r="76" fill="#fbbf24"/>
  <circle cx="386" cy="288" r="56" fill="#f59e0b"/>
  <circle cx="470" cy="286" r="52" fill="#eab308"/>
</g>
<g fill="#fcd34d" opacity=".9">
  <rect x="700" y="300" width="14" height="180" rx="6"/>
  <circle cx="707" cy="292" r="64" fill="#f59e0b"/>
  <circle cx="670" cy="322" r="46" fill="#fbbf24"/>
</g>
<g fill="#fde68a" opacity=".8">
  <circle cx="250" cy="480" r="7"/><circle cx="300" cy="520" r="5"/><circle cx="560" cy="500" r="6"/>
  <circle cx="620" cy="540" r="4"/><circle cx="840" cy="480" r="6"/><circle cx="90" cy="540" r="5"/>
</g>
<rect x="48" y="48" width="864" height="544" rx="20" fill="none" stroke="#fffbeb" stroke-width="2" opacity=".55"/>
<text x="90" y="104" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="34" font-weight="700" fill="#fffbeb">{title}</text>
<text x="90" y="138" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="16" fill="#fffbeb" opacity=".9">{subtitle}</text>
<text x="880" y="560" text-anchor="end" font-family="PingFang SC, Microsoft YaHei, sans-serif" font-size="15" fill="#fffbeb" opacity=".85">AUTUMN · CAMPUS</text>
</svg>"##,
        title = title,
        subtitle = subtitle
    );
    demo_attachment("秋日校园.svg", &svg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{hash_password, parse_time};

    fn temp_db() -> String {
        let path = format!("/tmp/smart-campus-test-{}.db", rand_string(8));
        path
    }

    #[test]
    fn seed_and_auth_lookup() {
        let path = temp_db();
        {
            let conn = open(&path).unwrap();
            let a = load_auth_by_key(&conn, DEMO_STUDENT_KEY).unwrap();
            assert_eq!(a.role, "student");
            assert_eq!(a.student_id.as_deref(), Some("G2603-01"));
            assert_eq!(a.class_name, "高一（3）班");
            let t = load_auth_by_key(&conn, DEMO_TEACHER_KEY).unwrap();
            assert!(t.is_staff());
            let adm = load_auth_by_key(&conn, DEMO_ADMIN_KEY).unwrap();
            assert!(adm.is_admin());
            let u = load_user_with_secret(&conn, "student").unwrap();
            assert_eq!(u.user.username, "student");
            let salt = u.salt.clone();
            assert_eq!(hash_password(&salt, "student123"), u.pwhash);
            assert_eq!(
                load_user_with_secret(&conn, "STUDENT").unwrap().user.id,
                u.user.id
            );
            assert!(load_auth_by_id(&conn, &a.id).is_some());
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn create_class_and_members() {
        let path = temp_db();
        {
            let conn = open(&path).unwrap();
            let admin = load_auth_by_key(&conn, DEMO_ADMIN_KEY).unwrap();
            let c = create_class(&conn, &admin, "测试班", "单元测试班级").unwrap();
            assert_eq!(c.code.len(), 6);
            let members = create_members(&conn, &admin, &admin.class_id, "Student", 2).unwrap();
            assert_eq!(members.len(), 2);
            let keys: Vec<&str> = members.iter().map(|m| m.key.as_str()).collect();
            assert!(keys.iter().all(|k| k.starts_with("SC-STU-")));
            let c2 = load_auth_by_key(&conn, &c.key).unwrap();
            assert_eq!(c2.class_id, c.id);
            assert!(c2.is_admin());
            // 不允许跨班新增
            assert!(create_members(&conn, &admin, &c.id, "Student", 1).is_err());
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn time_helpers_smoke() {
        let t = parse_time("2026-11-01T20:00").unwrap();
        assert_eq!(crate::tools::fmt_datetime(t), "2026-11-01 20:00");
    }
}
