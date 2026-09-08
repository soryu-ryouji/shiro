// 项目内条目操作：目录树、正文预览、文稿/目录 CRUD、重命名。
// 全部以已登记的规范化项目根（store::ensure_registered 的返回值）为根。

use crate::error::{ApiError, bad_request, conflict, internal_error, not_found};
use crate::infra::fs::{atomic_write, file_mtime, is_sheet_file, move_to_trash};
use crate::infra::paths::{resolve_inside, validate_name};
use crate::infra::text::{excerpt_from_content, read_file_head};
use serde::Serialize;
use std::path::Path;
use utoipa::ToSchema;

// ---- 目录树 ----

#[derive(Serialize, ToSchema)]
pub(crate) struct TreeNode {
    /// 节点名（文件含 .md/.markdown/.txt 后缀）
    pub name: String,
    /// 项目内相对路径
    pub path: String,
    /// 节点类型：folder / file
    pub kind: String,
    /// 文件修改时间（epoch 秒，仅 file）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<u64>,
    /// 子节点（仅 folder）
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(no_recursion)]
    pub children: Option<Vec<TreeNode>>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct TreeResponse {
    pub children: Vec<TreeNode>,
}

/// 递归构建目录树：只含目录与文稿文件（.md/.markdown/.txt）；排除 . 开头项（.shiro 等）；目录在前，按名称排序
fn build_tree(folder: &Path, root: &Path) -> Vec<TreeNode> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if path.is_dir() {
            folders.push(TreeNode {
                name,
                path: rel,
                kind: "folder".into(),
                modified: None,
                children: Some(build_tree(&path, root)),
            });
        } else if is_sheet_file(&name) {
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            files.push(TreeNode {
                name,
                path: rel,
                kind: "file".into(),
                modified,
                children: None,
            });
        }
    }
    folders.sort_by(|a, b| a.name.cmp(&b.name));
    files.sort_by(|a, b| a.name.cmp(&b.name));
    folders.extend(files);
    folders
}

pub(crate) fn tree(root: &Path) -> Vec<TreeNode> {
    build_tree(root, root)
}

// ---- 正文预览 ----

#[derive(Serialize, ToSchema)]
pub(crate) struct SheetExcerpt {
    /// 文稿在项目内的相对路径
    pub file: String,
    /// 正文预览（markdown 剥离标记后取开头约 160 字；纯文本直接取开头）
    pub excerpt: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct ExcerptsResponse {
    pub excerpts: Vec<SheetExcerpt>,
}

/// 目录内全部文稿的正文预览（每篇取开头几行，Ulysses 式列表）；folder 为空 = 项目根
pub(crate) fn excerpts(root: &Path, folder: &str) -> Result<Vec<SheetExcerpt>, ApiError> {
    const HEAD_BYTES: usize = 4096;
    const MAX_CHARS: usize = 160;
    let folder = if folder.is_empty() {
        root.to_path_buf()
    } else {
        let d = resolve_inside(root, folder)?;
        if !d.is_dir() {
            return Err(bad_request("目录不存在"));
        }
        d
    };
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return Ok(out);
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !is_sheet_file(&name) {
            continue;
        }
        let is_markdown = !name.to_lowercase().ends_with(".txt");
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map(|s| s.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let excerpt = read_file_head(&path, HEAD_BYTES)
            .map(|head| excerpt_from_content(&head, MAX_CHARS, is_markdown))
            .unwrap_or_default();
        out.push(SheetExcerpt { file: rel, excerpt });
    }
    Ok(out)
}

// ---- 文稿 CRUD ----

#[derive(Serialize, ToSchema)]
pub(crate) struct FileContent {
    pub content: String,
    /// 修改时间（epoch 秒）
    pub modified: u64,
}

pub(crate) fn read_file(root: &Path, file: &str) -> Result<FileContent, ApiError> {
    let target = resolve_inside(root, file)?;
    if !target.is_file() {
        return Err(not_found("文件不存在"));
    }
    let content = std::fs::read_to_string(&target).map_err(internal_error)?;
    Ok(FileContent {
        content,
        modified: file_mtime(&target),
    })
}

/// 保存文稿：同目录临时文件 + rename 原子覆盖，写入中断不腐蚀已有正文（见 storage.md 备份与快照）
pub(crate) fn write_file(root: &Path, file: &str, content: &str) -> Result<FileContent, ApiError> {
    let target = resolve_inside(root, file)?;
    atomic_write(&target, content).map_err(internal_error)?;
    Ok(FileContent {
        content: content.to_string(),
        modified: file_mtime(&target),
    })
}

pub(crate) fn create_file(root: &Path, file: &str) -> Result<FileContent, ApiError> {
    let target = resolve_inside(root, file)?;
    if target.exists() {
        return Err(conflict("文件已存在"));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(internal_error)?;
    }
    std::fs::write(&target, "").map_err(internal_error)?;
    Ok(FileContent {
        content: String::new(),
        modified: file_mtime(&target),
    })
}

pub(crate) fn remove_file(root: &Path, file: &str) -> Result<(), ApiError> {
    let target = resolve_inside(root, file)?;
    if !target.is_file() {
        return Err(not_found("文件不存在"));
    }
    move_to_trash(root, &target)
}

// ---- 目录操作 ----

/// 新建目录（幂等）；返回 true = 本次新建，false = 已存在
pub(crate) fn create_folder(root: &Path, folder: &str) -> Result<bool, ApiError> {
    let target = resolve_inside(root, folder)?;
    if target.is_dir() {
        return Ok(false);
    }
    std::fs::create_dir_all(&target).map_err(internal_error)?;
    Ok(true)
}

/// 删除目录：空目录直接删除；非空目录移入项目回收站（.shiro/trash/，可手动恢复）
pub(crate) fn remove_folder(root: &Path, folder: &str) -> Result<(), ApiError> {
    if folder.split(['/', '\\']).next() == Some(".shiro") {
        return Err(bad_request(".shiro 是 shiro 的工作目录，不能删除"));
    }
    let target = resolve_inside(root, folder)?;
    if !target.is_dir() {
        return Err(not_found("目录不存在"));
    }
    let is_empty = target
        .read_dir()
        .map(|mut d| d.next().is_none())
        .unwrap_or(false);
    if is_empty {
        std::fs::remove_dir(&target).map_err(internal_error)
    } else {
        move_to_trash(root, &target)
    }
}

// ---- 重命名条目 ----

/// 重命名项目内条目（文件或目录）：同目录改名
pub(crate) fn rename_entry(root: &Path, rel: &str, new_name: &str) -> Result<(), ApiError> {
    if rel.split(['/', '\\']).next() == Some(".shiro") {
        return Err(bad_request(".shiro 是 shiro 的工作目录，不能重命名"));
    }
    let src = resolve_inside(root, rel)?;
    if !src.exists() {
        return Err(not_found("条目不存在"));
    }
    let name = validate_name(new_name)?;
    let dst = src.parent().unwrap_or(root).join(name);
    if dst == src {
        return Ok(());
    }
    if dst.exists() {
        return Err(conflict("目标已存在"));
    }
    std::fs::rename(&src, &dst).map_err(internal_error)
}
