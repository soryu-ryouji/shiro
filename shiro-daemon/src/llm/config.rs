// LLM 档案配置（config.toml 的 [llm] 段）：多档案读写、默认档案、并发上限、key 掩码。

use crate::infra::paths::config_folder;
use serde::{Deserialize, Serialize};
use std::path::Path;

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

/// 全局并行调用上限（config.toml [llm] max_concurrency；缺省 4，夹取 1..=8）。
/// 多路并行管线（拆书/剧本类）的并发闸门，暂无调用方，为后续管线保留。
#[allow(dead_code)]
pub fn max_concurrency() -> u32 {
    std::fs::read_to_string(config_folder().join("config.toml"))
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|doc| doc.get("llm").cloned())
        .and_then(|v| v.try_into::<LlmSection>().ok())
        .and_then(|sec| sec.max_concurrency)
        .unwrap_or(4)
        .clamp(1, 8)
}

/// 写入全局并行上限（保留其他段落与字段）；暂无调用方，为后续管线保留
#[allow(dead_code)]
pub(crate) fn write_max_concurrency(folder: &Path, value: u32) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(folder)?;
    let path = folder.join("config.toml");
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
    let (profiles, default_key) = read_profiles(&config_folder());
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
pub(crate) fn read_profiles(folder: &Path) -> (Vec<Profile>, Option<String>) {
    let Some(sec) = std::fs::read_to_string(folder.join("config.toml"))
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
    folder: &Path,
    profiles: &[Profile],
    default: Option<&str>,
) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(folder)?;
    let path = folder.join("config.toml");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_roundtrip_and_migration() {
        let folder = std::env::temp_dir().join(format!("shiro-llm-prof-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("config.toml"),
            "[lan]\naccess_key = \"keep-me\"\n",
        )
        .unwrap();

        // 旧格式：顶层字段直接写 [llm] 段 → 读取时迁移合成为单档案
        let legacy = "[lan]\naccess_key = \"keep-me\"\n\n[llm]\nbase_url = \"https://api.deepseek.com/v1\"\napi_key = \"sk-legacy\"\nmodel = \"deepseek-chat\"\n";
        std::fs::write(folder.join("config.toml"), legacy).unwrap();
        let (profiles, default) = read_profiles(&folder);
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
        write_profiles(&folder, &ps, Some("kimi-code")).unwrap();
        let (read_back, default) = read_profiles(&folder);
        assert_eq!(read_back.len(), 2);
        assert_eq!(default.as_deref(), Some("kimi-code"));
        let kimi = read_back.iter().find(|p| p.key == "kimi-code").unwrap();
        assert_eq!(kimi.protocol, "anthropic");
        assert_eq!(kimi.thinking.as_deref(), Some("medium"), "thinking 字段往返");
        let text2 = std::fs::read_to_string(folder.join("config.toml")).unwrap();
        assert!(text2.contains("thinking = \"medium\""));
        let text = std::fs::read_to_string(folder.join("config.toml")).unwrap();
        assert!(text.contains("[[llm.profiles]]"));
        assert!(text.contains("default = \"kimi-code\""));
        assert!(text.contains("keep-me"), "其他段落保留");
        assert!(!text.contains("sk-legacy"), "旧字段不再写出");

        // 默认项缺失时回退第一个档案
        write_profiles(&folder, &ps, None).unwrap();
        let (_, default) = read_profiles(&folder);
        assert_eq!(default.as_deref(), Some("deepseek"));

        std::fs::remove_dir_all(&folder).unwrap();
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
        let folder = std::env::temp_dir().join(format!("shiro-llm-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let (profiles, default) = read_profiles(&folder);
        assert!(profiles.is_empty());
        assert_eq!(default, None);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
