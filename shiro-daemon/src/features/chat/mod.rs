//! Chat 会话：对话式修改项目文档（V1）。
//! 存储：<项目>/.shiro/chat/<id>.json（随项目迁移、可进版本管理）。
//! 生成流程见 api.rs 的 messages 端点：上下文组装 + llm::chat_stream + 提案解析。
//! 提案约定（草稿→确认）：模型在回复中用 ```shiro-edit 代码块给出整文件覆盖提案，
//! 解析成结构化 proposals，用户确认后经 apply 端点写盘，不经确认不落盘。

use crate::infra::fs::atomic_write;
use crate::infra::now_secs;
use crate::infra::paths::{folder_name, resolve_inside};
pub mod api;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// 历史消息窗口：每次请求携带最近 N 条（V1 无 token 预算，靠窗口截断）
const HISTORY_MAX_MESSAGES: usize = 20;
/// 单个引用文件的字符上限（超出截断，标注「已截断」）
const ATTACHMENT_MAX_CHARS: usize = 30_000;
/// 目录树文本最大行数
const TREE_MAX_LINES: usize = 200;

// ---- 数据结构 ----

/// 整文件覆盖提案（模型输出解析而来；applied = 已写盘）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct Proposal {
    pub id: String,
    /// 项目内相对路径
    pub file: String,
    /// 修改后的完整文件内容
    pub content: String,
    #[serde(default)]
    pub applied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct ChatMessage {
    /// user / assistant
    pub role: String,
    /// 原始内容（assistant 含 shiro-edit 块，前端渲染时剥离；保留原文供后续轮次引用）
    pub content: String,
    pub at: u64,
    /// user 消息引用的项目文件（相对路径）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
    /// assistant 消息解析出的提案
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proposals: Vec<Proposal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    /// 提案 id 自增计数（会话内唯一）
    #[serde(default)]
    pub next_proposal_seq: u64,
    pub messages: Vec<ChatMessage>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub message_count: usize,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SessionDetail {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub messages: Vec<ChatMessage>,
}

impl Session {
    fn summary(&self) -> SessionSummary {
        SessionSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            message_count: self.messages.len(),
        }
    }
    fn detail(&self) -> SessionDetail {
        SessionDetail {
            id: self.id.clone(),
            title: self.title.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            messages: self.messages.clone(),
        }
    }
}

// ---- 存储路径 ----

fn chat_folder(root: &Path) -> PathBuf {
    root.join(".shiro").join("chat")
}

fn valid_session_id(id: &str) -> bool {
    !id.is_empty() && !id.starts_with('.') && !id.contains(['/', '\\'])
}

fn session_path(root: &Path, id: &str) -> Option<PathBuf> {
    valid_session_id(id).then(|| chat_folder(root).join(format!("{id}.json")))
}

// ---- 会话 CRUD ----

pub fn list_sessions(root: &Path) -> Vec<SessionSummary> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(chat_folder(root)) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if let Some(s) = read_session(root, id) {
            out.push(s.summary());
        }
    }
    out.sort_by_key(|s| std::cmp::Reverse(s.updated_at));
    out
}

pub fn read_session(root: &Path, id: &str) -> Option<Session> {
    let text = std::fs::read_to_string(session_path(root, id)?).ok()?;
    serde_json::from_str(&text).ok()
}

fn save_session(root: &Path, session: &Session) -> Result<(), String> {
    let path = session_path(root, &session.id).ok_or("非法会话 id")?;
    let json = serde_json::to_string_pretty(session).map_err(|e| e.to_string())?;
    atomic_write(&path, &json).map_err(|e| e.to_string())
}

pub fn create_session(root: &Path, title: &str) -> Result<SessionDetail, String> {
    let id = format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let now = now_secs();
    let session = Session {
        id,
        title: if title.trim().is_empty() {
            "新会话".into()
        } else {
            title.trim().to_string()
        },
        created_at: now,
        updated_at: now,
        next_proposal_seq: 1,
        messages: Vec::new(),
    };
    save_session(root, &session)?;
    Ok(session.detail())
}

pub fn delete_session(root: &Path, id: &str) -> Result<(), String> {
    let path = session_path(root, id).ok_or("非法会话 id")?;
    std::fs::remove_file(path).map_err(|e| e.to_string())
}

/// 追加消息并保存（保留调用方已加载的会话状态与提案序号）
pub fn push_message(root: &Path, session: &mut Session, msg: ChatMessage) -> Result<(), String> {
    if msg.role == "user" && session.title == "新会话" {
        let head: String = msg.content.chars().take(20).collect();
        if !head.trim().is_empty() {
            session.title = head;
        }
    }
    session.messages.push(msg);
    session.updated_at = now_secs();
    save_session(root, session)
}

/// 追加消息并保存（内部加载会话）
pub fn append_message(root: &Path, id: &str, msg: ChatMessage) -> Result<Session, String> {
    let mut session = read_session(root, id).ok_or("会话不存在")?;
    push_message(root, &mut session, msg)?;
    Ok(session)
}

// ---- 运行守卫：同一会话同时只允许一个生成 ----

pub struct Hub {
    running: Mutex<HashSet<String>>,
}

impl Default for Hub {
    fn default() -> Self {
        Self {
            running: Mutex::new(HashSet::new()),
        }
    }
}

impl Hub {
    /// 登记生成中的会话；已在运行返回 false
    pub fn begin(&self, id: &str) -> bool {
        self.running.lock().unwrap().insert(id.to_string())
    }
    pub fn finish(&self, id: &str) {
        self.running.lock().unwrap().remove(id);
    }
}

/// 守卫：随 Drop 自动释放运行标记（生成成功/失败/客户端断开都走这里）
pub struct RunningGuard {
    hub: Arc<Hub>,
    id: String,
}

impl RunningGuard {
    pub fn new(hub: Arc<Hub>, id: String) -> Self {
        Self { hub, id }
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.hub.finish(&self.id);
    }
}

// ---- 上下文组装 ----

/// 项目目录树文本（缩进两格；只含目录与文稿；排除 . 开头项）
fn build_tree_text(root: &Path) -> String {
    fn walk(folder: &Path, prefix: &str, out: &mut Vec<String>) {
        if out.len() >= TREE_MAX_LINES {
            return;
        }
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        let mut items: Vec<_> = entries.flatten().collect();
        items.sort_by_key(|e| e.file_name());
        // 目录在前
        items.sort_by_key(|e| !e.path().is_dir());
        for entry in items {
            if out.len() >= TREE_MAX_LINES {
                out.push("…（已截断）".into());
                return;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                out.push(format!("{prefix}{name}/"));
                walk(&path, &format!("{prefix}  "), out);
            } else if crate::infra::fs::is_sheet_file(&name) {
                out.push(format!("{prefix}{name}"));
            }
        }
    }
    let mut lines = Vec::new();
    walk(root, "", &mut lines);
    lines.join("\n")
}

/// 引用文件内容（超出上限截断；文件不存在返回占位说明）
fn read_attachment(root: &Path, rel: &str) -> String {
    let content = resolve_inside(root, rel)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok());
    match content {
        Some(text) => {
            let count = text.chars().count();
            if count <= ATTACHMENT_MAX_CHARS {
                text
            } else {
                let head: String = text.chars().take(ATTACHMENT_MAX_CHARS).collect();
                format!("{head}\n…（已截断，全文 {} 字）", count)
            }
        }
        None => format!("（文件不存在或不可读：{rel}）"),
    }
}

fn system_prompt(root: &Path) -> String {
    format!(
        "你是 shiro 写作软件中的 AI 写作助手，帮助用户创作与修改项目文档。当前项目：{}\n\n\
         ## 项目结构\n```\n{}```\n\n\
         ## 修改提案\n需要修改或新建项目文档时，在回复中用 shiro-edit 代码块给出提案，每个提案一个块：\n\
         ```shiro-edit\n{{\"file\": \"正文/第一卷/001.md\", \"content\": \"修改后的完整文件内容\"}}\n```\n\
         规则：\n\
         - file 是项目内相对路径；content 必须是修改后的完整文件内容（整文件覆盖，不是增量 diff）\n\
         - 一条回复可以给出多个提案\n\
         - 提案不会直接生效，用户确认后才写入文件\n\
         - 不涉及文件修改时正常回答，不要输出 shiro-edit 块\n\
         - 回复使用中文",
        folder_name(&root.to_string_lossy()),
        build_tree_text(root),
    )
}

/// 组装发给模型的全部消息：system + 最近历史（原文含提案块）+ 本条用户消息（附引用文件内容）
pub fn build_llm_messages(
    root: &Path,
    session: &Session,
    user_content: &str,
    attachments: &[String],
) -> Vec<crate::llm::ChatMessage> {
    let mut out = Vec::new();
    out.push(crate::llm::ChatMessage {
        role: "system",
        content: system_prompt(root),
    });
    let history_start = session.messages.len().saturating_sub(HISTORY_MAX_MESSAGES);
    for msg in &session.messages[history_start..] {
        let role = match msg.role.as_str() {
            "user" => "user",
            "assistant" => "assistant",
            _ => continue,
        };
        out.push(crate::llm::ChatMessage {
            role,
            content: msg.content.clone(),
        });
    }
    // 本条消息：引用文件内容前置拼接（存储的 user 消息保留原文，发送时才组装）
    let mut content = String::new();
    for rel in attachments {
        content.push_str(&format!(
            "\n【引用文件：{rel}】\n{}\n【引用结束】\n",
            read_attachment(root, rel)
        ));
    }
    content.push_str(user_content);
    out.push(crate::llm::ChatMessage {
        role: "user",
        content,
    });
    out
}

// ---- 提案解析 ----

/// 相对路径合法性：非空、非绝对路径、无 . .. 逃逸段（与 infra::paths::resolve_inside 同规则）
fn legal_rel(rel: &str) -> bool {
    !rel.trim().is_empty()
        && !Path::new(rel).is_absolute()
        && !rel.split(['/', '\\']).any(|seg| seg == "." || seg == "..")
}

/// 从回复全文解析 ```shiro-edit 块为提案（解析失败或 file 非法的块跳过）。
/// 提案 id 由会话计数器分配（caller 传入并负责回写 next_proposal_seq）。
pub fn parse_proposals(session: &mut Session, text: &str) -> Vec<Proposal> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("```shiro-edit") {
        let Some(after_fence) = rest[start..].find('\n') else {
            break;
        };
        let body_start = start + after_fence + 1;
        let Some(end) = rest[body_start..].find("```") else {
            break;
        };
        let body = &rest[body_start..body_start + end];
        if let Ok(v) = crate::llm::extract_json(body) {
            let file = v.get("file").and_then(|f| f.as_str()).unwrap_or("").trim();
            let content = v.get("content").and_then(|c| c.as_str()).unwrap_or("");
            if !file.is_empty() && legal_rel(file) && !content.is_empty() {
                let id = format!("p{}", session.next_proposal_seq);
                session.next_proposal_seq += 1;
                out.push(Proposal {
                    id,
                    file: file.to_string(),
                    content: content.to_string(),
                    applied: false,
                    applied_at: None,
                });
            }
        }
        rest = &rest[body_start + end + 3..];
    }
    out
}

/// 应用提案：整文件覆盖写盘（父目录自动创建），标记 applied
pub fn apply_proposal(
    root: &Path,
    session: &mut Session,
    proposal_id: &str,
) -> Result<Proposal, String> {
    let mut loc = None;
    for (mi, msg) in session.messages.iter().enumerate() {
        if let Some(pi) = msg.proposals.iter().position(|p| p.id == proposal_id) {
            loc = Some((mi, pi));
            break;
        }
    }
    let Some((mi, pi)) = loc else {
        return Err("提案不存在".into());
    };
    let p = &mut session.messages[mi].proposals[pi];
    if p.applied {
        return Err("提案已应用".into());
    }
    let target = resolve_inside(root, &p.file).map_err(|_| "非法的文件路径".to_string())?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    atomic_write(&target, &p.content).map_err(|e| e.to_string())?;
    p.applied = true;
    p.applied_at = Some(now_secs());
    let out = p.clone();
    save_session(root, session)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proposals_parse_and_apply() {
        let mut session = Session {
            id: "t".into(),
            title: "t".into(),
            created_at: 0,
            updated_at: 0,
            next_proposal_seq: 1,
            messages: Vec::new(),
        };
        let text = "先看修改建议。\n```shiro-edit\n{\"file\": \"正文/001.md\", \"content\": \"新内容\"}\n```\n以上。";
        let ps = parse_proposals(&mut session, text);
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].file, "正文/001.md");
        assert_eq!(ps[0].id, "p1");

        // 非法路径（.. 逃逸）跳过
        let bad = "```shiro-edit\n{\"file\": \"../evil.md\", \"content\": \"x\"}\n```";
        assert!(parse_proposals(&mut session, bad).is_empty());
        assert_eq!(session.next_proposal_seq, 2);

        // 会话持久化后应用：写盘 + 标记
        let folder = std::env::temp_dir().join(format!("shiro-chat-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let mut s = Session {
            id: "t".into(),
            ..session.clone()
        };
        s.messages.push(ChatMessage {
            role: "assistant".into(),
            content: text.into(),
            at: 0,
            attachments: vec![],
            proposals: ps,
        });
        save_session(&folder, &s).unwrap();
        let mut loaded = read_session(&folder, "t").unwrap();
        let applied = apply_proposal(&folder, &mut loaded, "p1").unwrap();
        assert!(applied.applied);
        let written = std::fs::read_to_string(folder.join("正文/001.md")).unwrap();
        assert_eq!(written, "新内容");
        // 重复应用报错
        assert!(apply_proposal(&folder, &mut loaded, "p1").is_err());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
