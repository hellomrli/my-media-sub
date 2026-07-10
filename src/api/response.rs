use axum::Json;
use serde::Serialize;

/// Stable JSON envelope shared by read/query APIs.
#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            message: None,
        }
    }

    pub fn success(data: T) -> Self {
        Self::ok(data)
    }

    pub fn with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            ok: true,
            data: Some(data),
            message: Some(message.into()),
        }
    }
}

pub fn json_ok<T>(data: T) -> Json<ApiResponse<T>> {
    Json(ApiResponse::ok(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_stable_success_envelope() {
        let value = serde_json::to_value(ApiResponse::with_message(vec![1, 2], "done")).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["data"], serde_json::json!([1, 2]));
        assert_eq!(value["message"], "done");
    }
}
