// 角色库 HTTP 端点：列表 / 详情 / 新建 / 保存。领域逻辑见 store.rs。

use super::store::{self, CharacterDetail, CharacterListResponse, SaveCharacterResponse};
use crate::error::{ApiError, ErrorResponse};
use crate::state::AppState;
use axum::{Json, http::StatusCode};
use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct GetCharacterRequest {
    /// 角色 id（单文件卡去 .md 的文件名；深卡为目录名）
    pub id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct SaveCharacterRequest {
    /// 角色 id（单文件卡去 .md 的文件名；深卡为目录名）
    pub id: String,
    /// 完整文件内容（含 frontmatter）
    pub content: String,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateCharacterRequest {
    /// 角色显示名（写入 frontmatter 的 name）
    pub name: String,
}

/// 全局人物库角色列表
#[utoipa::path(
    post,
    path = "/api/v1/db/characters/list",
    tag = "db",
    responses(
        (status = 200, description = "角色列表（按显示名排序）", body = CharacterListResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn list_characters() -> Json<CharacterListResponse> {
    Json(CharacterListResponse {
        characters: store::list(),
    })
}

/// 角色详情（单文件简卡：frontmatter 字段 + 正文；目录深卡：index.md + 子文件列表）
#[utoipa::path(
    post,
    path = "/api/v1/db/characters/get",
    tag = "db",
    request_body = GetCharacterRequest,
    responses(
        (status = 200, description = "角色详情", body = CharacterDetail),
        (status = 400, description = "非法 id", body = ErrorResponse),
        (status = 404, description = "角色不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn get_character(
    Json(req): Json<GetCharacterRequest>,
) -> Result<Json<CharacterDetail>, ApiError> {
    Ok(Json(store::get(&req.id)?))
}

/// 保存角色卡：原子覆盖（同文稿保存）。
/// 宽容策略：解析失败也保存（用户手改中途不丢内容），但返回 parse_error 提示该卡会从列表消失。
#[utoipa::path(
    post,
    path = "/api/v1/db/characters/save",
    tag = "db",
    request_body = SaveCharacterRequest,
    responses(
        (status = 200, description = "已保存（含解析反馈）", body = SaveCharacterResponse),
        (status = 400, description = "非法 id", body = ErrorResponse),
        (status = 404, description = "角色不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn save_character(
    Json(req): Json<SaveCharacterRequest>,
) -> Result<Json<SaveCharacterResponse>, ApiError> {
    Ok(Json(store::save(&req.id, &req.content)?))
}

/// 新建角色卡（骨架）：同名自动追加 -2/-3 序号；返回可直接进入编辑的详情
#[utoipa::path(
    post,
    path = "/api/v1/db/characters/create",
    tag = "db",
    request_body = CreateCharacterRequest,
    responses(
        (status = 201, description = "已创建", body = CharacterDetail),
        (status = 400, description = "名称为空", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn create_character(
    Json(req): Json<CreateCharacterRequest>,
) -> Result<(StatusCode, Json<CharacterDetail>), ApiError> {
    Ok((StatusCode::CREATED, Json(store::create(&req.name)?)))
}

pub(crate) fn router() -> utoipa_axum::router::OpenApiRouter<AppState> {
    use utoipa_axum::routes;
    utoipa_axum::router::OpenApiRouter::new()
        .routes(routes!(list_characters))
        .routes(routes!(get_character))
        .routes(routes!(save_character))
        .routes(routes!(create_character))
}
