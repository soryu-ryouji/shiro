//! 模型档案（config.toml 的 [llm] 段：[[llm.profiles]] + default key）。
//! 设计见 docs/backend/model-access.md；Key 不回传明文，只回掩码预览。

use crate::error::{ApiError, bad_request, internal_error, not_found};
use crate::infra::paths::config_folder;
use crate::llm::{self, Profile};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub(crate) struct ProfileSummary {
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
pub(crate) struct ProfileListResponse {
    pub profiles: Vec<ProfileSummary>,
    /// 默认档案 key（无档案时为 null）
    pub default_key: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ProfileTestResponse {
    pub ok: bool,
    pub message: String,
    /// 测试耗时（毫秒，成功时返回）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

/// 注册/更新档案的输入（由 HTTP 层转换，领域层不依赖请求 DTO）
pub(crate) struct ProfileInput {
    pub key: String,
    pub base_url: String,
    pub model: String,
    pub protocol: Option<String>,
    pub thinking: Option<String>,
    pub api_key: Option<String>,
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
    let (profiles, default) = llm::read_profiles(&config_folder());
    ProfileListResponse {
        profiles: profiles.iter().map(summary).collect(),
        default_key: default,
    }
}

/// 档案列表（含默认档案指定）
pub(crate) fn list() -> ProfileListResponse {
    list_response()
}

/// 注册/更新档案（同 key 覆盖；首个档案自动成为默认）
pub(crate) fn save(input: &ProfileInput) -> Result<ProfileListResponse, ApiError> {
    let key = input.key.trim();
    let base = input.base_url.trim();
    let model = input.model.trim();
    if key.is_empty() || key.contains(['/', '\\', ' ']) {
        return Err(bad_request("档案 key 非法"));
    }
    if !base.starts_with("http://") && !base.starts_with("https://") {
        return Err(bad_request("端点地址需以 http:// 或 https:// 开头"));
    }
    if model.is_empty() {
        return Err(bad_request("模型名不能为空"));
    }
    let protocol = input
        .protocol
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .unwrap_or("openai");
    if protocol != "openai" && protocol != "anthropic" {
        return Err(bad_request("协议仅支持 openai / anthropic"));
    }
    let thinking = input
        .thinking
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
    if let Some(t) = thinking
        && !["low", "medium", "high"].contains(&t)
    {
        return Err(bad_request(
            "思考强度仅支持 low / medium / high（或不填不启用）",
        ));
    }

    let (mut profiles, mut default) = llm::read_profiles(&config_folder());
    let new_key_provided = input
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
    llm::write_profiles(&config_folder(), &profiles, default.as_deref()).map_err(internal_error)?;
    Ok(list_response())
}

/// 测试档案连接（发一个最小真实调用 ping；验证端点 + 密钥 + 模型 + 协议）
pub(crate) async fn test(key: &str) -> Result<ProfileTestResponse, ApiError> {
    let (profiles, _) = llm::read_profiles(&config_folder());
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
    Ok(match llm::test_connection(&cfg).await {
        Ok(ms) => ProfileTestResponse {
            ok: true,
            message: format!("连接正常（{ms}ms）"),
            latency_ms: Some(ms),
        },
        Err(e) => ProfileTestResponse {
            ok: false,
            message: e.message,
            latency_ms: None,
        },
    })
}

/// 删除档案；删除默认档案时默认项回退到剩余第一个
pub(crate) fn delete(key: &str) -> Result<ProfileListResponse, ApiError> {
    let (mut profiles, default) = llm::read_profiles(&config_folder());
    let before = profiles.len();
    profiles.retain(|p| p.key != key);
    if profiles.len() == before {
        return Err(not_found("档案不存在"));
    }
    let new_default = default.filter(|d| profiles.iter().any(|p| &p.key == d));
    let new_default = new_default.or_else(|| profiles.first().map(|p| p.key.clone()));
    llm::write_profiles(&config_folder(), &profiles, new_default.as_deref())
        .map_err(internal_error)?;
    Ok(list_response())
}
