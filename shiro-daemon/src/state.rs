// 应用状态：鉴权 token 与各基础设施 Hub，随 Router 注入 handler。
// 单独成模块（而非放在 app.rs），避免 features 依赖入口组装模块。

use crate::watch;

#[derive(Clone)]
pub struct AppState {
    pub token: String,
    pub watch_hub: watch::WatchHub,
    /// Chat 生成守卫（同一会话并发限制）
    pub chat: std::sync::Arc<crate::chat::Hub>,
}
