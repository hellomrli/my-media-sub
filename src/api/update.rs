use axum::{response::IntoResponse, routing::get, Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use crate::error::Result;

const GITHUB_REPO: &str = "hellomrli/my-media-sub";
const DOCKER_IMAGE: &str = "ghcr.io/hellomrli/my-media-sub";

#[derive(Serialize)]
struct Response<T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
}

impl<T> Response<T> {
    fn ok(data: T) -> Self {
        Self { data: Some(data) }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GithubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GithubAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateAsset {
    pub name: String,
    pub size: u64,
    pub download_url: String,
}

impl From<GithubAsset> for UpdateAsset {
    fn from(asset: GithubAsset) -> Self {
        Self {
            name: asset.name,
            size: asset.size,
            download_url: asset.browser_download_url,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UpdateCheckResponse {
    pub repository: String,
    pub current_version: String,
    pub latest_version: String,
    pub latest_tag: String,
    pub update_available: bool,
    pub release_name: String,
    pub release_url: String,
    pub release_notes: String,
    pub published_at: Option<String>,
    pub checked_at: String,
    pub runtime: String,
    pub assets: Vec<UpdateAsset>,
    pub linux_x86_64_asset: Option<UpdateAsset>,
    pub checksum_asset: Option<UpdateAsset>,
    pub docker_latest_image: String,
    pub docker_version_image: String,
    pub docker_compose_command: String,
}

async fn check_update() -> Result<impl IntoResponse> {
    let release = fetch_latest_release().await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let latest_version = normalize_version(&release.tag_name);
    let update_available = is_newer_version(&latest_version, &current_version);
    let linux_x86_64_asset = find_asset(&release.assets, "linux-x86_64.tar.gz").map(Into::into);
    let checksum_asset = find_asset(&release.assets, "linux-x86_64.tar.gz.sha256").map(Into::into);
    let docker_version_tag = if latest_version.is_empty() {
        "latest".to_string()
    } else {
        latest_version.clone()
    };

    let response = UpdateCheckResponse {
        repository: GITHUB_REPO.to_string(),
        current_version,
        latest_version,
        latest_tag: release.tag_name.clone(),
        update_available,
        release_name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        release_url: release.html_url,
        release_notes: release.body.unwrap_or_default(),
        published_at: release.published_at,
        checked_at: Utc::now().to_rfc3339(),
        runtime: detect_runtime(),
        assets: release.assets.into_iter().map(Into::into).collect(),
        linux_x86_64_asset,
        checksum_asset,
        docker_latest_image: format!("{}:latest", DOCKER_IMAGE),
        docker_version_image: format!("{}:{}", DOCKER_IMAGE, docker_version_tag),
        docker_compose_command: "docker compose pull && docker compose up -d".to_string(),
    };

    Ok(Json(Response::ok(response)))
}

async fn fetch_latest_release() -> Result<GithubRelease> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );
    let client = reqwest::Client::new();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-update-check")
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<GithubRelease>().await?)
}

fn detect_runtime() -> String {
    if std::path::Path::new("/.dockerenv").exists() {
        "docker".to_string()
    } else {
        "binary".to_string()
    }
}

fn normalize_version(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('v')
        .trim_start_matches('V')
        .to_string()
}

fn find_asset(assets: &[GithubAsset], suffix: &str) -> Option<GithubAsset> {
    assets
        .iter()
        .find(|asset| asset.name.ends_with(suffix))
        .cloned()
}

fn is_newer_version(candidate: &str, current: &str) -> bool {
    compare_versions(candidate, current) == Some(Ordering::Greater)
}

fn compare_versions(left: &str, right: &str) -> Option<Ordering> {
    let left_parts = version_parts(left);
    let right_parts = version_parts(right);
    if left_parts.is_empty() || right_parts.is_empty() {
        return None;
    }

    for index in 0..left_parts.len().max(right_parts.len()) {
        let left_value = left_parts.get(index).copied().unwrap_or(0);
        let right_value = right_parts.get(index).copied().unwrap_or(0);
        match left_value.cmp(&right_value) {
            Ordering::Equal => {}
            ordering => return Some(ordering),
        }
    }

    Some(Ordering::Equal)
}

fn version_parts(value: &str) -> Vec<u64> {
    normalize_version(value)
        .split(['.', '-', '+'])
        .filter_map(|part| {
            let digits: String = part.chars().take_while(|ch| ch.is_ascii_digit()).collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse::<u64>().ok()
            }
        })
        .collect()
}

pub fn routes() -> Router {
    Router::new().route("/api/update/check", get(check_update))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GithubAsset {
        GithubAsset {
            name: name.to_string(),
            size: 42,
            browser_download_url: format!("https://example.com/{}", name),
        }
    }

    #[test]
    fn test_version_compare_handles_tags() {
        assert!(is_newer_version("v0.7.15", "0.7.14"));
        assert!(is_newer_version("0.8.0", "0.7.99"));
        assert!(!is_newer_version("0.7.14", "0.7.14"));
        assert!(!is_newer_version("0.7.13", "0.7.14"));
    }

    #[test]
    fn test_find_release_assets() {
        let assets = vec![
            asset("my-media-sub-v0.7.15-linux-x86_64.tar.gz"),
            asset("my-media-sub-v0.7.15-linux-x86_64.tar.gz.sha256"),
        ];

        let archive = find_asset(&assets, "linux-x86_64.tar.gz").unwrap();
        let checksum = find_asset(&assets, "linux-x86_64.tar.gz.sha256").unwrap();

        assert_eq!(archive.name, "my-media-sub-v0.7.15-linux-x86_64.tar.gz");
        assert_eq!(
            checksum.name,
            "my-media-sub-v0.7.15-linux-x86_64.tar.gz.sha256"
        );
    }
}
