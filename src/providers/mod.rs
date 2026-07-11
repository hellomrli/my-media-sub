//! Cloud drive capability boundary and provider selection.
//!
//! Business services depend on [`CloudDriveProvider`] instead of a concrete
//! cloud vendor client. Provider-specific extensions (for example Quark sign-in)
//! intentionally stay outside this trait.

mod mock;
mod quark;

pub use mock::MockCloudDriveProvider;
pub use quark::QuarkCloudDriveProvider;

use crate::error::{AppError, Result};
use crate::models::Settings;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderFile {
    pub id: String,
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    #[serde(default)]
    pub parent_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderProbeResult {
    pub ok: bool,
    pub state: String,
    pub message: String,
    #[serde(default)]
    pub files: Vec<ProviderFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DriveItem {
    pub id: String,
    pub parent_id: String,
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    #[serde(default)]
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadInfo {
    pub id: String,
    pub file_name: String,
    pub size: i64,
    pub download_url: String,
    #[serde(default)]
    pub headers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderHealth {
    pub healthy: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferRequest {
    pub share_url: String,
    pub passcode: String,
    pub target_id: String,
    /// Stable IDs returned by `probe`; an empty list means all files.
    pub file_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferOutcome {
    pub transferred_files: Vec<ProviderFile>,
}

/// Vendor-neutral capabilities used by drive APIs and business services.
///
/// Returning boxed futures keeps the trait object-safe without coupling the
/// project to an async-trait macro.
pub trait CloudDriveProvider: Send + Sync {
    fn cloud_type(&self) -> &'static str;
    fn probe<'a>(
        &'a self,
        url: &'a str,
        passcode: &'a str,
        max_files: usize,
    ) -> ProviderFuture<'a, ProviderProbeResult>;
    fn list<'a>(&'a self, parent_id: &'a str) -> ProviderFuture<'a, Vec<DriveItem>>;
    fn find<'a>(
        &'a self,
        parent_id: &'a str,
        name: &'a str,
    ) -> ProviderFuture<'a, Option<DriveItem>>;
    fn ensure<'a>(&'a self, path: &'a str) -> ProviderFuture<'a, String>;
    fn transfer<'a>(&'a self, request: TransferRequest) -> ProviderFuture<'a, TransferOutcome>;
    fn rename<'a>(
        &'a self,
        id: &'a str,
        name: &'a str,
        parent_id: Option<&'a str>,
    ) -> ProviderFuture<'a, ()>;
    fn delete<'a>(&'a self, ids: &'a [String]) -> ProviderFuture<'a, ()>;
    fn download_info<'a>(&'a self, ids: &'a [String]) -> ProviderFuture<'a, Vec<DownloadInfo>>;
    fn health(&self) -> ProviderFuture<'_, ProviderHealth>;
}

/// Resolves `cloud_type` to a provider. Tests can inject deterministic providers;
/// production falls back to built-in provider factories.
#[derive(Clone, Default)]
pub struct CloudDriveProviderRegistry {
    overrides: Arc<HashMap<String, Arc<dyn CloudDriveProvider>>>,
}

impl CloudDriveProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_provider(mut self, provider: Arc<dyn CloudDriveProvider>) -> Self {
        let mut providers = (*self.overrides).clone();
        providers.insert(normalize_cloud_type(provider.cloud_type()), provider);
        self.overrides = Arc::new(providers);
        self
    }

    pub fn resolve(
        &self,
        cloud_type: &str,
        settings: &Settings,
    ) -> Result<Arc<dyn CloudDriveProvider>> {
        self.resolve_with_quark_cookie(cloud_type, settings.quark_cookie.clone())
    }

    pub fn resolve_with_quark_cookie(
        &self,
        cloud_type: &str,
        quark_cookie: impl Into<String>,
    ) -> Result<Arc<dyn CloudDriveProvider>> {
        let cloud_type = normalize_cloud_type(cloud_type);
        if let Some(provider) = self.overrides.get(&cloud_type) {
            return Ok(provider.clone());
        }
        match cloud_type.as_str() {
            "quark" => Ok(Arc::new(QuarkCloudDriveProvider::new(quark_cookie))),
            _ => Err(AppError::Validation(format!(
                "不支持的云盘类型: {}",
                display_cloud_type(&cloud_type)
            ))),
        }
    }
}

pub fn normalize_cloud_type(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() {
        "quark".to_string()
    } else {
        value
    }
}

/// Normalize and validate a cloud type accepted by the production registry.
pub fn validate_cloud_type(value: &str) -> Result<String> {
    let cloud_type = normalize_cloud_type(value);
    match cloud_type.as_str() {
        "quark" => Ok(cloud_type),
        _ => Err(AppError::Validation(format!(
            "不支持的云盘类型: {}",
            display_cloud_type(&cloud_type)
        ))),
    }
}

fn display_cloud_type(value: &str) -> &str {
    if value.is_empty() {
        "(empty)"
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_defaults_empty_cloud_type_to_quark() {
        let provider = CloudDriveProviderRegistry::new()
            .resolve("", &Settings::default())
            .unwrap();
        assert_eq!(provider.cloud_type(), "quark");
    }

    #[test]
    fn registry_rejects_unknown_cloud_type() {
        let error = CloudDriveProviderRegistry::new()
            .resolve("aliyun", &Settings::default())
            .err()
            .expect("unknown provider must fail");
        assert!(error.to_string().contains("不支持的云盘类型"));
    }
}
