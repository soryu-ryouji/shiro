<p align="center">
  <img src=".assets/README.assets/shiro_bold.svg" width="128" alt="hawk logo">
</p>

<h1 align="center">shiro</h1>

> shiro (白写) 是一个面向剧本创作的 AI 项目

## 开发

```bash
# 桌面应用开发模式（自动构建 daemon 与主进程，起 vite + electron）
cd shiro-app && npm install && npm run dev

# 后端：改 API 后重新固化契约并跑契约测试
cd shiro-daemon && cargo run -- --dump-openapi > openapi.json && cargo test

# 前端类型从契约生成
cd shiro-app && npm run gen:types
```

## 构建与安装

```powershell
# Windows：本机安装到 out/（shiro.exe 免安装就地运行）
./tools/install.ps1 [-Path <目录>]
# Windows：分发包 out/shiro-windows-x64.zip
./tools/build.ps1
```

```bash
# macOS / Linux：本机安装（mac → /Applications/shiro.app，Linux → out/*.AppImage）
./tools/install.sh
# macOS / Linux：分发包归置到 out/
./tools/build.sh
```

前置：Node.js 与 Rust 工具链。

## 文档

**总体**

- [架构设计](docs/architecture.md)：总体架构、核心原则、部署形态（桌面版/局域网访问/服务器版）、仓库结构
- [项目结构总览](docs/structure.md)：仓库布局、前后端分层与模块职责、REST API 清单
- [技术栈](docs/tech-stack.md)：语言与框架选型
- [AI写作工具制作思考](./docs/AI写作工具制作思考.md): 制作思考说明
- [资产格式](docs/asset-format.md)：资产文件的 frontmatter、区块与命名约定

**后端**

- [存储设计](docs/backend/storage.md)：真实文件夹项目、明文内容库、同步边界
- [模型接入设计](docs/backend/model-access.md)：provider 抽象、配置、能力场景

**前端**

- [正文编辑器](docs/frontend/editor.md)：实时预览、表格编辑、底部工具栏、自动保存
- [Chat](docs/frontend/chat.md)：对话式修改项目文档（提案 → 确认 → 写盘）
