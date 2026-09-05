use axum::{
    extract::{Query, Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tower_http::services::{ServeDir, ServeFile};
use utoipa::{IntoParams, OpenApi, ToSchema};
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

// ---- 项目管理（打开记录存 ~/.config/shiro/history.toml，见 docs/backend/storage.md） ----

#[derive(Serialize, Deserialize, Default)]
struct History {
    #[serde(default)]
    projects: Vec<HistoryEntry>,
}

#[derive(Serialize, Deserialize)]
struct HistoryEntry {
    path: String,
    /// 最近打开时间（epoch 秒）
    opened_at: u64,
}

fn config_dir() -> PathBuf {
    std::env::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("shiro")
}

fn load_history() -> History {
    let Ok(text) = std::fs::read_to_string(config_dir().join("history.toml")) else {
        return History::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

fn save_history(history: &History) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir())?;
    let text = toml::to_string_pretty(history).map_err(std::io::Error::other)?;
    std::fs::write(config_dir().join("history.toml"), text)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 路径显示名：目录 basename（容忍尾部斜杠）
fn dir_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    Path::new(trimmed)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| trimmed.to_string())
}

#[derive(Serialize, ToSchema)]
pub struct ProjectItem {
    /// 项目文件夹绝对路径
    pub path: String,
    /// 显示名（目录 basename）
    pub name: String,
    /// 目录当前是否存在（已移动/删除的历史项返回 false，前端置灰）
    pub exists: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectListResponse {
    pub projects: Vec<ProjectItem>,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub message: String,
}

type ApiError = (StatusCode, Json<ErrorResponse>);

fn bad_request(message: &str) -> ApiError {
    (StatusCode::BAD_REQUEST, Json(ErrorResponse { message: message.into() }))
}

fn internal_error(e: std::io::Error) -> ApiError {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { message: e.to_string() }))
}

/// 项目列表（history.toml 记录，最近打开在前）
#[utoipa::path(
    get,
    path = "/api/v1/projects",
    tag = "projects",
    responses(
        (status = 200, description = "项目列表", body = ProjectListResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn list_projects() -> Json<ProjectListResponse> {
    let history = load_history();
    let projects = history
        .projects
        .iter()
        .map(|e| ProjectItem {
            name: dir_name(&e.path),
            path: e.path.clone(),
            exists: Path::new(&e.path).is_dir(),
        })
        .collect();
    Json(ProjectListResponse { projects })
}

#[derive(Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    /// 父目录绝对路径（须已存在）
    pub parent: String,
    /// 项目名（即新建的子目录名）
    pub name: String,
}

/// 新建项目：在父目录下创建项目文件夹（初始化 .shiro/ 与 正文/），并导入记录。
/// 目录已存在时：空目录或已是 shiro 项目（含 .shiro/）则直接导入，否则 409。
#[utoipa::path(
    post,
    path = "/api/v1/projects",
    tag = "projects",
    request_body = CreateProjectRequest,
    responses(
        (status = 201, description = "已创建", body = ProjectItem),
        (status = 400, description = "参数错误", body = ErrorResponse),
        (status = 409, description = "目录已存在且非空", body = ErrorResponse),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
async fn create_project(Json(req): Json<CreateProjectRequest>) -> Result<(StatusCode, Json<ProjectItem>), ApiError> {
    let name = req.name.trim();
    const INVALID: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    if name.is_empty() || name == "." || name == ".." || name.chars().any(|c| INVALID.contains(&c)) {
        return Err(bad_request("项目名不能为空，且不能包含 \\ / : * ? \" < > | 字符"));
    }
    let parent = PathBuf::from(req.parent.trim());
    if !parent.is_dir() {
        return Err(bad_request("父目录不存在"));
    }

    let target = parent.join(name);
    if target.exists() {
        let is_empty = target.read_dir().map(|mut d| d.next().is_none()).unwrap_or(false);
        let is_shiro_project = target.join(".shiro").is_dir();
        if !is_empty && !is_shiro_project {
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse { message: "目录已存在且非空".into() }),
            ));
        }
    } else {
        std::fs::create_dir_all(&target).map_err(internal_error)?;
    }

    // 初始化项目结构（见 docs/backend/storage.md 推荐目录）
    std::fs::create_dir_all(target.join(".shiro")).map_err(internal_error)?;
    std::fs::create_dir_all(target.join("正文")).map_err(internal_error)?;
    let project_toml = target.join(".shiro").join("project.toml");
    if !project_toml.exists() {
        std::fs::write(&project_toml, format!("name = {:?}\n", name)).map_err(internal_error)?;
    }

    // 导入记录：去重、最新在前
    let key = target.to_string_lossy().to_string();
    let mut history = load_history();
    history.projects.retain(|e| e.path != key);
    history.projects.insert(
        0,
        HistoryEntry { path: key.clone(), opened_at: now_secs() },
    );
    save_history(&history).map_err(internal_error)?;

    Ok((
        StatusCode::CREATED,
        Json(ProjectItem { path: key, name: name.to_string(), exists: true }),
    ))
}

#[derive(Deserialize, IntoParams)]
pub struct RemoveProjectQuery {
    /// 要移除记录的项目路径（只删记录，不删文件夹）
    path: String,
}

/// 删除项目记录（只从 history.toml 移除，不删除文件夹本身）
#[utoipa::path(
    delete,
    path = "/api/v1/projects",
    tag = "projects",
    params(RemoveProjectQuery),
    responses(
        (status = 204, description = "已删除"),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn remove_project(Query(query): Query<RemoveProjectQuery>) -> StatusCode {
    let mut history = load_history();
    history.projects.retain(|e| e.path != query.path);
    let _ = save_history(&history);
    StatusCode::NO_CONTENT
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
    components(schemas(
        StartupResponse,
        StartupStatus,
        ProjectItem,
        ProjectListResponse,
        CreateProjectRequest,
        ErrorResponse
    )),
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
    let (api_router, api) = OpenApiRouter::new()
        .routes(routes!(startup))
        .routes(routes!(list_projects, create_project, remove_project))
        .split_for_parts();
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
