// 路径工具：配置目录、显示名、项目内相对路径安全校验。

use crate::error::{ApiError, bad_request};
use std::path::{Path, PathBuf};

/// 全局配置目录（~/.config/shiro：config.toml / history.toml / db）
pub(crate) fn config_folder() -> PathBuf {
    std::env::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("shiro")
}

/// 路径显示名：目录 basename（容忍尾部斜杠）
pub(crate) fn folder_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    Path::new(trimmed)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| trimmed.to_string())
}

/// 项目内相对路径安全校验：拒绝绝对路径与 .. 逃逸，返回拼接后的完整路径
pub(crate) fn resolve_inside(root: &Path, rel: &str) -> Result<PathBuf, ApiError> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute()
        || rel.split(['/', '\\']).any(|seg| seg == ".." || seg == ".")
        || rel.trim().is_empty()
    {
        return Err(bad_request("非法的文件路径"));
    }
    Ok(root.join(rel_path))
}

/// 路径等价：字符串相等，或 canonicalize 后相等（容忍尾斜杠、符号链接差异）
pub(crate) fn same_path(a: &str, b: &Path) -> bool {
    let pa = Path::new(a);
    if pa == b {
        return true;
    }
    std::fs::canonicalize(pa).map(|ca| ca == b).unwrap_or(false)
}
