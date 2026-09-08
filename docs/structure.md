# 项目结构总览

面向开发者的代码地图：仓库布局、前后端分层与依赖方向、API 清单（统一 POST）、关键数据流。
总体架构与部署形态见 [架构设计](./architecture.md)，技术选型见 [技术栈](./tech-stack.md)。

## 1. 仓库布局

```text
shiro/
├── shiro-daemon/                  ← Rust 后端（axum + tokio，独立二进制）
│   ├── src/
│   │   ├── main.rs                CLI 入口与启动（薄壳）
│   │   ├── lib.rs                 模块声明（集成测试可 import）
│   │   ├── app.rs                 路由组装 + startup/health + OpenAPI 契约测试
│   │   ├── state.rs               AppState（token / WatchHub / Chat Hub）
│   │   ├── error.rs               统一错误类型与构造（ApiError / ErrorResponse）
│   │   ├── auth.rs                鉴权中间件
│   │   ├── infra/                 基础设施：paths / fs（原子写）/ text
│   │   ├── features/              业务域（api + 领域逻辑 + 存储）
│   │   │   ├── projects/          登记（history.toml）+ 目录树/预览/文稿/目录
│   │   │   ├── characters/        全局人物库角色卡（扫描/解析/保存）
│   │   │   ├── models/            模型档案（config.toml [llm]）
│   │   │   └── chat/              会话 + 上下文 + 提案 + SSE 端点
│   │   ├── llm/                   provider 层：config / protocol / json
│   │   └── watch.rs               项目目录文件监听（notify + 防抖 + 广播 Hub）
│   ├── tests/api.rs               路由/鉴权集成测试
│   └── openapi.json               固化契约（契约测试校验与代码同步）
│
├── shiro-app/                     ← Electron 壳 + Vue 3 前端
│   ├── electron/                  壳层：主进程/窗口/IPC（业务数据不过 IPC）
│   ├── src/
│   │   ├── App.vue                应用布局：侧栏 + 顶栏 + 内容区（按导航切换视图）
│   │   ├── views/                 视图层：Project / Chat / Database / Model
│   │   ├── panels/                侧栏面板：ProjectPanel / DatabasePanel / ModelPanel
│   │   ├── components/            组件层：编辑器、列表、对话框、通用控件
│   │   ├── stores/                状态层：project / chat / database / model
│   │   ├── api.ts                 连接参数 + fetch 封装
│   │   ├── api-types.d.ts         契约类型（生成，不手改）
│   │   ├── platform.ts            Electron shell 能力收敛（浏览器形态 no-op）
│   │   └── utils/                 编辑器/预览/表格/字数/字体/滚动条等纯前端逻辑
│   └── tools/                     前端自检与测试脚本
│
├── tools/                         仓库级脚本（构建/安装/打包）
└── docs/                          设计文档（本文件为结构总览）
```

## 2. 进程与分层总图

```text
┌──────────────────── 渲染进程：shiro-app（Vue 3 + TS）─────────────────────┐
│  视图层      views/          页面组合与业务编排                             │
│  面板层      panels/         侧栏内容（随 Activity 切换）                   │
│  组件层      components/     编辑器与通用控件                               │
│  状态层      stores/         前端状态 + API 调用                            │
│  基础层      api.ts · platform.ts · utils/                                 │
└────────────────────────────────────┬──────────────────────────────────────┘
                                     │ HTTP：POST + JSON（SSE 流式）· Bearer token
┌────────────────────────────────────▼──────────────────────────────────────┐
│                        shiro-daemon（Rust，独立进程）                       │
│  入口      main.rs / lib.rs / app.rs（组装）· state.rs · auth.rs · error.rs  │
│  业务      features/：projects · characters · models · chat                 │
│  基础      infra/（paths/fs/text）· llm/（provider）· watch.rs              │
│  存储      项目文件夹（正文 + .shiro/）· ~/.config/shiro/（config.toml 等）    │
└────────────────────────────────────────────────────────────────────────────┘
        ▲
        │ IPC 仅壳功能（窗口控制/目录选择/文件管理器），业务数据一律走 HTTP
┌───────┴────────────┐
│ electron/ 壳层      │  main.ts 拉起 daemon（随机端口 + token）→ 加载前端
└────────────────────┘
```

## 3. 后端分层（shiro-daemon）

### 3.1 模块职责

| 层 | 模块 | 职责 |
| -- | ---- | ---- |
| 入口 | `main.rs` / `lib.rs` | CLI 解析与启动；lib 声明模块（集成测试可 import） |
| 组装 | `app.rs` | 合并各 feature 的 `router()`，挂鉴权/CORS/TraceLayer/静态资源，startup/health，OpenAPI 契约测试 |
| 状态 | `state.rs` | `AppState`（token / WatchHub / Chat Hub），features 依赖它而非入口 |
| 横切 | `error.rs` / `auth.rs` | 统一错误类型与构造；Bearer 鉴权中间件 |
| 基础 | `infra/` | `paths`（config 目录/路径安全/名称校验）、`fs`（原子写/回收站/文稿判定）、`text`（预览提取） |
| 业务 | `features/projects/` | `api`（handler/DTO）+ `files`（树/预览/文稿/目录）+ `store`（登记与 history.toml） |
| 业务 | `features/characters/` | `api` + `store`（角色卡扫描/解析/保存，简卡单文件 + 深卡目录） |
| 业务 | `features/models/` | `api` + `store`（档案校验/保存/连接测试，key 只回掩码） |
| 业务 | `features/chat/` | `mod`（会话持久化/上下文组装/提案解析/运行互斥）+ `api`（CRUD + SSE） |
| 基础 | `llm/` | `config`（档案读写/并发上限/掩码）、`protocol`（双协议请求与流式解析/退避）、`json`（容错解析） |
| 基础 | `watch.rs` | notify 递归监听 + 300ms 防抖，按项目广播变更集；订阅引用计数 |

### 3.2 依赖方向

```text
main.rs / tests
  └─→ app.rs（唯一组装点）
        └─→ features/*/api ─→ features/*/{store,files,mod} ─→ infra/*
                                     │                        └─→ error.rs
                                     └─→ llm/ · watch.rs
```

约定：依赖单向指向内部——`features` 可以依赖 `infra` / `llm` / `error`，反向不允许；HTTP handler 只做参数解析与响应组装，领域逻辑在 feature 的 `store`/`files`/`mod` 中；共享状态经 `state.rs` 注入，不依赖入口模块。

## 4. 前端分层（shiro-app）

### 4.1 模块职责

| 层 | 模块 | 职责 |
| -- | ---- | ---- |
| 壳 | `electron/main.ts` | 窗口创建、预选端口 + 随机 token、spawn/回收 daemon、加载前端 |
| 壳 | `electron/ipc.ts` · `preload.ts` · `ipc-contract.ts` | 壳功能 IPC（窗口控制、目录选择、文件管理器定位）；契约单一定义、白名单暴露 |
| 入口 | `App.vue` | 导航（Project / Chat / Database / Model）、启动轮询、侧栏显隐与栏宽、全局设置弹窗 |
| 视图 | `views/ProjectView.vue` | 写作模式：文稿列表 + 标签栏 + 编辑器 + 预览 |
| 视图 | `views/ChatView.vue` | 项目/会话选择、消息流、引用文件、提案卡片与应用 |
| 视图 | `views/DatabaseView.vue` | 人物库：列表 + 筛选 + 详情（渲染/编辑） |
| 视图 | `views/ModelView.vue` | 模型档案注册与管理、默认档案切换、连接测试 |
| 面板 | `panels/ProjectPanel.vue` | 项目列表/目录树、新建/重命名/删除入口 |
| 面板 | `panels/DatabasePanel.vue` · `ModelPanel.vue` | 各模块侧栏导航与列表 |
| 组件 | `components/` | `Editor`（CodeMirror）、`EditorTabs`、`SheetList`、`TreeNode`、对话框（Preview/Prompt/Confirm/NewProject/Settings）、`SearchSelect`、`ContextMenu`、`Icon`、布局件（Sidebar/ActivityBar/TitleBar/WindowControls） |
| 状态 | `stores/project.ts` | 当前项目、目录树、标签、文稿内容、目录监听订阅（fetch 流式 + 重连） |
| 状态 | `stores/chat.ts` | 项目/会话列表、消息流（fetch 流式解析 SSE）、提案应用 |
| 状态 | `stores/database.ts` | 人物库列表/详情/编辑 |
| 状态 | `stores/model.ts` | 模型档案列表/默认项 |
| 基础 | `api.ts` · `platform.ts` · `utils/` | 连接参数与 fetch；shell 能力收敛；编辑器/预览/表格/字数/字体等纯逻辑 |

`components/LogDialog.vue`（LLM 调用日志回放）当前未接线，保留备用。

### 4.2 依赖方向

```text
main.ts → App.vue ─→ Sidebar ─→ panels/
              └────→ views/ ─→ components/（展示与交互）
                       └────→ stores/ ─→ api.ts（apiPost）─→ daemon API
                                  └────→ utils/fileWatch（POST + fetch 流式订阅）
```

约定：业务请求集中在 stores（`project` / `chat` / `database` / `model`）；项目文稿相关的轻量操作（保存、新建、删除、目录树）存在组件内直调 `apiFetch` 的情况（Editor / SheetList / ProjectPanel / NewProjectDialog），不强行绕一层 store。

## 5. 契约链路

```text
Rust 代码（#[utoipa::path] + ToSchema）
   │  cargo run -- --dump-openapi > openapi.json
   ▼
shiro-daemon/openapi.json（固化契约）
   │  npm run gen:types
   ▼
shiro-app/src/api-types.d.ts（TS 类型，不手改）
```

- 契约测试 `api::tests::openapi_json_in_sync` 校验固化文件与代码一致
- 改 API 的固定动作：重新 dump → `gen:types` → `cargo test` → `typecheck`

## 6. API（统一 POST）

### 约定

- **全部 `/api/v1/*` 统一 POST + JSON body**（含读操作与 path 参数，避免路径进 URL）；唯一例外是 `GET /health` 与静态资源（基础设施探针）
- 鉴权一律走 `Authorization: Bearer <token>`（SSE 也用 header，前端以 fetch 流式消费，非 EventSource）
- 桌面版 token 由 Electron 启动时随机生成、env 传入不落盘；局域网 key 存 `~/.config/shiro/config.toml`
- 项目路径为绝对路径，文件路径为项目内相对路径（daemon 侧拒绝 `..` 逃逸）
- **项目内容端点只允许访问已登记项目**（history.toml，路径 canonicalize 后比对）：未登记返回 403。tree / excerpts / file / folder / entry / watch / chat 均适用；`projects/create` 是唯一的登记入口
- 目录统一用 `folder` 命名：API 路径/字段、Rust 自有标识符、前端状态与组件、IPC 通道；仅 std/第三方 API（`read_dir`、`ServeDir` 等）保持原样
- 方法不再承载语义，端点即操作：路径用动词后缀区分同资源的不同操作（`list` / `create` / `read` / `write` / `delete` / `save`）

### app

| 路径 | 请求 | 说明 |
| ---- | ---- | ---- |
| `POST /api/v1/app/startup` | `{}` | 启动状态（当前直接 ready，为初始化流程预留） |

### projects（项目与文稿）

| 路径 | 请求 | 说明 |
| ---- | ---- | ---- |
| `POST /api/v1/projects/list` | `{}` | 项目列表（history.toml，最近打开在前） |
| `POST /api/v1/projects/create` | `{path, name?}` | 登记已有文件夹为项目（VSCode 打开文件夹式） |
| `POST /api/v1/projects/remove` | `{path}` | 移除项目记录（不删文件夹） |
| `POST /api/v1/projects/rename` | `{path, new_name}` | 重命名项目文件夹本体 |
| `POST /api/v1/projects/tree` | `{path}` | 目录树（目录 + `.md/.markdown/.txt`） |
| `POST /api/v1/projects/excerpts` | `{path, folder}` | 目录内文稿正文预览（列表摘要用） |
| `POST /api/v1/projects/file/read` | `{path, file}` | 读取文稿 |
| `POST /api/v1/projects/file/write` | `{path, file, content}` | 保存文稿（临时文件 + rename 原子覆盖） |
| `POST /api/v1/projects/file/create` | `{path, file}` | 新建文稿 |
| `POST /api/v1/projects/file/delete` | `{path, file}` | 删除文稿（移入 `.shiro/trash/`） |
| `POST /api/v1/projects/folder/create` | `{path, folder}` | 新建目录（幂等） |
| `POST /api/v1/projects/folder/delete` | `{path, folder}` | 删除目录（空目录直删，非空入回收站） |
| `POST /api/v1/projects/entry/rename` | `{path, rel, new_name}` | 重命名文件/目录 |
| `POST /api/v1/projects/watch` | `{path}` | **SSE** 目录变动推送（防抖 300ms，`{"changed":[...]}`；客户端 fetch 流式消费 + 自动重连） |

### chat（对话式修改项目文档）

| 路径 | 请求 | 说明 |
| ---- | ---- | ---- |
| `POST /api/v1/chat/sessions/list` | `{path}` | 会话列表 |
| `POST /api/v1/chat/sessions/create` | `{path, title?}` | 新建会话 |
| `POST /api/v1/chat/sessions/get` | `{path, id}` | 会话详情（全部消息） |
| `POST /api/v1/chat/sessions/delete` | `{path, id}` | 删除会话 |
| `POST /api/v1/chat/sessions/messages` | `{path, id, content, attachments?}` | **SSE** 发消息流式生成：`delta` / `thinking` / `done`（含提案）/ `error`；断开即中止；同会话互斥（409） |
| `POST /api/v1/chat/sessions/apply` | `{path, id, proposal_id}` | 应用提案：整文件覆盖写盘 |

详见 [Chat 设计](./frontend/chat.md)。

### db（全局人物库）

| 路径 | 请求 | 说明 |
| ---- | ---- | ---- |
| `POST /api/v1/db/characters/list` | `{}` | 角色列表 |
| `POST /api/v1/db/characters/create` | `{name}` | 新建角色卡骨架（同名自动追加 `-2/-3`） |
| `POST /api/v1/db/characters/get` | `{id}` | 角色详情（单文件简卡 / 目录深卡） |
| `POST /api/v1/db/characters/save` | `{id, content}` | 保存角色卡（原子覆盖；解析失败也保存并返回提示） |

### model（模型档案）

| 路径 | 请求 | 说明 |
| ---- | ---- | ---- |
| `POST /api/v1/model/profiles/list` | `{}` | 档案列表（含默认档案） |
| `POST /api/v1/model/profiles/save` | `{key, base_url, model, protocol?, thinking?, api_key?}` | 注册/更新档案（同 key 覆盖；首个自动为默认） |
| `POST /api/v1/model/profiles/delete` | `{key}` | 删除档案（默认项自动回退） |
| `POST /api/v1/model/profiles/test` | `{key}` | 测试连接（最小真实调用 ping） |

### 其它

| 路径 | 说明 |
| ---- | ---- |
| `GET /health` | 存活探测（无鉴权） |
| `GET /{任意路径}` | `--serve-folder` 存在时挂载前端静态资源，SPA 回退 `index.html`（无鉴权） |

冒烟：`./tools/smoke-api.sh`（临时 HOME 隔离，逐端点校验，不触发真实 LLM 调用）。

## 7. 关键数据流

**编辑保存与外部变更同步**

```text
Editor 输入 → 防抖 → projectStore 保存（POST /projects/file/write）
  → daemon 临时文件 + rename 原子写
  → notify 防抖 300ms → SSE /projects/watch 推送（POST + fetch 流式，断线自动重连）
  → 前端刷新目录树；当前文稿被外部修改时重载编辑器
```

**Chat 生成与提案应用**

```text
ChatView 发消息 → POST /chat/sessions/messages（fetch 流式读 SSE）
  → chat/mod 组装上下文（system + 目录树 + 最近 20 条历史 + 引用文件）
  → llm::chat_stream（重试/双协议）→ delta/thinking 帧转发前端
  → 流结束：解析 shiro-edit 提案 → 助手消息落盘（.shiro/chat/<id>.json）→ done 帧
  → 用户点「应用」→ POST /chat/sessions/apply → 整文件覆盖写盘 → 文件监听 SSE 通知前端刷新
```

## 8. 构建、测试与发布入口

| 目的 | 命令 |
| ---- | ---- |
| 桌面开发模式 | `cd shiro-app && npm run dev`（构建 daemon + 主进程，起 vite + electron） |
| 后端测试 + 契约固化 | `cd shiro-daemon && cargo test`；改 API 后 `cargo run -- --dump-openapi > openapi.json` |
| 前端类型 + 构建 | `cd shiro-app && npm run gen:types && npm run build` |
| 前端自检 | `cd shiro-app && npm run test:ui`（Electron 会话） |
| 打包分发 | `./tools/build.sh`（macOS/Linux）· `./tools/build.ps1`（Windows） |
