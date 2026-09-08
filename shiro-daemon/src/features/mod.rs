//! 业务功能模块：每个 feature 自带 HTTP 端点（api）、领域逻辑与存储，通过 router() 暴露路由。

pub(crate) mod characters;
pub(crate) mod chat;
pub(crate) mod models;
pub(crate) mod projects;
