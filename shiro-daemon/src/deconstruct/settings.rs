// 角色制作运行设置（config.toml [deconstruct] 段）：
// 目前仅 segment_chars（切块目标切片长度）；模型档案与并发上限归 [llm] 段（llm.rs 管理）。

use crate::api::config_dir;
use crate::deconstruct::chunk::{DEFAULT_SEGMENT_CHARS, MAX_SEGMENT_CHARS, MIN_SEGMENT_CHARS};
use std::path::Path;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct DeconstructSection {
    /// 切块目标切片长度（字；缺省 10k）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    segment_chars: Option<usize>,
}

fn read_section() -> DeconstructSection {
    std::fs::read_to_string(config_dir().join("config.toml"))
        .ok()
        .and_then(|t| t.parse::<toml::Table>().ok())
        .and_then(|doc| doc.get("deconstruct").cloned())
        .and_then(|v| v.try_into::<DeconstructSection>().ok())
        .unwrap_or_default()
}

/// 读取切块目标长度（缺省 10k，夹取到允许范围）
pub fn segment_chars() -> usize {
    read_section()
        .segment_chars
        .unwrap_or(DEFAULT_SEGMENT_CHARS)
        .clamp(MIN_SEGMENT_CHARS, MAX_SEGMENT_CHARS)
}

/// 写入切块目标长度（保留其他段落与字段）
pub(crate) fn write_segment_chars(dir: &Path, value: usize) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("config.toml");
    let mut doc: toml::Table = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default();
    let mut sec = doc
        .get("deconstruct")
        .and_then(|v| v.clone().try_into::<DeconstructSection>().ok())
        .unwrap_or_default();
    sec.segment_chars = Some(value.clamp(MIN_SEGMENT_CHARS, MAX_SEGMENT_CHARS));
    let value = toml::Value::try_from(sec).map_err(std::io::Error::other)?;
    doc.insert("deconstruct".into(), value);
    let text = toml::to_string_pretty(&doc).map_err(std::io::Error::other)?;
    std::fs::write(&path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_chars_roundtrip() {
        let dir = std::env::temp_dir().join(format!("shiro-dc-set-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("config.toml"),
            "[lan]\naccess_key = \"keep-me\"\n",
        )
        .unwrap();

        // 缺省 10k；写出后读回；其他段落保留；越界值夹取
        // （segment_chars() 读全局 config_dir，这里只测写往返）
        write_segment_chars(&dir, 15_000).unwrap();
        let text = std::fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(text.contains("[deconstruct]"));
        assert!(text.contains("segment_chars = 15000"));
        assert!(text.contains("keep-me"));

        write_segment_chars(&dir, 999_999).unwrap();
        let text = std::fs::read_to_string(dir.join("config.toml")).unwrap();
        assert!(text.contains(&format!("segment_chars = {}", MAX_SEGMENT_CHARS)));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
