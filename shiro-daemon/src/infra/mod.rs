//! 基础设施层：与业务无关的通用能力（路径、文件系统、文本处理、时间）。

pub mod fs;
pub mod paths;
pub mod text;

/// 当前时间（epoch 秒）
pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
