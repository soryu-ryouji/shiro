// 应用组装：启动状态端点、路由注册、鉴权/CORS/静态资源层、OpenAPI 契约测试。
// 路由来源：各 feature 的 router() 在此合并，是唯一的组装点。

use crate::auth::auth;
use crate::state::AppState;
use axum::{Json, Router};
use serde::Serialize;
use std::path::PathBuf;
use utoipa::{Modify, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(Serialize, ToSchema, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)] // Starting/Error 为后续初始化流程预留
pub enum StartupStatus {
    Starting,
    Ready,
    Error,
}

#[derive(Serialize, ToSchema)]
pub struct StartupResponse {
    pub status: StartupStatus,
}

/// 启动状态。当前无后台初始化流程，直接返回 ready；
/// 后续加入初始化流程后按 starting → ready/error 流转。
#[utoipa::path(
    post,
    path = "/api/v1/app/startup",
    tag = "app",
    responses(
        (status = 200, description = "启动状态", body = StartupResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn startup() -> Json<StartupResponse> {
    Json(StartupResponse {
        status: StartupStatus::Ready,
    })
}

async fn health() -> &'static str {
    "ok"
}

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_token",
            utoipa::openapi::security::SecurityScheme::Http(
                utoipa::openapi::security::HttpBuilder::new()
                    .scheme(utoipa::openapi::security::HttpAuthScheme::Bearer)
                    .build(),
            ),
        );
    }
}

fn apply_info(doc: &mut utoipa::openapi::OpenApi) {
    doc.info.title = "shiro API".into();
    doc.info.version = env!("CARGO_PKG_VERSION").into();
}

/// 构建 API 路由与 OpenAPI 文档（同一来源：路由即文档，契约测试校验 openapi.json 同步）。
pub fn build_router(
    state: AppState,
    serve_folder: Option<PathBuf>,
) -> (Router, utoipa::openapi::OpenApi) {
    let (api_router, mut doc) = OpenApiRouter::new()
        .routes(routes!(startup))
        .merge(crate::features::projects::api::router())
        .merge(crate::features::characters::api::router())
        .merge(crate::features::models::api::router())
        .merge(crate::features::chat::api::router())
        .split_for_parts();
    SecurityAddon.modify(&mut doc);
    apply_info(&mut doc);

    let api_router = api_router.layer(axum::middleware::from_fn_with_state(state.clone(), auth));

    let mut router = Router::new()
        .merge(api_router)
        .route("/health", axum::routing::get(health))
        .with_state(state)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(tower_http::cors::CorsLayer::permissive());

    // 局域网访问形态：daemon 直接 serve 前端静态资源，SPA 回退到 index.html
    if let Some(folder) = serve_folder
        && folder.is_dir()
    {
        let index = tower_http::services::ServeFile::new(folder.join("index.html"));
        router = router
            .fallback_service(tower_http::services::ServeDir::new(folder).not_found_service(index));
    }

    (router, doc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch;

    /// 契约测试：固化的 openapi.json 必须与代码生成一致。
    /// 改 API 后执行 `cargo run -- --dump-openapi > openapi.json` 重新固化。
    #[test]
    fn openapi_json_in_sync() {
        let (_, doc) = build_router(
            AppState {
                token: String::new(),
                watch_hub: watch::WatchHub::default(),
                chat: Default::default(),
            },
            None,
        );
        let generated = serde_json::to_string_pretty(&doc).unwrap();
        let frozen = include_str!("../openapi.json");
        assert_eq!(
            generated.trim(),
            frozen.trim(),
            "openapi.json 与代码不同步，请执行 cargo run -- --dump-openapi > openapi.json"
        );
    }
}
