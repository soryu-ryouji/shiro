# Architecture

## 核心原则

**1. 前端只认识 HTTP API**

Web 前端不依赖 Electron IPC，只通过 REST API 通信。

**2. 后端不依赖 Electron**

后端是独立的 Rust 二进制，不依赖任何桌面端代码。

**3. API 契约先行**

REST API 由代码生成 OpenAPI schema（utoipa：`#[utoipa::path]` 标注端点、DTO derive `ToSchema`，路由与文档同一来源），固化于 `shiro-daemon/openapi.json`（改 API 后 `cargo run -- --dump-openapi > openapi.json` 重新固化，契约测试校验同步），TypeScript 类型从 schema 生成（`npm run gen:types`），前后端不允许手写对接口。

**4. 编辑计算归 app，存储与管理归 daemon**

**5. 模型接入归 daemon**

模型接入层在 shiro-daemon 内实现：

- 统一 provider 抽象，各模型（OpenAI 兼容 API、Anthropic 等）各自实现
- 密钥与 provider 配置存 `~/.config/shiro/config.toml`
- 前端通过 HTTP API 调用，流式输出走 SSE

## 仓库结构

```text
shiro/
├── shiro-daemon/             ← Rust 后端（桌面版与服务器版共用）
├── shiro-app/                ← 桌面应用
├── tools/                    ← 仓库级脚本（构建/安装/冒烟/性能压测）
└── docs/                     ← 设计文档
```

## 文档

- [系统存储](./storage.md)
