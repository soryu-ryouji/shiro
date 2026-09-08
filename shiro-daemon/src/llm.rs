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
    /// 思考强度：None = 不发参数（默认）；low / medium / high → 按协议映射
    /// （openai → reasoning_effort；anthropic → thinking.budget_tokens）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
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
    /// 思考强度（同 Profile.thinking；加载档案时带入）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
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

/// 全局并行调用上限（config.toml [llm] max_concurrency；缺省 4，夹取 1..=8）
pub fn max_concurrency() -> u32 {
    std::fs::read_to_string(config_dir().join("config.toml"))
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|doc| doc.get("llm").cloned())
        .and_then(|v| v.try_into::<LlmSection>().ok())
        .and_then(|sec| sec.max_concurrency)
        .unwrap_or(4)
        .clamp(1, 8)
}

/// 写入全局并行上限（保留其他段落与字段）
pub(crate) fn write_max_concurrency(dir: &Path, value: u32) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("config.toml");
    let mut doc: toml::Table = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default();
    let mut sec = doc
        .get("llm")
        .and_then(|v| v.clone().try_into::<LlmSection>().ok())
        .unwrap_or_default();
    sec.max_concurrency = Some(value.clamp(1, 8));
    let value = toml::Value::try_from(sec).map_err(std::io::Error::other)?;
    doc.insert("llm".into(), value);
    let text = toml::to_string_pretty(&doc).map_err(std::io::Error::other)?;
    std::fs::write(&path, text)
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
        thinking: p.thinking.clone(),
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
    /// 全局 LLM 并行调用上限（拆解笔记/生成共用；缺省 4）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_concurrency: Option<u32>,
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
            thinking: None,
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
    let prev_max = doc
        .get("llm")
        .and_then(|v| v.clone().try_into::<LlmSection>().ok())
        .and_then(|sec| sec.max_concurrency);
    let section = LlmSection {
        default: default.map(String::from),
        max_concurrency: prev_max,
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

/// token 用量（归一化：DeepSeek 的 cache_hit/cache_miss、Anthropic 的 cache_read/cache_creation）
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Usage {
    /// 缓存命中率（input 中含 cache_read 时）
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
    /// 思考过程（reasoning/thinking 内容；不进正文，展示用）
    pub thinking: String,
    pub usage: Option<Usage>,
    /// 是否来自非流式兜底（流式中断后的降级路径）
    pub fallback: bool,
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
const ANTHROPIC_MAX_TOKENS: u32 = 16_384;
/// anthropic 路径把 system 从 messages 里提出来（协议要求 system 是顶层字段）
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// 兼容端点对 temperature 字段容忍度高（pi 经验：maxTokensField 类字段差异才是主要坑），
/// 0.4 固定值用于结构化提炼任务，不做配置项。
const TASK_TEMPERATURE: f32 = 0.4;

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
                eprintln!("[llm] {diag}，整体重试");
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
            eprintln!("[llm] 流式失败后非流式兜底成功");
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

enum StreamChunk {
    Text(String),
    Thinking(String),
    Usage(Usage),
    Done,
}

fn u64_at(v: &serde_json::Value, path: &[&str]) -> u64 {
    let mut cur = v;
    for key in path {
        cur = cur.get(*key).unwrap_or(&serde_json::Value::Null);
    }
    cur.as_u64().unwrap_or(0)
}

/// OpenAI 兼容 SSE 行解析：正文 delta / 思考 delta（reasoning_content）/ usage（末帧）/ [DONE]
fn parse_openai_line(line: &str) -> Option<StreamChunk> {
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
fn parse_anthropic_line(line: &str) -> Option<StreamChunk> {
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
fn merge_usage(slot: &mut Option<Usage>, u: Usage) {
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
fn openai_request<'a>(
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
fn anthropic_request<'a>(
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
fn retry_after_ms_from(headers: &reqwest::header::HeaderMap) -> Option<u64> {
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
async fn backoff(attempt: u32, server_hint_ms: Option<u64>) {
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

/// anthropic 消息端点拼接（供测试与其他调用复用）
fn anthropic_url(base: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/messages")
    } else {
        format!("{base}/v1/messages")
    }
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
                thinking: None,
            },
            Profile {
                key: "kimi-code".into(),
                base_url: "https://api.kimi.com/coding".into(),
                api_key: "sk-b".into(),
                model: "k3".into(),
                protocol: "anthropic".into(),
                thinking: Some("medium".into()),
            },
        ];
        write_profiles(&dir, &ps, Some("kimi-code")).unwrap();
        let (read_back, default) = read_profiles(&dir);
        assert_eq!(read_back.len(), 2);
        assert_eq!(default.as_deref(), Some("kimi-code"));
        let kimi = read_back.iter().find(|p| p.key == "kimi-code").unwrap();
        assert_eq!(kimi.protocol, "anthropic");
        assert_eq!(kimi.thinking.as_deref(), Some("medium"), "thinking 字段往返");
        let text2 = std::fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(text2.contains("thinking = \"medium\""));
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
