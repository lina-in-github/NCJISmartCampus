//! UI 字符串：启动时读取 `strings/*.txt`，同步到 `strings.db`，并提供
//! `@[strings/key]` 标签替换与 `{}` 格式化能力。

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use rusqlite::{params, Connection};

#[derive(Clone, Debug)]
struct Entry {
    name: String,
    content: String,
}

#[derive(Debug)]
struct Store {
    languages: HashMap<String, BTreeMap<String, String>>,
    current: String,
}

static STORE: OnceLock<Store> = OnceLock::new();

/// 启动时调用。返回非致命警告（例如 strings.db 无法同步）。
pub fn init(dir: &str, db_path: &str, lang: &str) -> Result<Option<String>, String> {
    if STORE.get().is_some() {
        return Ok(None);
    }
    let mut files: Vec<_> = fs::read_dir(dir)
        .map_err(|e| format!("无法读取字符串目录 {dir}: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("txt"))
        .collect();
    files.sort();

    if files.is_empty() {
        return Err(format!("字符串目录 {dir} 中没有任何 .txt 文件"));
    }

    let mut languages: HashMap<String, BTreeMap<String, String>> = HashMap::new();
    for path in &files {
        let code = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("无法识别字符串文件名：{}", path.display()))?;
        let text = fs::read_to_string(path)
            .map_err(|e| format!("读取字符串文件 {} 失败：{e}", path.display()))?;
        let entries = parse_text(&text, &path.display().to_string())?;
        let mut map = BTreeMap::new();
        for e in entries {
            if let Some(old) = map.insert(e.name.clone(), e.content) {
                let _ = old;
                return Err(format!(
                    "字符串文件 {} 中存在重复 key：{}",
                    path.display(),
                    e.name
                ));
            }
        }
        languages.insert(code.to_string(), map);
    }

    let current = if languages.contains_key(lang) {
        lang.to_string()
    } else if languages.contains_key("zh-CN") {
        "zh-CN".to_string()
    } else {
        languages
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| "没有可用语言".to_string())?
    };

    let warning = match sync_db(db_path, &languages) {
        Ok(()) => None,
        Err(e) => Some(format!(
            "同步 strings.db 失败（继续使用内存中的字符串）：{e}"
        )),
    };

    let store = Store { languages, current };
    if STORE.set(store).is_err() {
        return Ok(None);
    }
    Ok(warning)
}

/// 读取一个字符串。缺失时返回 key 本身，便于发现漏配。
pub fn t(key: &str) -> String {
    lookup(key).unwrap_or_else(|| key.to_string())
}

fn lookup(key: &str) -> Option<String> {
    let store = STORE.get()?;
    if let Some(v) = store.languages.get(&store.current).and_then(|m| m.get(key)) {
        return Some(v.clone());
    }
    if let Some(v) = store.languages.get("zh-CN").and_then(|m| m.get(key)) {
        return Some(v.clone());
    }
    store.languages.values().find_map(|m| m.get(key).cloned())
}

/// 以 `{}`（按顺序）替换字符串中的占位符。支持 `{{` / `}}` 转义。
pub fn fmt(key: &str, args: &[String]) -> String {
    format_template(&t(key), args)
}

pub fn f1(key: &str, a: impl ToString) -> String {
    fmt(key, &[a.to_string()])
}

pub fn f2(key: &str, a: impl ToString, b: impl ToString) -> String {
    fmt(key, &[a.to_string(), b.to_string()])
}

fn format_template(template: &str, args: &[String]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut chars = template.chars().peekable();
    let mut next = 0usize;
    while let Some(c) = chars.next() {
        if c == '{' {
            match chars.peek().copied() {
                Some('{') => {
                    chars.next();
                    out.push('{');
                }
                Some('}') => {
                    chars.next();
                    if let Some(arg) = args.get(next) {
                        out.push_str(arg);
                        next += 1;
                    } else {
                        out.push_str("{}");
                    }
                }
                _ => out.push('{'),
            }
        } else if c == '}' && chars.peek() == Some(&'}') {
            chars.next();
            out.push('}');
        } else {
            out.push(c);
        }
    }
    out
}

/// 替换 HTML/JS 文本中的 `@[strings/key]` 标签。缺失的 key 保留原标签。
pub fn apply(text: &str) -> String {
    if STORE.get().is_none() || !text.contains("@[strings/") {
        return text.to_string();
    }
    static TAG: OnceLock<Regex> = OnceLock::new();
    let re = TAG.get_or_init(|| {
        Regex::new(r"@\[strings/([A-Za-z0-9_.-]+)\]").expect("内置字符串标签正则错误")
    });
    re.replace_all(text, |caps: &regex::Captures<'_>| {
        let key = &caps[1];
        lookup(key).unwrap_or_else(|| caps[0].to_string())
    })
    .into_owned()
}

/// 供 `/static/strings.js` 使用的 JS 对象。
pub fn js_object() -> String {
    let mut map = BTreeMap::new();
    if let Some(store) = STORE.get() {
        // 以 zh-CN 为兜底，再覆盖当前语言。
        if let Some(base) = store.languages.get("zh-CN") {
            map.extend(base.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        if store.current != "zh-CN" {
            if let Some(current) = store.languages.get(&store.current) {
                map.extend(current.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
        }
    }
    let json = serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string());
    let json = json
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    format!("window.STR = {json};")
}

pub fn current_lang() -> String {
    STORE
        .get()
        .map(|s| s.current.clone())
        .unwrap_or_else(|| "zh-CN".to_string())
}

pub fn stats() -> (usize, String) {
    let Some(store) = STORE.get() else {
        return (0, "uninitialized".to_string());
    };
    let n = store
        .languages
        .get(&store.current)
        .map(|m| m.len())
        .unwrap_or(0);
    (n, store.current.clone())
}

fn parse_text(text: &str, source: &str) -> Result<Vec<Entry>, String> {
    let raw: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut entries = Vec::new();
    let mut i = 0usize;
    while i < raw.len() {
        let line = raw[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if !trimmed.starts_with("name=") {
            return Err(format!("{}:{}：条目必须以 name= 开头", source, i + 1));
        }
        let line = trimmed;
        let lines_at = line
            .find(" lines=")
            .ok_or_else(|| format!("{}:{}：缺少 lines=", source, i + 1))?;
        let content_at = line
            .find(" content=")
            .ok_or_else(|| format!("{}:{}：缺少 content=", source, i + 1))?;
        if lines_at >= content_at {
            return Err(format!(
                "{}:{}：lines= 必须位于 content= 之前",
                source,
                i + 1
            ));
        }
        let name = line[5..lines_at].trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            return Err(format!("{}:{}：非法 key `{name}`", source, i + 1));
        }
        let n: usize = line[lines_at + 7..content_at]
            .trim()
            .parse()
            .map_err(|_| format!("{}:{}：lines= 必须是数字", source, i + 1))?;
        let inline = &line[content_at + 9..];
        i += 1;

        let mut content_lines: Vec<String> = Vec::new();
        if !inline.is_empty() {
            content_lines.push(inline.to_string());
        }
        let need = n.saturating_sub(content_lines.len());
        if i + need > raw.len() {
            return Err(format!("{}:{}：声明了 {} 行内容，但文件不足", source, i, n));
        }
        for _ in 0..need {
            content_lines.push(raw[i].to_string());
            i += 1;
        }
        entries.push(Entry {
            name: name.to_string(),
            content: content_lines.join("\n"),
        });
    }
    Ok(entries)
}

fn sync_db(
    db_path: &str,
    languages: &HashMap<String, BTreeMap<String, String>>,
) -> Result<(), String> {
    let mut conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS strings (
            language TEXT NOT NULL,
            name     TEXT NOT NULL,
            content  TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (language, name)
        );
        CREATE INDEX IF NOT EXISTS idx_strings_language ON strings(language);",
    )
    .map_err(|e| e.to_string())?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for (language, map) in languages {
        let existing: HashMap<String, String> = {
            let mut stmt = tx
                .prepare("SELECT name, content FROM strings WHERE language = ?1")
                .map_err(|e| e.to_string())?;
            let iter = stmt
                .query_map(params![language], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })
                .map_err(|e| e.to_string())?;
            iter.filter_map(Result::ok).collect()
        };
        for (name, content) in map {
            if existing.get(name) != Some(content) {
                tx.execute(
                    "INSERT INTO strings (language, name, content, updated_at)
                     VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(language, name) DO UPDATE SET
                         content = excluded.content,
                         updated_at = excluded.updated_at",
                    params![language, name, content, now],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        for name in existing.keys() {
            if !map.contains_key(name) {
                tx.execute(
                    "DELETE FROM strings WHERE language = ?1 AND name = ?2",
                    params![language, name],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

/// 仅用于测试/运维：手动应用一个外部 map 的标签替换。
#[cfg(test)]
pub fn apply_with(text: &str, map: &HashMap<String, String>) -> String {
    static TAG: OnceLock<Regex> = OnceLock::new();
    let re = TAG.get_or_init(|| {
        Regex::new(r"@\[strings/([A-Za-z0-9_.-]+)\]").expect("内置字符串标签正则错误")
    });
    re.replace_all(text, |caps: &regex::Captures<'_>| {
        map.get(&caps[1])
            .cloned()
            .unwrap_or_else(|| caps[0].to_string())
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_format_and_tags() {
        let src = "# comment\nname=hello lines=0 content=你好\nname=multi lines=2 content=\nabc\ndef\nname=blank lines=1 content=\n\nname=fmt lines=0 content=值为 {} 和 {}\n";
        let entries = parse_text(src, "test").unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].name, "hello");
        assert_eq!(entries[0].content, "你好");
        assert_eq!(entries[1].content, "abc\ndef");
        assert_eq!(entries[2].content, "");
        assert_eq!(
            format_template("值为 {} 和 {}", &["A".into(), "B".into()]),
            "值为 A 和 B"
        );
        assert_eq!(format_template("{{x}} {}", &["y".into()]), "{x} y");
        let mut map = HashMap::new();
        map.insert("hello".to_string(), "你好".to_string());
        map.insert("fmt".to_string(), "值为 {} 和 {}".to_string());
        assert_eq!(apply_with("A @[strings/hello] B", &map), "A 你好 B");
        assert_eq!(apply_with("@[strings/missing]", &map), "@[strings/missing]");
    }

    #[test]
    fn project_string_file_parses() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("strings/zh-CN.txt");
        let text = std::fs::read_to_string(path).expect("read zh-CN.txt");
        let entries = parse_text(&text, "zh-CN.txt").expect("parse zh-CN.txt");
        assert!(entries.len() > 100);
        assert!(entries
            .iter()
            .any(|e| e.name == "title" && e.content == "智慧校园"));
    }

    #[test]
    fn sync_db_upserts_and_deletes() {
        let path = std::env::temp_dir().join(format!(
            "smart-campus-strings-{}-{}.db",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let path_str = path.to_string_lossy().to_string();
        let mut languages = HashMap::new();
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), "one".to_string());
        map.insert("b".to_string(), "two".to_string());
        languages.insert("zh-CN".to_string(), map);
        sync_db(&path_str, &languages).unwrap();

        let conn = Connection::open(&path).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM strings WHERE language = 'zh-CN'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
        let a: String = conn
            .query_row(
                "SELECT content FROM strings WHERE language = 'zh-CN' AND name = 'a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(a, "one");
        drop(conn);

        let mut map = BTreeMap::new();
        map.insert("a".to_string(), "changed".to_string());
        map.insert("c".to_string(), "three".to_string());
        languages.insert("zh-CN".to_string(), map);
        sync_db(&path_str, &languages).unwrap();

        let conn = Connection::open(&path).unwrap();
        let a: String = conn
            .query_row(
                "SELECT content FROM strings WHERE language = 'zh-CN' AND name = 'a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(a, "changed");
        let b: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM strings WHERE language = 'zh-CN' AND name = 'b'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(b, 0);
        let c: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM strings WHERE language = 'zh-CN' AND name = 'c'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(c, 1);
        drop(conn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parser_rejects_bad_entries() {
        assert!(parse_text("hello=world\n", "t").is_err());
        assert!(parse_text("name=x lines=2 content=\nonly-one", "t").is_err());
        assert!(parse_text("name=bad/key lines=0 content=x\n", "t").is_err());
    }
}
