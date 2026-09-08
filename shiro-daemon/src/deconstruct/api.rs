// 拆解任务 API（角色制作）：创建/列表/详情/保存/删除/事件流。
// 进度推送用轮询（任务粒度低）；节点输出用 SSE 实时推送（鉴权走 ?key=，EventSource 无法自定义 header）。

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
    /// 显示名（用户自定义；None = 前端用角色名兜底）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
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
            title: r.meta.title.clone(),
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
pub struct SegmentInfoDto {
    /// 段序号（切块结果中）
    pub index: usize,
    /// 段标识（章节/场次标题或片段 N）
    pub label: String,
    /// 段字符数
    pub chars: usize,
    /// 开头预览
    pub excerpt: String,
    /// 段内是否出现目标角色名/别名（高亮标记）
    pub has_name: bool,
}

#[derive(Serialize, ToSchema)]
pub struct TaskDetail {
    pub id: String,
    /// 显示名（用户自定义；None = 前端用角色名兜底）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub source_name: String,
    pub character: String,
    pub aliases: Vec<String>,
    pub created_at: u64,
    /// 完整进度（阶段/切片数/笔记进度/生成进度/回查报告/用户选择）
    pub progress: engine::Progress,
    /// 段清单（切块完成后、待选择阶段提供）
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub segments: Vec<SegmentInfoDto>,
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
        selected: None,
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
    let segments = engine::read_segments(&id)
        .into_iter()
        .map(|s| SegmentInfoDto {
            index: s.index,
            label: s.label,
            chars: s.chars,
            excerpt: s.excerpt,
            has_name: s.has_name,
        })
        .collect();
    Ok(Json(TaskDetail {
        id: rec.meta.id.clone(),
        title: rec.meta.title.clone(),
        source_name: rec.meta.source_name.clone(),
        character: rec.meta.character.clone(),
        aliases: rec.meta.aliases.clone(),
        created_at: rec.meta.created_at,
        progress: rec.progress,
        segments,
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

#[derive(Deserialize, ToSchema)]
pub struct SelectSegmentsRequest {
    /// 选中的段序号（切块结果 index；去重后须非空）
    pub selected: Vec<usize>,
}

/// 选择闸门：提交段选择后开始分析（全选则全收）
#[utoipa::path(
    post,
    path = "/api/v1/db/deconstruct/tasks/{id}/select",
    tag = "deconstruct",
    params(("id" = String, Path, description = "任务 id")),
    request_body = SelectSegmentsRequest,
    responses(
        (status = 200, description = "已提交选择并开始分析", body = TaskSummary),
        (status = 400, description = "非待选择状态/选择为空", body = ErrorResponse),
        (status = 404, description = "任务不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn select_segments(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
    Json(req): Json<SelectSegmentsRequest>,
) -> Result<Json<TaskSummary>, ApiError> {
    let record = engine::select_segments(&state.deconstruct, &id, req.selected)
        .map_err(|e| bad_request(&e))?;
    Ok(Json(TaskSummary::from_record(&record)))
}

/// 任务原文与元信息（复制为新制作的表单回填）
#[utoipa::path(
    get,
    path = "/api/v1/db/deconstruct/tasks/{id}/source",
    tag = "deconstruct",
    params(("id" = String, Path, description = "任务 id")),
    responses(
        (status = 200, description = "原文与元信息", body = TaskSourceResponse),
        (status = 404, description = "任务不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn get_task_source(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> Result<Json<TaskSourceResponse>, ApiError> {
    let (meta, content) = engine::read_source(&id).ok_or_else(not_found)?;
    let _ = state;
    Ok(Json(TaskSourceResponse {
        source_name: meta.source_name,
        character: meta.character,
        aliases: meta.aliases,
        content,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct TaskSourceResponse {
    pub source_name: String,
    pub character: String,
    pub aliases: Vec<String>,
    /// 剧本原文（复制任务时回填）
    pub content: String,
}

/// 调用日志列表（轻量摘要；全文走详情端点）
#[utoipa::path(
    get,
    path = "/api/v1/db/deconstruct/tasks/{id}/logs",
    tag = "deconstruct",
    params(("id" = String, Path, description = "任务 id")),
    responses(
        (status = 200, description = "调用日志列表", body = LogListResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn list_task_logs(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> Json<LogListResponse> {
    let _ = state;
    Json(LogListResponse {
        logs: engine::read_logs(&id),
    })
}

#[derive(Serialize, ToSchema)]
pub struct LogListResponse {
    pub logs: Vec<engine::LogSummary>,
}

/// 单条调用日志全文（请求 + 响应/错误）
#[utoipa::path(
    get,
    path = "/api/v1/db/deconstruct/tasks/{id}/logs/{seq}",
    tag = "deconstruct",
    params(
        ("id" = String, Path, description = "任务 id"),
        ("seq" = u32, Path, description = "日志序号")
    ),
    responses(
        (status = 200, description = "日志全文", body = serde_json::Value),
        (status = 404, description = "日志不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn get_task_log(
    State(state): State<crate::api::AppState>,
    Path((id, seq)): Path<(String, u32)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let _ = state;
    let log = engine::read_log(&id, seq).ok_or_else(not_found)?;
    Ok(Json(log))
}

/// 中止运行中的任务（LLM 流式请求随之断开；可从断点重试）
#[utoipa::path(
    post,
    path = "/api/v1/db/deconstruct/tasks/{id}/abort",
    tag = "deconstruct",
    params(("id" = String, Path, description = "任务 id")),
    responses(
        (status = 200, description = "已中止", body = TaskSummary),
        (status = 400, description = "任务不在运行中", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn abort_task(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> Result<Json<TaskSummary>, ApiError> {
    let record = engine::abort_task(&state.deconstruct, &id).map_err(|e| bad_request(&e))?;
    Ok(Json(TaskSummary::from_record(&record)))
}

#[derive(Deserialize, ToSchema)]
pub struct RenameTaskRequest {
    /// 新显示名；空串 = 清除自定义名（回退角色名显示）
    pub title: String,
}

/// 任务改名
#[utoipa::path(
    post,
    path = "/api/v1/db/deconstruct/tasks/{id}/rename",
    tag = "deconstruct",
    params(("id" = String, Path, description = "任务 id")),
    request_body = RenameTaskRequest,
    responses(
        (status = 200, description = "已改名", body = TaskSummary),
        (status = 404, description = "任务不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn rename_task(
    Path(id): Path<String>,
    Json(req): Json<RenameTaskRequest>,
) -> Result<Json<TaskSummary>, ApiError> {
    let record = engine::rename_task(&id, &req.title).map_err(|e| bad_request(&e))?;
    Ok(Json(TaskSummary::from_record(&record)))
}

/// 失败/中断任务从断点重试（跳过已有产物的阶段）
#[utoipa::path(
    post,
    path = "/api/v1/db/deconstruct/tasks/{id}/retry",
    tag = "deconstruct",
    params(("id" = String, Path, description = "任务 id")),
    responses(
        (status = 200, description = "已从断点重启", body = TaskSummary),
        (status = 400, description = "状态不允许重试（运行中/已完成）", body = ErrorResponse),
        (status = 404, description = "任务不存在", body = ErrorResponse),
        (status = 401, description = "未鉴权")
    ),
    security(("bearer_token" = []))
)]
pub(crate) async fn retry_task(
    State(state): State<crate::api::AppState>,
    Path(id): Path<String>,
) -> Result<Json<TaskSummary>, ApiError> {
    let record = engine::retry_task(&state.deconstruct, &id).map_err(|e| bad_request(&e))?;
    Ok(Json(TaskSummary::from_record(&record)))
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
