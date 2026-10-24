#!/usr/bin/env bash
# 智慧校园 · HTTP 冒烟测试
# 依赖：bash、curl、jq、sqlite3、cargo。会在临时数据库上启动独立服务，结束后自动清理。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PORT="${SC_TEST_PORT:-3999}"
BASE="http://127.0.0.1:${PORT}"
TMP="$(mktemp -d)"
BIN="$ROOT/target/debug/smart-campus"
PID=""

ADMIN="SC-ADM-DEMO-2026"
TEACHER="SC-TEA-DEMO-2026"
STUDENT="SC-STU-DEMO-2026"

cleanup() {
  local code=$?
  if [[ -n "$PID" ]] && kill -0 "$PID" 2>/dev/null; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  if [[ $code -ne 0 ]]; then
    echo
    echo "--- 服务日志（最后 40 行）---"
    tail -n 40 "$TMP/server.log" 2>/dev/null || true
  fi
  rm -rf "$TMP"
  exit "$code"
}
trap cleanup EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

assert_eq() {
  local got="$1" want="$2" what="$3"
  [[ "$got" == "$want" ]] || fail "$what：期望 [$want]，实际 [$got]"
}

assert_jq() {
  local json="$1" filter="$2" what="$3"
  echo "$json" | jq -e "$filter" >/dev/null || fail "$what（jq: $filter）"
}

post() {
  local path="$1" body="$2"
  curl -sS -X POST "$BASE$path" -H 'Content-Type: application/json' -d "$body"
}

http_code() {
  local path="$1" body="$2"
  curl -sS -o "$TMP/body" -w '%{http_code}' -X POST "$BASE$path" \
    -H 'Content-Type: application/json' -d "$body"
}

echo "==> 构建"
cargo build --offline --quiet
[[ -x "$BIN" ]] || fail "找不到可执行文件 $BIN"

echo "==> 启动临时服务（端口 $PORT，数据库 $TMP/test.db）"
SC_DB="$TMP/test.db" SC_STRINGS_DIR="$ROOT/strings" SC_STRINGS_DB="$TMP/strings.db" SC_LANG=zh-CN SC_PORT="$PORT" SC_BIND="127.0.0.1:$PORT" "$BIN" >"$TMP/server.log" 2>&1 &
PID=$!

ready=0
for _ in $(seq 1 100); do
  if curl -sf "$BASE/api/health" >/dev/null 2>&1; then ready=1; break; fi
  if ! kill -0 "$PID" 2>/dev/null; then fail "服务进程提前退出"; fi
  sleep 0.15
done
[[ "$ready" == "1" ]] || fail "服务未在预期时间内就绪"

echo "==> 健康检查 / 页面 / 字符串"
health="$(curl -sS "$BASE/api/health")"
assert_jq "$health" '.ok == true' "健康检查"
LOGIN_HTML="$(curl -sS "$BASE/login")"
grep -q '智慧校园' <<<"$LOGIN_HTML" || fail "登录页未渲染"
if grep -q '@\[strings/' <<<"$LOGIN_HTML"; then fail "登录页存在未替换的字符串标签"; fi
if grep -q 'style=' <<<"$LOGIN_HTML"; then fail "登录页存在行内样式（HTML/CSS 未解耦）"; fi
CSS="$(curl -sS "$BASE/static/app.css")"
grep -q '#0010ff' <<<"$CSS" || fail "CSS 未使用强调色 #0010ff"
grep -q '#ff0000' <<<"$CSS" || fail "CSS 未使用红色 #ff0000"
grep -q '#eedd00' <<<"$CSS" || fail "CSS 未使用黄色 #eedd00"
code="$(curl -sS -o /dev/null -w '%{http_code}' "$BASE/")"
assert_eq "$code" "303" "根路径跳转"
[[ -f "$TMP/strings.db" ]] || fail "strings.db 未生成"
STRING_COUNT="$(sqlite3 "$TMP/strings.db" "SELECT COUNT(*) FROM strings WHERE language = 'zh-CN';")"
[[ "$STRING_COUNT" -gt 100 ]] || fail "strings.db 同步数量异常：$STRING_COUNT"
curl -sS "$BASE/static/strings.js" | grep -q '"title"' || fail "strings.js 未输出"

echo "==> 发布征集"
SUB_JSON="$(post /api/new-submission "{\"auth\":\"$TEACHER\",\"name\":\"冒烟测试征集\",\"desc\":\"自动化测试\",\"type\":\"homework\",\"deadLine\":\"2026-12-31T20:00\"}")"
assert_jq "$SUB_JSON" '.ok == true and (.id | startswith("sub_"))' "new-submission"
SUB="$(echo "$SUB_JSON" | jq -r .id)"

echo "==> 学生上传"
UP_JSON="$(post /api/upload "{\"auth\":\"$STUDENT\",\"which\":\"$SUB\",\"title\":\"冒烟测试作业\",\"desc\":\"已完成\",\"data\":[{\"name\":\"hello.txt\",\"mime\":\"text/plain\",\"data\":\"aGVsbG8=\"}]}")"
assert_jq "$UP_JSON" '.ok == true and (.id | startswith("up_"))' "upload"
UP="$(echo "$UP_JSON" | jq -r .id)"

echo "==> 下载与 MatchMode"
DL="$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":\"All\"}")"
assert_jq "$DL" '.ok == true and (.datas | length) == 1' "download All"
assert_jq "$DL" '.datas[0].studentID == "G2603-01" and .datas[0].shortStudentID == 1' "download 学号"
assert_jq "$DL" '.datas[0].data[0].data == "aGVsbG8="' "download 附件"
assert_jq "$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":{\"Random\":1}}")" '.datas | length == 1' "Random"
assert_jq "$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":{\"StudentID\":\"G2603-01\"}}")" '.datas | length == 1' "StudentID"
assert_jq "$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":{\"ShortStudentID\":1}}")" '.datas | length == 1' "ShortStudentID"
assert_jq "$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":{\"TitleReGex\":\"冒烟\"}}")" '.datas | length == 1' "TitleReGex"
assert_jq "$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":{\"Before\":\"1h\"}}")" '.datas | length == 0' "Before 1h"
assert_jq "$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":{\"Before\":\"2099-01-01\"}}")" '.datas | length == 1' "Before 未来"
assert_eq "$(http_code /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":{\"TitleReGex\":\"[\"}}")" "400" "非法正则"
assert_jq "$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":{\"type\":\"Random\",\"quantity\":1}}")" '.datas | length == 1' "显式 match_mode"

echo "==> 回复"
REP_JSON="$(post /api/reply "{\"auth\":\"$TEACHER\",\"uploadID\":\"$UP\",\"content\":\"收到，完成得不错。\"}")"
assert_jq "$REP_JSON" '.ok == true and (.id | startswith("rep_"))' "reply"
REP="$(echo "$REP_JSON" | jq -r .id)"
DL="$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":\"All\"}")"
assert_jq "$DL" '.datas[0].reply == ["收到，完成得不错。"]' "download 回复"

echo "==> 学生页面（列表范围与渲染）"
curl -sS -c "$TMP/stu-cookie" -o /dev/null -X POST -d "key=$STUDENT" "$BASE/login/mob/student"
HOME_HTML="$(curl -sS -b "$TMP/stu-cookie" "$BASE/home/mob/student/")"
grep -q "函数与导数" <<<"$HOME_HTML" || fail "学生首页没有显示班级征集"
if grep -q "还没有征集" <<<"$HOME_HTML"; then fail "学生首页错误显示暂无征集"; fi
if grep -q '@\[strings/' <<<"$HOME_HTML"; then fail "学生首页存在未替换的字符串标签"; fi
if grep -q 'style=' <<<"$HOME_HTML"; then fail "学生首页存在行内样式（HTML/CSS 未解耦）"; fi
SUBS_HTML="$(curl -sS -b "$TMP/stu-cookie" "$BASE/home/mob/student/submissions/")"
grep -q "函数与导数" <<<"$SUBS_HTML" || fail "学生征集列表为空"
if grep -q '@\[strings/' <<<"$SUBS_HTML"; then fail "学生征集页存在未替换的字符串标签"; fi
if grep -q 'style=' <<<"$SUBS_HTML"; then fail "学生征集页存在行内样式（HTML/CSS 未解耦）"; fi
# 老师空间别名 /home/*/teachers
curl -sS -c "$TMP/tea-cookie" -o /dev/null -X POST -d "key=$TEACHER" "$BASE/login/pc/teacher"
assert_eq "$(curl -sS -o /dev/null -w '%{http_code}' -b "$TMP/tea-cookie" "$BASE/home/pc/teachers/")" "200" "teachers 路径别名"
curl -sS -b "$TMP/tea-cookie" "$BASE/home/pc/teachers/" | grep -q "林书言" || fail "老师空间页面缺少身份信息"
# 已登录时 /login 仍展示身份选择，便于切换 auth；/ 则回到当前空间
assert_eq "$(curl -sS -o /dev/null -w '%{http_code}' -b "$TMP/tea-cookie" "$BASE/login")" "200" "已登录访问登录页"
curl -sS -b "$TMP/tea-cookie" "$BASE/login" | grep -q "选择你的登录身份" || fail "登录页缺少身份选择"
assert_eq "$(curl -sS -o /dev/null -w '%{http_code}' -b "$TMP/tea-cookie" "$BASE/")" "303" "已登录访问根路径"

echo "==> 问卷模式"
SURVEY_DESC='姓名：<input id="form_name" required/>
想参加的社团：<checkbox id="form_club" value="math"/>数学社 <checkbox id="form_club" value="art"/>美术社
接收通知：<switch id="form_notify"/>'
SURVEY_BODY="$(jq -nc --arg a "$TEACHER" --arg n "问卷征集" --arg d "$SURVEY_DESC" \
  '{auth:$a,name:$n,desc:$d,type:"survey"}')"
SURVEY_JSON="$(post /api/new-submission "$SURVEY_BODY")"
assert_jq "$SURVEY_JSON" '.ok == true and (.id | startswith("sub_"))' "发布问卷征集"
SURVEY="$(echo "$SURVEY_JSON" | jq -r .id)"

# 详情页：简介里的控件被渲染成真正的表单 + 末尾提交按钮
SURVEY_DETAIL="$(curl -sS -b "$TMP/stu-cookie" "$BASE/home/mob/student/submission/$SURVEY/all")"
grep -q 'data-survey-form' <<<"$SURVEY_DETAIL" || fail "问卷详情页缺少问卷表单"
grep -q 'id="form_name"' <<<"$SURVEY_DETAIL" || fail "问卷详情页未渲染 input 控件"
grep -q 'data-survey-kind="switch"' <<<"$SURVEY_DETAIL" || fail "问卷详情页未渲染 switch 控件"
grep -q 'data-survey-submit' <<<"$SURVEY_DETAIL" || fail "问卷详情页缺少提交按钮"
if grep -q '&lt;input id=&quot;form_name&quot;' <<<"$SURVEY_DETAIL"; then
  fail "问卷控件的标签被当成纯文本转义输出了"
fi
if grep -q '@\[strings/' <<<"$SURVEY_DETAIL"; then fail "问卷详情页存在未替换的字符串标签"; fi
if grep -q 'style=' <<<"$SURVEY_DETAIL"; then fail "问卷详情页存在行内样式（HTML/CSS 未解耦）"; fi

# 上传页：关闭默认上传表单，改为问卷
SURVEY_UPLOAD_PAGE="$(curl -sS -b "$TMP/stu-cookie" "$BASE/home/mob/student/submission/$SURVEY/upload")"
grep -q 'data-survey-form' <<<"$SURVEY_UPLOAD_PAGE" || fail "问卷上传页缺少问卷表单"
if grep -q 'id="upload-form"' <<<"$SURVEY_UPLOAD_PAGE"; then fail "问卷模式仍渲染了默认上传表单"; fi
if grep -q '@\[strings/' <<<"$SURVEY_UPLOAD_PAGE"; then fail "问卷上传页存在未替换的字符串标签"; fi

# API：必填项缺失被拒绝，填写完整后按 bot-upload 保存
BAD_ANSWER="$(jq -nc '{form_notify:"on"}')"
BAD_BODY="$(jq -nc --arg a "$STUDENT" --arg w "$SURVEY" --arg d "$BAD_ANSWER" \
  '{auth:$a,which:$w,title:"bot-upload",desc:$d}')"
assert_eq "$(http_code /api/upload "$BAD_BODY")" "400" "问卷缺少必填项"

ANSWER="$(jq -nc '{form_name:"小明",form_club:"math,art",form_notify:"on"}')"
GOOD_BODY="$(jq -nc --arg a "$STUDENT" --arg w "$SURVEY" --arg d "$ANSWER" \
  '{auth:$a,which:$w,title:"bot-upload",desc:$d}')"
SURVEY_UP_JSON="$(post /api/upload "$GOOD_BODY")"
assert_jq "$SURVEY_UP_JSON" '.ok == true and (.id | startswith("up_"))' "问卷 bot 提交"
SURVEY_UP="$(echo "$SURVEY_UP_JSON" | jq -r .id)"
SURVEY_DL="$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SURVEY\",\"match_mode\":\"All\"}")"
echo "$SURVEY_DL" | jq -e --arg ans "$ANSWER" \
  '.datas[0].title == "bot-upload" and .datas[0].desc == $ans' >/dev/null \
  || fail "问卷答案经 API 下载"

# 提交详情：JSON 答案渲染成可读的键值（题目取自简介）
SURVEY_ANSWER_HTML="$(curl -sS -b "$TMP/tea-cookie" "$BASE/home/pc/teacher/upload/$SURVEY_UP/")"
grep -q '小明' <<<"$SURVEY_ANSWER_HTML" || fail "问卷提交详情未显示答案"
grep -q '姓名' <<<"$SURVEY_ANSWER_HTML" || fail "问卷提交详情未显示题目标签"
if grep -q '@\[strings/' <<<"$SURVEY_ANSWER_HTML"; then fail "问卷提交详情存在未替换的字符串标签"; fi

# 仍允许 API 提交非 JSON 说明文本
assert_jq "$(post /api/upload "{\"auth\":\"$STUDENT\",\"which\":\"$SURVEY\",\"title\":\"plain\",\"desc\":\"随便写点说明\"}")" \
  '.ok == true' "问卷仍允许 API 直接提交"
assert_jq "$(post /api/del-object "{\"auth\":\"$ADMIN\",\"id\":\"$SURVEY\"}")" '.ok == true' "删除问卷征集"

echo "==> 答题卡模式（问卷派生：全必填 + 前端自动批改）"
SHEET_DESC='1 + 1 = <input id="form_q1" data-answer="2" data-score="5"/>
选出正确答案：<checkbox id="form_q2" value="a" data-answer="b" data-score="3"/>A <checkbox id="form_q2" value="b"/>B
地球是圆的：<switch id="form_q3" data-answer="on" data-score="2"/>'
SHEET_BODY="$(jq -nc --arg a "$TEACHER" --arg n "答题卡征集" --arg d "$SHEET_DESC" \
  '{auth:$a,name:$n,desc:$d,type:"answer sheet"}')"
SHEET_JSON="$(post /api/new-submission "$SHEET_BODY")"
assert_jq "$SHEET_JSON" '.ok == true and (.id | startswith("sub_"))' "发布答题卡征集"
SHEET="$(echo "$SHEET_JSON" | jq -r .id)"

# 缺少 data-answer / data-score 的答题卡不允许发布
BROKEN_BODY="$(jq -nc --arg a "$TEACHER" '{auth:$a,name:"坏答题卡",desc:"1+1=<input id=\"form_q1\"/>",type:"answer sheet"}')"
assert_eq "$(http_code /api/new-submission "$BROKEN_BODY")" "400" "答题卡缺少 data-answer"

SHEET_DETAIL="$(curl -sS -b "$TMP/stu-cookie" "$BASE/home/mob/student/submission/$SHEET/all")"
grep -q 'data-answer-sheet="1"' <<<"$SHEET_DETAIL" || fail "答题卡详情页缺少自动批改标记"
grep -q 'data-answer="2"' <<<"$SHEET_DETAIL" || fail "答题卡未下发 data-answer"
grep -q 'data-score="5"' <<<"$SHEET_DETAIL" || fail "答题卡未下发 data-score"
[[ "$(grep -o 'required' <<<"$SHEET_DETAIL" | wc -l)" -ge 3 ]] || fail "答题卡并非所有项目都必填"
if grep -q '@\[strings/' <<<"$SHEET_DETAIL"; then fail "答题卡详情页存在未替换的字符串标签"; fi
if grep -q 'style=' <<<"$SHEET_DETAIL"; then fail "答题卡详情页存在行内样式（HTML/CSS 未解耦）"; fi

SHEET_UPLOAD_PAGE="$(curl -sS -b "$TMP/stu-cookie" "$BASE/home/mob/student/submission/$SHEET/upload")"
grep -q 'data-answer-sheet="1"' <<<"$SHEET_UPLOAD_PAGE" || fail "答题卡上传页缺少自动批改标记"
if grep -q 'id="upload-form"' <<<"$SHEET_UPLOAD_PAGE"; then fail "答题卡模式仍渲染了默认上传表单"; fi

# 所有项目必填（服务端校验）
PARTIAL="$(jq -nc '{form_q1:"2"}')"
PARTIAL_BODY="$(jq -nc --arg a "$STUDENT" --arg w "$SHEET" --arg d "$PARTIAL" \
  '{auth:$a,which:$w,title:"bot-upload",desc:$d}')"
assert_eq "$(http_code /api/upload "$PARTIAL_BODY")" "400" "答题卡未全部作答"

# 前端自动批改：正误与分数追加在提交内容末尾（__grade）
FULL_ANSWER="$(jq -nc '{form_q1:"2",form_q2:"b",form_q3:"on",
  __grade:{score:10,full:10,right:3,count:3,detail:{
    form_q1:{ok:true,score:5,answer:"2"},
    form_q2:{ok:true,score:3,answer:"b"},
    form_q3:{ok:true,score:2,answer:"on"}}}}')"
FULL_BODY="$(jq -nc --arg a "$STUDENT" --arg w "$SHEET" --arg d "$FULL_ANSWER" \
  '{auth:$a,which:$w,title:"bot-upload",desc:$d}')"
SHEET_UP_JSON="$(post /api/upload "$FULL_BODY")"
assert_jq "$SHEET_UP_JSON" '.ok == true and (.id | startswith("up_"))' "答题卡 bot 提交"
SHEET_UP="$(echo "$SHEET_UP_JSON" | jq -r .id)"
SHEET_DL="$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SHEET\",\"match_mode\":\"All\"}")"
echo "$SHEET_DL" | jq -e '.datas[0].desc | fromjson | .__grade.score == 10 and .__grade.full == 10' >/dev/null \
  || fail "答题卡批改结果未追加在提交内容末尾"

SHEET_ANSWER_HTML="$(curl -sS -b "$TMP/tea-cookie" "$BASE/home/pc/teacher/upload/$SHEET_UP/")"
grep -q '自动批改结果' <<<"$SHEET_ANSWER_HTML" || fail "答题卡提交详情未显示批改结果"
grep -q '10 / 10' <<<"$SHEET_ANSWER_HTML" || fail "答题卡提交详情未显示得分"
grep -q '正确 3 / 3' <<<"$SHEET_ANSWER_HTML" || fail "答题卡提交详情未显示正误统计"
if grep -q '@\[strings/' <<<"$SHEET_ANSWER_HTML"; then fail "答题卡提交详情存在未替换的字符串标签"; fi
curl -sS -b "$TMP/tea-cookie" "$BASE/home/pc/teacher/submission/$SHEET/all" | grep -q '10 / 10' \
  || fail "答题卡提交列表未显示得分"
assert_jq "$(post /api/del-object "{\"auth\":\"$ADMIN\",\"id\":\"$SHEET\"}")" '.ok == true' "删除答题卡征集"

echo "==> 账号入口与角色身份隔离"
assert_eq "$(curl -sS -o /dev/null -w '%{http_code}' "$BASE/user")" "303" "未登录访问账号中心"
assert_eq "$(curl -sS -o /dev/null -w '%{redirect_url}' "$BASE/user")" "$BASE/login/account" "未登录账号中心跳转登录页"
if grep -q 'sc_uid' "$TMP/tea-cookie"; then fail "角色登录不应建立账号会话 sc_uid"; fi
assert_eq "$(curl -sS -o /dev/null -w '%{http_code}' -b "$TMP/tea-cookie" "$BASE/user")" "303" "仅角色身份访问账号中心"
assert_eq "$(curl -sS -o /dev/null -w '%{redirect_url}' -b "$TMP/tea-cookie" "$BASE/user")" "$BASE/login/account" "角色身份不反查账号"
assert_eq "$(curl -sS -o /dev/null -w '%{http_code}' -b "$TMP/tea-cookie" "$BASE/login/account")" "200" "账号登录页可直接访问"
ACCOUNT_LOGIN_HTML="$(curl -sS -b "$TMP/tea-cookie" "$BASE/login/account")"
grep -q "账号中心" <<<"$ACCOUNT_LOGIN_HTML" || fail "账号登录页未渲染"
if grep -q '@\[strings/' <<<"$ACCOUNT_LOGIN_HTML"; then fail "账号登录页存在未替换的字符串标签"; fi

echo "==> 账号中心表单（用户 / 偏好 / 密码 / 删除身份）"
USER_COOKIE="$TMP/user-cookie"
assert_eq "$(curl -sS -c "$USER_COOKIE" -o /dev/null -w '%{http_code}' -X POST -d 'username=student&password=student123' "$BASE/user/login")" "303" "账号登录"
USER_HTML="$(curl -sS -b "$USER_COOKIE" "$BASE/user")"
grep -q "我的班级身份" <<<"$USER_HTML" || fail "账号中心未渲染"
grep -q "陈小满" <<<"$USER_HTML" || fail "账号中心未显示账号本人"

# 账号会话与角色会话同时存在时，/user 只看 sc_uid，不能串到角色绑定的用户。
MIXED_COOKIE="$TMP/mixed-cookie"
curl -sS -c "$MIXED_COOKIE" -o /dev/null -X POST -d 'username=student&password=student123' "$BASE/user/login"
curl -sS -c "$MIXED_COOKIE" -b "$MIXED_COOKIE" -o /dev/null -X POST -d "key=$TEACHER" "$BASE/login/pc/teacher"
MIXED_HTML="$(curl -sS -b "$MIXED_COOKIE" "$BASE/user")"
grep -q "陈小满" <<<"$MIXED_HTML" || fail "混合会话下账号中心未显示账号本人"
if grep -q "林书言" <<<"$MIXED_HTML"; then fail "混合会话下账号中心被角色身份覆盖"; fi

REG_COOKIE="$TMP/reg-cookie"
assert_eq "$(curl -sS -c "$REG_COOKIE" -b "$REG_COOKIE" -o /dev/null -w '%{http_code}' -X POST \
  --data-urlencode 'username=smokeuser' --data-urlencode 'password=pass1234' \
  --data-urlencode 'password2=pass1234' --data-urlencode 'key=SC-STU-DEMO-2026-02' \
  "$BASE/user/register")" "303" "账号注册并绑定密钥"
REG_HTML="$(curl -sS -b "$REG_COOKIE" "$BASE/user")"
grep -q "SC-STU-DEMO-2026-02" <<<"$REG_HTML" || fail "注册后未显示绑定密钥"
assert_eq "$(curl -sS -b "$REG_COOKIE" -o /dev/null -w '%{http_code}' -X POST -d 'display_name=Smoke&theme=dark&mode=pc' "$BASE/user/prefs")" "303" "保存偏好"
assert_eq "$(curl -sS -b "$REG_COOKIE" -o /dev/null -w '%{http_code}' -X POST -d 'old=pass1234&new=pass5678&new2=pass5678' "$BASE/user/password")" "303" "修改密码"
assert_eq "$(curl -sS -c "$TMP/relogin-cookie" -o /dev/null -w '%{http_code}' -X POST -d 'username=smokeuser&password=pass5678' "$BASE/user/login")" "303" "新密码登录"
AUTH_ID="$(grep -o 'name="auth_id" value="[^"]*"' <<<"$REG_HTML" | head -1 | cut -d'"' -f4)"
[[ -n "$AUTH_ID" ]] || fail "未找到待删除的 auth id"
assert_eq "$(curl -sS -b "$REG_COOKIE" -o /dev/null -w '%{http_code}' -X POST "$BASE/user/auth/$AUTH_ID/delete")" "303" "删除自己的身份"

echo "==> 身份改名 /api/rename-auth"
NEW_TEACHER_NAME="林书言（改）"
NEW_STUDENT_NAME="陈小满（改）"

RENAME_T="$(post /api/rename-auth "{\"auth\":\"$TEACHER\",\"name\":\"$NEW_TEACHER_NAME\"}")"
assert_jq "$RENAME_T" ".ok == true and .name == \"$NEW_TEACHER_NAME\"" "rename-auth 老师"
ALL_RENAMED="$(post /api/all-submission "{\"auth\":\"$TEACHER\",\"match_mode\":\"All\"}")"
assert_jq "$ALL_RENAMED" ".datas[] | select(.id == \"$SUB\") | .createdBy == \"$NEW_TEACHER_NAME\"" "改名同步征集发布者"
curl -sS -b "$TMP/tea-cookie" "$BASE/home/pc/teachers/" | grep -q "$NEW_TEACHER_NAME" || fail "老师空间未显示新名称"
TEA_USER_COOKIE="$TMP/tea-user-cookie"
assert_eq "$(curl -sS -c "$TEA_USER_COOKIE" -o /dev/null -w '%{http_code}' -X POST -d 'username=teacher&password=teacher123' "$BASE/user/login")" "303" "老师账号登录"
ACCOUNT_HTML="$(curl -sS -b "$TEA_USER_COOKIE" "$BASE/user")"
grep -q 'data-rename=' <<<"$ACCOUNT_HTML" || fail "账号中心缺少改名入口"
if grep -q '@\[strings/' <<<"$ACCOUNT_HTML"; then fail "账号中心存在未替换的字符串标签"; fi
curl -sS -b "$TMP/tea-cookie" "$BASE/home/pc/teacher/upload/$UP/" | grep -q "$NEW_TEACHER_NAME" || fail "改名未同步历史回复人"

RENAME_S="$(post /api/rename-auth "{\"auth\":\"$STUDENT\",\"name\":\"$NEW_STUDENT_NAME\"}")"
assert_jq "$RENAME_S" ".ok == true and .name == \"$NEW_STUDENT_NAME\"" "rename-auth 学生"
DL_RENAMED="$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":\"All\"}")"
assert_jq "$DL_RENAMED" ".datas[0].uploader == \"$NEW_STUDENT_NAME\"" "改名同步历史提交人"
assert_eq "$(http_code /api/rename-auth "{\"auth\":\"$TEACHER\",\"name\":\"   \"}")" "400" "改名空名称"
BAD_API="$(post /api/rename-auth '{"auth":"SC-INVALID","name":"x"}')"
assert_eq "$(http_code /api/rename-auth '{"auth":"SC-INVALID","name":"x"}')" "401" "改名无效密钥"
if echo "$BAD_API" | jq -r .msg | grep -qP '[\x{4e00}-\x{9fff}]'; then fail "API 错误信息不是英文"; fi

echo "==> 学生视角（API）"
ALL="$(post /api/all-submission "{\"auth\":\"$STUDENT\",\"match_mode\":\"All\"}")"
assert_jq "$ALL" ".datas[] | select(.id == \"$SUB\") | .myUploadCount == 1 and .uploadCount == 1" "all-submission 学生视角"
assert_eq "$(http_code /api/download "{\"auth\":\"$STUDENT\",\"which\":\"$SUB\",\"match_mode\":\"All\"}")" "403" "学生禁止下载"
assert_eq "$(http_code /api/new-submission "{\"auth\":\"$STUDENT\",\"name\":\"x\"}")" "403" "学生禁止发布征集"
assert_eq "$(http_code /api/new-class "{\"auth\":\"$TEACHER\",\"desc\":\"x\"}")" "403" "老师禁止创建班级"

echo "==> 创建班级 / 成员密钥"
CLASS_JSON="$(post /api/new-class "{\"auth\":\"$ADMIN\",\"name\":\"冒烟测试班\",\"desc\":\"自动化\"}")"
assert_jq "$CLASS_JSON" '.ok == true and (.id | startswith("cls_")) and (.auth | startswith("SC-ADM-"))' "new-class"
CLASS="$(echo "$CLASS_JSON" | jq -r .id)"
CLASS_KEY="$(echo "$CLASS_JSON" | jq -r .auth)"
MEMBERS="$(post /api/new-user "{\"auth\":\"$CLASS_KEY\",\"classID\":\"$CLASS\",\"quantity\":2,\"type\":\"Student\"}")"
assert_jq "$MEMBERS" '.ok == true and (.IDs | length) == 2 and (.keys | length) == 2' "new-user"
NEW_KEY="$(echo "$MEMBERS" | jq -r '.keys[0]')"
NEW_KEY2="$(echo "$MEMBERS" | jq -r '.keys[1]')"
assert_jq "$(post /api/all-submission "{\"auth\":\"$NEW_KEY\"}")" '.ok == true' "新成员可登录（API）"
assert_jq "$(post /api/del-key "{\"auth\":\"$NEW_KEY\"}")" '.ok == true' "del-key"
assert_eq "$(http_code /api/all-submission "{\"auth\":\"$NEW_KEY\"}")" "401" "删除后的密钥失效"

echo "==> 管理员删除对象"
assert_eq "$(http_code /api/del-object "{\"id\":\"$SUB\"}")" "401" "del-object 未鉴权"
assert_jq "$(post /api/del-object "{\"auth\":\"$ADMIN\",\"id\":\"$REP\"}")" '.ok == true' "删除回复"
assert_jq "$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":\"All\"}")" '.datas[0].reply | length == 0' "回复已删除"
assert_jq "$(post /api/del-object "{\"auth\":\"$ADMIN\",\"id\":\"$UP\"}")" '.ok == true' "删除提交"
assert_jq "$(post /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":\"All\"}")" '.datas | length == 0' "提交已删除"
assert_jq "$(post /api/del-object "{\"auth\":\"$ADMIN\",\"id\":\"$SUB\"}")" '.ok == true' "删除征集"
assert_eq "$(http_code /api/download "{\"auth\":\"$TEACHER\",\"which\":\"$SUB\",\"match_mode\":\"All\"}")" "404" "征集已删除"

echo "==> 清理创建的新成员密钥 / 班级"
post /api/del-key "{\"auth\":\"$NEW_KEY2\"}" >/dev/null || true
assert_jq "$(post /api/del-object "{\"auth\":\"$CLASS_KEY\",\"id\":\"$CLASS\"}")" '.ok == true' "删除班级"
assert_eq "$(http_code /api/all-submission "{\"auth\":\"$CLASS_KEY\"}")" "401" "班级删除后密钥失效"

echo
echo "PASS: 智慧校园冒烟测试全部通过 ✅"
