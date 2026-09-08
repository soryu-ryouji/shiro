// 文件系统工具：原子写、回收站、文稿判定、修改时间。

use crate::error::{ApiError, internal_error};
use crate::infra::now_secs;
use std::path::Path;

/// 原子写（同目录临时文件 + rename）：写入中断不腐蚀已有内容。
/// 临时文件后缀 `.shiro-tmp`（目录监听过滤同后缀，避免把中间态推送给前端）。
pub(crate) fn atomic_write(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("shiro-tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)
}

/// 回收站目标路径：<项目>/.shiro/trash/<时间戳>-<原名>
pub(crate) fn move_to_trash(root: &Path, target: &Path) -> Result<(), ApiError> {
    let trash = root.join(".shiro").join("trash");
    std::fs::create_dir_all(&trash).map_err(internal_error)?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "item".into());
    std::fs::rename(target, trash.join(format!("{}-{}", now_secs(), name))).map_err(internal_error)
}

/// 文稿文件判定：.md/.markdown（markdown）与 .txt（纯文本）
pub(crate) fn is_sheet_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".txt")
}

pub(crate) fn file_mtime(path: &Path) -> u64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
