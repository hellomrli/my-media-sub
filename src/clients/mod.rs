pub mod aria2;
pub mod douban;
pub mod http_pool;
pub mod pansou;
pub mod quark;
pub mod quark_save;

pub use aria2::Aria2Client;
pub use pansou::PanSouClient;
pub use quark::QuarkShareProbe;
pub use quark_save::{NormalizedItem, QuarkSaveClient, QuarkSigninResult};

use crate::error::{AppError, Result};

pub(crate) fn ensure_upstream_status(response: &reqwest::Response, operation: &str) -> Result<()> {
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok());
    if let Some(error) = upstream_status_error(response.status(), retry_after, operation) {
        return Err(error);
    }
    Ok(())
}

fn upstream_status_error(
    status: reqwest::StatusCode,
    retry_after: Option<&str>,
    operation: &str,
) -> Option<AppError> {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = retry_after
            .map(|value| format!("，建议 {} 秒后重试", value))
            .unwrap_or_default();
        return Some(AppError::RateLimited(format!(
            "{} 触发上游限速{}",
            operation, retry_after
        )));
    }
    if !status.is_success() {
        return Some(AppError::UpstreamStatus {
            status,
            message: format!("{} HTTP 状态异常: {}", operation, status),
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_429_preserves_retry_after_hint() {
        let error = upstream_status_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            Some("12"),
            "夸克请求",
        )
        .unwrap();

        assert!(matches!(error, AppError::RateLimited(_)));
        assert!(error.to_string().contains("12 秒后重试"));
    }

    #[test]
    fn upstream_non_success_status_preserves_status() {
        let error =
            upstream_status_error(reqwest::StatusCode::BAD_GATEWAY, None, "夸克请求").unwrap();

        assert!(matches!(
            error,
            AppError::UpstreamStatus {
                status: reqwest::StatusCode::BAD_GATEWAY,
                ..
            }
        ));
    }

    #[test]
    fn upstream_not_found_status_is_preserved() {
        let error =
            upstream_status_error(reqwest::StatusCode::NOT_FOUND, None, "请求夸克 token").unwrap();

        match error {
            AppError::UpstreamStatus { status, message } => {
                assert_eq!(status, reqwest::StatusCode::NOT_FOUND);
                assert!(message.contains("404 Not Found"));
            }
            other => panic!("expected upstream status error, got {other:?}"),
        }
    }
}
