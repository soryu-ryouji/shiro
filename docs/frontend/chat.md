# Chat（对话式修改项目文档）

导航第二项「Chat」：选择已导入的项目，通过对话让 AI 修改项目文档。核心原则是**草稿 → 确认**：AI 产物不直接落盘，以提案（proposal）形式呈现，用户确认后才写入文件。

## 交互

- 左列：项目下拉（默认选中写作页正在打开的项目）+ 会话列表（新建 / 删除）
- 右列：消息流 + 输入区
  - 输入框可「引用文件」（SearchSelect 搜索项目内文稿，可多个）；引用内容只在 daemon 组装上下文时注入，界面只显示文件名
  - Enter 发送，Shift+Enter 换行；生成中可「停止」（断开 SSE，daemon 检测断开后中止，不保存部分回复）
  - 助手回复中解析出的提案渲染为卡片：文件名 + 内容预览 + 「应用」按钮；应用后整文件覆盖写盘（父目录自动创建），文件监听自动推送前端刷新
- 模型使用 Model 页的「默认供应商」档案；未配置时发消息直接报错引导

## 提案格式约定

模型在回复中用 `shiro-edit` 代码块给出提案，每个提案一个块：

    ```shiro-edit
    {"file": "正文/第一卷/001.md", "content": "修改后的完整文件内容"}
    ```

- `file` 是项目内相对路径；`content` 是修改后的**完整文件内容**（整文件覆盖，不是增量 diff）
- 一条回复可含多个提案；解析失败或路径非法的块跳过，不影响正文展示
- 消息原文保留提案块（后续轮次模型可见自己给过的提案），前端渲染时剥离、以卡片呈现

## 存储

- 会话存 `<项目>/.shiro/chat/<id>.json`：随项目文件夹迁移、可进版本管理
- 结构：`{ id, title, created_at, updated_at, next_proposal_seq, messages }`；消息 `{ role, content, at, attachments?, proposals? }`；提案 `{ id, file, content, applied, applied_at? }`
- 会话标题缺省「新会话」，首条用户消息前 20 字自动回填

## 上下文组装（daemon 侧）

1. system：助手身份 + 项目目录树（文稿与目录，排除 `.` 开头项）+ 提案格式约定
2. 会话历史：最近 20 条（原文，含提案块）
3. 本条用户消息：引用文件内容前置注入（单文件截断 30k 字符）

V1 无 token 预算制，靠上述截断上限控制；预算制与摘要链见 [AI写作工具制作思考](../AI写作工具制作思考.md) 的 push 上下文设计。

## API（tag `chat`，见 `shiro-daemon/src/chat/api.rs`）

全部为 `POST` + JSON body（约定见 [项目结构总览](../structure.md#6-api统一-post)）：

| 路径 | 请求 | 说明 |
| ---- | ---- | ---- |
| `POST /api/v1/chat/sessions/list` | `{path}` | 会话列表（最近更新在前） |
| `POST /api/v1/chat/sessions/create` | `{path, title?}` | 新建会话 |
| `POST /api/v1/chat/sessions/get` | `{path, id}` | 会话详情（全部消息） |
| `POST /api/v1/chat/sessions/delete` | `{path, id}` | 删除会话 |
| `POST /api/v1/chat/sessions/messages` | `{path, id, content, attachments?}` | 发消息，SSE 流式响应（delta / thinking / done / error 帧）；客户端断开即中止；同一会话同时只允许一个生成（409） |
| `POST /api/v1/chat/sessions/apply` | `{path, id, proposal_id}` | 应用提案，整文件覆盖写盘 |

## 已知边界（V1）

- 无 diff 预览：提案是整文件内容，`details` 展开看全文
- 应用提案不做冲突检测（多端同时编辑同一文件时后写覆盖）；文件夹是唯一权威数据源，确认前自行核对
- 历史全量重发（窗口 20 条），长会话 token 消耗线性增长；摘要链后续版本接入
