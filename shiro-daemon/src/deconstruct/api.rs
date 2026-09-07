// 拆解任务 API（角色制作）：创建/列表/详情/保存/删除。
// 进度推送 V1 用轮询（任务粒度低）；鉴权与其他 API 一致走 Bearer。

use crate::api::{ApiError, ErrorResponse, bad_request};
use crate::deconstruct::engine;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct CreateTaskRequest {
    /// 素材名（作品名或文件名）
    pub source_name: String,
    /// 剧本全文
    pub content: String,
    /// 待分析角色名（主名）
    pub character: String,
    /// 别名/其他写法（避免归属遗漏）
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct TaskSummary {
    pub id: String,
    pub source_name: String,
    pub character: String,
    pub stage: String,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    pub segment_count: usize,
    pub notes_done: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_character_id: Option<String>,
}

impl TaskSummary {
    fn from_record(r: &engine::TaskRecord) -> Self {
        Self {
            id: r.meta.id.clone(),
            source_name: r.meta.source_name.clone(),
            character: r.meta.character.clone(),
            stage: r.progress.stage.clone(),
            created_at: r.meta.created_at,
            finished_at: r.progress.finished_at,
            segment_count: r.progress.segment_count,
            notes_done: r.progress.notes_done,
            error: r.progress.error.clone(),
            saved_character_id: r.progress.saved_character_id.clone(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct TaskListResponse {
    pub tasks: Vec<TaskSummary>,
}

#[derive(Serialize, ToSchema)]
pub struct CardFileDto {
    /// 文件名（不带扩展名）：soul / speech_patterns / … / index
    pub name: String,
    /// 文件内容（markdown）
    pub body: String,
}

#[derive(Serialize, ToSchema)]
pub struct TaskDetail {
    pub id: String,
    pub source_name: String,
    pub character: String,
    pub aliases: Vec<String>,
    pub created_at: u64,
    /// 完整进度（阶段/切片数/笔记进度/生成进度/回查报告）
    pub progress: engine::Progress,
    /// 产物文件（已生成的部分）
    pub files: Vec<CardFileDto>,
}

#[derive(Serialize, ToSchema)]
pub struct SaveTaskResponse {
    /// 保存后的人物库角色 id（db/人物/<id>/）
    pub character_id: String,
}

/// 创建角色制作任务（导入剧本 + 角色名/别名），创建即后台执行
#[utoipa::path(
    post,
    path = "/api/v1/db/deconstruct/tasks",
    tag = "deconstruct",
    request_body = CreateTaskRequest,
    responses(
        (status = 201, description = "已创建并开始执行", body = TaskSummary),
        (status = 400, description = "参数错误（内容为空/角色名为空）", body = ErrorResponse),
        (status = 401, description = "未鉴权"),
        (status = 500, description = "任务目录写入失败", body = ErrorResponse)
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn create_task(
    State(state): State<crate::api::AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<(StatusCode, Json<TaskSummary>), ApiError> {
    let record = engine::create_task(&state.deconstruct, engine::CreateParams {
        source_name: req.source_name,
        content: req.content,
        character: req.character,
        aliases: req.aliases,
    })
    .map_err(|e| bad_request(&e))?;
    Ok((StatusCode::CREATED, Json(TaskSummary::from_record(&record))))
}

/// 任务列表（最新在前；运行中的任务含实时进度快照）
#[utoipa::path(
    get,
    path = "/api/v1/db/deconstruct/tasks",
    tag = "deconstruct",
    responses(
        (status = 200, description = "任务列表", body = TaskListResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn list_tasks(
    State(state): State<crate::api::AppState>,
) -> Json<TaskListResponse> {
    Json(TaskListResponse {
        tasks: state
            .deconstruct
            .list_tasks()
            .iter()
            .map(TaskSummary::from_record)
            .collect(),
    })
}

fn not_found() -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            message: "任务不存在".into(),
        }),
    )
}

/// 任务详情：完整进度 + 已生成产物
#[utoipa::path(
    get,
    path = "/api/v1/db/deconstruct/tasks/{id}",
    tag = "deconstruct",
    params(("id" = String, Path, description = "任务 id")),
    responses(
        (status = 200, description = "任务详情", body = TaskDetail),
        (status = 404, description = "任务不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn get_task(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> Result<Json<TaskDetail>, ApiError> {
    let rec = state.deconstruct.get_task(&id).ok_or_else(not_found)?;
    let files = engine::read_card_files(&id)
        .into_iter()
        .map(|(name, body)| CardFileDto { name, body })
        .collect();
    Ok(Json(TaskDetail {
        id: rec.meta.id.clone(),
        source_name: rec.meta.source_name.clone(),
        character: rec.meta.character.clone(),
        aliases: rec.meta.aliases.clone(),
        created_at: rec.meta.created_at,
        progress: rec.progress,
        files,
    }))
}

/// 产物保存到人物库（目录形态深卡，db/人物/<id>/）
#[utoipa::path(
    post,
    path = "/api/v1/db/deconstruct/tasks/{id}/save",
    tag = "deconstruct",
    params(("id" = String, Path, description = "任务 id")),
    responses(
        (status = 200, description = "已保存", body = SaveTaskResponse),
        (status = 400, description = "任务未完成或产物为空", body = ErrorResponse),
        (status = 404, description = "任务不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn save_task(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> Result<Json<SaveTaskResponse>, ApiError> {
    let rec = state.deconstruct.get_task(&id).ok_or_else(not_found)?;
    let slug = crate::assets::slug_for(&rec.meta.character);
    let character_id = engine::save_task_to_library(&id, slug).map_err(|e| bad_request(&e))?;
    Ok(Json(SaveTaskResponse { character_id }))
}

/// 删除任务（连同任务目录；运行中不可删）
#[utoipa::path(
    delete,
    path = "/api/v1/db/deconstruct/tasks/{id}",
    tag = "deconstruct",
    params(("id" = String, Path, description = "任务 id")),
    responses(
        (status = 204, description = "已删除"),
        (status = 400, description = "运行中不可删", body = ErrorResponse),
        (status = 404, description = "任务不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn delete_task(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    engine::delete_task(&state.deconstruct, &id).map_err(|e| bad_request(&e))?;
    Ok(StatusCode::NO_CONTENT)
}
