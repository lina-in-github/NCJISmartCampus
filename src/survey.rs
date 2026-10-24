//! 问卷模式：把征集简介中的控件标签渲染成可填写的表单。
//!
//! 征集类型为 `survey` 时，简介（`desc`）里允许出现三种控件标签，`id` 必须以
//! `form_` 开头，`required` 可选：
//!
//! ```text
//! 姓名：<input id="form_name" required/>
//! 想参加的社团：<checkbox id="form_club" value="math"/>数学社 <checkbox id="form_club" value="art"/>美术社
//! 是否接收通知：<switch id="form_notify"/>
//! ```
//!
//! * `input` 渲染为文本输入框（可选 `type` / `placeholder` / `value`）；
//! * `checkbox` 渲染为复选框，同 `id` 出现多次时视为多选组；
//! * `switch` 渲染为开关。
//!
//! 除这几个标签外的一切文本都会转义输出，标签本身由本模块重新拼装属性，
//! 因此简介作者无法注入任意 HTML。
//!
//! 提交格式（前端「bot 提交」与直接调用 API 一致）：
//!
//! ```json
//! { "which": "sub_xxx", "title": "bot-upload", "desc": "{\"form_name\":\"陈小满\",\"form_notify\":\"on\"}" }
//! ```
//!
//! `desc` 是 `{控件 id: 值}` 的 JSON 字符串；`switch` 的值为 `on` / `off`，
//! `checkbox` 未勾选时为空字符串（多选组以 `,` 连接）。

use std::collections::HashMap;

use serde_json::Value;

use crate::tools::{html_escape, truncate_chars};

/// 控件 id 的固定前缀。
pub const FIELD_PREFIX: &str = "form_";

/// 简介里可用的控件种类。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Input,
    Checkbox,
    Switch,
}

impl FieldKind {
    fn from_tag(name: &str) -> Option<Self> {
        match name {
            "input" => Some(Self::Input),
            "checkbox" => Some(Self::Checkbox),
            "switch" => Some(Self::Switch),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Checkbox => "checkbox",
            Self::Switch => "switch",
        }
    }
}

/// 一个解析好的问卷控件。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub kind: FieldKind,
    /// 简介里书写的 id，同时也是提交 JSON 的键名。
    pub id: String,
    pub required: bool,
    /// `input` 的 `type`（已白名单化）。
    pub input_type: String,
    pub placeholder: String,
    /// `input` 的默认值 / `checkbox` 的选项值。
    pub value: String,
    /// 取自简介文本的题目标签。
    pub label: String,
    /// 多选组（同一 id 的多个 checkbox）的组标题，取自简介中组前的文本。
    pub group_label: String,
    /// 同一 id 出现的次数（大于 1 表示多选组）。
    pub group_size: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Node {
    Text(String),
    Field(usize),
}

/// 解析结果：既能渲染 HTML，也能回答「哪些字段必填」。
#[derive(Clone, Debug, Default)]
pub struct Survey {
    nodes: Vec<Node>,
    fields: Vec<Field>,
}

/// `input` 允许的 type 白名单。
const INPUT_TYPES: [&str; 7] = ["text", "number", "email", "tel", "url", "date", "time"];

impl Survey {
    /// 解析简介文本。
    pub fn parse(desc: &str) -> Self {
        let mut nodes: Vec<Node> = Vec::new();
        let mut fields: Vec<Field> = Vec::new();
        let mut buffer = String::new();
        let mut rest = desc;

        while let Some(lt) = rest.find('<') {
            let Some(gt_rel) = rest[lt..].find('>') else {
                break;
            };
            let gt = lt + gt_rel;
            let inner = &rest[lt + 1..gt];
            if is_closing_tag(inner) {
                // `</input>` 这类闭合标签直接吞掉，保持简介可读。
                buffer.push_str(&rest[..lt]);
                rest = &rest[gt + 1..];
                continue;
            }
            match parse_tag(inner) {
                Some(field) => {
                    buffer.push_str(&rest[..lt]);
                    if !buffer.is_empty() {
                        nodes.push(Node::Text(std::mem::take(&mut buffer)));
                    }
                    fields.push(field);
                    nodes.push(Node::Field(fields.len() - 1));
                }
                None => {
                    // 不是合法控件：原样作为文本输出（渲染时会转义）。
                    buffer.push_str(&rest[..=gt]);
                }
            }
            rest = &rest[gt + 1..];
        }
        buffer.push_str(rest);
        if !buffer.is_empty() {
            nodes.push(Node::Text(buffer));
        }

        assign_labels(&mut nodes, &mut fields);

        let mut counts: HashMap<String, usize> = HashMap::new();
        for field in &fields {
            *counts.entry(field.id.clone()).or_insert(0) += 1;
        }
        for field in &mut fields {
            field.group_size = counts.get(&field.id).copied().unwrap_or(1);
        }

        Self { nodes, fields }
    }

    #[cfg(test)]
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// 必填控件 id（去重、保持简介中的顺序）。
    pub fn required_ids(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for field in &self.fields {
            if field.required && !seen.contains(&field.id) {
                seen.push(field.id.clone());
            }
        }
        seen
    }

    /// 控件 id -> 题目标签，用于把提交的 JSON 渲染成可读的回答。
    /// 多选组优先使用组标题，其次是选项文字。
    pub fn labels(&self) -> HashMap<String, String> {
        let mut out: HashMap<String, String> = HashMap::new();
        for field in &self.fields {
            let label = if field.group_size > 1 {
                field.group_label.trim()
            } else {
                ""
            };
            let label = if label.is_empty() {
                field.label.trim()
            } else {
                label
            };
            if !label.is_empty() {
                out.entry(field.id.clone())
                    .or_insert_with(|| label.to_string());
            }
        }
        out
    }

    /// 渲染为可填写的 HTML 表单体（不含 `<form>` 外壳与提交按钮）。
    pub fn to_html(&self) -> String {
        let mut out = String::with_capacity(512);
        out.push_str(r#"<div class="survey-body">"#);
        let mut occurrences: HashMap<&str, usize> = HashMap::new();
        for node in &self.nodes {
            match node {
                Node::Text(text) => push_text(&mut out, text),
                Node::Field(idx) => {
                    let field = &self.fields[*idx];
                    let seen = occurrences.entry(field.id.as_str()).or_insert(0);
                    *seen += 1;
                    push_field(&mut out, field, *seen);
                }
            }
        }
        out.push_str("</div>");
        out
    }
}

/// 不合法/非法控件 id：必须以 `form_` 开头，只允许字母数字下划线与短横线。
pub fn valid_field_id(id: &str) -> bool {
    let Some(rest) = id.strip_prefix(FIELD_PREFIX) else {
        return false;
    };
    !rest.is_empty()
        && id.len() <= 64
        && rest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 校验一次问卷提交：`answer` 为 `/api/upload` 的 `desc`。
///
/// 仅当简介里存在必填控件、且 `answer` 本身是 JSON 对象时才校验（保证
/// 「仍允许 API 提交」的约定：非 JSON 的说明文本不会被拒绝）。
pub fn validate_answer(form_desc: &str, answer: &str) -> Option<String> {
    let required = Survey::parse(form_desc).required_ids();
    if required.is_empty() {
        return None;
    }
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(answer.trim()) else {
        return None;
    };
    let missing: Vec<String> = required
        .into_iter()
        .filter(|id| match map.get(id) {
            Some(value) => value_is_blank(value),
            None => true,
        })
        .collect();
    if missing.is_empty() {
        None
    } else {
        Some(format!(
            "missing required survey fields: {}",
            missing.join(", ")
        ))
    }
}

fn value_is_blank(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Bool(b) => !*b,
        Value::Number(_) | Value::Object(_) => false,
    }
}

/// 把问卷回答（JSON 对象）拆成 `(id, 值)` 列表；不是 JSON 对象时返回 `None`。
fn answer_pairs(answer: &str) -> Option<Vec<(String, String)>> {
    let Value::Object(map) = serde_json::from_str::<Value>(answer.trim()).ok()? else {
        return None;
    };
    if map.is_empty() {
        return None;
    }
    Some(
        map.into_iter()
            .map(|(k, v)| {
                let text = match v {
                    Value::String(s) => s,
                    Value::Null => String::new(),
                    other => other.to_string(),
                };
                (k, text)
            })
            .collect(),
    )
}

/// 把问卷回答转换成可读的 `(题目, 展示值)` 列表。
///
/// * 顺序按简介里的控件顺序，简介未声明的键追加在末尾；
/// * 多选组把选项值换成选项文字，用「、」连接。
pub fn answer_rows(form_desc: &str, answer: &str) -> Option<Vec<(String, String)>> {
    let survey = Survey::parse(form_desc);
    let pairs = answer_pairs(answer)?;
    let labels = survey.labels();
    let mut taken: Vec<bool> = vec![false; pairs.len()];
    let mut rows: Vec<(String, String)> = Vec::new();

    for field in &survey.fields {
        if let Some(pos) = pairs
            .iter()
            .position(|(key, _)| key == &field.id)
            .filter(|pos| !taken[*pos])
        {
            taken[pos] = true;
            rows.push((
                labels
                    .get(&field.id)
                    .cloned()
                    .unwrap_or_else(|| field.id.clone()),
                display_value(&survey, field, &pairs[pos].1),
            ));
        }
    }
    for (i, (key, value)) in pairs.iter().enumerate() {
        if !taken[i] {
            rows.push((key.clone(), value.clone()));
        }
    }
    Some(rows)
}

fn display_value(survey: &Survey, field: &Field, value: &str) -> String {
    if field.group_size <= 1 || value.trim().is_empty() {
        return value.to_string();
    }
    value
        .split(',')
        .map(|token| {
            let token = token.trim();
            survey
                .fields
                .iter()
                .filter(|f| f.id == field.id && f.value.trim() == token)
                .map(|f| f.label.trim())
                .find(|label| !label.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| token.to_string())
        })
        .collect::<Vec<String>>()
        .join("、")
}

// ---------------------------------------------------------------------------
// 解析细节
// ---------------------------------------------------------------------------

fn is_closing_tag(inner: &str) -> bool {
    let Some(rest) = inner.trim().strip_prefix('/') else {
        return false;
    };
    let name = rest
        .trim()
        .trim_end_matches('/')
        .trim()
        .to_ascii_lowercase();
    FieldKind::from_tag(&name).is_some()
}

fn parse_tag(inner: &str) -> Option<Field> {
    let trimmed = inner.trim();
    let name_end = trimmed
        .find(|c: char| c.is_whitespace() || c == '/')
        .unwrap_or(trimmed.len());
    let kind = FieldKind::from_tag(&trimmed[..name_end].to_ascii_lowercase())?;
    let (attrs, flags) = parse_attrs(&trimmed[name_end..])?;

    let id = attrs.iter().find(|(k, _)| k == "id")?.1.trim().to_string();
    if !valid_field_id(&id) {
        return None;
    }
    let required = attrs.iter().any(|(k, v)| {
        k == "required"
            && !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "no" | "off"
            )
    }) || flags.iter().any(|f| f == "required");
    let value = attrs
        .iter()
        .find(|(k, _)| k == "value")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let placeholder = attrs
        .iter()
        .find(|(k, _)| k == "placeholder")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    let input_type = attrs
        .iter()
        .find(|(k, _)| k == "type")
        .map(|(_, v)| v.trim().to_ascii_lowercase())
        .filter(|v| INPUT_TYPES.contains(&v.as_str()))
        .unwrap_or_else(|| "text".to_string());

    Some(Field {
        kind,
        id,
        required,
        input_type,
        placeholder,
        value,
        label: String::new(),
        group_label: String::new(),
        group_size: 1,
    })
}

type Attrs = (Vec<(String, String)>, Vec<String>);

fn parse_attrs(mut s: &str) -> Option<Attrs> {
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut flags: Vec<String> = Vec::new();
    loop {
        s = s.trim_start();
        if s.is_empty() {
            return Some((attrs, flags));
        }
        if let Some(rest) = s.strip_prefix('/') {
            // 自闭合结尾
            s = rest;
            continue;
        }
        let key_end = s
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/')
            .unwrap_or(s.len());
        if key_end == 0 {
            return None;
        }
        let key = s[..key_end].to_ascii_lowercase();
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':'))
        {
            return None;
        }
        s = &s[key_end..];
        let after_ws = s.trim_start();
        match after_ws.strip_prefix('=') {
            Some(rest) => {
                let (value, next) = read_value(rest.trim_start())?;
                attrs.push((key, value));
                s = next;
            }
            None => {
                flags.push(key);
                s = after_ws;
            }
        }
    }
}

fn read_value(s: &str) -> Option<(String, &str)> {
    let quote = s.chars().next()?;
    if quote == '"' || quote == '\'' {
        let rest = &s[quote.len_utf8()..];
        let end = rest.find(quote)?;
        Some((rest[..end].to_string(), &rest[end + quote.len_utf8()..]))
    } else {
        let end = s
            .find(|c: char| c.is_whitespace() || c == '/')
            .unwrap_or(s.len());
        Some((s[..end].to_string(), &s[end..]))
    }
}

/// 给控件分配题目标签：
/// * `input` 用同一行里它前面的文本当标签（「姓名：<input …/>」→「姓名」）；
/// * `checkbox` / `switch` 优先用它后面的文本（「<checkbox …/>数学社」→「数学社」），
///   否则同样回退到前面的文本。
fn assign_labels(nodes: &mut [Node], fields: &mut [Field]) {
    for i in 0..nodes.len() {
        let Node::Field(idx) = nodes[i] else {
            continue;
        };
        let kind = fields[idx].kind;
        if kind == FieldKind::Checkbox {
            // 多选组的组标题：同组第一个 checkbox 前面的文本（保留在原位显示）。
            if let Some(group) = peek_line_suffix(nodes, i) {
                fields[idx].group_label = group;
            }
        }
        if matches!(kind, FieldKind::Checkbox | FieldKind::Switch) {
            if let Some(label) = take_line_prefix(nodes, i + 1) {
                fields[idx].label = label;
                continue;
            }
        }
        if let Some(label) = take_line_suffix(nodes, i) {
            fields[idx].label = label;
        }
    }
}

/// 取 `nodes[j]` 文本中同一行开头的一段（作为控件后置标签），并从文本里移除。
fn take_line_prefix(nodes: &mut [Node], j: usize) -> Option<String> {
    let Node::Text(text) = nodes.get_mut(j)? else {
        return None;
    };
    if text.starts_with('\n') {
        return None;
    }
    let cut = text.find('\n').unwrap_or(text.len());
    let head = text[..cut].trim();
    if head.is_empty() {
        return None;
    }
    let label = clean_label(head);
    *text = text[cut..].to_string();
    Some(label)
}

/// 取 `nodes[j - 1]` 文本中同一行结尾的一段（作为控件前置标签），并从文本里移除。
fn take_line_suffix(nodes: &mut [Node], j: usize) -> Option<String> {
    if j == 0 {
        return None;
    }
    let Node::Text(text) = &mut nodes[j - 1] else {
        return None;
    };
    let start = text.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let tail = text[start..].trim();
    let label = clean_label(tail.trim_end_matches([':', '：', '=', '＝']));
    if label.is_empty() {
        return None;
    }
    *text = text[..start].to_string();
    Some(label)
}

/// 只读取 `nodes[j - 1]` 同一行结尾的文本，不做消费。
fn peek_line_suffix(nodes: &[Node], j: usize) -> Option<String> {
    if j == 0 {
        return None;
    }
    let Node::Text(text) = &nodes[j - 1] else {
        return None;
    };
    let start = text.rfind('\n').map(|p| p + 1).unwrap_or(0);
    let label = clean_label(text[start..].trim_end_matches([':', '：', '=', '＝']));
    if label.is_empty() {
        None
    } else {
        Some(label)
    }
}

// ---------------------------------------------------------------------------
// 渲染
// ---------------------------------------------------------------------------

fn esc(s: &str) -> String {
    html_escape(s)
}

/// 题目标签保持简短：去掉首尾空白并截断过长内容。
fn clean_label(s: &str) -> String {
    truncate_chars(s.trim(), 80)
}

fn push_text(out: &mut String, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    for para in text.split("\n\n") {
        if para.trim().is_empty() {
            continue;
        }
        out.push_str(r#"<p class="survey-text">"#);
        for (i, line) in para.split('\n').enumerate() {
            if i > 0 {
                out.push_str("<br>");
            }
            out.push_str(&esc(line));
        }
        out.push_str("</p>");
    }
}

fn push_field(out: &mut String, field: &Field, occurrence: usize) {
    let key = esc(&field.id);
    let dom_id = if occurrence == 1 {
        key.clone()
    } else {
        format!("{key}__{occurrence}")
    };
    let label = field.label.trim();
    let label_html = if label.is_empty() {
        String::new()
    } else {
        esc(label)
    };
    let req_mark = if field.required {
        r#"<span class="req">*</span>"#
    } else {
        ""
    };
    // 多选组只在第一个选项上标必填，避免每组选项都带星号。
    let req_mark = if occurrence == 1 { req_mark } else { "" };
    let required_attr = if field.required {
        if field.group_size > 1 {
            r#" data-survey-required="1""#
        } else {
            r#" required"#
        }
    } else {
        ""
    };
    let value_attr = if field.value.is_empty() {
        String::new()
    } else {
        format!(r#" value="{}""#, esc(&field.value))
    };
    let kind = field.kind.as_str();

    match field.kind {
        FieldKind::Input => {
            let mut extra = String::new();
            if !field.placeholder.trim().is_empty() {
                extra.push_str(&format!(
                    r#" placeholder="{}""#,
                    esc(field.placeholder.trim())
                ));
            }
            if !field.value.is_empty() {
                extra.push_str(&format!(r#" value="{}""#, esc(&field.value)));
            }
            if field.required {
                extra.push_str(" required");
            }
            let label_block = if label_html.is_empty() {
                String::new()
            } else {
                format!(r#"<label class="label" for="{dom_id}">{label_html}{req_mark}</label>"#)
            };
            out.push_str(&format!(
                concat!(
                    r#"<div class="survey-field">"#,
                    r#"{label_block}"#,
                    r#"<input class="input" id="{dom_id}" name="{key}" type="{itype}" "#,
                    r#"data-survey-id="{key}" data-survey-kind="{kind}"{extra}>"#,
                    r#"</div>"#
                ),
                label_block = label_block,
                dom_id = dom_id,
                key = key,
                itype = esc(&field.input_type),
                kind = kind,
                extra = extra
            ));
        }
        FieldKind::Checkbox => {
            let caption = if !label_html.is_empty() {
                label_html
            } else if !field.value.is_empty() {
                esc(field.value.trim())
            } else {
                key.clone()
            };
            out.push_str(&format!(
                concat!(
                    r#"<label class="survey-choice">"#,
                    r#"<input class="survey-check" type="checkbox" id="{dom_id}" name="{key}" "#,
                    r#"data-survey-id="{key}" data-survey-kind="{kind}"{value_attr}{required_attr}>"#,
                    r#"<span class="survey-choice-text">{caption}{req_mark}</span>"#,
                    r#"</label>"#
                ),
                dom_id = dom_id,
                key = key,
                kind = kind,
                value_attr = value_attr,
                required_attr = required_attr,
                caption = caption,
                req_mark = req_mark
            ));
        }
        FieldKind::Switch => {
            let caption = if !label_html.is_empty() {
                label_html
            } else {
                key.clone()
            };
            out.push_str(&format!(
                concat!(
                    r#"<label class="survey-choice">"#,
                    r#"<span class="switch"><input type="checkbox" role="switch" id="{dom_id}" name="{key}" "#,
                    r#"data-survey-id="{key}" data-survey-kind="{kind}"{required_attr}>"#,
                    r#"<span class="switch-track"></span></span>"#,
                    r#"<span class="survey-choice-text">{caption}{req_mark}</span>"#,
                    r#"</label>"#
                ),
                dom_id = dom_id,
                key = key,
                kind = kind,
                required_attr = required_attr,
                caption = caption,
                req_mark = req_mark
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_three_kinds_and_labels() {
        let desc = "姓名：<input id=\"form_name\" required/>\n\
                    想参加的社团：<checkbox id=\"form_club\" value=\"math\"/>数学社 \
                    <checkbox id=\"form_club\" value=\"art\"/>美术社\n\
                    接收通知：<switch id=\"form_notify\"/>";
        let survey = Survey::parse(desc);
        assert_eq!(survey.fields().len(), 4);
        let name = &survey.fields()[0];
        assert_eq!(name.kind, FieldKind::Input);
        assert_eq!(name.label, "姓名");
        assert!(name.required);
        let club = &survey.fields()[1];
        assert_eq!(club.kind, FieldKind::Checkbox);
        assert_eq!(club.label, "数学社");
        assert_eq!(club.value, "math");
        assert_eq!(club.group_size, 2);
        let notify = &survey.fields()[3];
        assert_eq!(notify.kind, FieldKind::Switch);
        assert_eq!(notify.label, "接收通知");
        assert_eq!(survey.required_ids(), vec!["form_name".to_string()]);
    }

    #[test]
    fn rejects_invalid_ids_and_escapes_text() {
        let desc = "<input id=\"name\"/> <input id=\"form_a b\"/> <script>x</script> <checkbox id=\"form_ok\"/>";
        let survey = Survey::parse(desc);
        assert_eq!(survey.fields().len(), 1);
        let html = survey.to_html();
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains(r#"id="form_ok""#));
    }

    #[test]
    fn renders_controls_with_attributes() {
        let desc = "备注：<input id=\"form_note\" placeholder=\"写点什么\" type=\"number\"/>";
        let html = Survey::parse(desc).to_html();
        assert!(
            html.contains(r#"<input class="input" id="form_note" name="form_note" type="number""#)
        );
        assert!(html.contains(r#"placeholder="写点什么""#));
        assert!(html.contains("<label class=\"label\" for=\"form_note\">备注</label>"));
    }

    #[test]
    fn duplicate_ids_get_unique_dom_ids() {
        let desc = "<checkbox id=\"form_x\" value=\"a\"/>A <checkbox id=\"form_x\" value=\"b\"/>B";
        let html = Survey::parse(desc).to_html();
        assert!(html.contains(r#"id="form_x""#));
        assert!(html.contains(r#"id="form_x__2""#));
        assert_eq!(html.matches(r#"data-survey-id="form_x""#).count(), 2);
    }

    #[test]
    fn closing_tags_are_swallowed_and_bad_types_fall_back() {
        let survey = Survey::parse("<input id=\"form_a\" type=\"password\"></input>");
        assert_eq!(survey.fields().len(), 1);
        assert_eq!(survey.fields()[0].input_type, "text");
        assert!(!survey.to_html().contains("</input>"));
    }

    #[test]
    fn validates_required_answers() {
        let form = "姓名：<input id=\"form_name\" required/> 备注：<input id=\"form_note\"/>";
        assert!(validate_answer(form, "{\"form_name\":\"\"}").is_some());
        assert!(validate_answer(form, "{\"form_name\":\"小明\"}").is_none());
        // 非 JSON 说明文本不被拦截（仍允许 API 提交）
        assert!(validate_answer(form, "随便写点说明").is_none());
        // 没有必填项时永远通过
        assert!(validate_answer("备注：<input id=\"form_note\"/>", "{}").is_none());
    }

    #[test]
    fn answer_rows_use_labels_and_option_text() {
        let form = "姓名：<input id=\"form_name\"/>\n\
                    想参加的社团：<checkbox id=\"form_club\" value=\"math\"/>数学社 \
                    <checkbox id=\"form_club\" value=\"art\"/>美术社";
        let rows =
            answer_rows(form, "{\"form_club\":\"math,art\",\"form_name\":\"小明\"}").unwrap();
        assert_eq!(rows[0], ("姓名".to_string(), "小明".to_string()));
        assert_eq!(rows[1].0, "想参加的社团");
        assert_eq!(rows[1].1, "数学社、美术社");
    }

    #[test]
    fn answer_pairs_reads_json_object() {
        let pairs = answer_pairs("{\"form_name\":\"小明\",\"form_notify\":\"on\"}").unwrap();
        assert_eq!(pairs.len(), 2);
        assert!(pairs.iter().any(|(k, v)| k == "form_name" && v == "小明"));
        assert!(answer_pairs("plain text").is_none());
        assert!(answer_pairs("[1,2]").is_none());
    }
}
