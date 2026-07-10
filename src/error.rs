use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use std::fmt;

/// 应用错误类型
#[derive(Debug)]
pub enum AppError {
    /// 数据库错误
    Database(String),
    /// 网络请求错误
    Http(String),
    /// 配置错误
    Config(String),
    /// 验证错误
    Validation(String),
    /// 上游或本地并发保护触发
    RateLimited(String),
    /// 未找到
    NotFound(String),
    /// 内部错误
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Database(msg) => write!(f, "Database error: {}", msg),
            AppError::Http(msg) => write!(f, "HTTP error: {}", msg),
            AppError::Config(msg) => write!(f, "Config error: {}", msg),
            AppError::Validation(msg) => write!(f, "Validation error: {}", msg),
            AppError::RateLimited(msg) => write!(f, "Rate limited: {}", msg),
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

/// 错误响应
#[derive(Debug, Clone, Serialize)]
pub(crate) struct ErrorResponse {
    ok: bool,
    error: String,
    message: String,
}

impl ErrorResponse {
    pub(crate) fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: error.into(),
            message: message.into(),
        }
    }
}

pub(crate) fn json_error_response(
    status: StatusCode,
    error: impl Into<String>,
    message: impl Into<String>,
) -> Response {
    (status, Json(ErrorResponse::new(error, message))).into_response()
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match &self {
            AppError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "database_error",
                "数据存储错误".to_string(),
            ),
            AppError::Http(_) => (
                StatusCode::BAD_GATEWAY,
                "http_error",
                "上游服务请求失败".to_string(),
            ),
            AppError::Config(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "config_error",
                "服务配置错误".to_string(),
            ),
            AppError::Validation(msg) => (StatusCode::BAD_REQUEST, "validation_error", msg.clone()),
            AppError::RateLimited(msg) => {
                (StatusCode::TOO_MANY_REQUESTS, "rate_limited", msg.clone())
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "服务内部错误".to_string(),
            ),
        };

        if matches!(
            &self,
            AppError::Database(_) | AppError::Http(_) | AppError::Config(_) | AppError::Internal(_)
        ) {
            tracing::error!("请求处理失败: {}", self);
        }

        json_error_response(status, error_type, message)
    }
}

/// From implementations for common error types
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::Http(err.to_string())
    }
}

impl From<tokio_cron_scheduler::JobSchedulerError> for AppError {
    fn from(err: tokio_cron_scheduler::JobSchedulerError) -> Self {
        AppError::Internal(format!("Scheduler error: {}", err))
    }
}

/// 结果类型别名
pub type Result<T> = std::result::Result<T, AppError>;
