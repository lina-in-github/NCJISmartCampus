# 智慧校园 · Campus OS

一个面向班级的智慧校园 Web 服务：**征集发布 → 学生提交（含附件）→ 老师/管理员回复**，并提供班级、成员密钥与多身份账号管理。原生 HTML / CSS / JavaScript 前端，无 CDN、无前端构建步骤，支持手机版与电脑版布局。

- 技术栈：Rust 2021 · Axum 0.8 · rusqlite（SQLite，bundled）· serde · sha2 · regex
- 数据库：SQLite，首次启动自动建表并写入演示数据
- 需求来源：[`doc/requirements.md`](doc/requirements.md)
- 完整 API 文档：[`doc/api.md`](doc/api.md)
- 当前版本：**v0.1.6** — 学生 / 家长视角改为**前台展示区分**：同一个学生密钥可在「学生视角（我）」与「家长视角（孩子）」之间切换，并在个人主页配置默认显示模式；API 完全不变
- 历史版本：v0.1.5 新增征集模式「答题卡」（问卷派生：`id` / `data-answer` / `data-score`、全部必填、前端自动批改）；v0.1.4 新增征集模式「问卷」（简介中的 `<input/>` / `<checkbox/>` / `<switch/>` 渲染为表单、bot 提交 JSON 答案）；v0.1.2 UI 文案集中到 `strings/zh-CN.txt`、API 文案改为英文、角色身份与账号会话隔离；v0.1.1 新增身份改名 `/api/rename-auth` 与账号中心改名入口

---

## 1. 快速开始

环境要求：Rust 1.80+（本项目在 1.99 上验证）、无需 Node.js 或外部数据库。

```bash
cargo run
# 打开 http://localhost:3000
```

首次启动会在当前目录创建 `smartcampus.db` 并写入演示班级、征集、提交和回复；同时读取 `strings/*.txt`、同步到 `strings.db`。删除 `smartcampus.db` 即可重新生成演示数据。

### 环境变量

| 变量 | 默认值 | 说明 |
| --- | --- | --- |
| `SC_DB` | `smartcampus.db` | SQLite 数据库路径 |
| `SC_PORT` | `3000` | 监听端口（用于日志中的地址展示） |
| `SC_BIND` | `0.0.0.0:<SC_PORT>` | 完整监听地址，例如 `127.0.0.1:3000` |
| `SC_STRINGS_DIR` | `strings` | UI 字符串目录，启动时读取其中的 `*.txt` |
| `SC_STRINGS_DB` | `strings.db` | 字符串同步数据库 |
| `SC_LANG` | `zh-CN` | 运行时选择的语言（对应 `strings/<lang>.txt`） |
| `SC_TLS_CERT` | — | PEM 证书链路径；与 `SC_TLS_KEY` 同时设置时启用 HTTPS |
| `SC_TLS_KEY` | — | PEM 私钥路径；与 `SC_TLS_CERT` 同时设置时启用 HTTPS |

```bash
SC_DB=./campus.db SC_BIND=127.0.0.1:8080 cargo run
```

### 启用 HTTPS（TLS）

服务内置 Rustls 支持。使用受信任的证书（例如 Let's Encrypt 或公司 CA 签发的 PEM 证书链）启动：

```bash
SC_BIND=0.0.0.0:3443 \
SC_TLS_CERT=/etc/ssl/campus/fullchain.pem \
SC_TLS_KEY=/etc/ssl/campus/privkey.pem \
cargo run
# 打开 https://localhost:3443
```

本地开发可生成自签名证书（浏览器会提示不受信任，可手动信任该证书）：

```bash
mkdir -p certs
openssl req -x509 -newkey rsa:2048 -nodes -days 365 \
  -keyout certs/key.pem -out certs/cert.pem \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

SC_TLS_CERT=certs/cert.pem SC_TLS_KEY=certs/key.pem cargo run
# 打开 https://localhost:3000
```

`SC_TLS_CERT` 和 `SC_TLS_KEY` 必须同时配置；都不配置时保持原来的 HTTP 模式。生产环境更推荐由 Nginx/Caddy 等反向代理终止 TLS 并转发到本服务的 HTTP 端口，同时确保私钥文件仅服务账号可读。证书路径相对于启动目录解析。

### 演示账号与密钥

| 身份 | 角色密钥 | 绑定账号 | 说明 |
| --- | --- | --- | --- |
| 家长 | `SC-STU-DEMO-2026` | `student` / `student123` | 家长使用孩子的学生密钥登录 |
| 学生 | `SC-STU-DEMO-2026` | `student` / `student123` | 高一（3）班 · 陈小满 |
| 老师 | `SC-TEA-DEMO-2026` | `teacher` / `teacher123` | 高一（3）班 · 林书言 |
| 管理员 | `SC-ADM-DEMO-2026` | `admin` / `admin123` | 高一（3）班 · 赵明远 |
| 管理员（第二班级） | `SC-ADM-DEMO-2027` | `admin` / `admin123` | 高二（1）班，可在账号中心切换 |
| 未认领学生 | `SC-STU-DEMO-2026-02`、`SC-STU-DEMO-2026-03` | — | 可在账号中心绑定 |
| 未认领老师 | `SC-TEA-DEMO-2027` | — | 高二（1）班 |
| 未认领学生 | `SC-STU-DEMO-2027` | — | 高二（1）班 |

> **Auth 模型**：一个 `auth`（班级密钥）只属于一个班级、拥有一个角色；一个 `user` 可以绑定多个 `auth`。切换班级 = 切换身份 = 切换密钥。家长没有独立角色，使用孩子的学生密钥以「家长模式」登录。

登录方式：

- `/login`：先选择家长 / 老师 / 管理员，再进入对应角色登录页；
- 角色页直接输入班级密钥（演示密钥可一键填入），只建立班级身份会话；
- `/login/account`：用户名 + 密码登录账号中心（或首次注册并绑定密钥）；
- `/user`：已登录账号的账号中心；未登录访问会跳转到 `/login/account`。

---

## 2. 页面路由

所有 `/home/*` 页面均以「班级 + 身份」为上下文，路径中的 `*` 对应 `mob` 或 `pc` 视图。

| 路径 | 说明 |
| --- | --- |
| `/` | 已登录 → `/home`；否则 → `/login` |
| `/login` | 选择家长 / 老师 / 管理员；`?mode=pc` 可切换视图 |
| `/login/<mob\|pc>/<parent\|teacher\|admin>` | 角色密钥登录页 |
| `/login/account` | 账号登录 / 注册页；已有账号会话时跳转 `/user` |
| `/home` | 跳转到当前身份的空间 |
| `/home/<mob\|pc>/<parent\|student\|teacher\|admin>/` | 首页（卡片：最近征集、最近/待回复提交、账号入口） |
| `…/submissions/` | 所有征集；老师/管理员可发布 |
| `…/submissions/new` | 发布征集（老师/管理员） |
| `…/uploads/` | 学生：自己的提交；老师/管理员：全班提交，可筛选待回复/已回复 |
| `…/submission/<id>/` | 有提交则跳转最后一条本站提交，否则跳上传表单；公示版（notice）跳转详情页 |
| `…/submission/<id>/upload` | 上传表单（支持多附件、拖拽、预览）；公示版禁用并跳转详情页；问卷/答题卡改为填写简介里的题目 |
| `…/submission/<id>/all` | 该征集下的提交（学生仅自己，老师/管理员全班）；公示版仅渲染详情；问卷/答题卡渲染可填写的表单 |
| `…/upload/<id>/` | 提交详情：附件预览、回复列表、重新上传、管理员删除 |
| `/user` | 账号中心：偏好设置、密码、绑定/删除身份、创建班级、生成成员密钥；未登录 → `/login/account` |
| `/logout` | POST，退出登录 |

兼容说明：

- 路径同时接受带/不带末尾 `/` 的形式；`/home/*/teachers` 也会被识别为老师空间。
- 页面内部使用带末尾 `/` 的规范 URL，因此需求中的 `./submissions` 等相对路径同样可用。
- 登录后 URL 中的 `mob` / `pc` 决定界面形态；账号偏好里的「默认视图」用于账号中心与后续进入。

---

## 3. API

所有接口为 `POST`，请求与响应均为 JSON（`Content-Type: application/json`）。角色密钥通过请求体的 `auth` 字段传递；`/api/del-object` 可按需求只传 `id`，此时回退使用登录 cookie 鉴权。

> **语言约定（v0.1.2）**：API 字段、错误与 `msg` 使用英文；页面 UI 文案全部来自 `strings/zh-CN.txt`。

| 接口 | 权限 | 说明 |
| --- | --- | --- |
| `/api/upload` | 任意身份 | 上传提交（标题、说明、base64 附件）；公示版（notice）返回 403 |
| `/api/download` | 老师 / 管理员 | 按条件下载/查看某征集的提交与回复 |
| `/api/new-submission` | 老师 / 管理员 | 发布征集 |
| `/api/all-submission` | 任意身份 | 学生：本人视角；老师/管理员：全班视角 |
| `/api/reply` | 老师 / 管理员 | 回复一条提交 |
| `/api/new-class` | 管理员 | 创建班级（自动生成一个绑定到当前用户的管理员密钥） |
| `/api/new-user` | 管理员 | 在本班批量生成成员密钥 |
| `/api/del-key` | 任意身份 | 删除当前 `auth` 密钥 |
| `/api/rename-auth` | 任意身份 | 修改当前 `auth` 的显示名称（v0.1.1 新增） |
| `/api/del-object` | 管理员 | 按 id 删除征集 / 提交 / 回复 / 班级 / auth |
| `/api/health` | 公开 | 健康检查（额外提供） |

详细字段、匹配模式、示例与错误码见 **[`doc/api.md`](doc/api.md)**。

---

## 4. 数据模型与关键设计

### 4.1 user 与 auth

```
users  1 ─── n  auths  n ─── 1  classes
                 │
                 ├─ submissions (created_by)
                 ├─ uploads     (auth_id / student_id / short_id)
                 └─ replies     (auth_id)
```

- `users`：用户名、密码、显示名称、偏好设置。
- `classes`：班级名称、简介、班级代码。
- `auths`：班级密钥（`key`）、所属班级、绑定用户、角色（student / teacher / admin）、学生学号与短学号、**显示名称**（可通过 `/api/rename-auth` 修改）。
- `submissions`：班级征集，含类型、说明、截止时间。类型为 `notice` 的公示版仅展示详情、不开放提交；未到截止时间的公示版会置顶。
- `uploads`：提交，含附件 JSON、提交人身份快照（姓名、学号、角色）。
- `replies`：老师/管理员回复，按时间排序。
- 删除班级会级联删除该班级的 auth、征集、提交与回复；删除 auth 不会删除历史提交（保留提交人快照）。删除征集/提交会级联删除其下提交/回复。
- 身份改名会同步刷新该 auth 在历史提交、回复与征集中的显示名称快照，改名后旧内容也会立即使用新名称。

### 4.2 权限矩阵

| 能力 | 学生 | 家长 | 老师 | 管理员 |
| --- | --- | --- | --- | --- |
| 查看班级征集 | ✅ | ✅ | ✅ | ✅ |
| 上传 / 重新上传 | ✅ | ✅ | ✅ | ✅ |
| 查看全班提交 | ❌（仅自己） | ❌（仅孩子） | ✅ | ✅ |
| 发布征集 | ❌ | ❌ | ✅ | ✅ |
| 回复提交 | ❌ | ❌ | ✅ | ✅ |
| 删除回复 / 提交 / 征集 | ❌ | ❌ | ❌ | ✅ |
| 创建班级 / 生成成员密钥 | ❌ | ❌ | ❌ | ✅ |
| 删除当前密钥 | ✅ | ✅ | ✅ | ✅ |
| 修改自身 auth 名称 | ✅ | ✅ | ✅ | ✅ |

学生/家长的直接访问也做服务端校验：即使手动拼出其他同学的 `/upload/<id>/` 地址也返回 404。

> **学生 / 家长只是前台视角**（v0.1.6）：家长使用孩子的学生 `auth`，二者数据范围完全相同，
> 区别只在页面文案（「我的…」/「孩子的…」）。因此服务端只按 `auth.role`（student / teacher / admin）判权限，
> 视角只影响 `page_role` 与展示；`/api/*` 中不存在也没有任何「家长」概念。

### 4.3 登录与会话

- 角色登录成功后只写入 `sc_sid`（HttpOnly、SameSite=Lax、7 天）会话，保存 `auth_id / 页面角色 / 视图模式`；它代表班级空间身份，**不会**反查或建立账号会话。
- 账号登录写入 `sc_uid` 会话（同样 HttpOnly、SameSite=Lax）。这是 `/user` 与账号相关操作的唯一用户来源。
- 未登录访问 `/user` 会 `303` 跳转到 `/login/account`；仅持有角色 `sc_sid` 时同样不会进入账号中心。
- 两个会话可以同时存在且互不覆盖：此时 `/user` 始终显示 `sc_uid` 对应的账号，而不是角色密钥绑定的用户。
- 已注册用户从登录页点击「账号中心」会进入 `/login/account`；已有 `sc_uid` 时该页直接跳回 `/user`，即自己的账号。
- 会话保存在内存中：**重启服务后需要重新登录**，但业务数据全部持久化在 SQLite。
- 账号中心支持「进入」任一已绑定身份；这会创建新的 `sc_sid`，满足「切换班级必须切换 auth」。
- 已登录角色时访问 `/login` 仍会展示身份选择页，可直接输入另一个班级的密钥完成切换；访问 `/` 则回到当前身份的空间。
- 首次使用：管理员通过 `/api/new-user` 生成密钥 → 成员在 `/login/account` 注册账号并绑定密钥（或登录后添加密钥）。
- 学生身份的页面角色是 `student` / `parent` 两种展示视角（见 4.11），都存在 `sc_sid` 里；老师 / 管理员固定为自己的角色。

### 4.4 附件格式

`data` 为数组（最多 8 个、总计约 18MB 原始数据），推荐元素格式：

```json
{
  "name": "作业第1页.png",
  "mime": "image/png",
  "data": "iVBORw0KGgoAAAANSUhEUg..."
}
```

同时兼容：

- `"data:image/png;base64,..."` data URL 字符串；
- 裸 base64 字符串（会包装为 `application/octet-stream`）。

图片在详情页内联预览（灯箱放大），其他类型提供下载。MIME 与 base64 会在写入前校验，避免在页面中注入非 data URL 内容。附件内容存储在 SQLite 中，适合演示/小规模班级；生产环境建议改为对象存储。

### 4.5 时间

- 所有页面与 API 时间统一使用 **北京时间（UTC+8）** 展示。
- 输入接受 `2026-11-01T20:00`、`2026-11-01 20:00:00`、`2026-11-01`、带 `Z` / `+08:00` 的 ISO 时间或 Unix 时间戳。
- API 返回 `deadLine` / `createdAt` 为带 `+08:00` 的 ISO-8601 字符串。
- 截止时间用于展示和 `Before` 匹配；**到截止后仍允许补交**，页面会提示「已截止」，避免误操作造成不可补交。

### 4.6 MatchMode

`/api/download` 与 `/api/all-submission` 的 `match_mode` 支持：

```json
"All"
{ "Random": 3 }
{ "StudentID": "G2603-01" }
{ "ShortStudentID": 1 }
{ "TitleReGex": "^函数" }
{ "Before": "2026-11-01T20:00" }
```

也支持显式对象写法 `{"type":"Random","quantity":3}`，`Before` 接受 ISO 时间或 `7d`/`12h`/`30m` 相对时长。`TitleReGex` 对上传标题（`/api/download`）或征集名称（`/api/all-submission`）做正则匹配；`Before` 对提交时间或截止时间做过滤。

### 4.7 本地化字符串（v0.1.2）

所有页面 UI 文案集中在 [`strings/zh-CN.txt`](strings/zh-CN.txt)，格式为：

```txt
name=title lines=0 content=智慧校园
name=admin_slogan lines=2 content=
让班级的每件事，
都有迹可循
```

- `lines=0`：内容写在 `content=` 同行；
- `lines=N`：`content=` 后可为空，随后 N 行即内容（保留换行）；
- 启动时读取 `strings/` 下所有 `*.txt`，与 `strings.db` 比较并同步：新增/修改的 key 写入，文件中已删除的 key 从数据库删除；
- `strings.db` 是生成文件（已加入 `.gitignore`），每次启动自动同步，不需要手工维护；
- 页面 HTML 支持 `@[strings/key]` 标签，渲染完成后自动替换；缺失 key 会保留原标签，便于排查；
- 动态文案使用 `{}` 占位符：
  - Rust：`strings::t("key")` 返回 `String`，可直接作为 `format!` 的参数；含占位符时用 `strings::f1("key", arg)` / `f2`；
  - 前端：`S("key", arg, ...)`，按顺序替换 `{}`；`{{` / `}}` 表示字面量花括号；
- 页面渲染中的静态标签可带 HTML（如 `<br>`、`<em>`），替换时原样插入；
- 需要新语言时增加 `strings/en-US.txt`，并用 `SC_LANG=en-US` 启动即可（缺失 key 回退 `zh-CN`）。

API 不使用该字符串库：API 的错误与成功消息统一为英文。

### 4.8 数据库访问边界（v0.1.2）

- `src/db.rs` 是业务数据库的唯一访问层：**所有 SQL、事务、连接生命周期、行映射**都在这里；
  `db::Database` 封装 `Mutex<Connection>`，页面/API/工具层只调用 `db::*` 函数；
- `src/schema.sql` 只包含建表 DDL，由 `db.rs` 通过 `include_str!` 加载；查询、写入、事务不会出现在其他模块；
- `src/strings.rs` 独立负责 `strings.db` 的建表、同步与读取；
- `src/pages.rs`、`src/api.rs`、`src/tools.rs`、`src/main.rs` 不再包含 SQL 或 `rusqlite` 调用；
- 可以用下面的命令验证没有数据库逻辑泄漏：
  ```bash
  grep -RInE 'query_row|query_map|\.prepare\(|params!|rusqlite' src \
    --include='*.rs' | grep -v 'src/db.rs' | grep -v 'src/strings.rs'
  # 期望：无输出
  ```

### 4.9 问卷模式（v0.1.4）

征集类型为 `survey`（「问卷」）时，简介不再是纯文本，而是问卷定义：

```text
姓名：<input id="form_name" required/>

想参加的社团：<checkbox id="form_club" value="math"/>数学社 <checkbox id="form_club" value="art"/>美术社
接收通知：<switch id="form_notify"/>
备注：<input id="form_note" placeholder="写点什么"/>
```

- 允许的标签只有 `input`、`checkbox`、`switch`；**`id` 必填且必须以 `form_` 开头**，`required` 可选；
  `input` 还支持 `type`（`text`/`number`/`email`/`tel`/`url`/`date`/`time`）、`placeholder`、`value`，
  `checkbox` 支持 `value`（选项值）。不合法或未知标签会按普通文本转义显示，简介作者无法注入任意 HTML。
- 题目标签自动从简介文字推断：`input` 取前文（「姓名：」→「姓名」），`checkbox` / `switch` 取后文
  （「`<checkbox/>`数学社」→「数学社」），多选组的组标题取组前文本。
- 同一 `id` 的多个 `checkbox` 视为多选组（DOM id 依次为 `form_x`、`form_x__2`…，答案键仍是 `form_x`）。
- **关闭前端默认提交**：问卷模式下不再显示标题/说明/附件表单，简介被渲染成可填写控件，
  末尾是一个「提交问卷」按钮；`/submission/<id>/all` 详情页同样可以就地填写。
- 提交按钮把答案打包成 `{控件 id: 值}` 的 JSON 字符串，以 `title: "bot-upload"` 调用 `/api/upload`
  （`desc` 即该 JSON），随后跳转到提交详情页查看老师的回复。
- 答案的值约定：`input` → 输入内容；`switch` → `on` / `off`；`checkbox` → 勾选时为 `value`（缺省 `on`），
  未勾选为空字符串，多选组以 `,` 连接。服务端在 `desc` 可解析为 JSON 对象时校验 `required` 控件，
  缺少必填项返回 `400`；非 JSON 的说明文本仍可直接通过 API 提交（「仍允许 API 提交」）。
- 提交详情页会把 JSON 答案按简介顺序渲染成「题目 / 回答」列表（多选组显示选项文字、开关显示本地化的开/关），
  `/api/download` 返回的 `desc` 仍是原始 JSON 字符串，方便导出统计。

### 4.10 答题卡模式（v0.1.5，问卷派生）

征集类型为 `answer sheet`（「答题卡」）时，控件在 `id` 之外**必须**再写 `data-answer` 与 `data-score`：

```text
1 + 1 = <input id="form_q1" data-answer="2" data-score="5"/>
选出正确答案：<checkbox id="form_q2" value="a" data-answer="b" data-score="3"/>A <checkbox id="form_q2" value="b"/>B
地球是圆的：<switch id="form_q3" data-answer="on" data-score="2"/>
```

- **所有项目都必填**：渲染时统一加 `required`（多选组用 `data-survey-required` + 前端校验），`*` 标记与「全部必答」提示；
- **前端自动批改**：提交时浏览器逐题比对 `data-answer`，按 `data-score` 计分（答对得满分，答错 0 分），
  结果作为 `__grade` 对象**追加在提交内容（`desc`）末尾**：`{score, full, right, count, detail: {id: {ok, score, answer}}}`；
- 比对规则：`input` 去空白、忽略大小写，可用 `|` 给多个可接受答案；`checkbox` 多选组按集合比较（`,`/`，`/`、` 分隔）；
  `switch` 比较 `on` / `off`（也接受 `true`/`false`、`1`/`0`）；
- 提交详情页展示「自动批改结果」卡片（总分、答对题数、逐题 ✅/❌ 与得分，错题附正确答案），
  提交列表也会显示 `得分 / 满分` 标签；
- 发布时服务端校验简介（至少一项合法题目、每项都带 `id` + `data-answer` + `data-score`；多选组后续选项可省略后两者），
  不合法返回 `400`；提交时服务端要求所有题目作答；
- 已知限制：前端批改意味着正确答案随页面下发（`data-answer`），适合练习/自测，正式考试应改为服务端批改。

### 4.11 学生 / 家长视角（v0.1.6，仅前台）

家长没有独立 `auth`，一直使用孩子的学生密钥。是否显示成「家长口吻」属于**纯展示**差异，
因此做成前端视角，而不是新的角色或 API 概念：

| 视角 | 页面角色 | 文案示例 | 数据范围 |
| --- | --- | --- | --- |
| 学生视角 | `student` | 「我的上传」「我的提交」「尚未提交」 | 自己的提交（由 `auth` 决定） |
| 家长视角 | `parent` | 「孩子提交」「孩子尚未提交」 | 同一个 `auth`，范围完全相同 |

- 登录时用「家长」卡片还是「学生」卡片，就进入对应视角；选一次会被记住（写进账号偏好）。
- 班级空间顶栏有「学生视角 / 家长视角」快捷切换（只对学生身份出现）；直接改 URL
  `/home/<mob|pc>/<student|parent>/` 同样生效，同样会被记住。
- 个人主页（账号中心 `/user`）的「显示模式（学生身份）」下拉是正式配置入口，保存后：
  - 立刻套用到当前正在使用的学生身份会话；
  - 之后从账号中心「进入」任何学生身份时，按该视角打开班级空间。
- 老师 / 管理员身份没有家长视角：请求 `/home/*/parent/` 会跳回自己的空间。
- **API 不变**：`/api/*` 只认 `auth` 与其中的 `role`，没有也没有引入任何「家长」参数；
  同一份数据、同一套权限。

---

## 5. 界面

- **登录**：品牌页 + 身份卡片 + 手机/电脑视图切换 + 演示密钥一键填入。
- **首页**：按角色生成欢迎区、统计卡片、最近征集、待回复/最近提交。
- **征集**：卡片式列表、类型与截止状态标签、发布表单。
- **提交**：拖拽上传、多个附件、图片缩略图、base64 编码由浏览器完成。
- **详情**：图片灯箱、附件下载、回复时间线、快捷回复、管理员删除。
- **问卷**：简介中的输入框 / 复选框 / 开关渲染为扁平控件（开关为自绘滑块），末尾一键提交，答案自动整理成键值列表。
- **答题卡**：全部必答，提交后由浏览器自动批改，详情页显示得分卡（总分、答对题数、逐题正误与错题答案），列表显示得分标签。
- **视角**：学生身份在顶栏一键切换「学生视角 / 家长视角」，只用词不同、数据相同。
- **账号中心**：深/浅/跟随系统主题、默认视图、学生身份显示模式（学生 / 家长视角）、显示名称、密码、多身份密钥（显示/复制/改名/进入/删除）、创建班级、生成成员密钥。
- 断点：>1120px 侧边栏电脑版；≤920px 顶栏 + 底部导航手机版；≤640px 单列紧凑布局。
- 主题会记住到 `localStorage`；未登录页也支持系统深色模式。

### 5.1 结构与样式分离

- `src/view.rs`、`src/tools.rs` 生成的 HTML 只使用语义 class，**不含任何 `style=` 行内样式**；`src/assets/app.js` 也只通过 class 切换状态（如 `is-hiding`、`drag`、`loading`、`mt-sm`），不再直接写样式。
- `src/assets/app.css` 只保留必要内容：浏览器差异重置、浅色 / 深色主题变量、电脑 / 手机双端布局与扁平控件样式；不再包含阴影、渐变、模糊、动画等装饰性效果。
- 配色以 `#0010ff` 为强调色，页面主色调只做明暗深浅的变化；危险状态使用 `#ff0000`，警告状态使用 `#eedd00`。
- 深色模式使用 `<html data-theme="dark">`；没有执行脚本的页面（如纯错误页）由 `prefers-color-scheme` 兜底。
- 参考备份 `src/assets/app.css.bak` 保留了改造前的复杂实现，仅作对照，不参与构建。

---

## 6. 测试

```bash
cargo test          # 时间/编码/MatchMode/附件/改名校验/问卷解析与校验/字符串解析同步/数据库种子等单元测试
./scripts/smoke.sh  # 启动临时服务并在真实 HTTP 层跑一遍主流程（依赖 sqlite3）
```

`scripts/smoke.sh` 会在临时 SQLite 上验证字符串同步、页面无未替换标签、页面无行内样式、CSS 使用 `#0010ff` / `#ff0000` / `#eedd00`、登录、发布征集、上传、下载、匹配模式、回复、账号中心表单（注册/偏好/密码/删除身份）、账号与角色会话隔离、改名、权限拒绝、创建/删除班级与成员、删除密钥与对象，并自动清理进程与数据。

学生 / 家长视角在冒烟测试中覆盖：家长卡片登录后的家长文案、顶栏切换入口、学生视角文案互斥、
老师身份无法进入家长视角、个人主页保存显示模式后「进入」按该视角打开、以及 API 与权限不受视角影响。

答题卡模式在冒烟测试中覆盖：发布答题卡、缺少 `data-answer` 时拒绝发布、页面下发 `data-answer` / `data-score` 且全部必填、
未全部作答返回 400、`__grade` 批改结果追加在 `desc` 末尾、详情页与列表显示得分。

问卷模式在冒烟测试中覆盖：发布问卷、详情页/上传页渲染控件、必填项缺失返回 400、`bot-upload` 答案经 `/api/download` 原样返回、提交详情按题目展示答案、仍允许非 JSON 说明的 API 提交。

---

## 7. 目录结构

```
.
├── Cargo.toml
├── doc/
│   ├── requirements.md     # 原始需求
│   └── api.md              # API 参考
├── strings/
│   └── zh-CN.txt           # 简体中文 UI 文案（启动时同步到 strings.db）
├── scripts/
│   └── smoke.sh            # HTTP 冒烟测试
└── src/
    ├── main.rs             # 路由与启动
    ├── schema.sql          # SQLite DDL（由 db.rs 加载）
    ├── strings.rs          # 字符串解析、strings.db 同步、标签/格式化
    ├── survey.rs           # 问卷 / 答题卡：控件解析、渲染、必填校验、答案整理、批改结果
    ├── db.rs               # 唯一业务 SQL/事务/连接层、建班/成员、演示数据
    ├── types.rs            # 状态、模型、请求/表单
    ├── tools.rs            # 时间、base64、转义、会话、MatchMode
    ├── api.rs              # /api/* 处理（只调用 db::*，英文消息）
    ├── pages.rs            # 页面路由、账号中心动作（只调用 db::*）
    ├── view.rs             # 服务端 HTML 渲染（@[strings/key] 标签）
    ├── views/             # 页面片段模板（format! 占位符）
    └── assets/
        ├── app.css         # 重置 / 主题变量 / 双端适配 / 扁平控件样式
        └── app.js          # 上传、回复、删除、主题等交互（无行内样式）
```

---

## 8. 安全说明与已知限制

- 密码使用加盐、多轮 SHA-256；班级密钥为随机大写串。生产环境建议接入 Argon2/bcrypt、登录限流与 CSRF token，并通过内置 TLS（见第 1 节）或反向代理启用 HTTPS。
- 会话保存在内存，重启失效；多实例部署需要共享会话存储。
- 附件以 base64 存进 SQLite，单次上传限制约 18MB；大文件/高并发场景应使用对象存储与流式接口。
- 没有分页；演示数据与班级规模下足够，数据量增大后应在 `/api/download`、提交列表与账号页增加游标分页。
- `/api/new-user` 生成的密钥只返回一次，请管理员妥善保存；重新发放会创建新密钥。
- 演示密钥、演示账号仅用于本地体验，部署前请删除数据库或替换种子数据。
- `strings.db` 只保存 UI 文案，不包含用户数据；`strings/` 缺失或格式错误时服务会拒绝启动，以避免页面显示未翻译文本。

---

## 9. 原始需求对照

| 需求 | 实现 |
| --- | --- |
| `/`、`/login`、`/home` 跳转 | ✅ |
| 家长 / 老师 / 管理员登录 | ✅（另有学生路径） |
| `/api/upload`、`/api/download` | ✅（含全部 MatchMode） |
| `/api/new-submission`、`/api/all-submission` | ✅ |
| `/api/reply` | ✅ |
| `/api/new-class`、`/api/new-user` | ✅ |
| `/api/del-key`、`/api/del-object` | ✅（del-object 可只传 id） |
| `/home/*/student` 全部页面 | ✅ |
| `/home/*/teachers` 全部页面 | ✅（含 `teachers` 别名） |
| `/home/*/admin` 全部页面与删除能力 | ✅ |
| `/user` 偏好、新增班级、增减 auth | ✅ |
| v0.1.1 为某个 auth 改名 | ✅ `/api/rename-auth` + 账号中心改名入口 |
| v0.1.2 全部 UI 字符串移至 `zh-CN.txt` | ✅ `strings/zh-CN.txt` + 启动同步 `strings.db` |
| v0.1.2 API 使用英文 | ✅ API 字段/错误/消息为英文 |
| v0.1.2 自动替换 `@[strings/key]` 标签 | ✅ HTML 渲染后统一替换 |
| v0.1.2 字符串支持 `{}` 格式化 | ✅ Rust `t/f1/f2` + JS `S()` |
| 新增征集模式「问卷」：简介中渲染 `input`/`checkbox`/`switch` | ✅ `src/survey.rs`，id 必须 `form_` 开头、支持 `required` |
| 问卷：关闭前端默认提交，末尾新增提交按钮 | ✅ 上传页/详情页改为问卷表单，末尾「提交问卷」 |
| 问卷：按 `title=bot-upload` + `desc={id:value}` 自动 bot 提交 | ✅ `app.js` 收集 `{id: value}` 后调用 `/api/upload` |
| 问卷：仍允许 API 提交 | ✅ `/api/upload` 不限制标题与格式，仅对 JSON 答案校验必填项 |
| 答题卡：表单项目必须包含 `id` / `data-answer` / `data-score` | ✅ 解析时强制，缺项退化为文本，发布时返回 `400` |
| 答题卡：所有项目都必填 | ✅ 渲染统一 `required`，前端补齐多选组校验，服务端校验全部题目 |
| 答题卡：提交后由前端自动批改，正误与分数添加在提交末尾 | ✅ `app.js` 计算 `__grade` 追加在 `desc` 末尾，详情页/列表展示得分 |
| 前端区分学生 / 家长身份，API 保持不变 | ✅ 只切换 `page_role` 与文案；`/api/*` 仍只按 `auth` 鉴权 |
| 个人主页配置学生模式 / 家长模式显示 | ✅ `/user` 的「显示模式（学生身份）」，并立即套用 + 决定「进入」时的视角 |
| 角色登录不反查 user；未登录账号中心跳登录页 | ✅ `sc_sid` / `sc_uid` 分离，`/user` → `/login/account` |
| 「以班级为单位 / 学生以自身为单位」 | ✅ 服务端数据范围隔离 |

祝使用愉快。🎓
