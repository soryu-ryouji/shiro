// LLM provider 层（最小实现，见 docs/backend/model-access.md）：
// OpenAI 兼容 chat completions 非流式调用，配置读 ~/.config/shiro/config.toml 的 [llm] 段。
// 协议形状参考 pi-ai 的 openai-completions 实现（compat 经验：字段级差异用 Option，不硬编码官方形状）。
// 错误分类参考 pi-ai：可重试（429/408/5xx/网络/超时）走指数退避，其余 4xx 直接失败。

use crate::api::config_dir;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

/// 档案：一家供应商的完整配置（端点 + 密钥 + 默认模型 + 协议）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    /// 档案 key（供应商 preset key；自定义端点为手工命名）
    pub key: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 协议：openai（默认）| anthropic
    #[serde(default = "default_protocol")]
    pub protocol: String,
}

/// 运行时配置（引擎使用；由默认档案派生）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmConfig {
    /// 端点基址（如 https://api.deepseek.com/v1；Kimi Code 为 https://api.kimi.com/coding）
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// 协议：openai（OpenAI 兼容，默认）| anthropic（Anthropic Messages，Kimi Code 等）
    #[serde(default = "default_protocol")]
    pub protocol: String,
    /// 单次调用超时（秒，默认 180——生成节点上下文大）
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// 可重试错误的最大尝试次数（默认 3）
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,
}

fn default_timeout() -> u64 {
    180
}
fn default_max_attempts() -> u32 {
    3
}
fn default_protocol() -> String {
    "openai".into()
}

impl LlmConfig {
    pub fn is_anthropic(&self) -> bool {
        self.protocol.trim() == "anthropic"
    }
}

/// 读取默认档案（引擎使用）；未配置或默认项缺失返回 None
pub fn load_llm_config() -> Option<LlmConfig> {
    let (profiles, default_key) = read_profiles(&config_dir());
    let p = profiles.iter().find(|p| Some(&p.key) == default_key.as_ref())?;
    let usable = !p.base_url.trim().is_empty()
        && !p.api_key.trim().is_empty()
        && !p.model.trim().is_empty();
    usable.then(|| LlmConfig {
        base_url: p.base_url.clone(),
        api_key: p.api_key.clone(),
        model: p.model.clone(),
        protocol: p.protocol.clone(),
        // derive(Default) 不走 serde default 函数，超时与重试必须显式给默认值
        timeout_secs: default_timeout(),
        max_attempts: default_max_attempts(),
    })
}

// ---- 档案读写（路径参数化，便于单测） ----

/// config.toml 结构：
/// ```toml
/// [llm]
/// default = "deepseek"
/// [[llm.profiles]]
/// key = "deepseek" ...
/// ```
/// 迁移：旧格式把档案字段直接写 [llm] 段（base_url/api_key/model[/protocol]）→
/// 读取时自动合成为单个档案（key 由端点反查预设，未知端点为 custom），下次写盘即新结构。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LlmSection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    profiles: Vec<Profile>,
    // 旧格式遗留字段（读取兼容；写出时不再包含）
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    protocol: Option<String>,
}

/// 读取档案列表与默认项（含旧格式迁移合成）
pub(crate) fn read_profiles(dir: &Path) -> (Vec<Profile>, Option<String>) {
    let Some(sec) = std::fs::read_to_string(dir.join("config.toml"))
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|doc| doc.get("llm").cloned())
        .and_then(|v| v.try_into::<LlmSection>().ok())
    else {
        return (Vec::new(), None);
    };
    if !sec.profiles.is_empty() {
        let default = sec
            .default
            .filter(|d| sec.profiles.iter().any(|p| &p.key == d))
            .or_else(|| sec.profiles.first().map(|p| p.key.clone()));
        return (sec.profiles, default);
    }
    // 旧格式迁移：顶层字段合成为单档案
    if let (Some(base), Some(model)) = (sec.base_url, sec.model) {
        let key = preset_key_for(&base).unwrap_or("custom").to_string();
        let profile = Profile {
            key,
            base_url: base,
            api_key: sec.api_key.unwrap_or_default(),
            model,
            protocol: sec.protocol.unwrap_or_else(default_protocol),
        };
        let default = Some(profile.key.clone());
        return (vec![profile], default);
    }
    (Vec::new(), None)
}

/// 由端点反查预设 key（迁移时的命名；未知端点 → custom）
fn preset_key_for(base_url: &str) -> Option<&'static str> {
    let b = base_url.trim().trim_end_matches('/');
    PRESET_ENDPOINTS
        .iter()
        .find(|(_, url)| b == url.trim_end_matches('/'))
        .map(|(key, _)| *key)
}

/// 迁移命名用的端点表（与前端 stores/model.ts 的 PROVIDERS 对齐）
const PRESET_ENDPOINTS: [(&str, &str); 9] = [
    ("deepseek", "https://api.deepseek.com/v1"),
    ("moonshot-cn", "https://api.moonshot.cn/v1"),
    ("moonshot", "https://api.moonshot.ai/v1"),
    ("kimi-code", "https://api.kimi.com/coding"),
    ("zhipu", "https://open.bigmodel.cn/api/paas/v4"),
    ("qwen", "https://dashscope.aliyuncs.com/compatible-mode/v1"),
    ("siliconflow", "https://api.siliconflow.cn/v1"),
    ("openai", "https://api.openai.com/v1"),
    ("openrouter", "https://openrouter.ai/api/v1"),
];

/// 写档案列表与默认项（整体替换 [llm] 段，保留其他段落；旧格式字段随之消失）
pub(crate) fn write_profiles(
    dir: &Path,
    profiles: &[Profile],
    default: Option<&str>,
) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("config.toml");
    let mut doc: toml::Table = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default();
    let section = LlmSection {
        default: default.map(String::from),
        profiles: profiles.to_vec(),
        ..Default::default()
    };
    let value = toml::Value::try_from(section).map_err(std::io::Error::other)?;
    doc.insert("llm".into(), value);
    let text = toml::to_string_pretty(&doc).map_err(std::io::Error::other)?;
    std::fs::write(&path, text)
}

/// 掩码预览：sk-abc…xyz9（长度 ≤ 8 全掩）；未配置返回 None
pub(crate) fn mask_key(key: &str) -> Option<String> {
    let k = key.trim();
    if k.is_empty() {
        return None;
    }
    let chars: Vec<char> = k.chars().collect();
    Some(if chars.len() > 8 {
        let head: String = chars[..3].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}…{tail}")
    } else {
        "••••".into()
    })
}

#[derive(Debug)]
pub struct LlmError {
    /// true = 网络类/限流类（上游暂时不可用）；false = 配置或协议错误
    pub retryable: bool,
    pub message: String,
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Serialize)]
pub struct ChatMessage {
    pub role: &'static str,
    pub content: String,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    temperature: f32,
}

/// Anthropic Messages 协议请求体（Kimi Code 等；max_tokens 必填）
#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: &'a [AnthropicMessage],
    temperature: f32,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    #[serde(default)]
    content: Vec<AnthropicBlock>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

/// anthropic 路径的 max_tokens（协议必填；K3 上限 131072，取生成档案够用的值）
const ANTHROPIC_MAX_TOKENS: u32 = 16_384;
/// anthropic 路径把 system 从 messages 里提出来（协议要求 system 是顶层字段）
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 兼容端点对 temperature 字段容忍度高（pi 经验：maxTokensField 类字段差异才是主要坑），
/// 0.4 固定值用于结构化提炼任务，不做配置项。
const TASK_TEMPERATURE: f32 = 0.4;

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatRespMessage,
}

#[derive(Deserialize)]
struct ChatRespMessage {
    #[serde(default)]
    content: Option<String>,
    // 兼容端点差异（pi compat 清单）：部分实现把正文放 reasoning_content 或空 content
    #[serde(default)]
    reasoning_content: Option<String>,
}

/// 非流式对话补全（按协议分流：openai 兼容 / anthropic messages）。调用方只关心正文文本。
pub async fn chat(cfg: &LlmConfig, messages: Vec<ChatMessage>) -> Result<String, LlmError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cfg.timeout_secs))
        .build()
        .map_err(|e| LlmError {
            retryable: false,
            message: format!("HTTP 客户端构建失败：{e}"),
        })?;

    let anthropic = cfg.is_anthropic();
    let mut attempt = 1u32;
    loop {
        // 请求构造按协议分流（RequestBuilder 一次性，每次尝试重建）
        let builder = if anthropic {
            anthropic_request(&client, cfg, &messages)
        } else {
            openai_request(&client, cfg, &messages)
        };
        let result = builder.send().await;

        let resp = match result {
            Ok(r) => r,
            Err(e) => {
                // 网络错误与超时统一按可重试处理
                if attempt < cfg.max_attempts {
                    backoff(attempt).await;
                    attempt += 1;
                    continue;
                }
                return Err(LlmError {
                    retryable: true,
                    message: format!("网络错误（已尝试 {attempt} 次）：{e}"),
                });
            }
        };

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            let retryable =
                status.as_u16() == 429 || status.as_u16() == 408 || status.is_server_error();
            if retryable && attempt < cfg.max_attempts {
                backoff(attempt).await;
                attempt += 1;
                continue;
            }
            let brief: String = body.chars().take(500).collect();
            return Err(LlmError {
                retryable,
                message: format!("上游返回 {status}：{brief}"),
            });
        }

        // 成功：按协议解析正文
        let text = if anthropic {
            let parsed: Result<AnthropicResponse, _> = resp.json().await;
            parsed
                .map_err(|e| LlmError {
                    retryable: false,
                    message: format!("响应解析失败：{e}"),
                })?
                .content
                .into_iter()
                .filter(|b| b.kind == "text")
                .filter_map(|b| b.text)
                .collect::<Vec<_>>()
                .join("")
        } else {
            let parsed: Result<ChatResponse, _> = resp.json().await;
            let r = parsed.map_err(|e| LlmError {
                retryable: false,
                message: format!("响应解析失败：{e}"),
            })?;
            let choice = r.choices.into_iter().next().ok_or_else(|| LlmError {
                retryable: false,
                message: "上游响应缺少 choices".into(),
            })?;
            // 兼容：content 为空时回退 reasoning_content（个别兼容端点的输出位置差异）
            choice
                .message
                .content
                .or(choice.message.reasoning_content)
                .unwrap_or_default()
        };
        if text.trim().is_empty() {
            return Err(LlmError {
                retryable: false,
                message: "上游返回空内容".into(),
            });
        }
        return Ok(text);
    }
}

/// OpenAI 兼容：POST {base}/chat/completions，Bearer 鉴权
fn openai_request<'a>(
    client: &'a reqwest::Client,
    cfg: &'a LlmConfig,
    messages: &'a [ChatMessage],
) -> reqwest::RequestBuilder {
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    client.post(url).bearer_auth(&cfg.api_key).json(&ChatRequest {
        model: &cfg.model,
        messages,
        temperature: TASK_TEMPERATURE,
    })
}

/// Anthropic Messages：POST {base}/v1/messages，x-api-key + anthropic-version；
/// system 提升为顶层字段（协议要求），非 system 消息以 user 角色传递（单轮提炼场景无对话历史）
fn anthropic_request<'a>(
    client: &'a reqwest::Client,
    cfg: &'a LlmConfig,
    messages: &'a [ChatMessage],
) -> reqwest::RequestBuilder {
    let base = cfg.base_url.trim_end_matches('/');
    // 官方 SDK 语义：baseURL + /v1/messages；用户填的 base 已带 /v1 时不重复拼
    let url = if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    };
    let system = messages
        .iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content.clone())
        .collect::<Vec<_>>()
        .join("\n\n");
    let chat: Vec<AnthropicMessage> = messages
        .iter()
        .filter(|m| m.role != "system")
        .map(|m| AnthropicMessage {
            role: "user",
            content: m.content.clone(),
        })
        .collect();
    client
        .post(url)
        .header("x-api-key", &cfg.api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&AnthropicRequest {
            model: &cfg.model,
            max_tokens: ANTHROPIC_MAX_TOKENS,
            system: &system,
            messages: &chat,
            temperature: TASK_TEMPERATURE,
        })
}

/// 指数退避：1s / 2s / 4s …（封顶 8s）。async 上下文用 tokio sleep，不阻塞执行线程
async fn backoff(attempt: u32) {
    let secs = std::cmp::min(1u64 << (attempt - 1).min(3), 8);
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

/// 模型输出的 JSON 容错解析（参考 pi-ai 的流式 JSON 容错思路）：
/// 剥 ```json 代码围栏 → 截取首个 {/[ 到末个 }/] → serde 解析。
pub fn extract_json(text: &str) -> Result<serde_json::Value, String> {
    let mut s = text.trim();
    // 剥代码围栏：```json ... ``` 或 ``` ... ```
    if let Some(rest) = s.strip_prefix("```") {
        let rest = rest.split_once('\n').map(|(_, r)| r).unwrap_or(rest);
        s = rest.trim_end().trim_end_matches("```").trim();
    }
    let start = s.find(['{', '[']).ok_or("输出中未找到 JSON")?;
    let end_brace = s.rfind('}');
    let end_bracket = s.rfind(']');
    let end = end_brace
        .zip(end_bracket)
        .map(|(a, b)| a.max(b))
        .or(end_brace)
        .or(end_bracket)
        .ok_or("输出中未找到 JSON 结束符")?;
    if end <= start {
        return Err("JSON 边界异常".into());
    }
    let json_str = &s[start..=end];
    serde_json::from_str(json_str).map_err(|e| format!("JSON 解析失败：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_tolerant() {
        let v = extract_json("```json\n{\"a\": 1}\n```").unwrap();
        assert_eq!(v["a"], 1);
        let v = extract_json("前置说明 {\"a\": [1,2]} 后置说明").unwrap();
        assert_eq!(v["a"].as_array().unwrap().len(), 2);
        let v = extract_json("废话一段\n[{\"x\": \"y\"}]\n结尾").unwrap();
        assert_eq!(v[0]["x"], "y");
        assert!(extract_json("完全没有 json").is_err());
        assert!(extract_json("```json\n{\"a\": 1").is_err()); // 截断
    }

    #[test]
    fn profiles_roundtrip_and_migration() {
        let dir = std::env::temp_dir().join(format!("shiro-llm-prof-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.toml"),
            "[lan]\naccess_key = \"keep-me\"\n",
        )
        .unwrap();

        // 旧格式：顶层字段直接写 [llm] 段 → 读取时迁移合成为单档案
        let legacy = "[lan]\naccess_key = \"keep-me\"\n\n[llm]\nbase_url = \"https://api.deepseek.com/v1\"\napi_key = \"sk-legacy\"\nmodel = \"deepseek-chat\"\n";
        std::fs::write(dir.join("config.toml"), legacy).unwrap();
        let (profiles, default) = read_profiles(&dir);
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].key, "deepseek", "端点反查预设 key");
        assert_eq!(profiles[0].api_key, "sk-legacy");
        assert_eq!(default.as_deref(), Some("deepseek"));

        // 写新结构：多个档案 + 默认项；旧字段消失
        let ps = vec![
            Profile {
                key: "deepseek".into(),
                base_url: "https://api.deepseek.com/v1".into(),
                api_key: "sk-a".into(),
                model: "deepseek-v4-flash".into(),
                protocol: "openai".into(),
            },
            Profile {
                key: "kimi-code".into(),
                base_url: "https://api.kimi.com/coding".into(),
                api_key: "sk-b".into(),
                model: "k3".into(),
                protocol: "anthropic".into(),
            },
        ];
        write_profiles(&dir, &ps, Some("kimi-code")).unwrap();
        let (read_back, default) = read_profiles(&dir);
        assert_eq!(read_back.len(), 2);
        assert_eq!(default.as_deref(), Some("kimi-code"));
        let kimi = read_back.iter().find(|p| p.key == "kimi-code").unwrap();
        assert_eq!(kimi.protocol, "anthropic");
        let text = std::fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(text.contains("[[llm.profiles]]"));
        assert!(text.contains("default = \"kimi-code\""));
        assert!(text.contains("keep-me"), "其他段落保留");
        assert!(!text.contains("sk-legacy"), "旧字段不再写出");

        // 默认项缺失时回退第一个档案
        write_profiles(&dir, &ps, None).unwrap();
        let (_, default) = read_profiles(&dir);
        assert_eq!(default.as_deref(), Some("deepseek"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mask_rules() {
        assert_eq!(mask_key("sk-abcdef123456").as_deref(), Some("sk-…3456"));
        assert_eq!(mask_key("short").as_deref(), Some("••••"));
        assert_eq!(mask_key(""), None);
        assert_eq!(mask_key("   "), None);
    }

    #[test]
    fn empty_profiles() {
        let dir = std::env::temp_dir().join(format!("shiro-llm-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (profiles, default) = read_profiles(&dir);
        assert!(profiles.is_empty());
        assert_eq!(default, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
