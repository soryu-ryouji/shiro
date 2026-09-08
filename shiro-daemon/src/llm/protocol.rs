// 协议适配：请求构造（OpenAI 兼容 / Anthropic）与流式响应解析、重试退避。
// 由 llm::chat_stream / test_connection 调用。

use super::config::LlmConfig;
use super::{ChatMessage, Usage};
use serde::Serialize;
use std::time::Duration;

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    temperature: f32,
    stream: bool,
    /// 思考强度（OpenAI o 系 / gpt-5 reasoning 系参数；不设置则不发送）
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<&'a str>,
    /// 流式时带 usage（统计 token；不支持的兼容端点忽略）
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<serde_json::Value>,
}

/// Anthropic Messages 协议请求体（Kimi Code 等；max_tokens 必填）
#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: &'a [AnthropicMessage],
    temperature: f32,
    stream: bool,
    /// 思考强度（Anthropic thinking 参数；不设置则不发送）
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AnthropicThinking>,
}

#[derive(Serialize)]
struct AnthropicThinking {
    r#type: &'static str,
    budget_tokens: u32,
}

/// 思考强度 → budget_tokens（必须 < max_tokens 16384）
fn thinking_budget(level: &str) -> u32 {
    match level {
        "low" => 2_048,
        "high" => 12_288,
        _ => 8_192, // medium 及未知值
    }
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: &'static str,
    content: String,
}

/// anthropic 路径的 max_tokens（协议必填；K3 上限 131072，取生成档案够用的值）
pub(super) const ANTHROPIC_MAX_TOKENS: u32 = 16_384;
/// anthropic 路径把 system 从 messages 里提出来（协议要求 system 是顶层字段）
pub(super) const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 兼容端点对 temperature 字段容忍度高（pi 经验：maxTokensField 类字段差异才是主要坑），
/// 0.4 固定值用于结构化提炼任务，不做配置项。
pub(super) const TASK_TEMPERATURE: f32 = 0.4;

pub(super) enum StreamChunk {
    Text(String),
    Thinking(String),
    Usage(Usage),
    Done,
}

pub(super) fn u64_at(v: &serde_json::Value, path: &[&str]) -> u64 {
    let mut cur = v;
    for key in path {
        cur = cur.get(*key).unwrap_or(&serde_json::Value::Null);
    }
    cur.as_u64().unwrap_or(0)
}

/// OpenAI 兼容 SSE 行解析：正文 delta / 思考 delta（reasoning_content）/ usage（末帧）/ [DONE]
pub(super) fn parse_openai_line(line: &str) -> Option<StreamChunk> {
    let data = line.strip_prefix("data:")?.trim();
    if data == "[DONE]" {
        return Some(StreamChunk::Done);
    }
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    // usage 帧（stream_options.include_usage）：choices 为空、usage 在场
    if let Some(u) = v.get("usage") {
        if u.is_object() {
            return Some(StreamChunk::Usage(Usage {
                input: u64_at(u, &["prompt_tokens"]),
                output: u64_at(u, &["completion_tokens"]),
                // DeepSeek 的缓存命中/未命中命名
                cache_read: u64_at(u, &["prompt_cache_hit_tokens"])
                    .max(u64_at(u, &["cache_read_input_tokens"])),
                cache_write: u64_at(u, &["prompt_cache_miss_tokens"])
                    .max(u64_at(u, &["cache_creation_input_tokens"])),
            }));
        }
    }
    let delta = v.get("choices")?.as_array()?.first()?.get("delta")?;
    if let Some(t) = delta.get("reasoning_content").and_then(|c| c.as_str()) {
        if !t.is_empty() {
            return Some(StreamChunk::Thinking(t.to_string()));
        }
    }
    let text = delta.get("content").and_then(|c| c.as_str())?;
    if text.is_empty() {
        None
    } else {
        Some(StreamChunk::Text(text.to_string()))
    }
}

/// Anthropic SSE 行解析：text_delta / thinking_delta / message_start+message_delta 的 usage
pub(super) fn parse_anthropic_line(line: &str) -> Option<StreamChunk> {
    let data = line.strip_prefix("data:")?.trim();
    let v: serde_json::Value = serde_json::from_str(data).ok()?;
    match v.get("type")?.as_str()? {
        "message_start" => {
            let u = v.get("message")?.get("usage")?;
            Some(StreamChunk::Usage(Usage {
                input: u64_at(u, &["input_tokens"]),
                output: u64_at(u, &["output_tokens"]),
                cache_read: u64_at(u, &["cache_read_input_tokens"]),
                cache_write: u64_at(u, &["cache_creation_input_tokens"]),
            }))
        }
        "message_delta" => {
            let u = v.get("usage")?;
            Some(StreamChunk::Usage(Usage {
                input: 0,
                output: u64_at(u, &["output_tokens"]),
                cache_read: 0,
                cache_write: 0,
            }))
        }
        "content_block_delta" => {
            let delta = v.get("delta")?;
            match delta.get("type")?.as_str()? {
                "text_delta" => {
                    let t = delta.get("text")?.as_str()?;
                    if t.is_empty() {
                        None
                    } else {
                        Some(StreamChunk::Text(t.to_string()))
                    }
                }
                "thinking_delta" => {
                    let t = delta.get("thinking")?.as_str()?;
                    if t.is_empty() {
                        None
                    } else {
                        Some(StreamChunk::Thinking(t.to_string()))
                    }
                }
                _ => None,
            }
        }
        "message_stop" => Some(StreamChunk::Done),
        _ => None,
    }
}

/// usage 帧合并（openai 单帧全量；anthropic message_start 首帧 + message_delta 增量）
pub(super) fn merge_usage(slot: &mut Option<Usage>, u: Usage) {
    match slot {
        None => *slot = Some(u),
        Some(s) => {
            s.input = s.input.max(u.input);
            s.output = s.output.max(u.output);
            s.cache_read = s.cache_read.max(u.cache_read);
            s.cache_write = s.cache_write.max(u.cache_write);
        }
    }
}

/// OpenAI 兼容：POST {base}/chat/completions，Bearer 鉴权（stream:true）
pub(super) fn openai_request<'a>(
    client: &'a reqwest::Client,
    cfg: &'a LlmConfig,
    messages: &'a [ChatMessage],
    stream: bool,
) -> reqwest::RequestBuilder {
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    client.post(url).bearer_auth(&cfg.api_key).json(&ChatRequest {
        model: &cfg.model,
        messages,
        temperature: TASK_TEMPERATURE,
        stream,
        reasoning_effort: cfg.thinking.as_deref(),
        stream_options: stream.then(|| serde_json::json!({ "include_usage": true })),
    })
}

/// Anthropic Messages：POST {base}/v1/messages，x-api-key + anthropic-version（stream:true）；
/// system 提升为顶层字段（协议要求），非 system 消息以 user 角色传递（单轮提炼场景无对话历史）
pub(super) fn anthropic_request<'a>(
    client: &'a reqwest::Client,
    cfg: &'a LlmConfig,
    messages: &'a [ChatMessage],
    stream: bool,
) -> reqwest::RequestBuilder {
    // 官方 SDK 语义：baseURL + /v1/messages；用户填的 base 已带 /v1 时不重复拼
    let url = anthropic_url(&cfg.base_url);
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
    let thinking = cfg.thinking.as_deref().map(|level| AnthropicThinking {
        r#type: "enabled",
        budget_tokens: thinking_budget(level),
    });
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
            stream,
            thinking,
        })
}

/// 上游要求等待的毫秒数（retry-after-ms / retry-after 头；封顶 60s，超出拒绝重试）
pub(super) fn retry_after_ms_from(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    const CAP_MS: u64 = 60_000;
    if let Some(v) = headers.get("retry-after-ms").and_then(|v| v.to_str().ok())
        && let Ok(ms) = v.parse::<f64>() {
            let ms = ms as u64;
            return (ms <= CAP_MS).then_some(ms);
        }
    if let Some(v) = headers.get("retry-after").and_then(|v| v.to_str().ok())
        && let Ok(secs) = v.parse::<f64>() {
            let ms = (secs * 1000.0) as u64;
            return (ms <= CAP_MS).then_some(ms);
        }
    None
}

/// 退避：上游指定时按其要求（已封顶 60s），否则指数 0.5s·2^i（封顶 8s）带 25% 抖动（pi 口径）
pub(super) async fn backoff(attempt: u32, server_hint_ms: Option<u64>) {
    if let Some(ms) = server_hint_ms {
        tokio::time::sleep(Duration::from_millis(ms)).await;
        return;
    }
    let base_ms = (0.5 * 2f64.powi(attempt as i32)).min(8.0) * 1000.0;
    // 25% 负向抖动，避免多任务同步重试
    let jitter = 1.0 - (now_nanos_frac() * 0.25);
    tokio::time::sleep(Duration::from_millis((base_ms * jitter) as u64)).await;
}

/// 0..1 伪随机（时间纳秒尾数；避免为此引入 rand 依赖）
fn now_nanos_frac() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    (nanos % 1_000_000) as f64 / 1_000_000.0
}

/// anthropic 消息端点拼接（供测试与其他调用复用）
pub(super) fn anthropic_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_headers() {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("retry-after-ms", "500".parse().unwrap());
        assert_eq!(retry_after_ms_from(&h), Some(500));
        h.insert("retry-after-ms", "99999999".parse().unwrap());
        assert_eq!(retry_after_ms_from(&h), None, "超 60s 上限不认");
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("retry-after", "2".parse().unwrap());
        assert_eq!(retry_after_ms_from(&h), Some(2000));
        assert_eq!(retry_after_ms_from(&reqwest::header::HeaderMap::new()), None);
    }
}
