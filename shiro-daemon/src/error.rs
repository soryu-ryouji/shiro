// 统一错误响应：所有 handler 返回 `ApiError`，由 axum 序列化为 `{message}` JSON。
// 共享叶子模块（不依赖任何业务模块），infra / features / auth 均可使用。

use axum::Json;
use axum::http::StatusCode;

#[derive(serde::Serialize, utoipa::ToSchema)]
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

pub(crate) fn forbidden(message: &str) -> ApiError {
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse {
            message: message.into(),
        }),
    )
}

pub(crate) fn not_found(message: &str) -> ApiError {
    (
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            message: message.into(),
        }),
    )
}

pub(crate) fn conflict(message: &str) -> ApiError {
    (
        StatusCode::CONFLICT,
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
