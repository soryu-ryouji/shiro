use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
    Json, Router,
};
use serde::Serialize;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(Clone)]
pub struct AppState {
    pub token: String,
}

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
    get,
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

async fn auth(State(state): State<AppState>, req: Request, next: Next) -> Result<Response, StatusCode> {
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match token {
        Some(t) if t == state.token => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

#[derive(OpenApi)]
#[openapi(
    info(title = "shiro API", version = env!("CARGO_PKG_VERSION")),
    components(schemas(StartupResponse, StartupStatus)),
    modifiers(&SecurityAddon)
)]
struct ApiDoc;

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

/// 构建 API 路由与 OpenAPI 文档（同一来源，契约测试校验 openapi.json 同步）。
pub fn build_router(state: AppState, serve_dir: Option<PathBuf>) -> (Router, utoipa::openapi::OpenApi) {
    let (api_router, api) = OpenApiRouter::new().routes(routes!(startup)).split_for_parts();
    // ApiDoc 与 OpenApiRouter 各自声明；以 ApiDoc 为准输出 schema（含 security scheme）。
    let _ = &api;
    let doc = ApiDoc::openapi();

    let api_router = api_router
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth));

    let mut router = Router::new()
        .merge(api_router)
        .route("/health", axum::routing::get(health))
        .with_state(state)
        .layer(tower_http::cors::CorsLayer::permissive());

    // 局域网访问形态：daemon 直接 serve 前端静态资源，SPA 回退到 index.html
    if let Some(dir) = serve_dir {
        if dir.is_dir() {
            let index = ServeFile::new(dir.join("index.html"));
            router = router.fallback_service(ServeDir::new(dir).not_found_service(index));
        }
    }

    (router, doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约测试：固化的 openapi.json 必须与代码生成一致。
    /// 改 API 后执行 `cargo run -- --dump-openapi > openapi.json` 重新固化。
    #[test]
    fn openapi_json_in_sync() {
        let doc = ApiDoc::openapi();
        let generated = serde_json::to_string_pretty(&doc).unwrap();
        let frozen = include_str!("../openapi.json");
        assert_eq!(
            generated.trim(),
            frozen.trim(),
            "openapi.json 与代码不同步，请执行 cargo run -- --dump-openapi > openapi.json"
        );
    }
}
