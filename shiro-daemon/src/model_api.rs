// 模型档案 API（Model 视图）：多档案 CRUD + 默认档案指定（config.toml 的 [llm] 段，
// [[llm.profiles]] 数组 + default key）。设计见 docs/backend/model-access.md；
// Key 不回传明文，只回掩码预览。

use crate::api::{ApiError, ErrorResponse, bad_request, config_dir, internal_error};
use crate::llm::{self, Profile};
use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct ProfileSummary {
    /// 档案 key（供应商 preset key）
    pub key: String,
    pub base_url: String,
    /// 该档案当前使用的模型
    pub model: String,
    /// 协议：openai | anthropic
    pub protocol: String,
    /// 思考强度（未启用为 null）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    pub api_key_set: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_preview: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ProfileListResponse {
    pub profiles: Vec<ProfileSummary>,
    /// 默认档案 key（无档案时为 null）
    pub default_key: Option<String>,
}

fn summary(p: &Profile) -> ProfileSummary {
    ProfileSummary {
        key: p.key.clone(),
        base_url: p.base_url.clone(),
        model: p.model.clone(),
        protocol: p.protocol.clone(),
        thinking: p.thinking.clone(),
        api_key_set: !p.api_key.trim().is_empty(),
        api_key_preview: llm::mask_key(&p.api_key),
    }
}

fn list_response() -> ProfileListResponse {
    let (profiles, default) = llm::read_profiles(&config_dir());
    ProfileListResponse {
        profiles: profiles.iter().map(summary).collect(),
        default_key: default,
    }
}

/// 档案列表（含默认档案指定）
#[utoipa::path(
    get,
    path = "/api/v1/model/profiles",
    tag = "model",
    responses(
        (status = 200, description = "档案列表", body = ProfileListResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn list_profiles() -> Json<ProfileListResponse> {
    Json(list_response())
}

#[derive(Serialize, ToSchema)]
pub struct ModelSettings {
    /// 全局 LLM 并行调用上限（1-8）
    pub max_concurrency: u32,
}

/// 读取全局运行设置
#[utoipa::path(
    get,
    path = "/api/v1/model/settings",
    tag = "model",
    responses(
        (status = 200, description = "运行设置", body = ModelSettings),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn get_model_settings() -> Json<ModelSettings> {
    Json(ModelSettings {
        max_concurrency: llm::max_concurrency(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct SaveSettingsRequest {
    pub max_concurrency: u32,
}

/// 保存全局运行设置（调节即生效，无需重启——并发闸门每次获取时现读）
#[utoipa::path(
    put,
    path = "/api/v1/model/settings",
    tag = "model",
    request_body = SaveSettingsRequest,
    responses(
        (status = 200, description = "已保存", body = ModelSettings),
        (status = 400, description = "超出 1-8 范围", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn save_model_settings(
    Json(req): Json<SaveSettingsRequest>,
) -> Result<Json<ModelSettings>, ApiError> {
    if req.max_concurrency == 0 || req.max_concurrency > 8 {
        return Err(bad_request("并行数范围为 1-8"));
    }
    llm::write_max_concurrency(&config_dir(), req.max_concurrency).map_err(internal_error)?;
    Ok(Json(ModelSettings {
        max_concurrency: llm::max_concurrency(),
    }))
}

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

/// 注册/更新档案（同 key 覆盖；首个档案自动成为默认）
#[utoipa::path(
    post,
    path = "/api/v1/model/profiles",
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
    let key = req.key.trim();
    let base = req.base_url.trim();
    let model = req.model.trim();
    if key.is_empty() || key.contains(['/', '\\', ' ']) {
        return Err(bad_request("档案 key 非法"));
    }
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err(bad_request("端点地址需以 http:// 或 https:// 开头"));
    }
    if model.is_empty() {
        return Err(bad_request("模型名不能为空"));
    }
    let protocol = req
        .protocol
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or("openai");
    if protocol != "openai" && protocol != "anthropic" {
        return Err(bad_request("协议仅支持 openai / anthropic"));
    }
    let thinking = req
        .thinking
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    if let Some(t) = thinking {
        if !["low", "medium", "high"].contains(&t) {
            return Err(bad_request("思考强度仅支持 low / medium / high（或不填不启用）"));
        }
    }

    let (mut profiles, mut default) = llm::read_profiles(&config_dir());
    let new_key_provided = req
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());
    match profiles.iter_mut().find(|p| p.key == key) {
        Some(p) => {
            p.base_url = base.to_string();
            p.model = model.to_string();
            p.protocol = protocol.to_string();
            if thinking.is_some() {
                p.thinking = thinking.map(String::from);
            }
            if let Some(k) = new_key_provided {
                p.api_key = k.to_string();
            }
        }
        None => {
            let Some(k) = new_key_provided else {
                return Err(bad_request("新建档案必须提供 API Key"));
            };
            profiles.push(Profile {
                key: key.to_string(),
                base_url: base.to_string(),
                api_key: k.to_string(),
                model: model.to_string(),
                protocol: protocol.to_string(),
                thinking: thinking.map(String::from),
            });
            // 首个档案自动成为默认
            if default.is_none() {
                default = Some(key.to_string());
            }
        }
    }
    llm::write_profiles(&config_dir(), &profiles, default.as_deref())
        .map_err(internal_error)?;
    Ok(Json(list_response()))
}

#[derive(Serialize, ToSchema)]
pub struct ProfileTestResponse {
    pub ok: bool,
    pub message: String,
    /// 测试耗时（毫秒，成功时返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

/// 测试档案连接（发一个最小真实调用 ping；验证端点 + 密钥 + 模型 + 协议）
#[utoipa::path(
    post,
    path = "/api/v1/model/profiles/{key}/test",
    tag = "model",
    params(("key" = String, Path, description = "档案 key")),
    responses(
        (status = 200, description = "测试结果（ok 标识连通与否，message 说明）", body = ProfileTestResponse),
        (status = 404, description = "档案不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn test_profile(
    Path(key): Path<String>,
) -> Result<Json<ProfileTestResponse>, ApiError> {
    let (profiles, _) = llm::read_profiles(&config_dir());
    let p = profiles
        .iter()
        .find(|p| p.key == key)
        .ok_or_else(|| bad_request("档案不存在"))?;
    let cfg = llm::LlmConfig {
        base_url: p.base_url.clone(),
        api_key: p.api_key.clone(),
        model: p.model.clone(),
        protocol: p.protocol.clone(),
        thinking: p.thinking.clone(),
        timeout_secs: 20,
        max_attempts: 1,
    };
    match llm::test_connection(&cfg).await {
        Ok(ms) => Ok(Json(ProfileTestResponse {
            ok: true,
            message: format!("连接正常（{ms}ms）"),
            latency_ms: Some(ms),
        })),
        Err(e) => Ok(Json(ProfileTestResponse {
            ok: false,
            message: e.message,
            latency_ms: None,
        })),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct SetDefaultRequest {
    pub key: String,
}

/// 指定默认档案（拆解等任务使用）
#[utoipa::path(
    post,
    path = "/api/v1/model/profiles/default",
    tag = "model",
    request_body = SetDefaultRequest,
    responses(
        (status = 200, description = "已指定", body = ProfileListResponse),
        (status = 400, description = "档案不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn set_default_profile(
    Json(req): Json<SetDefaultRequest>,
) -> Result<Json<ProfileListResponse>, ApiError> {
    let (profiles, _) = llm::read_profiles(&config_dir());
    if !profiles.iter().any(|p| p.key == req.key.trim()) {
        return Err(bad_request("档案不存在"));
    }
    llm::write_profiles(&config_dir(), &profiles, Some(req.key.trim()))
        .map_err(internal_error)?;
    Ok(Json(list_response()))
}

/// 删除档案；删除默认档案时默认项回退到剩余第一个
#[utoipa::path(
    delete,
    path = "/api/v1/model/profiles/{key}",
    tag = "model",
    params(("key" = String, Path, description = "档案 key")),
    responses(
        (status = 200, description = "已删除（含更新后列表）", body = ProfileListResponse),
        (status = 404, description = "档案不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn delete_profile(
    Path(key): Path<String>,
) -> Result<Json<ProfileListResponse>, ApiError> {
    let (mut profiles, default) = llm::read_profiles(&config_dir());
    let before = profiles.len();
    profiles.retain(|p| p.key != key);
    if profiles.len() == before {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                message: "档案不存在".into(),
            }),
        ));
    }
    let new_default = default.filter(|d| profiles.iter().any(|p| &p.key == d));
    let new_default = new_default.or_else(|| profiles.first().map(|p| p.key.clone()));
    llm::write_profiles(&config_dir(), &profiles, new_default.as_deref())
        .map_err(internal_error)?;
    Ok(Json(list_response()))
}
