//! Chat HTTP 端点：会话 CRUD + 消息流式生成（SSE）+ 提案应用。
//! 存储与生成辅助见 mod.rs；模型调用走 llm::chat_stream（重试/双协议见 llm.rs）。

use crate::api::{ErrorResponse, bad_request, now_secs};
use crate::chat::{self, ChatMessage, Proposal, RunningGuard, SessionDetail, SessionSummary};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;
use tokio_stream::wrappers::ReceiverStream;
use utoipa::ToSchema;
use utoipa_axum::routes;

// ---- 会话列表 / 创建 ----

#[derive(Deserialize, ToSchema)]
pub struct ChatProjectRequest {
    /// 项目根目录绝对路径
    pub path: String,
}

#[derive(Serialize, ToSchema)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionSummary>,
}

/// 会话列表（最近更新在前）
#[utoipa::path(
    post,
    path = "/api/v1/chat/sessions/list",
    tag = "chat",
    request_body = ChatProjectRequest,
    responses(
        (status = 200, description = "会话列表", body = SessionListResponse),
        (status = 400, description = "项目目录不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn list_chat_sessions(
    Json(req): Json<ChatProjectRequest>,
) -> Result<Json<SessionListResponse>, crate::api::ApiError> {
    let root = PathBuf::from(&req.path);
    if !root.is_dir() {
        return Err(bad_request("项目目录不存在"));
    }
    Ok(Json(SessionListResponse {
        sessions: chat::list_sessions(&root),
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateSessionRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 会话标题（缺省「新会话」，首条用户消息自动回填）
    #[serde(default)]
    pub title: Option<String>,
}

/// 新建会话
#[utoipa::path(
    post,
    path = "/api/v1/chat/sessions/create",
    tag = "chat",
    request_body = CreateSessionRequest,
    responses(
        (status = 201, description = "已创建", body = SessionDetail),
        (status = 400, description = "项目目录不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn create_chat_session(
    Json(req): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<SessionDetail>), crate::api::ApiError> {
    let root = PathBuf::from(&req.path);
    if !root.is_dir() {
        return Err(bad_request("项目目录不存在"));
    }
    let detail = chat::create_session(&root, req.title.as_deref().unwrap_or(""))
        .map_err(|e| bad_request(&e))?;
    Ok((StatusCode::CREATED, Json(detail)))
}

// ---- 会话详情 / 删除 ----

#[derive(Deserialize, ToSchema)]
pub struct SessionRefRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 会话 id
    pub id: String,
}

/// 会话详情（全部消息）
#[utoipa::path(
    post,
    path = "/api/v1/chat/sessions/get",
    tag = "chat",
    request_body = SessionRefRequest,
    responses(
        (status = 200, description = "会话详情", body = SessionDetail),
        (status = 404, description = "会话不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn get_chat_session(
    Json(req): Json<SessionRefRequest>,
) -> Result<Json<SessionDetail>, crate::api::ApiError> {
    let root = PathBuf::from(&req.path);
    let session = chat::read_session(&root, &req.id)
        .ok_or_else(|| not_found("会话不存在"))?;
    Ok(Json(session.detail()))
}

/// 删除会话
#[utoipa::path(
    post,
    path = "/api/v1/chat/sessions/delete",
    tag = "chat",
    request_body = SessionRefRequest,
    responses(
        (status = 204, description = "已删除"),
        (status = 404, description = "会话不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn delete_chat_session(
    Json(req): Json<SessionRefRequest>,
) -> Result<StatusCode, crate::api::ApiError> {
    let root = PathBuf::from(&req.path);
    chat::delete_session(&root, &req.id).map_err(|_| not_found("会话不存在"))?;
    Ok(StatusCode::NO_CONTENT)
}

fn not_found(message: &str) -> crate::api::ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            message: message.into(),
        }),
    )
}

// ---- 发消息（SSE 流式生成） ----

#[derive(Deserialize, ToSchema)]
pub struct SendMessageRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 会话 id
    pub id: String,
    /// 用户输入原文
    pub content: String,
    /// 引用的项目文件（相对路径；内容在前端不可见，由 daemon 注入上下文）
    #[serde(default)]
    pub attachments: Vec<String>,
}

/// 发送消息并流式生成回复。响应为 SSE（text/event-stream），data 帧为 JSON：
/// `{"type":"delta","text":"…"}` 正文增量 / `{"type":"thinking","text":"…"}` 思考增量 /
/// `{"type":"done","message":{…}}` 完成（完整助手消息，含解析出的提案）/
/// `{"type":"error","message":"…"}` 失败。
/// 客户端断开连接即中止生成（不保存部分回复）。同一会话同时只允许一个生成。
#[utoipa::path(
    post,
    path = "/api/v1/chat/sessions/messages",
    tag = "chat",
    request_body = SendMessageRequest,
    responses(
        (status = 200, description = "SSE 事件流（text/event-stream）：data 为 {type: delta|thinking|done|error, …}"),
        (status = 400, description = "参数错误（目录不存在 / 内容为空 / 模型未配置）", body = ErrorResponse),
        (status = 409, description = "上一条回复还在生成中", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn send_chat_message(
    State(state): State<crate::api::AppState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>, crate::api::ApiError>
{
    let id = req.id.clone();
    let root = PathBuf::from(&req.path);
    if !root.is_dir() {
        return Err(bad_request("项目目录不存在"));
    }
    if req.content.trim().is_empty() {
        return Err(bad_request("消息内容为空"));
    }
    // 模型配置缺失是最常见的失败，进来就报（fail fast）
    let Some(cfg) = crate::llm::load_llm_config() else {
        return Err(bad_request(
            "模型未配置：请在 Model 页注册模型并设为默认供应商",
        ));
    };
    if !state.chat.begin(&id) {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                message: "上一条回复还在生成中，请稍候".into(),
            }),
        ));
    }
    let guard = RunningGuard::new(state.chat.clone(), id.clone());

    // 用户消息先落盘（断线/失败也不丢输入）
    let user_msg = ChatMessage {
        role: "user".into(),
        content: req.content.clone(),
        at: now_secs(),
        attachments: req.attachments.clone(),
        proposals: Vec::new(),
    };
    let session = chat::append_message(&root, &id, user_msg).map_err(|e| bad_request(&e))?;
    let messages = chat::build_llm_messages(&root, &session, &req.content, &req.attachments);

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(64);
    let project = root.clone();
    let session_id = id.clone();
    tokio::spawn(async move {
        let _guard = guard; // Drop 释放运行标记（成功/失败/断开都走这里）
        let tx_delta = tx.clone();
        let tx_think = tx.clone();
        let result = tokio::select! {
            r = crate::llm::chat_stream(
                &cfg,
                messages,
                move |d: &str| {
                    let _ = tx_delta.blocking_send(Ok(
                        Event::default().data(json!({ "type": "delta", "text": d }).to_string()),
                    ));
                },
                move |t: &str| {
                    let _ = tx_think.blocking_send(Ok(
                        Event::default().data(json!({ "type": "thinking", "text": t }).to_string()),
                    ));
                },
            ) => Some(r),
            // 客户端断开 → Receiver drop → 中止生成，不保存部分回复
            _ = tx.closed() => None,
        };
        match result {
            None => {}
            Some(Ok(out)) => {
                let Some(mut session) = chat::read_session(&project, &session_id) else {
                    return;
                };
                let proposals = chat::parse_proposals(&mut session, &out.text);
                let msg = ChatMessage {
                    role: "assistant".into(),
                    content: out.text,
                    at: now_secs(),
                    attachments: Vec::new(),
                    proposals,
                };
                let dto = msg.clone();
                if chat::push_message(&project, &mut session, msg).is_err() {
                    return;
                }
                let _ = tx
                    .send(Ok(Event::default().data(json!({ "type": "done", "message": dto }).to_string())))
                    .await;
            }
            Some(Err(e)) => {
                let _ = tx
                    .send(Ok(Event::default().data(json!({ "type": "error", "message": e.message }).to_string())))
                    .await;
            }
        }
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    ))
}

// ---- 提案应用 ----

#[derive(Deserialize, ToSchema)]
pub struct ApplyProposalRequest {
    /// 项目根目录绝对路径
    pub path: String,
    /// 会话 id
    pub id: String,
    /// 提案 id（会话内唯一，见消息的 proposals[].id）
    pub proposal_id: String,
}

#[derive(Serialize, ToSchema)]
pub struct ApplyProposalResponse {
    pub proposal: Proposal,
}

/// 应用提案：整文件覆盖写盘（父目录自动创建），标记 applied。写盘后文件监听自动推送前端刷新。
#[utoipa::path(
    post,
    path = "/api/v1/chat/sessions/apply",
    tag = "chat",
    request_body = ApplyProposalRequest,
    responses(
        (status = 200, description = "已写入文件", body = ApplyProposalResponse),
        (status = 400, description = "非法路径或提案已应用", body = ErrorResponse),
        (status = 404, description = "会话或提案不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn apply_chat_proposal(
    Json(req): Json<ApplyProposalRequest>,
) -> Result<Json<ApplyProposalResponse>, crate::api::ApiError> {
    let root = PathBuf::from(&req.path);
    let mut session =
        chat::read_session(&root, &req.id).ok_or_else(|| not_found("会话不存在"))?;
    let proposal = chat::apply_proposal(&root, &mut session, &req.proposal_id)
        .map_err(|e| bad_request(&e))?;
    Ok(Json(ApplyProposalResponse { proposal }))
}

/// 注册路由（在 api.rs 的 build_router 中调用；每条路径单独注册）
pub fn router() -> utoipa_axum::router::OpenApiRouter<crate::api::AppState> {
    utoipa_axum::router::OpenApiRouter::new()
        .routes(routes!(list_chat_sessions))
        .routes(routes!(create_chat_session))
        .routes(routes!(get_chat_session))
        .routes(routes!(delete_chat_session))
        .routes(routes!(send_chat_message))
        .routes(routes!(apply_chat_proposal))
}
