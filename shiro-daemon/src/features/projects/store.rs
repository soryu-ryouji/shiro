// 项目登记与 history.toml 持久化（~/.config/shiro/history.toml，见 docs/backend/storage.md）。
// 项目以绝对路径为身份（文件夹是唯一权威数据源），登记校验是内容端点的访问边界。

use crate::error::{ApiError, bad_request, forbidden, internal_error};
use crate::infra::now_secs;
use crate::infra::paths::{config_folder, folder_name, same_path};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Default)]
struct History {
    #[serde(default)]
    projects: Vec<HistoryEntry>,
}

#[derive(Serialize, Deserialize)]
struct HistoryEntry {
    path: String,
    /// 最近打开时间（epoch 秒）
    opened_at: u64,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ProjectItem {
    /// 项目文件夹绝对路径
    pub path: String,
    /// 显示名（目录 basename）
    pub name: String,
    /// 目录当前是否存在（已移动/删除的历史项返回 false，前端置灰）
    pub exists: bool,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ProjectListResponse {
    pub projects: Vec<ProjectItem>,
}

fn load_history() -> History {
    let Ok(text) = std::fs::read_to_string(config_folder().join("history.toml")) else {
        return History::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

fn save_history(history: &History) -> std::io::Result<()> {
    std::fs::create_dir_all(config_folder())?;
    let text = toml::to_string_pretty(history).map_err(std::io::Error::other)?;
    std::fs::write(config_folder().join("history.toml"), text)
}

/// 项目列表（history.toml 记录，最近打开在前）
pub(crate) fn list() -> Vec<ProjectItem> {
    load_history()
        .projects
        .iter()
        .map(|e| ProjectItem {
            name: project_display_name(&e.path),
            path: e.path.clone(),
            exists: Path::new(&e.path).is_dir(),
        })
        .collect()
}

/// 项目显示名：.shiro/project.toml 的 name（缺失、损坏或为空时回退目录 basename）
fn project_display_name(path: &str) -> String {
    let toml_path = Path::new(path).join(".shiro").join("project.toml");
    std::fs::read_to_string(&toml_path)
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .and_then(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| folder_name(path))
}

/// 写项目显示名到 .shiro/project.toml（解析保留其他字段；文件不存在或损坏则新建）
fn write_project_name(target: &Path, name: &str) -> Result<(), ApiError> {
    let toml_path = target.join(".shiro").join("project.toml");
    let mut doc: toml::Table = std::fs::read_to_string(&toml_path)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_default();
    doc.insert("name".into(), toml::Value::String(name.into()));
    let text =
        toml::to_string_pretty(&doc).map_err(|e| bad_request(&format!("配置写入失败：{e}")))?;
    std::fs::write(&toml_path, text).map_err(internal_error)
}

/// 登记项目：规范化路径（canonicalize，避免尾斜杠/符号链接重复），补齐 .shiro/ 元数据，
/// 写入 history（去重、最新在前）。display_name 有值时写入 project.toml 的 name。
pub(crate) fn register(path: &str, display_name: Option<&str>) -> Result<ProjectItem, ApiError> {
    let target = std::fs::canonicalize(path.trim()).map_err(|_| bad_request("项目文件夹不存在"))?;
    if !target.is_dir() {
        return Err(bad_request("项目文件夹不存在"));
    }
    // 只初始化 .shiro/ 元数据目录；正文/、大纲/ 等是推荐约定而非强制结构，不主动创建（见 docs/backend/storage.md）
    std::fs::create_dir_all(target.join(".shiro")).map_err(internal_error)?;
    match display_name {
        Some(name) => write_project_name(&target, name)?,
        None => {
            let project_toml = target.join(".shiro").join("project.toml");
            if !project_toml.exists() {
                let name = folder_name(&target.to_string_lossy());
                std::fs::write(&project_toml, format!("name = {:?}\n", name))
                    .map_err(internal_error)?;
            }
        }
    }

    let key = target.to_string_lossy().to_string();
    let mut history = load_history();
    history.projects.retain(|e| e.path != key);
    history.projects.insert(
        0,
        HistoryEntry {
            path: key.clone(),
            opened_at: now_secs(),
        },
    );
    save_history(&history).map_err(internal_error)?;
    Ok(ProjectItem {
        name: project_display_name(&key),
        path: key,
        exists: true,
    })
}

/// 移除项目记录（只从 history.toml 移除，不删除文件夹本身）；请求路径容忍非规范形式
pub(crate) fn remove(path: &str) {
    let key = std::fs::canonicalize(path.trim()).ok();
    let mut history = load_history();
    history.projects.retain(|e| match &key {
        Some(k) => !same_path(&e.path, k),
        None => e.path != path,
    });
    let _ = save_history(&history);
}

/// 重命名项目文件夹本体，history 记录同步更新；返回更新后的项目
pub(crate) fn rename(src: &Path, new_name: &str) -> Result<ProjectItem, ApiError> {
    let Some(parent) = src.parent() else {
        return Err(bad_request("不能重命名根目录"));
    };
    let dst = parent.join(new_name);
    if dst == src {
        return Ok(ProjectItem {
            path: src.to_string_lossy().to_string(),
            name: new_name.into(),
            exists: true,
        });
    }
    if dst.exists() {
        return Err(crate::error::conflict("目标目录已存在"));
    }
    std::fs::rename(src, &dst).map_err(internal_error)?;

    // 文件夹名与项目显示名（.shiro/project.toml 的 name）解耦：重命名文件夹不改显示名
    let new_path = dst.to_string_lossy().to_string();
    let mut history = load_history();
    for entry in &mut history.projects {
        if same_path(&entry.path, src) {
            entry.path = new_path.clone();
            entry.opened_at = now_secs();
        }
    }
    save_history(&history).map_err(internal_error)?;
    Ok(ProjectItem {
        name: project_display_name(&new_path),
        path: new_path,
        exists: true,
    })
}

/// 项目登记校验：路径必须已登记在 history.toml（防止通过 API 读写任意路径）。
/// 返回规范化后的项目根（canonicalize：解析符号链接、去尾斜杠），后续路径拼接均以它为根。
pub(crate) fn ensure_registered(path: &str) -> Result<PathBuf, ApiError> {
    let root = std::fs::canonicalize(path).map_err(|_| bad_request("项目目录不存在"))?;
    if !root.is_dir() {
        return Err(bad_request("项目目录不存在"));
    }
    let registered = load_history()
        .projects
        .iter()
        .any(|e| same_path(&e.path, &root));
    if registered {
        Ok(root)
    } else {
        Err(forbidden("项目未登记：请先在 Project 页打开该项目"))
    }
}
