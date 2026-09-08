use crate::watch;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
    response::sse::{Event, KeepAlive, Sse},
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::BroadcastStream;
use tower_http::services::{ServeDir, ServeFile};
use utoipa::{Modify, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(Clone)]
pub struct AppState {
    pub token: String,
    pub watch_hub: watch::WatchHub,
    /// Chat 生成守卫（同一会话并发限制）
    pub chat: std::sync::Arc<crate::chat::Hub>,
}

#[derive(Serialize, ToSchema, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)] // Starting/Error 为后续初始化流程预留
pub enum StartupStatus {
    Starting,
    Ready,
    Error,
}

#[derive(Serialize, ToSchema)]
pub struct StartupResponse {
    pub status: StartupStatus,
}

/// 启动状态。当前无后台初始化流程，直接返回 ready；
/// 后续加入初始化流程后按 starting → ready/error 流转。
#[utoipa::path(
    post,
    path = "/api/v1/app/startup",
    tag = "app",
    responses(
        (status = 200, description = "启动状态", body = StartupResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn startup() -> Json<StartupResponse> {
    Json(StartupResponse {
        status: StartupStatus::Ready,
    })
}

async fn health() -> &'static str {
    "ok"
}

// ---- 项目管理（打开记录存 ~/.config/shiro/history.toml，见 docs/backend/storage.md） ----

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

pub(crate) fn config_folder() -> PathBuf {
    std::env::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("shiro")
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

pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 名称合法性校验（目录/文件名共用）：非空、非 . ..、不含路径与 Windows 保留字符
fn validate_name(name: &str) -> Result<&str, ApiError> {
    const INVALID: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.chars().any(|c| INVALID.contains(&c))
    {
        return Err(bad_request(
            "名称不能为空，且不能包含 \\ / : * ? \" < > | 字符",
        ));
    }
    Ok(name)
}

/// 回收站目标路径：<项目>/.shiro/trash/<时间戳>-<原名>
fn move_to_trash(root: &Path, target: &Path) -> Result<(), ApiError> {
    let trash = root.join(".shiro").join("trash");
    std::fs::create_dir_all(&trash).map_err(internal_error)?;
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "item".into());
    std::fs::rename(target, trash.join(format!("{}-{}", now_secs(), name))).map_err(internal_error)
}

/// 路径显示名：目录 basename（容忍尾部斜杠）
pub(crate) fn folder_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    Path::new(trimmed)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| trimmed.to_string())
}

#[derive(Serialize, ToSchema)]
pub struct ProjectItem {
    /// 项目文件夹绝对路径
    pub path: String,
    /// 显示名（目录 basename）
    pub name: String,
    /// 目录当前是否存在（已移动/删除的历史项返回 false，前端置灰）
    pub exists: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ProjectListResponse {
    pub projects: Vec<ProjectItem>,
}

#[derive(Serialize, ToSchema)]
pub struct ErrorResponse {
    pub message: String,
}

pub(crate) type ApiError = (StatusCode, Json<ErrorResponse>);

pub(crate) fn bad_request(message: &str) -> ApiError {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            message: message.into(),
        }),
    )
}

pub(crate) fn internal_error(e: std::io::Error) -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            message: e.to_string(),
        }),
    )
}

/// 项目列表（history.toml 记录，最近打开在前）
#[utoipa::path(
    post,
    path = "/api/v1/projects/list",
    tag = "projects",
    responses(
        (status = 200, description = "项目列表", body = ProjectListResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn list_projects() -> Json<ProjectListResponse> {
    let history = load_history();
    let projects = history
        .projects
        .iter()
        .map(|e| ProjectItem {
            name: project_display_name(&e.path),
            path: e.path.clone(),
            exists: Path::new(&e.path).is_dir(),
        })
        .collect();
    Json(ProjectListResponse { projects })
}

#[derive(Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    /// 项目文件夹绝对路径（须已存在；任意内容的文件夹均可）
    pub path: String,
    /// 项目显示名（选填；写入 .shiro/project.toml 的 name，留空则保留已有或用文件夹名）
    #[serde(default)]
    pub name: String,
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
    let text = toml::to_string_pretty(&doc)
        .map_err(|e| bad_request(&format!("配置写入失败：{e}")))?;
    std::fs::write(&toml_path, text).map_err(internal_error)
}

/// 新建项目：把选中的已有文件夹登记为项目（VSCode「打开文件夹」式），并导入记录。
/// 项目显示名写入 .shiro/project.toml 的 name（与文件夹名解耦）；留空则保留已有或用文件夹名。
#[utoipa::path(
    post,
    path = "/api/v1/projects/create",
    tag = "projects",
    request_body = CreateProjectRequest,
    responses(
        (status = 201, description = "已创建", body = ProjectItem),
        (status = 400, description = "参数错误", body = ErrorResponse),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
async fn create_project(
    Json(req): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ProjectItem>), ApiError> {
    // 规范化（解析符号链接、去尾斜杠）：避免同一文件夹以不同字符串重复登记
    let target = std::fs::canonicalize(req.path.trim())
        .map_err(|_| bad_request("项目文件夹不存在"))?;
    if !target.is_dir() {
        return Err(bad_request("项目文件夹不存在"));
    }
    let name = req.name.trim();
    let display_name = (!name.is_empty()).then_some(name);
    let key = register_project(&target, display_name)?;
    Ok((
        StatusCode::CREATED,
        Json(ProjectItem {
            path: key,
            name: project_display_name(&target.to_string_lossy()),
            exists: true,
        }),
    ))
}

/// 项目登记：补齐缺失的 .shiro/ 元数据并写入 history（去重、最新在前），返回项目路径。
/// display_name 有值时写入 project.toml 的 name（保留其他字段），无值时仅在配置缺失时以文件夹名初始化。
fn register_project(target: &Path, display_name: Option<&str>) -> Result<String, ApiError> {
    // 只初始化 .shiro/ 元数据目录；正文/、大纲/ 等是推荐约定而非强制结构，不主动创建（见 docs/backend/storage.md）
    std::fs::create_dir_all(target.join(".shiro")).map_err(internal_error)?;
    match display_name {
        Some(name) => write_project_name(target, name)?,
        None => {
            let project_toml = target.join(".shiro").join("project.toml");
            if !project_toml.exists() {
                let name = folder_name(&target.to_string_lossy());
                std::fs::write(&project_toml, format!("name = {:?}\n", name))
                    .map_err(internal_error)?;
            }
        }
    }

    // 导入记录：去重、最新在前
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
    Ok(key)
}

#[derive(Deserialize, ToSchema)]
pub struct RemoveProjectRequest {
    /// 要移除记录的项目路径（只删记录，不删文件夹）
    pub path: String,
}

/// 删除项目记录（只从 history.toml 移除，不删除文件夹本身）
#[utoipa::path(
    post,
    path = "/api/v1/projects/remove",
    tag = "projects",
    request_body = RemoveProjectRequest,
    responses(
        (status = 204, description = "已删除"),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn remove_project(Json(req): Json<RemoveProjectRequest>) -> StatusCode {
    // 请求路径与登记路径都容忍非规范形式（尾斜杠/符号链接）
    let key = std::fs::canonicalize(req.path.trim()).ok();
    let mut history = load_history();
    history.projects.retain(|e| match &key {
        Some(k) => !same_path(&e.path, k),
        None => e.path != req.path,
    });
    let _ = save_history(&history);
    StatusCode::NO_CONTENT
}

// ---- 项目文件（真实文件夹直读直写，见 docs/backend/storage.md） ----

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
fn same_path(a: &str, b: &Path) -> bool {
    let pa = Path::new(a);
    if pa == b {
        return true;
    }
    std::fs::canonicalize(pa).map(|ca| ca == b).unwrap_or(false)
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
        Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                message: "项目未登记：请先在 Project 页打开该项目".into(),
            }),
        ))
    }
}

#[derive(Serialize, ToSchema)]
pub struct TreeNode {
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

/// 文稿文件判定：.md/.markdown（markdown）与 .txt（纯文本）
pub(crate) fn is_sheet_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".txt")
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

#[derive(Deserialize, ToSchema)]
pub struct TreeRequest {
    /// 项目根目录绝对路径
    pub path: String,
}

#[derive(Serialize, ToSchema)]
pub struct TreeResponse {
    pub children: Vec<TreeNode>,
}

/// 项目目录树（目录与文稿文件；目录在前，文件名排序）
#[utoipa::path(
    post,
    path = "/api/v1/projects/tree",
    tag = "projects",
    request_body = TreeRequest,
    responses(
        (status = 200, description = "目录树", body = TreeResponse),
        (status = 400, description = "项目目录不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn project_tree(Json(req): Json<TreeRequest>) -> Result<Json<TreeResponse>, ApiError> {
    let root = ensure_registered(&req.path)?;
    Ok(Json(TreeResponse {
        children: build_tree(&root, &root),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct ExcerptsRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径（'' = 项目根）
    pub folder: String,
}

#[derive(Serialize, ToSchema)]
pub struct SheetExcerpt {
    /// 文稿在项目内的相对路径
    pub file: String,
    /// 正文预览（markdown 剥离标记后取开头约 160 字；纯文本直接取开头）
    pub excerpt: String,
}

#[derive(Serialize, ToSchema)]
pub struct ExcerptsResponse {
    pub excerpts: Vec<SheetExcerpt>,
}

/// 读文件头部（最多 max_bytes 字节；截断处的多字节字符经 lossy 变替换符，由剥离逻辑过滤）
fn read_file_head(path: &Path, max_bytes: usize) -> Option<String> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    f.take(max_bytes as u64).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// 剥离单行 markdown 块级标记（# 标题、> 引用、-/*/+ 与 "1." 列表），返回行内文本
fn strip_markdown_line(line: &str) -> String {
    let mut s = line.trim();
    loop {
        let t = s.trim_start();
        let Some(c) = t.chars().next() else { break };
        match c {
            '#' | '>' | '-' | '*' | '+' => s = &t[c.len_utf8()..],
            '0'..='9' => {
                let digits = t
                    .char_indices()
                    .take_while(|(_, ch)| ch.is_ascii_digit())
                    .count();
                let rest = &t[digits..];
                match rest.strip_prefix(". ") {
                    Some(after) => s = after,
                    None => break,
                }
            }
            _ => break,
        }
    }
    s.trim().to_string()
}

/// 剥离行内 markdown：图片 ![alt](url) 整体移除，链接 [text](url) 保留 text，粗体/行内码/删除线标记移除
fn strip_inline_markdown(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    loop {
        let img = rest.find("![");
        let link = rest.find('[');
        // 图片与链接标记取更靠前的一个
        let take_img = match (img, link) {
            (Some(i), Some(l)) => i < l,
            (Some(_), None) => true,
            (None, _) => false,
        };
        if take_img {
            let i = img.expect("take_img 为 true 时 img 必为 Some");
            out.push_str(&rest[..i]);
            match rest[i..]
                .find("](")
                .and_then(|j| rest[i + j..].find(')').map(|k| (j, k)))
            {
                Some((j, k)) => rest = &rest[i + j + k + 1..],
                None => {
                    rest = &rest[i + 2..];
                }
            }
        } else if let Some(i) = link {
            out.push_str(&rest[..i]);
            let Some(j) = rest[i..].find(']') else {
                rest = &rest[i + 1..];
                continue;
            };
            let text = &rest[i + 1..i + j];
            out.push_str(text);
            if let Some(after) = rest[i + j..].strip_prefix("](") {
                match after.find(')') {
                    Some(k) => rest = &after[k + 1..],
                    None => rest = after,
                }
            } else {
                rest = &rest[i + j + 1..];
            }
        } else {
            out.push_str(rest);
            break;
        }
    }
    out.chars()
        .filter(|c| !matches!(c, '*' | '`' | '~'))
        .collect()
}

/// 从文稿内容提取正文预览：markdown 逐行剥离标记，纯文本直接取开头约 max_chars 字
pub(crate) fn excerpt_from_content(content: &str, max_chars: usize, is_markdown: bool) -> String {
    let mut out = String::new();
    for line in content.lines() {
        let text = if is_markdown {
            strip_inline_markdown(&strip_markdown_line(line))
        } else {
            line.trim().to_string()
        };
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&text);
        if out.chars().count() >= max_chars {
            break;
        }
    }
    if out.chars().count() > max_chars {
        out.chars().take(max_chars).collect()
    } else {
        out
    }
}

/// 目录内全部文稿的正文预览（每篇取开头几行，Ulysses 式列表）
#[utoipa::path(
    post,
    path = "/api/v1/projects/excerpts",
    tag = "projects",
    request_body = ExcerptsRequest,
    responses(
        (status = 200, description = "预览列表", body = ExcerptsResponse),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn project_excerpts(
    Json(req): Json<ExcerptsRequest>,
) -> Result<Json<ExcerptsResponse>, ApiError> {
    const HEAD_BYTES: usize = 4096;
    const MAX_CHARS: usize = 160;
    let root = ensure_registered(&req.path)?;
    let folder = if req.folder.is_empty() {
        root.clone()
    } else {
        let d = resolve_inside(&root, &req.folder)?;
        if !d.is_dir() {
            return Err(bad_request("目录不存在"));
        }
        d
    };
    let mut excerpts = Vec::new();
    let Ok(entries) = std::fs::read_dir(&folder) else {
        return Ok(Json(ExcerptsResponse { excerpts }));
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let lower = name.to_lowercase();
        if !is_sheet_file(&name) {
            continue;
        }
        let is_markdown = !lower.ends_with(".txt");
        let p = entry.path();
        let rel = p
            .strip_prefix(&root)
            .map(|s| s.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        let excerpt = read_file_head(&p, HEAD_BYTES)
            .map(|head| excerpt_from_content(&head, MAX_CHARS, is_markdown))
            .unwrap_or_default();
        excerpts.push(SheetExcerpt { file: rel, excerpt });
    }
    Ok(Json(ExcerptsResponse { excerpts }))
}

#[derive(Deserialize, ToSchema)]
pub struct ReadFileRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径（如 正文/第一卷/001 序章.md）
    pub file: String,
}

#[derive(Serialize, ToSchema)]
pub struct FileContent {
    pub content: String,
    /// 修改时间（epoch 秒）
    pub modified: u64,
}

pub(crate) fn file_mtime(path: &Path) -> u64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 读取文稿内容
#[utoipa::path(
    post,
    path = "/api/v1/projects/file/read",
    tag = "projects",
    request_body = ReadFileRequest,
    responses(
        (status = 200, description = "文稿内容", body = FileContent),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 404, description = "文件不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn read_project_file(Json(req): Json<ReadFileRequest>) -> Result<Json<FileContent>, ApiError> {
    let target = resolve_inside(&ensure_registered(&req.path)?, &req.file)?;
    if !target.is_file() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                message: "文件不存在".into(),
            }),
        ));
    }
    let content = std::fs::read_to_string(&target).map_err(internal_error)?;
    Ok(Json(FileContent {
        content,
        modified: file_mtime(&target),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct WriteFileRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径
    pub file: String,
    pub content: String,
}

/// 保存文稿：同目录临时文件 + rename 原子覆盖，写入中断不腐蚀已有正文（见 storage.md 备份与快照）
#[utoipa::path(
    post,
    path = "/api/v1/projects/file/write",
    tag = "projects",
    request_body = WriteFileRequest,
    responses(
        (status = 200, description = "已保存，返回新修改时间", body = FileContent),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
async fn write_project_file(
    Json(req): Json<WriteFileRequest>,
) -> Result<Json<FileContent>, ApiError> {
    let target = resolve_inside(&ensure_registered(&req.path)?, &req.file)?;
    let tmp = target.with_extension("md.shiro-tmp");
    std::fs::write(&tmp, &req.content).map_err(internal_error)?;
    std::fs::rename(&tmp, &target).map_err(internal_error)?;
    Ok(Json(FileContent {
        content: req.content,
        modified: file_mtime(&target),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateFileRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径（父目录自动创建）
    pub file: String,
}

/// 新建文稿（空文件）
#[utoipa::path(
    post,
    path = "/api/v1/projects/file/create",
    tag = "projects",
    request_body = CreateFileRequest,
    responses(
        (status = 201, description = "已创建", body = FileContent),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 409, description = "文件已存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn create_project_file(
    Json(req): Json<CreateFileRequest>,
) -> Result<(StatusCode, Json<FileContent>), ApiError> {
    let target = resolve_inside(&ensure_registered(&req.path)?, &req.file)?;
    if target.exists() {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                message: "文件已存在".into(),
            }),
        ));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(internal_error)?;
    }
    std::fs::write(&target, "").map_err(internal_error)?;
    Ok((
        StatusCode::CREATED,
        Json(FileContent {
            content: String::new(),
            modified: file_mtime(&target),
        }),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateFolderRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径（多级自动创建）
    pub folder: String,
}

/// 新建目录（幂等：已存在返回 200）
#[utoipa::path(
    post,
    path = "/api/v1/projects/folder/create",
    tag = "projects",
    request_body = CreateFolderRequest,
    responses(
        (status = 201, description = "已创建"),
        (status = 200, description = "目录已存在"),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn create_project_folder(
    Json(req): Json<CreateFolderRequest>,
) -> Result<StatusCode, ApiError> {
    let target = resolve_inside(&ensure_registered(&req.path)?, &req.folder)?;
    if target.is_dir() {
        return Ok(StatusCode::OK);
    }
    std::fs::create_dir_all(&target).map_err(internal_error)?;
    Ok(StatusCode::CREATED)
}

#[derive(Deserialize, ToSchema)]
pub struct RemoveFolderRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 项目内相对路径（不允许根目录与 .shiro）
    pub folder: String,
}

/// 删除目录：空目录直接删除；非空目录移入项目回收站（.shiro/trash/，可手动恢复）
#[utoipa::path(
    post,
    path = "/api/v1/projects/folder/delete",
    tag = "projects",
    request_body = RemoveFolderRequest,
    responses(
        (status = 204, description = "已删除"),
        (status = 400, description = "路径非法或受保护", body = ErrorResponse),
        (status = 404, description = "目录不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn remove_project_folder(
    Json(req): Json<RemoveFolderRequest>,
) -> Result<StatusCode, ApiError> {
    let root = ensure_registered(&req.path)?;
    let target = resolve_inside(&root, &req.folder)?;
    if req.folder.split(['/', '\\']).next() == Some(".shiro") {
        return Err(bad_request(".shiro 是 shiro 的工作目录，不能删除"));
    }
    if !target.is_dir() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                message: "目录不存在".into(),
            }),
        ));
    }
    let is_empty = target
        .read_dir()
        .map(|mut d| d.next().is_none())
        .unwrap_or(false);
    if is_empty {
        std::fs::remove_dir(&target).map_err(internal_error)?;
    } else {
        move_to_trash(&root, &target)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct RenameProjectRequest {
    /// 项目当前绝对路径
    pub path: String,
    /// 新名称（即新目录名）
    pub new_name: String,
}

/// 重命名项目：重命名项目文件夹本体，history.toml 记录同步更新
#[utoipa::path(
    post,
    path = "/api/v1/projects/rename",
    tag = "projects",
    request_body = RenameProjectRequest,
    responses(
        (status = 200, description = "已重命名", body = ProjectItem),
        (status = 400, description = "名称非法", body = ErrorResponse),
        (status = 404, description = "项目目录不存在", body = ErrorResponse),
        (status = 409, description = "目标目录已存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn rename_project(
    Json(req): Json<RenameProjectRequest>,
) -> Result<Json<ProjectItem>, ApiError> {
    let src = ensure_registered(&req.path)?;
    let name = validate_name(&req.new_name)?;
    let Some(parent) = src.parent() else {
        return Err(bad_request("不能重命名根目录"));
    };
    let dst = parent.join(name);
    if dst == src {
        return Ok(Json(ProjectItem {
            path: src.to_string_lossy().to_string(),
            name: name.into(),
            exists: true,
        }));
    }
    if dst.exists() {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                message: "目标目录已存在".into(),
            }),
        ));
    }
    std::fs::rename(&src, &dst).map_err(internal_error)?;

    // 文件夹名与项目显示名（.shiro/project.toml 的 name）解耦：重命名文件夹不改显示名

    // history 记录换为新路径
    let new_path = dst.to_string_lossy().to_string();
    let mut history = load_history();
    for entry in &mut history.projects {
        if same_path(&entry.path, &src) {
            entry.path = new_path.clone();
            entry.opened_at = now_secs();
        }
    }
    save_history(&history).map_err(internal_error)?;

    let display_name = project_display_name(&new_path);
    Ok(Json(ProjectItem {
        path: new_path,
        name: display_name,
        exists: true,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct RenameEntryRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 条目相对路径（文件或目录；文件需含后缀）
    pub rel: String,
    /// 新名字（文件名含后缀）
    pub new_name: String,
}

/// 重命名项目内条目（文件或目录）：同目录改名
#[utoipa::path(
    post,
    path = "/api/v1/projects/entry/rename",
    tag = "projects",
    request_body = RenameEntryRequest,
    responses(
        (status = 204, description = "已重命名"),
        (status = 400, description = "路径非法或受保护", body = ErrorResponse),
        (status = 404, description = "条目不存在", body = ErrorResponse),
        (status = 409, description = "目标已存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn rename_entry(Json(req): Json<RenameEntryRequest>) -> Result<StatusCode, ApiError> {
    if req.rel.split(['/', '\\']).next() == Some(".shiro") {
        return Err(bad_request(".shiro 是 shiro 的工作目录，不能重命名"));
    }
    let root = ensure_registered(&req.path)?;
    let src = resolve_inside(&root, &req.rel)?;
    if !src.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                message: "条目不存在".into(),
            }),
        ));
    }
    let name = validate_name(&req.new_name)?;
    let dst = src.parent().unwrap_or(&root).join(name);
    if dst == src {
        return Ok(StatusCode::NO_CONTENT);
    }
    if dst.exists() {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                message: "目标已存在".into(),
            }),
        ));
    }
    std::fs::rename(&src, &dst).map_err(internal_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct RemoveFileRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 文稿相对路径
    pub file: String,
}

/// 删除文稿：移入项目回收站（.shiro/trash/，可手动恢复）
#[utoipa::path(
    post,
    path = "/api/v1/projects/file/delete",
    tag = "projects",
    request_body = RemoveFileRequest,
    responses(
        (status = 204, description = "已删除（移入回收站）"),
        (status = 400, description = "路径非法", body = ErrorResponse),
        (status = 404, description = "文件不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn remove_project_file(Json(req): Json<RemoveFileRequest>) -> Result<StatusCode, ApiError> {
    let root = ensure_registered(&req.path)?;
    let target = resolve_inside(&root, &req.file)?;
    if !target.is_file() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                message: "文件不存在".into(),
            }),
        ));
    }
    move_to_trash(&root, &target)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- 项目目录监听（SSE 推送；实现见 watch.rs，供编辑器外部变动重载，后续 AI 写稿可订阅同一 Hub） ----

#[derive(Deserialize, ToSchema)]
pub struct WatchRequest {
    /// 项目根目录绝对路径
    pub path: String,
}

/// 监听项目目录变动：SSE 推送防抖 300ms 后的变更相对路径集合（`.` 开头路径段与临时文件已过滤）。
/// 每帧 data 为 JSON：`{"changed":["正文/a.md"]}`；changed 为空数组表示事件滞后溢出，订阅方应全量刷新。
/// 监听随连接建立而启动、所有订阅断开后停止。
#[utoipa::path(
    post,
    path = "/api/v1/projects/watch",
    tag = "projects",
    request_body = WatchRequest,
    responses(
        (status = 200, description = "SSE 事件流（text/event-stream）：data 为 {\"changed\":[...]}"),
        (status = 400, description = "项目目录不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
async fn watch_project(
    State(state): State<AppState>,
    Json(req): Json<WatchRequest>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let root = ensure_registered(&req.path)?;
    let (rx, guard) = state.watch_hub.subscribe(&root);
    let stream = BroadcastStream::new(rx).map(move |item| {
        let _guard = &guard; // 订阅守卫随流存活：客户端断开 → 流 drop → 退订（归零停监听）
        let changed = item.unwrap_or_default(); // Lagged（消费滞后丢帧）→ 空数组，订阅方全量刷新
        let data = serde_json::json!({ "changed": changed }).to_string();
        Ok(Event::default().data(data))
    });
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

async fn auth(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let header_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match header_token {
        Some(t) if t == state.token => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_token",
            utoipa::openapi::security::SecurityScheme::Http(
                utoipa::openapi::security::HttpBuilder::new()
                    .scheme(utoipa::openapi::security::HttpAuthScheme::Bearer)
                    .build(),
            ),
        );
    }
    // 让 modifier 同时补全 info（OpenApiRouter 生成的文档不含 title/version）
}

fn apply_info(doc: &mut utoipa::openapi::OpenApi) {
    doc.info.title = "shiro API".into();
    doc.info.version = env!("CARGO_PKG_VERSION").into();
}

/// 构建 API 路由与 OpenAPI 文档（同一来源：路由即文档，契约测试校验 openapi.json 同步）。
pub fn build_router(
    state: AppState,
    serve_folder: Option<PathBuf>,
) -> (Router, utoipa::openapi::OpenApi) {
    let (api_router, mut doc) = OpenApiRouter::new()
        .routes(routes!(startup))
        .routes(routes!(list_projects))
        .routes(routes!(create_project))
        .routes(routes!(remove_project))
        .routes(routes!(project_tree))
        .routes(routes!(project_excerpts))
        .routes(routes!(read_project_file))
        .routes(routes!(write_project_file))
        .routes(routes!(create_project_file))
        .routes(routes!(remove_project_file))
        .routes(routes!(create_project_folder))
        .routes(routes!(remove_project_folder))
        .routes(routes!(rename_project))
        .routes(routes!(rename_entry))
        .routes(routes!(watch_project))
        .routes(routes!(crate::assets::list_characters))
        .routes(routes!(crate::assets::get_character))
        .routes(routes!(crate::assets::save_character))
        .routes(routes!(crate::assets::create_character))
        .merge(crate::chat::api::router())
        .routes(routes!(crate::model_api::list_profiles))
        .routes(routes!(crate::model_api::upsert_profile))
        .routes(routes!(crate::model_api::delete_profile))
        .routes(routes!(crate::model_api::test_profile))
        .split_for_parts();
    SecurityAddon.modify(&mut doc);
    apply_info(&mut doc);

    let api_router = api_router.layer(axum::middleware::from_fn_with_state(state.clone(), auth));

    let mut router = Router::new()
        .merge(api_router)
        .route("/health", axum::routing::get(health))
        .with_state(state)
        .layer(tower_http::cors::CorsLayer::permissive());

    // 局域网访问形态：daemon 直接 serve 前端静态资源，SPA 回退到 index.html
    if let Some(folder) = serve_folder
        && folder.is_dir() {
            let index = ServeFile::new(folder.join("index.html"));
            router = router.fallback_service(ServeDir::new(folder).not_found_service(index));
        }

    (router, doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 契约测试：固化的 openapi.json 必须与代码生成一致。
    /// 改 API 后执行 `cargo run -- --dump-openapi > openapi.json` 重新固化。
    #[test]
    fn openapi_json_in_sync() {
        let (_, doc) = build_router(
            AppState {
                token: String::new(),
                watch_hub: watch::WatchHub::default(),
                chat: Default::default(),
            },
            None,
        );
        let generated = serde_json::to_string_pretty(&doc).unwrap();
        let frozen = include_str!("../openapi.json");
        assert_eq!(
            generated.trim(),
            frozen.trim(),
            "openapi.json 与代码不同步，请执行 cargo run -- --dump-openapi > openapi.json"
        );
    }
}
