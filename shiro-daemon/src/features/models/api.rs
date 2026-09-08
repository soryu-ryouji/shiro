// 模型档案 HTTP 端点：列表 / 注册更新 / 删除 / 连接测试。领域逻辑见 store.rs。

use super::store::{self, ProfileInput, ProfileListResponse, ProfileTestResponse};
use crate::error::{ApiError, ErrorResponse};
use crate::state::AppState;
use axum::Json;
use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct UpsertProfileRequest {
    /// 档案 key（供应商 preset key；同 key 覆盖更新）
    pub key: String,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub protocol: Option<String>,
    /// 思考强度：缺省/空 = 不启用；low / medium / high
    #[serde(default)]
    pub thinking: Option<String>,
    /// 新密钥；缺省或空字符串 = 保留原值（新建档案必填）
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ProfileKeyRequest {
    /// 档案 key
    pub key: String,
}

/// 档案列表（含默认档案指定）
#[utoipa::path(
    post,
    path = "/api/v1/model/profiles/list",
    tag = "model",
    responses(
        (status = 200, description = "档案列表", body = ProfileListResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn list_profiles() -> Json<ProfileListResponse> {
    Json(store::list())
}

/// 注册/更新档案（同 key 覆盖；首个档案自动成为默认）
#[utoipa::path(
    post,
    path = "/api/v1/model/profiles/save",
    tag = "model",
    request_body = UpsertProfileRequest,
    responses(
        (status = 200, description = "已保存（含更新后列表）", body = ProfileListResponse),
        (status = 400, description = "参数错误", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn upsert_profile(
    Json(req): Json<UpsertProfileRequest>,
) -> Result<Json<ProfileListResponse>, ApiError> {
    let input = ProfileInput {
        key: req.key,
        base_url: req.base_url,
        model: req.model,
        protocol: req.protocol,
        thinking: req.thinking,
        api_key: req.api_key,
    };
    Ok(Json(store::save(&input)?))
}

/// 测试档案连接（发一个最小真实调用 ping；验证端点 + 密钥 + 模型 + 协议）
#[utoipa::path(
    post,
    path = "/api/v1/model/profiles/test",
    tag = "model",
    request_body = ProfileKeyRequest,
    responses(
        (status = 200, description = "测试结果（ok 标识连通与否，message 说明）", body = ProfileTestResponse),
        (status = 404, description = "档案不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn test_profile(
    Json(req): Json<ProfileKeyRequest>,
) -> Result<Json<ProfileTestResponse>, ApiError> {
    Ok(Json(store::test(&req.key).await?))
}

/// 删除档案；删除默认档案时默认项回退到剩余第一个
#[utoipa::path(
    post,
    path = "/api/v1/model/profiles/delete",
    tag = "model",
    request_body = ProfileKeyRequest,
    responses(
        (status = 200, description = "已删除（含更新后列表）", body = ProfileListResponse),
        (status = 404, description = "档案不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn delete_profile(
    Json(req): Json<ProfileKeyRequest>,
) -> Result<Json<ProfileListResponse>, ApiError> {
    Ok(Json(store::delete(&req.key)?))
}

pub(crate) fn router() -> utoipa_axum::router::OpenApiRouter<AppState> {
    use utoipa_axum::routes;
    utoipa_axum::router::OpenApiRouter::new()
        .routes(routes!(list_profiles))
        .routes(routes!(upsert_profile))
        .routes(routes!(delete_profile))
        .routes(routes!(test_profile))
}
