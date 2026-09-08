// 项目 HTTP 端点：登记/移除/重命名 + 目录树/预览/文稿与目录 CRUD/条目重命名 + 目录监听 SSE。
// handler 只做参数解析与响应组装，领域逻辑在 store.rs（登记与 history）与 files.rs（条目操作）。

use super::files::{self, ExcerptsResponse, FileContent, TreeResponse};
use super::store::{self, ProjectItem, ProjectListResponse};
use crate::error::{ApiError, ErrorResponse};
use crate::infra::paths::validate_name;
use crate::state::AppState;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use serde::Deserialize;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use utoipa::ToSchema;

// ---- 项目登记 ----

#[derive(Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    /// 项目文件夹绝对路径（须已存在；任意内容的文件夹均可）
    pub path: String,
    /// 项目显示名（选填；写入 .shiro/project.toml 的 name，留空则保留已有或用文件夹名）
    #[serde(default)]
    pub name: String,
}

/// 新建项目：把选中的已有文件夹登记为项目（VSCode「打开文件夹」式），并导入记录。
/// 项目显示名写入 .shiro/project.toml 的 name（与文件夹名解耦）；留空则保留已有或用文件夹名。
#[utoipa::path(
    post,
    path = "/api/v1/projects/create",
    tag = "projects",
    request_body = CreateProjectRequest,
    responses(
        (status = 201, description = "已创建", body = ProjectItem),
        (status = 400, description = "参数错误", body = ErrorResponse),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn create_project(
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ProjectItem>), ApiError> {
    let name = req.name.trim();
    let display_name = (!name.is_empty()).then_some(name);
    let item = store::register(&req.path, display_name)?;
    Ok((StatusCode::CREATED, Json(item)))
}

/// 项目列表（history.toml 记录，最近打开在前）
#[utoipa::path(
    post,
    path = "/api/v1/projects/list",
    tag = "projects",
    responses(
        (status = 200, description = "项目列表", body = ProjectListResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn list_projects() -> Json<ProjectListResponse> {
    Json(ProjectListResponse {
        projects: store::list(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct RemoveProjectRequest {
    /// 要移除记录的项目路径（只删记录，不删文件夹）
    pub path: String,
}

/// 删除项目记录（只从 history.toml 移除，不删除文件夹本身）
#[utoipa::path(
    post,
    path = "/api/v1/projects/remove",
    tag = "projects",
    request_body = RemoveProjectRequest,
    responses(
        (status = 204, description = "已删除"),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn remove_project(Json(req): Json<RemoveProjectRequest>) -> StatusCode {
    store::remove(&req.path);
    StatusCode::NO_CONTENT
}

#[derive(Deserialize, ToSchema)]
pub struct RenameProjectRequest {
    /// 项目当前绝对路径
    pub path: String,
    /// 新名称（即新目录名）
    pub new_name: String,
}

/// 重命名项目：重命名项目文件夹本体，history.toml 记录同步更新
#[utoipa::path(
    post,
    path = "/api/v1/projects/rename",
    tag = "projects",
    request_body = RenameProjectRequest,
    responses(
        (status = 200, description = "已重命名", body = ProjectItem),
        (status = 400, description = "名称非法", body = ErrorResponse),
        (status = 404, description = "项目目录不存在", body = ErrorResponse),
        (status = 409, description = "目标目录已存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn rename_project(
    Json(req): Json<RenameProjectRequest>,
) -> Result<Json<ProjectItem>, ApiError> {
    let src = store::ensure_registered(&req.path)?;
    let name = validate_name(&req.new_name)?;
    Ok(Json(store::rename(&src, name)?))
}

// ---- 目录树与预览 ----

#[derive(Deserialize, ToSchema)]
pub struct TreeRequest {
    /// 项目根目录绝对路径
    pub path: String,
}

/// 项目目录树（目录与文稿文件；目录在前，文件名排序）
#[utoipa::path(
    post,
    path = "/api/v1/projects/tree",
    tag = "projects",
    request_body = TreeRequest,
    responses(
        (status = 200, description = "目录树", body = TreeResponse),
        (status = 400, description = "项目目录不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn project_tree(
    Json(req): Json<TreeRequest>,
) -> Result<Json<TreeResponse>, ApiError> {
    let root = store::ensure_registered(&req.path)?;
    Ok(Json(TreeResponse {
        children: files::tree(&root),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct ExcerptsRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径（'' = 项目根）
    pub folder: String,
}

/// 目录内全部文稿的正文预览（每篇取开头几行，Ulysses 式列表）
#[utoipa::path(
    post,
    path = "/api/v1/projects/excerpts",
    tag = "projects",
    request_body = ExcerptsRequest,
    responses(
        (status = 200, description = "预览列表", body = ExcerptsResponse),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn project_excerpts(
    Json(req): Json<ExcerptsRequest>,
) -> Result<Json<ExcerptsResponse>, ApiError> {
    let root = store::ensure_registered(&req.path)?;
    Ok(Json(ExcerptsResponse {
        excerpts: files::excerpts(&root, &req.folder)?,
    }))
}

// ---- 文稿 ----

#[derive(Deserialize, ToSchema)]
pub struct ReadFileRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径（如 正文/第一卷/001 序章.md）
    pub file: String,
}

/// 读取文稿内容
#[utoipa::path(
    post,
    path = "/api/v1/projects/file/read",
    tag = "projects",
    request_body = ReadFileRequest,
    responses(
        (status = 200, description = "文稿内容", body = FileContent),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 404, description = "文件不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn read_project_file(
    Json(req): Json<ReadFileRequest>,
) -> Result<Json<FileContent>, ApiError> {
    let root = store::ensure_registered(&req.path)?;
    Ok(Json(files::read_file(&root, &req.file)?))
}

#[derive(Deserialize, ToSchema)]
pub struct WriteFileRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径
    pub file: String,
    pub content: String,
}

/// 保存文稿：同目录临时文件 + rename 原子覆盖，写入中断不腐蚀已有正文（见 storage.md 备份与快照）
#[utoipa::path(
    post,
    path = "/api/v1/projects/file/write",
    tag = "projects",
    request_body = WriteFileRequest,
    responses(
        (status = 200, description = "已保存，返回新修改时间", body = FileContent),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn write_project_file(
    Json(req): Json<WriteFileRequest>,
) -> Result<Json<FileContent>, ApiError> {
    let root = store::ensure_registered(&req.path)?;
    Ok(Json(files::write_file(&root, &req.file, &req.content)?))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateFileRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径（父目录自动创建）
    pub file: String,
}

/// 新建文稿（空文件）
#[utoipa::path(
    post,
    path = "/api/v1/projects/file/create",
    tag = "projects",
    request_body = CreateFileRequest,
    responses(
        (status = 201, description = "已创建", body = FileContent),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 409, description = "文件已存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn create_project_file(
    Json(req): Json<CreateFileRequest>,
) -> Result<(StatusCode, Json<FileContent>), ApiError> {
    let root = store::ensure_registered(&req.path)?;
    Ok((StatusCode::CREATED, Json(files::create_file(&root, &req.file)?)))
}

#[derive(Deserialize, ToSchema)]
pub struct RemoveFileRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 文稿相对路径
    pub file: String,
}

/// 删除文稿：移入项目回收站（.shiro/trash/，可手动恢复）
#[utoipa::path(
    post,
    path = "/api/v1/projects/file/delete",
    tag = "projects",
    request_body = RemoveFileRequest,
    responses(
        (status = 204, description = "已删除（移入回收站）"),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 404, description = "文件不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn remove_project_file(
    Json(req): Json<RemoveFileRequest>,
) -> Result<StatusCode, ApiError> {
    let root = store::ensure_registered(&req.path)?;
    files::remove_file(&root, &req.file)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- 目录 ----

#[derive(Deserialize, ToSchema)]
pub struct CreateFolderRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径（多级自动创建）
    pub folder: String,
}

/// 新建目录（幂等：已存在返回 200）
#[utoipa::path(
    post,
    path = "/api/v1/projects/folder/create",
    tag = "projects",
    request_body = CreateFolderRequest,
    responses(
        (status = 201, description = "已创建"),
        (status = 200, description = "目录已存在"),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn create_project_folder(
    Json(req): Json<CreateFolderRequest>,
) -> Result<StatusCode, ApiError> {
    let root = store::ensure_registered(&req.path)?;
    let created = files::create_folder(&root, &req.folder)?;
    Ok(if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    })
}

#[derive(Deserialize, ToSchema)]
pub struct RemoveFolderRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径（不允许根目录与 .shiro）
    pub folder: String,
}

/// 删除目录：空目录直接删除；非空目录移入项目回收站（.shiro/trash/，可手动恢复）
#[utoipa::path(
    post,
    path = "/api/v1/projects/folder/delete",
    tag = "projects",
    request_body = RemoveFolderRequest,
    responses(
        (status = 204, description = "已删除"),
        (status = 400, description = "路径非法或受保护", body = ErrorResponse),
        (status = 404, description = "目录不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn remove_project_folder(
    Json(req): Json<RemoveFolderRequest>,
) -> Result<StatusCode, ApiError> {
    let root = store::ensure_registered(&req.path)?;
    files::remove_folder(&root, &req.folder)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct RenameEntryRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 条目相对路径（文件或目录；文件需含后缀）
    pub rel: String,
    /// 新名字（文件名含后缀）
    pub new_name: String,
}

/// 重命名项目内条目（文件或目录）：同目录改名
#[utoipa::path(
    post,
    path = "/api/v1/projects/entry/rename",
    tag = "projects",
    request_body = RenameEntryRequest,
    responses(
        (status = 204, description = "已重命名"),
        (status = 400, description = "路径非法或受保护", body = ErrorResponse),
        (status = 404, description = "条目不存在", body = ErrorResponse),
        (status = 409, description = "目标已存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn rename_entry(
    Json(req): Json<RenameEntryRequest>,
) -> Result<StatusCode, ApiError> {
    let root = store::ensure_registered(&req.path)?;
    files::rename_entry(&root, &req.rel, &req.new_name)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- 目录监听（SSE 推送；实现见 watch.rs，供编辑器外部变动重载，后续 AI 写稿可订阅同一 Hub） ----

#[derive(Deserialize, ToSchema)]
pub struct WatchRequest {
    /// 项目根目录绝对路径
    pub path: String,
}

/// 监听项目目录变动：SSE 推送防抖 300ms 后的变更相对路径集合（`.` 开头路径段与临时文件已过滤）。
/// 每帧 data 为 JSON：`{"changed":["正文/a.md"]}`；changed 为空数组表示事件滞后溢出，订阅方应全量刷新。
/// 监听随连接建立而启动、所有订阅断开后停止。
#[utoipa::path(
    post,
    path = "/api/v1/projects/watch",
    tag = "projects",
    request_body = WatchRequest,
    responses(
        (status = 200, description = "SSE 事件流（text/event-stream）：data 为 {\"changed\":[...]}"),
        (status = 400, description = "项目目录不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn watch_project(
    State(state): State<AppState>,
    Json(req): Json<WatchRequest>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let root = store::ensure_registered(&req.path)?;
    let (rx, guard) = state.watch_hub.subscribe(&root);
    let stream = BroadcastStream::new(rx).map(move |item| {
        let _guard = &guard; // 订阅守卫随流存活：客户端断开 → 流 drop → 退订（归零停监听）
        let changed = item.unwrap_or_default(); // Lagged（消费滞后丢帧）→ 空数组，订阅方全量刷新
        let data = serde_json::json!({ "changed": changed }).to_string();
        Ok(Event::default().data(data))
    });
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

// ---- 路由 ----

pub(crate) fn router() -> utoipa_axum::router::OpenApiRouter<AppState> {
    use utoipa_axum::routes;
    utoipa_axum::router::OpenApiRouter::new()
        .routes(routes!(list_projects))
        .routes(routes!(create_project))
        .routes(routes!(remove_project))
        .routes(routes!(rename_project))
        .routes(routes!(project_tree))
        .routes(routes!(project_excerpts))
        .routes(routes!(read_project_file))
        .routes(routes!(write_project_file))
        .routes(routes!(create_project_file))
        .routes(routes!(remove_project_file))
        .routes(routes!(create_project_folder))
        .routes(routes!(remove_project_folder))
        .routes(routes!(rename_entry))
        .routes(routes!(watch_project))
}
