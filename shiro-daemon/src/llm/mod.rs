// LLM provider 层（最小实现，见 docs/backend/model-access.md）：
// 配置读 ~/.config/shiro/config.toml 的 [llm] 段，OpenAI 兼容 + Anthropic 双协议流式调用。
// 协议形状参考 pi-ai 的 openai-completions 实现（compat 经验：字段级差异用 Option，不硬编码官方形状）。
// 错误分类参考 pi-ai：可重试（429/408/5xx/网络/超时）走指数退避，其余 4xx 直接失败。

mod config;
mod json;
mod protocol;

// 并发上限暂无调用方，为后续多路并行管线保留
#[allow(unused_imports)]
pub(crate) use config::{
    LlmConfig, Profile, load_llm_config, mask_key, max_concurrency, read_profiles,
    write_max_concurrency, write_profiles,
};
pub(crate) use json::extract_json;

use protocol::{
    ANTHROPIC_VERSION, StreamChunk, anthropic_request, anthropic_url, backoff, merge_usage,
    openai_request, parse_anthropic_line, parse_openai_line, retry_after_ms_from, u64_at,
};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug)]
pub struct LlmError {
    /// true = 网络类/限流类（上游暂时不可用）；false = 配置或协议错误（重试策略用，暂无读取方）
    #[allow(dead_code)]
    pub retryable: bool,
    pub message: String,
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// token 用量（归一化：DeepSeek 的 cache_hit/cache_miss、Anthropic 的 cache_read/cache_creation）
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Usage {
    /// 缓存命中率（input 中含 cache_read 时；暂无读取方，用量分析用）
    #[allow(dead_code)]
    pub fn cache_hit_rate(&self) -> Option<u32> {
        if self.cache_read == 0 || self.input == 0 {
            return None;
        }
        Some((self.cache_read * 100 / self.input) as u32)
    }
}

/// 一次流式调用的完整产出
pub struct ChatOutput {
    pub text: String,
    /// 思考过程（reasoning/thinking 内容；不进正文，展示用；暂无读取方）
    #[allow(dead_code)]
    pub thinking: String,
    /// 用量统计（暂无读取方）
    #[allow(dead_code)]
    pub usage: Option<Usage>,
    /// 是否来自非流式兜底（流式中断后的降级路径；暂无读取方）
    #[allow(dead_code)]
    pub fallback: bool,
}

#[derive(Serialize)]
pub struct ChatMessage {
    pub role: &'static str,
    pub content: String,
}

/// 流式对话补全（两协议 SSE；正文增量经 on_delta 回调，返回完整文本）。
/// 调用方用回调把增量写入日志等持久层（无实时频道，展示由轮询驱动）。
/// 重试只发生在流开始之前：408/409/429/5xx/网络错误，尊重 retry-after(-ms) 头（封顶 60s）。
pub async fn chat_stream<F, G>(
    cfg: &LlmConfig,
    messages: Vec<ChatMessage>,
    mut on_delta: F,
    mut on_thinking: G,
) -> Result<ChatOutput, LlmError>
where
    F: FnMut(&str) + Send,
    G: FnMut(&str) + Send,
{
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
        let builder = if anthropic {
            anthropic_request(&client, cfg, &messages, true)
        } else {
            openai_request(&client, cfg, &messages, true)
        };
        let resp = match builder.send().await {
            Ok(r) => r,
            Err(e) => {
                if attempt < cfg.max_attempts {
                    backoff(attempt, None).await;
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
            let retry_after_ms = retry_after_ms_from(resp.headers());
            let body = resp.text().await.unwrap_or_default();
            let retryable = matches!(status.as_u16(), 408 | 409 | 429) || status.is_server_error();
            if retryable && attempt < cfg.max_attempts {
                backoff(attempt, retry_after_ms).await;
                attempt += 1;
                continue;
            }
            let brief: String = body.chars().take(500).collect();
            return Err(LlmError {
                retryable,
                message: format!("上游返回 {status}：{brief}"),
            });
        }

        // 流式读取 SSE（从字节流按行切，容忍跨包断行）
        use futures_util::StreamExt;
        let mut full = String::new();
        let mut thinking = String::new();
        let mut usage: Option<Usage> = None;
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();
        let mut stream_error: Option<String> = None;
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    buf.push_str(&String::from_utf8_lossy(&bytes));
                    while let Some(pos) = buf.find('\n') {
                        let line = buf[..pos].trim_end_matches('\r').to_string();
                        buf = buf[pos + 1..].to_string();
                        let parsed = if anthropic {
                            parse_anthropic_line(&line)
                        } else {
                            parse_openai_line(&line)
                        };
                        match parsed {
                            Some(StreamChunk::Text(t)) => {
                                full.push_str(&t);
                                on_delta(&t);
                            }
                            Some(StreamChunk::Thinking(t)) => {
                                thinking.push_str(&t);
                                on_thinking(&t);
                            }
                            Some(StreamChunk::Usage(u)) => {
                                merge_usage(&mut usage, u);
                            }
                            Some(StreamChunk::Done) => break,
                            None => {}
                        }
                    }
                }
                Err(e) => {
                    stream_error = Some(e.to_string());
                    break;
                }
            }
        }
        if let Some(e) = stream_error {
            // 流中断（代理/CDN 掐长连接的常见形态）：整体重试；次数用尽后非流式兜底
            let diag = format!(
                "流式响应中断：{e}（已收 {} 字符 / 尝试 {attempt}）",
                full.chars().count()
            );
            if attempt < cfg.max_attempts {
                tracing::warn!("{diag}，整体重试");
                backoff(attempt, None).await;
                attempt += 1;
                continue;
            }
            // 流式反复中断（代理持续掐流）→ 非流式兜底：没有实时增量，但要拿到正确结果
            return chat_nonstream_fallback(&client, cfg, &messages, anthropic, diag).await;
        }
        if full.trim().is_empty() {
            // 空内容 = 截断/垃圾响应的签名（代理网关错误页等）→ 非流式兜底
            return chat_nonstream_fallback(
                &client,
                cfg,
                &messages,
                anthropic,
                "上游流式响应为空（疑似截断）".into(),
            )
            .await;
        }
        return Ok(ChatOutput {
            text: full,
            thinking,
            usage,
            fallback: false,
        });
    }
}

/// 非流式兜底：流式反复中断/为空时降级——拿不到实时增量，但要拿到正确结果
async fn chat_nonstream_fallback(
    client: &reqwest::Client,
    cfg: &LlmConfig,
    messages: &[ChatMessage],
    anthropic: bool,
    cause: String,
) -> Result<ChatOutput, LlmError> {
    let builder = if anthropic {
        anthropic_request(client, cfg, messages, false)
    } else {
        openai_request(client, cfg, messages, false)
    };
    let resp = builder.send().await.map_err(|e| LlmError {
        retryable: true,
        message: format!("{cause}；非流式兜底也失败：{e}"),
    })?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(LlmError {
            retryable: matches!(status.as_u16(), 408 | 409 | 429) || status.is_server_error(),
            message: format!(
                "{cause}；兜底上游返回 {status}：{}",
                body.chars().take(300).collect::<String>()
            ),
        });
    }
    let body = resp.text().await.map_err(|e| LlmError {
        retryable: true,
        message: format!("{cause}；兜底读取失败：{e}"),
    })?;

    #[derive(serde::Deserialize)]
    struct OpenAiResp {
        choices: Vec<OpenAiChoice>,
        #[serde(default)]
        usage: Option<serde_json::Value>,
    }
    #[derive(serde::Deserialize)]
    struct OpenAiChoice {
        message: OpenAiMsg,
    }
    #[derive(serde::Deserialize)]
    struct OpenAiMsg {
        #[serde(default)]
        content: Option<String>,
        #[serde(default)]
        reasoning_content: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct AnthResp {
        #[serde(default)]
        content: Vec<AnthBlock>,
        #[serde(default)]
        usage: Option<serde_json::Value>,
    }
    #[derive(serde::Deserialize)]
    struct AnthBlock {
        #[serde(default, rename = "type")]
        kind: String,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        thinking: Option<String>,
    }

    let out: Option<ChatOutput> = if anthropic {
        serde_json::from_str::<AnthResp>(&body).ok().map(|r| {
            let text = r
                .content
                .iter()
                .filter(|b| b.kind == "text")
                .filter_map(|b| b.text.clone())
                .collect::<Vec<_>>()
                .join("");
            let thinking = r
                .content
                .iter()
                .filter(|b| b.kind == "thinking")
                .filter_map(|b| b.thinking.clone())
                .collect::<Vec<_>>()
                .join("");
            let usage = r.usage.as_ref().map(|u| Usage {
                input: u64_at(u, &["input_tokens"]),
                output: u64_at(u, &["output_tokens"]),
                cache_read: u64_at(u, &["cache_read_input_tokens"]),
                cache_write: u64_at(u, &["cache_creation_input_tokens"]),
            });
            ChatOutput {
                text,
                thinking,
                usage,
                fallback: true,
            }
        })
    } else {
        serde_json::from_str::<OpenAiResp>(&body).ok().map(|r| {
            let msg = r.choices.into_iter().next().map(|c| c.message);
            let (text, thinking) = match msg {
                Some(m) => (
                    m.content.clone().unwrap_or_default(),
                    m.reasoning_content.unwrap_or_default(),
                ),
                None => (String::new(), String::new()),
            };
            let usage = r.usage.as_ref().map(|u| Usage {
                input: u64_at(u, &["prompt_tokens"]),
                output: u64_at(u, &["completion_tokens"]),
                cache_read: u64_at(u, &["prompt_cache_hit_tokens"])
                    .max(u64_at(u, &["cache_read_input_tokens"])),
                cache_write: u64_at(u, &["prompt_cache_miss_tokens"])
                    .max(u64_at(u, &["cache_creation_input_tokens"])),
            });
            ChatOutput {
                text,
                thinking,
                usage,
                fallback: true,
            }
        })
    };
    match out {
        Some(o) if !o.text.trim().is_empty() => {
            tracing::warn!("流式失败后非流式兜底成功");
            Ok(o)
        }
        _ => Err(LlmError {
            retryable: true,
            message: format!(
                "{cause}；兜底响应解析失败或为空；开头：{}",
                body.chars().take(200).collect::<String>()
            ),
        }),
    }
}

/// 连接测试：对档案发一个最小真实调用（ping），验证端点 + 密钥 + 模型名 + 协议。
/// 成功返回耗时毫秒；429 视为配置有效（限流说明上游已认 key）
pub async fn test_connection(cfg: &LlmConfig) -> Result<u64, LlmError> {
    let started = std::time::Instant::now();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| LlmError {
            retryable: false,
            message: format!("HTTP 客户端构建失败：{e}"),
        })?;
    let builder = if cfg.is_anthropic() {
        let url = anthropic_url(&cfg.base_url);
        client
            .post(url)
            .header("x-api-key", &cfg.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .json(&serde_json::json!({
                "model": cfg.model,
                "max_tokens": 8,
                "messages": [{ "role": "user", "content": "ping" }],
            }))
    } else {
        let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
        client
            .post(url)
            .bearer_auth(&cfg.api_key)
            .json(&serde_json::json!({
                "model": cfg.model,
                "messages": [{ "role": "user", "content": "ping" }],
                "max_tokens": 8,
            }))
    };
    let resp = builder.send().await.map_err(|e| LlmError {
        retryable: true,
        message: format!("连接失败：{e}"),
    })?;
    let status = resp.status();
    match status.as_u16() {
        429 => Ok(started.elapsed().as_millis() as u64), // 限流说明 key 已被认
        s if (200..300).contains(&s) => {
            // 只验证响应可读（不关心正文内容）
            let _ = resp.text().await;
            Ok(started.elapsed().as_millis() as u64)
        }
        401 | 403 => Err(LlmError {
            retryable: false,
            message: "密钥无效或无权限".into(),
        }),
        404 => Err(LlmError {
            retryable: false,
            message: "端点或模型不存在（检查端点与模型名）".into(),
        }),
        _ => {
            let body = resp.text().await.unwrap_or_default();
            let brief: String = body.chars().take(200).collect();
            Err(LlmError {
                retryable: status.is_server_error(),
                message: format!("上游返回 {status}：{brief}"),
            })
        }
    }
}
