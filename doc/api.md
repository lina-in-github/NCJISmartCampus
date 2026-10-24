# 智慧校园 · API 参考

所有接口均位于 `/api` 下，使用 `POST` + JSON。示例中的 base URL 默认为 `http://localhost:3000`。

## 0. 通用约定

### 0.1 请求头

```http
Content-Type: application/json
```

### 0.2 鉴权

- 除 `/api/health` 外，均需要请求体中的 `auth`：一个班级密钥，例如 `SC-STU-DEMO-2026`。
- 密钥同时决定 **班级** 与 **角色**，不允许跨班操作。
- `/api/del-object` 按原始需求只传 `id`；也接受可选的 `auth`，缺省时使用浏览器登录 cookie（`sc_sid`）鉴权。
- 角色：`student`（学生）、`teacher`（老师）、`admin`（管理员）。家长使用学生密钥，不具备教师/管理员权限。

### 0.3 响应与错误

成功响应都会包含 `"ok": true`，并额外包含需求指定的字段：

```json
{ "ok": true, "id": "up_xxxxxxxxxxxx" }
```

失败响应为：

```json
{ "ok": false, "msg": "human-readable error message" }
```

> v0.1.2 起，API 的字段、错误与 `msg` 统一使用英文；页面 UI 文案由 `strings/zh-CN.txt` 提供。

| HTTP | 含义 |
| --- | --- |
| 200 | 成功 |
| 400 | 参数/格式错误（如非法正则、附件格式错误） |
| 401 | 缺少或无效的 `auth` |
| 403 | 权限不足（如学生调用下载） |
| 404 | 对象不存在，或不属于当前班级 |
| 422 | JSON 字段缺失或类型错误（框架级） |
| 413 | 请求体超过 32MB |

### 0.4 ID 与格式

- 班级：`cls_*`
- 身份/密钥记录：`auth_*`（密钥本身是 `SC-XXX-XXXXXXXX`）
- 征集：`sub_*`
- 提交：`up_*`
- 回复：`rep_*`

### 0.5 时间

- 请求接受：
  - `2026-11-01T20:00`
  - `2026-11-01 20:00:00`
  - `2026-11-01`
  - `2026-11-01T12:00:00Z`
  - `2026-11-01T20:00:00+08:00`
  - Unix 时间戳（秒）
- 无时区输入按 **北京时间（UTC+8）** 解释。
- 响应中的 `deadLine` / `createdAt` 为 `2026-11-01T20:00:00+08:00` 形式。
- `deadLine: null` 或空字符串表示无截止时间。

### 0.6 附件

`data` 必须为数组，最多 8 项，base64 文本总长度上限约 24MB（原始数据约 18MB）。

推荐：

```json
[
  {
    "name": "作业第1页.png",
    "mime": "image/png",
    "data": "iVBORw0KGgoAAAANSUhEUg..."
  }
]
```

兼容：

- `"data:image/png;base64,iVBOR..."`；
- 裸 base64 字符串（会按 `application/octet-stream` 保存）。

服务端会校验 MIME 与 base64，并统一保存为对象数组（data URL 字符串会保持字符串形式）。图片在页面中内联预览，其他附件提供下载。

---

## 1. `/api/upload`

上传一份提交到指定征集（任意角色）。类型为 `notice` 的公示版禁用提交，会返回 `403`。

### 请求

```json
{
  "auth": "SC-STU-DEMO-2026",
  "which": "sub_xxxxxxxxxxxx",
  "title": "周末练习 · 第 1 页",
  "desc": "已完成 P42–P45，第 3 题不太确定。",
  "data": [
    {
      "name": "page1.png",
      "mime": "image/png",
      "data": "iVBORw0KGgo..."
    }
  ]
}
```

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `auth` | string | ✅ | 班级密钥 |
| `which` | string | ✅ | 征集 ID，必须在同一班级 |
| `title` | string | ✅ | 提交标题，≤200 字 |
| `desc` | string | — | 说明，≤5000 字 |
| `data` | array | — | 附件数组，默认 `[]` |

### 响应

```json
{ "ok": true, "id": "up_xxxxxxxxxxxx" }
```

### 示例

```bash
curl -s http://localhost:3000/api/upload \
  -H 'Content-Type: application/json' \
  -d '{
    "auth":"SC-STU-DEMO-2026",
    "which":"SUB_ID",
    "title":"周末练习",
    "desc":"已完成",
    "data":[{"name":"hello.txt","mime":"text/plain","data":"aGVsbG8="}]
  }'
```

说明：学生始终以自己的身份记录 `studentID` / `shortStudentID`；老师/管理员上传时这两项为空/0。截止后仍允许补交。

---

## 2. `/api/download`

查看/下载某征集的提交（**老师 / 管理员**）。

### 请求

```json
{
  "auth": "SC-TEA-DEMO-2026",
  "which": "sub_xxxxxxxxxxxx",
  "match_mode": "All"
}
```

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `auth` | string | ✅ | 老师/管理员密钥 |
| `which` | string | ✅ | 征集 ID |
| `match_mode` | string / object | — | 缺省为 `"All"` |

### MatchMode

| 写法 | 说明 |
| --- | --- |
| `"All"` / `{"All":null}` | 全部提交 |
| `{"Random":3}` / `{"Random":{"quantity":3}}` | 随机取 3 条（保持时间顺序） |
| `{"StudentID":"G2603-01"}` | 按学号精确匹配 |
| `{"ShortStudentID":1}` | 按班内短学号匹配 |
| `{"TitleReGex":"^函数"}` | 对标题做正则匹配；非法正则返回 400 |
| `{"Before":"2026-11-01T20:00"}` | 提交时间早于指定时间 |
| `{"Before":"7d"}` | 提交时间早于 7 天前（支持 `s/m/h/d/w`） |

也支持显式对象：

```json
{ "type": "Random", "quantity": 3 }
{ "type": "Before", "time": "7d" }
```

### 响应

```json
{
  "ok": true,
  "datas": [
    {
      "id": "up_xxxxxxxxxxxx",
      "studentID": "G2603-01",
      "shortStudentID": 1,
      "title": "周末练习",
      "desc": "已完成",
      "data": [],
      "reply": ["整体完成得很好。", "注意第 3 题的链式法则。"],
      "uploader": "陈小满",
      "role": "student",
      "createdAt": "2026-10-24T11:00:00+08:00"
    }
  ]
}
```

- 需求字段：`studentID`、`shortStudentID`、`title`、`desc`、`data`、`reply`。
- `reply` 是回复内容字符串数组，按时间升序。
- 额外字段 `id`、`uploader`、`role`、`createdAt` 便于页面与工具使用。
- 老师/管理员上传的提交 `studentID` 为空字符串、`shortStudentID` 为 0。

---

## 3. `/api/new-submission`

发布征集（**老师 / 管理员**）。

### 请求

```json
{
  "auth": "SC-TEA-DEMO-2026",
  "name": "函数与导数 · 周末练习",
  "desc": "完成练习册 P42–P45，拍照上传。",
  "type": "homework",
  "deadLine": "2026-11-01T20:00"
}
```

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `auth` | string | ✅ | 老师/管理员密钥 |
| `name` | string | ✅ | 征集名称，≤200 字 |
| `desc` | string | — | 说明，≤2000 字 |
| `type` | string | — | 例：`homework`、`open call`、`leave request`、`notice`，缺省 `homework` |
| `deadLine` | string / number / null | — | 截止时间 |

### 响应

```json
{ "ok": true, "id": "sub_xxxxxxxxxxxx" }
```

---

## 4. `/api/all-submission`

查看当前班级的征集列表。学生视角只统计自己的提交；老师/管理员统计全班。

### 请求

```json
{
  "auth": "SC-STU-DEMO-2026",
  "match_mode": "All"
}
```

- `match_mode` 可选，缺省 `All`。
- 支持的匹配：`All`、`Random`、`StudentID`、`ShortStudentID`、`TitleReGex`（作用于征集名称）、`Before`（作用于截止时间）。`StudentID` / `ShortStudentID` 返回该学生提交过的征集。

### 响应

```json
{
  "ok": true,
  "datas": [
    {
      "id": "sub_xxxxxxxxxxxx",
      "name": "函数与导数 · 周末练习",
      "desc": "完成练习册 P42–P45，拍照上传。",
      "deadLine": "2026-11-01T20:00:00+08:00",
      "type": "homework",
      "createdAt": "2026-10-20T10:00:00+08:00",
      "createdBy": "林书言",
      "uploadCount": 2,
      "myUploadCount": 1,
      "pendingCount": 1,
      "latestUploadID": "up_xxxxxxxxxxxx"
    }
  ]
}
```

- 学生视角：`uploadCount` 与 `myUploadCount` 都表示本人提交次数；`pendingCount` 表示本人尚未收到回复的提交数。
- 老师/管理员视角：`uploadCount` 为全班提交数，`pendingCount` 为全班待回复数。
- `deadLine` 可能为 `null`。
- 时间状态由页面根据 `deadLine` 计算，API 不再返回展示文案。
- 截至截止时间的 `notice` 公示版会排在普通征集之前；未设置截止时间的公示版同样置顶。

---

## 5. `/api/reply`

回复一条提交（**老师 / 管理员**）。

### 请求

```json
{
  "auth": "SC-TEA-DEMO-2026",
  "uploadID": "up_xxxxxxxxxxxx",
  "content": "整体完成得很好，第 3 题注意链式法则。"
}
```

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `auth` | string | ✅ | 老师/管理员密钥 |
| `uploadID` | string | ✅ | 提交 ID（也接受 `upload_id`） |
| `content` | string | ✅ | 回复内容，≤5000 字 |

### 响应

```json
{ "ok": true, "id": "rep_xxxxxxxxxxxx" }
```

---

## 6. `/api/new-class`

创建班级（**管理员**）。

### 请求

```json
{
  "auth": "SC-ADM-DEMO-2026",
  "name": "高一（4）班",
  "desc": "2026 级 · 新班级"
}
```

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `auth` | string | ✅ | 管理员密钥 |
| `desc` | string | 与 `name` 至少一个 | 班级描述（原始需求字段） |
| `name` | string | 可选 | 班级名称（扩展字段）；缺省时从 `desc` 推导 |

### 响应

```json
{
  "ok": true,
  "id": "cls_xxxxxxxxxxxx",
  "code": "C3X9EZ",
  "auth": "SC-ADM-XXXXXXXX",
  "authID": "auth_xxxxxxxxxxxx"
}
```

创建成功时会自动签发一个绑定到当前用户账号（若已绑定）的管理员密钥，方便立即进入新班级。`id` 为需求要求的班级 ID，其余为扩展字段。

---

## 7. `/api/new-user`

在指定班级批量生成成员密钥（**管理员**）。

> 只能在自己当前 `auth` 所属班级内新增成员；跨班级请求返回 400。

### 请求

```json
{
  "auth": "SC-ADM-DEMO-2026",
  "classID": "cls_xxxxxxxxxxxx",
  "quantity": 3,
  "type": "Student"
}
```

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `auth` | string | ✅ | 管理员密钥 |
| `classID` | string | ✅ | 班级 ID，必须与管理员当前班级一致 |
| `quantity` | int | ✅ | 1–100 |
| `type` | string | ✅ | `Student` / `Teacher` / `Admin`（大小写不敏感，也接受中文） |

### 响应

```json
{
  "ok": true,
  "IDs": ["auth_xxxxxxxxxxxx", "auth_yyyyyyyyyyyy"],
  "keys": ["SC-STU-XXXXXXXX", "SC-STU-YYYYYYYY"],
  "members": [
    {
      "id": "auth_xxxxxxxxxxxx",
      "key": "SC-STU-XXXXXXXX",
      "role": "student",
      "studentID": "C3X9EZ-01",
      "displayName": "学生 01"
    }
  ]
}
```

- 需求字段 `IDs` 为 auth 记录 ID。
- `keys` 是对应登录密钥；**密钥只在响应中出现一次**，请及时保存并分发。
- 新成员处于「未认领」状态：成员在 `/user` 注册账号并绑定密钥，或登录已有账号后添加密钥。

---

## 8. `/api/del-key`

删除当前使用的 auth 密钥（任意角色）。

### 请求

```json
{ "auth": "SC-STU-DEMO-2026" }
```

### 响应

```json
{ "ok": true, "msg": "key deleted" }
```

删除后该密钥立即失效，相关登录会话会在下一次请求时被清除；该 auth 的历史提交会保留（展示提交时的姓名/学号快照）。

---

## 9. `/api/rename-auth`（v0.1.1）

修改当前 auth（班级身份）的显示名称。任何持有有效 auth 密钥的角色都可以改名。

### 请求

```json
{
  "auth": "SC-TEA-DEMO-2026",
  "name": "林书言"
}
```

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `auth` | string | ✅ | 要改名的 auth 密钥 |
| `name` | string | ✅ | 新显示名称；去除首尾空白后 1–40 个字符，不能包含控制字符 |

### 响应

```json
{
  "ok": true,
  "name": "林书言",
  "msg": "identity name updated"
}
```

失败时返回 `400`（名称为空 / 过长 / 含控制字符）或 `401`（密钥无效）。
改名会同步刷新该 auth 在 `uploads`、`replies`、`submissions` 中的历史显示名称快照，因此已存在的提交/回复/征集也会立即显示新名称。

```bash
curl -s http://localhost:3000/api/rename-auth \
  -H 'Content-Type: application/json' \
  -d '{"auth":"SC-TEA-DEMO-2026","name":"林老师"}'
```

## 10. `/api/del-object`

按对象 ID 删除（**管理员**）。这是原始需求中唯一没有 `auth` 字段的接口，因此也支持使用登录 cookie 鉴权。

### 请求（两种方式）

```json
{ "auth": "SC-ADM-DEMO-2026", "id": "sub_xxxxxxxxxxxx" }
```

```json
{ "id": "rep_xxxxxxxxxxxx" }
```

第二种方式需要浏览器已通过角色登录（携带 `sc_sid` cookie）。建议服务端调用时仍显式传 `auth`。

### 支持的对象

| 前缀 | 对象 | 说明 |
| --- | --- | --- |
| `sub_` | 征集 | 级联删除其提交与回复 |
| `up_` | 提交 | 级联删除其回复 |
| `rep_` | 回复 | 仅删除该回复 |
| `cls_` | 班级 | 仅能删除自己当前管理的班级，级联删除班级数据 |
| `auth_` | 身份密钥 | 仅能删除当前班级的 auth |

### 响应

```json
{ "ok": true, "msg": "deleted" }
```

失败示例：

```json
{ "ok": false, "msg": "you can only delete objects in the current class" }
```

---

## 11. `/api/health`

公开健康检查（扩展接口）。

```bash
curl http://localhost:3000/api/health
```

```json
{ "ok": true, "service": "smart-campus", "time": 1792814400 }
```

---

## 12. curl 全流程示例

```bash
BASE=http://localhost:3000

# 1. 老师发布征集
SUB=$(curl -s $BASE/api/new-submission \
  -H 'Content-Type: application/json' \
  -d '{"auth":"SC-TEA-DEMO-2026","name":"API 示例征集","desc":"示例","type":"homework","deadLine":"2026-12-31T20:00"}' \
  | jq -r .id)

# 2. 学生上传
UP=$(curl -s $BASE/api/upload \
  -H 'Content-Type: application/json' \
  -d "{\"auth\":\"SC-STU-DEMO-2026\",\"which\":\"$SUB\",\"title\":\"我的提交\",\"desc\":\"已完成\",\"data\":[{\"name\":\"hello.txt\",\"mime\":\"text/plain\",\"data\":\"aGVsbG8=\"}]}" \
  | jq -r .id)

# 3. 老师回复
curl -s $BASE/api/reply \
  -H 'Content-Type: application/json' \
  -d "{\"auth\":\"SC-TEA-DEMO-2026\",\"uploadID\":\"$UP\",\"content\":\"收到，做得不错。\"}"

# 4. 老师按标题正则下载
curl -s $BASE/api/download \
  -H 'Content-Type: application/json' \
  -d "{\"auth\":\"SC-TEA-DEMO-2026\",\"which\":\"$SUB\",\"match_mode\":{\"TitleReGex\":\"提交\"}}" | jq .

# 5. 学生查看本人视角的征集
curl -s $BASE/api/all-submission \
  -H 'Content-Type: application/json' \
  -d '{"auth":"SC-STU-DEMO-2026"}' | jq .

# 6. 管理员删除征集
curl -s $BASE/api/del-object \
  -H 'Content-Type: application/json' \
  -d "{\"auth\":\"SC-ADM-DEMO-2026\",\"id\":\"$SUB\"}"
```

---

## 13. 页面使用的接口

页面本身由服务端渲染，首页、提交、回复、上传、删除、账号中心等交互按需调用上述 API；账号中心的密码、偏好、身份绑定、创建班级使用同源表单 POST（`/user/*`），避免在浏览器中暴露密码到 JSON 请求。上传页在浏览器内使用 `FileReader` 将文件转换为 base64 后调用 `/api/upload`。
