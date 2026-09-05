# 技术栈

## 总览

```text
后端：Rust
前端：Vue 3 + TypeScript (Vite)，只走 HTTP
桌面壳：Electron，sidecar 拉起后端
契约：OpenAPI
```

## 后端：shiro-daemon/

Rust（axum + tokio），见[存储设计](backend/storage.md)与[模型接入设计](backend/model-access.md)。承担：

- 剧本项目、写作规范、套路库、人物库的存储与管理
- 模型接入：provider 抽象，调用上游 LLM API（reqwest），SSE 流式转发
- 前端静态资源 serve（局域网访问）

## 前端与桌面壳：shiro-app/

Electron 壳 + Vue 3 前端。约束：

- 前端只通过 REST API 与后端通信，不依赖 Electron IPC
- Electron 主进程只负责：创建窗口、拉起/回收后端进程、注入 token
- electron-builder 打包，`extraResources` 携带各平台后端二进制（Windows/macOS/Linux）

编辑器组件尚未定型，候选 CodeMirror 6（大文档虚拟渲染、移动端触屏支持），选型时以这两点优先。

## 契约：OpenAPI

- OpenAPI schema 由后端代码生成（utoipa：`#[utoipa::path]` + `ToSchema` derive，路由即文档），固化于 `shiro-daemon/openapi.json`（`cargo run -- --dump-openapi` 重新生成）
- 固化文件与代码的同步由契约测试保证；同时校验端点真实响应符合 schema
- TypeScript 类型从 schema 生成（openapi-typescript，`npm run gen:types`）
