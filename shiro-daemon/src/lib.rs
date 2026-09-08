//! shiro-daemon 库入口：模块声明与集成测试可用的公开面。
//! 二进制入口在 main.rs（CLI + 启动），组装逻辑在 app.rs。

pub mod app;
pub mod auth;
pub mod error;
pub mod features;
pub mod infra;
pub mod llm;
pub mod state;
pub mod watch;
