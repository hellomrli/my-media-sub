use axum::{
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use ring::digest;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;
use tokio::io::AsyncWriteExt;

use super::response::ApiResponse as Response;
use crate::clients::http_pool;
use crate::error::{AppError, Result};
use crate::restart::RestartPlan;
use crate::utils::constant_time_eq;
use crate::utils::format_bytes;

const GITHUB_REPO: &str = "hellomrli/my-media-sub";
const REQUIRED_STATIC_ASSETS: &[&str] = &[
    "index.html",
    "manifest.webmanifest",
    "service-worker.js",
    "openapi.json",
];
static UPDATE_PROGRESS: LazyLock<Mutex<UpdateProgressResponse>> =
    LazyLock::new(|| Mutex::new(UpdateProgressResponse::idle()));
static PENDING_RESTART: LazyLock<Mutex<Option<RestartPlan>>> = LazyLock::new(|| Mutex::new(None));

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
    /// Whether this process can atomically replace its executable and WebUI.
    /// Standard Docker images opt in by running from a writable persistent
    /// runtime volume instead of directly from the immutable image layer.
    pub online_update_supported: bool,
    pub linux_x86_64_asset: Option<UpdateAsset>,
}

#[derive(Debug, Serialize)]
pub struct UpdateReleaseResponse {
    pub tag: String,
    pub version: String,
    pub name: String,
    pub release_url: String,
    pub published_at: Option<String>,
    pub asset: Option<UpdateAsset>,
    pub is_current: bool,
    pub is_newer: bool,
}

#[derive(Debug, Deserialize)]
pub struct UpdateApplyRequest {
    pub tag: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateApplyResponse {
    pub success: bool,
    pub previous_version: String,
    pub new_version: String,
    pub binary_path: String,
    pub backup_path: String,
    pub restart_required: bool,
    pub auto_restart_scheduled: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateRestartResponse {
    pub success: bool,
    pub restart_scheduled: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateProgressResponse {
    pub running: bool,
    pub percent: u8,
    pub stage: String,
    pub message: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<String>,
    pub updated_at: String,
}

impl UpdateProgressResponse {
    fn idle() -> Self {
        Self {
            running: false,
            percent: 0,
            stage: "idle".to_string(),
            message: "等待升级".to_string(),
            downloaded_bytes: 0,
            total_bytes: None,
            error: None,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

async fn check_update() -> Result<impl IntoResponse> {
    let release = fetch_latest_release().await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let latest_version = normalize_version(&release.tag_name);
    let update_available = is_newer_version(&latest_version, &current_version);
    let linux_x86_64_asset = find_asset(&release.assets, "linux-x86_64.tar.gz").map(Into::into);
    let runtime = detect_runtime();

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
        online_update_supported: online_update_supported(&runtime),
        runtime,
        linux_x86_64_asset,
    };

    Ok(Json(Response::ok(response)))
}

async fn list_releases() -> Result<impl IntoResponse> {
    let releases = fetch_releases().await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let response = releases
        .into_iter()
        .map(|release| release_to_response(release, &current_version))
        .collect::<Vec<_>>();

    Ok(Json(Response::ok(response)))
}

async fn apply_update(request: Option<Json<UpdateApplyRequest>>) -> Result<impl IntoResponse> {
    let runtime = detect_runtime();
    if !online_update_supported(&runtime) {
        return Err(AppError::Validation(online_update_unavailable_message(
            &runtime,
        )));
    }
    ensure_no_pending_restart()?;

    let target_tag = request.and_then(|Json(req)| req.tag).and_then(|tag| {
        let tag = tag.trim().to_string();
        (!tag.is_empty()).then_some(tag)
    });
    let message = target_tag
        .as_deref()
        .map(|tag| format!("正在准备切换到 {}", tag))
        .unwrap_or_else(|| "正在检查最新版本".to_string());

    try_begin_update_progress(message)?;
    match apply_update_inner(target_tag).await {
        Ok(response) => Ok(Json(Response::ok(response))),
        Err(error) => {
            fail_update_progress(error.to_string());
            Err(error)
        }
    }
}

async fn update_progress() -> Result<impl IntoResponse> {
    Ok(Json(Response::ok(current_update_progress())))
}

async fn restart_update() -> Result<impl IntoResponse> {
    let plan = PENDING_RESTART
        .lock()
        .map_err(|_| AppError::Internal("读取重启计划失败".to_string()))?
        .take()
        .ok_or_else(|| AppError::Validation("当前没有待重启的升级任务".to_string()))?;

    if let Err(message) = crate::restart::request(plan.clone()) {
        store_pending_restart(plan)?;
        return Err(AppError::Validation(message));
    }
    finish_update_progress("服务正在重启，请稍后刷新页面", "restarting");

    Ok(Json(Response::ok(UpdateRestartResponse {
        success: true,
        restart_scheduled: true,
        message: "服务正在重启，请稍后刷新页面".to_string(),
    })))
}

fn current_update_progress() -> UpdateProgressResponse {
    UPDATE_PROGRESS
        .lock()
        .map(|progress| progress.clone())
        .unwrap_or_else(|_| UpdateProgressResponse::idle())
}

fn try_begin_update_progress(message: impl Into<String>) -> Result<()> {
    let mut progress = UPDATE_PROGRESS
        .lock()
        .map_err(|_| AppError::Internal("读取升级状态失败".to_string()))?;
    if progress.running {
        return Err(AppError::Validation("已有升级任务正在执行".to_string()));
    }

    *progress = UpdateProgressResponse {
        running: true,
        percent: 1,
        stage: "starting".to_string(),
        message: message.into(),
        downloaded_bytes: 0,
        total_bytes: None,
        error: None,
        updated_at: Utc::now().to_rfc3339(),
    };
    Ok(())
}

fn set_update_progress(percent: u8, stage: &str, message: impl Into<String>) {
    if let Ok(mut progress) = UPDATE_PROGRESS.lock() {
        progress.running = true;
        progress.percent = percent.min(100);
        progress.stage = stage.to_string();
        progress.message = message.into();
        progress.error = None;
        progress.updated_at = Utc::now().to_rfc3339();
    }
}

fn set_download_progress(downloaded_bytes: u64, total_bytes: Option<u64>) {
    let percent = total_bytes
        .filter(|total| *total > 0)
        .map(|total| 10 + ((downloaded_bytes.saturating_mul(58) / total).min(58) as u8))
        .unwrap_or(20);
    let message = match total_bytes {
        Some(total) if total > 0 => format!(
            "正在下载升级包 {} / {}",
            format_bytes(downloaded_bytes),
            format_bytes(total)
        ),
        _ => format!("正在下载升级包 {}", format_bytes(downloaded_bytes)),
    };

    if let Ok(mut progress) = UPDATE_PROGRESS.lock() {
        progress.running = true;
        progress.percent = percent.min(68);
        progress.stage = "download".to_string();
        progress.message = message;
        progress.downloaded_bytes = downloaded_bytes;
        progress.total_bytes = total_bytes;
        progress.error = None;
        progress.updated_at = Utc::now().to_rfc3339();
    }
}

fn finish_update_progress(message: impl Into<String>, stage: &str) {
    if let Ok(mut progress) = UPDATE_PROGRESS.lock() {
        progress.running = false;
        progress.percent = 100;
        progress.stage = stage.to_string();
        progress.message = message.into();
        progress.error = None;
        progress.updated_at = Utc::now().to_rfc3339();
    }
}

fn fail_update_progress(message: impl Into<String>) {
    let message = message.into();
    if let Ok(mut progress) = UPDATE_PROGRESS.lock() {
        progress.running = false;
        progress.stage = "failed".to_string();
        progress.message = message.clone();
        progress.error = Some(message);
        progress.updated_at = Utc::now().to_rfc3339();
    }
}

async fn apply_update_inner(target_tag: Option<String>) -> Result<UpdateApplyResponse> {
    let release = match target_tag {
        Some(ref tag) => fetch_release_by_tag(tag).await?,
        None => fetch_latest_release().await?,
    };
    set_update_progress(5, "checking", "正在校验版本信息");
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let target_version = normalize_version(&release.tag_name);
    if target_version == current_version {
        return Err(AppError::Validation(format!(
            "当前已经是 {}",
            release.tag_name
        )));
    }
    if target_tag.is_none() && !is_newer_version(&target_version, &current_version) {
        return Err(AppError::Validation("当前已是最新版本".to_string()));
    }

    let asset = find_asset(&release.assets, "linux-x86_64.tar.gz")
        .ok_or_else(|| AppError::NotFound("Release 中未找到 Linux x86_64 二进制包".to_string()))?;
    let checksum_asset = find_asset(&release.assets, "linux-x86_64.tar.gz.sha256")
        .ok_or_else(|| AppError::NotFound("Release 中未找到 SHA256 校验文件".to_string()))?;
    let current_exe = std::env::current_exe()
        .map_err(|e| AppError::Internal(format!("无法定位当前二进制: {}", e)))?;
    let target_static_dir = crate::utils::static_dir();
    let restart_plan = RestartPlan::for_executable(&current_exe);
    let backup_path = backup_path(&current_exe);
    let work_dir = std::env::temp_dir().join(format!(
        "my-media-sub-update-{}-{}",
        target_version,
        uuid::Uuid::new_v4()
    ));
    tokio::fs::create_dir_all(&work_dir)
        .await
        .map_err(|e| AppError::Internal(format!("创建升级临时目录失败: {}", e)))?;

    let install_result = download_and_install_release(
        &asset,
        &checksum_asset,
        &work_dir,
        &current_exe,
        &target_static_dir,
        &backup_path,
    )
    .await;
    if install_result.is_ok() {
        set_update_progress(97, "cleanup", "正在清理升级临时文件");
    }
    let _ = tokio::fs::remove_dir_all(&work_dir).await;
    install_result?;

    record_runtime_version(&current_exe, &target_version).await;
    prune_update_backups(&current_exe, &target_static_dir).await;
    store_pending_restart(restart_plan)?;
    finish_update_progress("升级完成，请点击按钮重启服务并刷新页面", "restart_required");

    Ok(UpdateApplyResponse {
        success: true,
        previous_version: current_version,
        new_version: target_version,
        binary_path: current_exe.display().to_string(),
        backup_path: backup_path.display().to_string(),
        restart_required: true,
        auto_restart_scheduled: false,
        message: format!("已切换到 {}，请重启服务后生效", release.tag_name),
    })
}

async fn download_and_install_release(
    asset: &GithubAsset,
    checksum_asset: &GithubAsset,
    work_dir: &Path,
    current_exe: &Path,
    target_static_dir: &Path,
    backup_path: &Path,
) -> Result<()> {
    let archive_path = work_dir.join(&asset.name);
    set_update_progress(8, "checksum", "正在下载校验文件");
    let checksum_content = download_asset_bytes(&checksum_asset.browser_download_url).await?;
    download_asset(&asset.browser_download_url, &archive_path, asset.size).await?;
    set_update_progress(69, "checksum", "正在校验升级包 SHA256");
    verify_sha256(&archive_path, &asset.name, &checksum_content).await?;
    set_update_progress(70, "extracting", "正在解压升级包");
    extract_archive(&archive_path, work_dir).await?;
    set_update_progress(82, "locating", "正在检查升级包内容");
    let new_binary = find_binary(work_dir)
        .ok_or_else(|| AppError::Internal("升级包中未找到 my-media-sub 二进制".to_string()))?;
    let new_static_dir = find_static_dir(work_dir)
        .ok_or_else(|| AppError::Internal("升级包中未找到完整 static 目录".to_string()))?;
    set_update_progress(86, "assets", "正在暂存二进制和 WebUI 静态资源");
    replace_update_payload(
        &new_binary,
        &new_static_dir,
        current_exe,
        target_static_dir,
        backup_path,
    )
    .await?;
    Ok(())
}

fn store_pending_restart(plan: RestartPlan) -> Result<()> {
    let mut pending = PENDING_RESTART
        .lock()
        .map_err(|_| AppError::Internal("保存重启计划失败".to_string()))?;
    if pending.is_some() {
        return Err(AppError::Validation(
            "已有升级等待重启，请先完成重启".to_string(),
        ));
    }
    *pending = Some(plan);
    Ok(())
}

fn ensure_no_pending_restart() -> Result<()> {
    let pending = PENDING_RESTART
        .lock()
        .map_err(|_| AppError::Internal("读取重启计划失败".to_string()))?;
    if pending.is_some() {
        return Err(AppError::Validation(
            "已有升级等待重启，请先完成重启".to_string(),
        ));
    }
    Ok(())
}

async fn fetch_latest_release() -> Result<GithubRelease> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );
    let client = http_pool::default_client();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-update-check")
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<GithubRelease>().await?)
}

async fn fetch_release_by_tag(tag: &str) -> Result<GithubRelease> {
    let tag = tag.trim().trim_start_matches('/').to_string();
    if tag.is_empty() || tag.contains('/') {
        return Err(AppError::Validation("版本标签无效".to_string()));
    }

    let url = format!(
        "https://api.github.com/repos/{}/releases/tags/{}",
        GITHUB_REPO, tag
    );
    let client = http_pool::default_client();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-update-check")
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<GithubRelease>().await?)
}

async fn fetch_releases() -> Result<Vec<GithubRelease>> {
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page=20",
        GITHUB_REPO
    );
    let client = http_pool::default_client();
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-update-check")
        .send()
        .await?
        .error_for_status()?;

    Ok(response.json::<Vec<GithubRelease>>().await?)
}

fn release_to_response(release: GithubRelease, current_version: &str) -> UpdateReleaseResponse {
    let version = normalize_version(&release.tag_name);
    let is_current = version == current_version;
    let is_newer = is_newer_version(&version, current_version);
    UpdateReleaseResponse {
        tag: release.tag_name.clone(),
        version,
        name: release.name.unwrap_or_else(|| release.tag_name.clone()),
        release_url: release.html_url,
        published_at: release.published_at,
        asset: find_asset(&release.assets, "linux-x86_64.tar.gz").map(Into::into),
        is_current,
        is_newer,
    }
}

fn detect_runtime() -> String {
    if std::path::Path::new("/.dockerenv").exists() {
        "docker".to_string()
    } else {
        "binary".to_string()
    }
}

fn online_update_supported(runtime: &str) -> bool {
    online_update_supported_for(
        runtime,
        optional_env_flag("SELF_UPDATE_ENABLED"),
        managed_docker_runtime_layout(),
    )
}

fn online_update_supported_for(
    runtime: &str,
    configured_enabled: Option<bool>,
    managed_runtime_layout: bool,
) -> bool {
    match runtime {
        "binary" => configured_enabled.unwrap_or(true),
        "docker" => configured_enabled.unwrap_or(false) && managed_runtime_layout,
        _ => false,
    }
}

fn optional_env_flag(key: &str) -> Option<bool> {
    std::env::var(key)
        .ok()
        .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
}

fn managed_runtime_dir() -> Option<PathBuf> {
    std::env::var("APP_RUNTIME_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn managed_docker_runtime_layout() -> bool {
    let Some(runtime_dir) = managed_runtime_dir() else {
        return false;
    };
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    path_is_within(&executable, &runtime_dir)
        && path_is_within(&crate::utils::static_dir(), &runtime_dir)
        && directory_is_writable(&runtime_dir)
}

fn path_is_within(path: &Path, directory: &Path) -> bool {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let directory = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    path.starts_with(directory)
}

#[cfg(unix)]
fn directory_is_writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // Replacing entries requires both write and search permission on the
    // containing directory. `access` evaluates the real uid/gid of the app.
    unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
}

#[cfg(not(unix))]
fn directory_is_writable(_path: &Path) -> bool {
    true
}

fn online_update_unavailable_message(runtime: &str) -> String {
    if runtime == "docker" {
        "当前 Docker 容器未启用可写的持久化运行目录，不能在线替换程序；请升级到新版 Compose 配置，或在宿主机执行：docker compose pull && docker compose up -d"
            .to_string()
    } else {
        "当前运行环境不支持在线替换程序，请手工升级二进制和完整 static 目录".to_string()
    }
}

async fn download_asset(url: &str, path: &Path, expected_size: u64) -> Result<()> {
    set_update_progress(10, "download", "正在连接 Release 下载地址");
    let mut response = http_pool::default_client()
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-self-update")
        .send()
        .await?
        .error_for_status()?;

    let fallback_total_bytes = (expected_size > 0).then_some(expected_size);
    let total_bytes = response.content_length().or(fallback_total_bytes);
    let mut downloaded_bytes = 0u64;
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|e| AppError::Internal(format!("创建升级包文件失败: {}", e)))?;

    while let Some(chunk) = response.chunk().await? {
        file.write_all(&chunk)
            .await
            .map_err(|e| AppError::Internal(format!("写入升级包失败: {}", e)))?;
        downloaded_bytes += chunk.len() as u64;
        set_download_progress(downloaded_bytes, total_bytes);
    }
    file.flush()
        .await
        .map_err(|e| AppError::Internal(format!("刷新升级包文件失败: {}", e)))?;
    file.sync_all()
        .await
        .map_err(|e| AppError::Internal(format!("同步升级包文件失败: {}", e)))?;
    set_download_progress(downloaded_bytes, total_bytes);
    Ok(())
}

async fn download_asset_bytes(url: &str) -> Result<Vec<u8>> {
    let response = http_pool::default_client()
        .get(url)
        .header(reqwest::header::USER_AGENT, "my-media-sub-self-update")
        .send()
        .await?
        .error_for_status()?;
    Ok(response.bytes().await?.to_vec())
}

async fn verify_sha256(path: &Path, asset_name: &str, checksum_content: &[u8]) -> Result<()> {
    let expected = parse_sha256_checksum(checksum_content, asset_name)
        .ok_or_else(|| AppError::Validation("SHA256 校验文件格式无效".to_string()))?;
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| AppError::Internal(format!("读取升级包失败: {}", e)))?;
    let actual = digest::digest(&digest::SHA256, &bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect::<String>();

    if !constant_time_eq(&actual, &expected) {
        return Err(AppError::Validation("升级包 SHA256 校验失败".to_string()));
    }

    Ok(())
}

fn parse_sha256_checksum(content: &[u8], asset_name: &str) -> Option<String> {
    let text = String::from_utf8_lossy(content);
    let mut bare_checksum = None;
    let mut bare_count = 0usize;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts = line.split_whitespace().collect::<Vec<_>>();
        let Some(checksum) = parts.iter().copied().find(|part| is_sha256_checksum(part)) else {
            continue;
        };

        if checksum_matches_asset_line(line, checksum, asset_name) {
            return Some(checksum.to_ascii_lowercase());
        }

        if parts.len() == 1 && parts[0] == checksum {
            bare_count += 1;
            bare_checksum = Some(checksum.to_ascii_lowercase());
        }
    }

    (bare_count == 1).then_some(bare_checksum?).filter(|_| {
        text.lines()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('#')
            })
            .count()
            == 1
    })
}

fn is_sha256_checksum(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn checksum_matches_asset_line(line: &str, checksum: &str, asset_name: &str) -> bool {
    let normalized = line.replace('*', " ");
    if normalized.split_whitespace().any(|part| part == asset_name) {
        return true;
    }

    let bsd_prefix = format!("SHA256 ({asset_name}) =");
    line.starts_with(&bsd_prefix) && line.split_whitespace().last() == Some(checksum)
}

async fn extract_archive(archive_path: &Path, output_dir: &Path) -> Result<()> {
    let archive_path = archive_path.to_path_buf();
    let output_dir = output_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        verify_archive_members(&archive_path)?;
        let output = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&output_dir)
            // 升级包由发行流水线以非 root 构建，不保留属主/权限位可避免本地
            // 覆盖时把升级包里的 uid/可执行位原样写进运行目录。
            .arg("--no-same-owner")
            .arg("--no-same-permissions")
            .output()
            .map_err(|e| AppError::Internal(format!("执行 tar 解压失败: {}", e)))?;
        if !output.status.success() {
            return Err(AppError::Internal(format!(
                "解压升级包失败: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        ensure_extracted_inside(&output_dir)
    })
    .await
    .map_err(|e| AppError::Internal(format!("解压任务失败: {}", e)))?
}

/// 解压前校验归档成员：成员名必须为相对路径、不含 `..` 组件且非空，链接
/// 目标不得为绝对路径或包含 `..`。校验通过前不执行任何解压，防止恶意
/// 升级包把文件写出 work_dir。
fn verify_archive_members(archive_path: &Path) -> Result<()> {
    let list = std::process::Command::new("tar")
        .arg("-tvzf")
        .arg(archive_path)
        .output()
        .map_err(|e| AppError::Internal(format!("列出升级包内容失败: {}", e)))?;
    if !list.status.success() {
        return Err(AppError::Internal(format!(
            "列出升级包内容失败: {}",
            String::from_utf8_lossy(&list.stderr)
        )));
    }
    let listing = String::from_utf8_lossy(&list.stdout);
    let mut member_count = 0usize;
    for line in listing.lines() {
        // GNU tar 冗长列表格式：`permissions owner/group size date name [-> target]`。
        // 符号链接行含 ` -> `，需额外校验链接目标不逃逸。
        let (name, target) = parse_tar_listing_line(line);
        if !is_safe_member_path(name) {
            return Err(AppError::Validation(format!(
                "升级包包含不安全路径: {:?}",
                line
            )));
        }
        if let Some(target) = target {
            if !is_safe_member_path(target) {
                return Err(AppError::Validation(format!(
                    "升级包包含逃逸链接目标: {:?}",
                    line
                )));
            }
        }
        member_count += 1;
    }
    if member_count == 0 {
        return Err(AppError::Validation("升级包为空".to_string()));
    }
    Ok(())
}

/// 从 tar 冗长列表行解析成员名与可选的符号链接目标。
fn parse_tar_listing_line(line: &str) -> (&str, Option<&str>) {
    // GNU tar 冗长列表：`权限 owner/group 大小 日期 时间 name [-> target]`。
    // 字段间空白数量不定（size 前有大量填充空格），因此按「空白分隔字段计数」
    // 跳过前 5 个不含空格的字段，剩余部分从第 6 个字段起是成员名（可含空格）。
    let rest = line.trim_start();
    let mut field_count = 0usize;
    let mut in_field = false;
    let mut name_start = rest.len();
    for (index, ch) in rest.char_indices() {
        if ch.is_whitespace() {
            if in_field {
                field_count += 1;
                in_field = false;
                if field_count == 5 {
                    name_start = index;
                    break;
                }
            }
        } else {
            in_field = true;
        }
    }
    let name_with_target = rest[name_start..].trim_start();
    match name_with_target.split_once(" -> ") {
        Some((name, target)) => (name.trim(), Some(target.trim())),
        None => (name_with_target.trim(), None),
    }
}

/// 安全成员路径：相对路径、无 `..`/`.` 组件、无 NUL、不以 `/` 开头。
fn is_safe_member_path(path: &str) -> bool {
    // 目录成员名以 `/` 结尾，先剥掉再做逐组件校验。
    let path = path.trim_end_matches('/');
    if path.is_empty() || path.starts_with('/') || path.contains('\0') {
        return false;
    }
    path.split('/')
        .all(|component| !component.is_empty() && component != ".." && component != ".")
}

/// 解压完成后兜底校验：work_dir 下每个真实路径（含通过符号链接到达的）必须
/// 位于输出目录内；发现越界项则删除并报错，阻断符号链接绕过。
fn ensure_extracted_inside(output_dir: &Path) -> Result<()> {
    let output_dir = output_dir
        .canonicalize()
        .map_err(|e| AppError::Internal(format!("解析解压目录失败: {}", e)))?;
    let mut stack = vec![output_dir.clone()];
    let mut offenders = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let canonical = match path.canonicalize() {
                Ok(canonical) => canonical,
                Err(_) => {
                    offenders.push(path);
                    continue;
                }
            };
            if !canonical.starts_with(&output_dir) {
                offenders.push(path);
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            }
        }
    }
    if !offenders.is_empty() {
        let joined = offenders
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let _ = std::fs::remove_dir_all(&output_dir);
        return Err(AppError::Validation(format!(
            "升级包解压结果越界，已回滚: {}",
            joined
        )));
    }
    Ok(())
}

fn backup_path(current_exe: &Path) -> PathBuf {
    let file_name = current_exe
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("my-media-sub");
    current_exe.with_file_name(format!(
        "{}.bak-{}-{}",
        file_name,
        Utc::now().format("%Y%m%d%H%M%S"),
        uuid::Uuid::new_v4()
    ))
}

async fn replace_update_payload(
    new_binary: &Path,
    new_static_dir: &Path,
    current_exe: &Path,
    target_static_dir: &Path,
    backup_path: &Path,
) -> Result<()> {
    let new_binary = new_binary.to_path_buf();
    let new_static_dir = new_static_dir.to_path_buf();
    let current_exe = current_exe.to_path_buf();
    let target_static_dir = target_static_dir.to_path_buf();
    let backup_path = backup_path.to_path_buf();
    set_update_progress(90, "replacing", "正在提交二进制和 WebUI 静态资源升级事务");
    tokio::task::spawn_blocking(move || {
        replace_update_payload_blocking(
            &new_binary,
            &new_static_dir,
            &current_exe,
            &target_static_dir,
            &backup_path,
        )
    })
    .await
    .map_err(|e| AppError::Internal(format!("安装升级文件任务失败: {}", e)))?
}

fn replace_update_payload_blocking(
    new_binary: &Path,
    new_static_dir: &Path,
    current_exe: &Path,
    target_static_dir: &Path,
    backup_path: &Path,
) -> Result<()> {
    if !current_exe.is_file() {
        return Err(AppError::Internal(format!(
            "当前二进制不存在: {}",
            current_exe.display()
        )));
    }
    if !new_binary.is_file()
        || std::fs::symlink_metadata(new_binary)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(true)
    {
        return Err(AppError::Internal(
            "升级包中的 my-media-sub 不是普通文件".to_string(),
        ));
    }
    if !static_payload_is_complete(new_static_dir) {
        return Err(AppError::Internal(
            "升级包中的 static 目录不完整".to_string(),
        ));
    }

    let binary_parent = current_exe
        .parent()
        .ok_or_else(|| AppError::Internal("无法定位二进制所在目录".to_string()))?;
    let static_parent = target_static_dir
        .parent()
        .ok_or_else(|| AppError::Internal("无法定位静态资源所在目录".to_string()))?;
    std::fs::create_dir_all(static_parent)
        .map_err(|e| AppError::Internal(format!("创建静态资源父目录失败: {}", e)))?;

    let token = uuid::Uuid::new_v4();
    let binary_name = current_exe
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("my-media-sub");
    let static_name = target_static_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("static");
    let binary_stage = binary_parent.join(format!(".{binary_name}.new-{token}"));
    let static_stage = static_parent.join(format!(".{static_name}.new-{token}"));
    let static_backup = static_parent.join(format!(
        "{static_name}.bak-{}-{token}",
        Utc::now().format("%Y%m%d%H%M%S")
    ));

    let install_result = (|| -> Result<()> {
        std::fs::copy(current_exe, backup_path)
            .map_err(|e| AppError::Internal(format!("备份当前二进制失败: {}", e)))?;
        let current_metadata = std::fs::metadata(current_exe)
            .map_err(|e| AppError::Internal(format!("读取当前二进制权限失败: {}", e)))?;
        std::fs::copy(new_binary, &binary_stage)
            .map_err(|e| AppError::Internal(format!("暂存新二进制失败: {}", e)))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &binary_stage,
                std::fs::Permissions::from_mode(current_metadata.permissions().mode()),
            )
            .map_err(|e| AppError::Internal(format!("设置新二进制权限失败: {}", e)))?;
        }

        std::fs::File::open(&binary_stage)
            .and_then(|file| file.sync_all())
            .map_err(|e| AppError::Internal(format!("同步新二进制失败: {}", e)))?;
        copy_dir_all(new_static_dir, &static_stage)?;

        let had_static = target_static_dir.exists();
        if had_static {
            std::fs::rename(target_static_dir, &static_backup)
                .map_err(|e| AppError::Internal(format!("备份静态资源失败: {}", e)))?;
        }
        if let Err(error) = std::fs::rename(&static_stage, target_static_dir) {
            let rollback_error = if had_static {
                std::fs::rename(&static_backup, target_static_dir).err()
            } else {
                None
            };
            return Err(AppError::Internal(match rollback_error {
                Some(rollback_error) => {
                    format!("切换静态资源失败: {error}；恢复旧静态资源也失败: {rollback_error}")
                }
                None => format!("切换静态资源失败: {error}"),
            }));
        }

        if let Err(error) = std::fs::rename(&binary_stage, current_exe) {
            let _ = std::fs::remove_dir_all(target_static_dir);
            let rollback_error = if had_static {
                std::fs::rename(&static_backup, target_static_dir).err()
            } else {
                None
            };
            return Err(AppError::Internal(match rollback_error {
                Some(rollback_error) => {
                    format!("替换当前二进制失败: {error}；恢复旧静态资源也失败: {rollback_error}")
                }
                None => format!("替换当前二进制失败: {error}"),
            }));
        }

        if let Err(error) = sync_directory(binary_parent) {
            tracing::warn!("{}", error);
        }
        if static_parent != binary_parent {
            if let Err(error) = sync_directory(static_parent) {
                tracing::warn!("{}", error);
            }
        }
        Ok(())
    })();

    let _ = std::fs::remove_file(&binary_stage);
    let _ = std::fs::remove_dir_all(&static_stage);
    if install_result.is_err() {
        let _ = std::fs::remove_file(backup_path);
    }
    install_result
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(path)
            .and_then(|directory| directory.sync_all())
            .map_err(|e| AppError::Internal(format!("同步升级目录失败: {}", e)))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn find_binary(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = std::fs::read_dir(path).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == "my-media-sub")
                .unwrap_or(false)
            {
                return Some(path);
            }
        }
    }
    None
}

fn find_static_dir(root: &Path) -> Option<PathBuf> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let entries = std::fs::read_dir(path).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == "static")
                .unwrap_or(false)
                && static_payload_is_complete(&path)
            {
                return Some(path);
            }
            stack.push(path);
        }
    }
    None
}

fn static_payload_is_complete(path: &Path) -> bool {
    path.is_dir()
        && REQUIRED_STATIC_ASSETS
            .iter()
            .all(|asset| path.join(asset).is_file())
}

fn copy_dir_all(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)
        .map_err(|e| AppError::Internal(format!("创建静态资源目录失败: {}", e)))?;

    for entry in std::fs::read_dir(source)
        .map_err(|e| AppError::Internal(format!("读取静态资源目录失败: {}", e)))?
    {
        let entry = entry.map_err(|e| AppError::Internal(format!("读取静态资源项失败: {}", e)))?;
        let file_type = entry
            .file_type()
            .map_err(|e| AppError::Internal(format!("读取静态资源类型失败: {}", e)))?;
        if file_type.is_symlink() {
            return Err(AppError::Validation(
                "升级包中的 static 目录不能包含符号链接".to_string(),
            ));
        }
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&source_path, &target_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&source_path, &target_path)
                .map_err(|e| AppError::Internal(format!("复制静态资源失败: {}", e)))?;
        } else {
            return Err(AppError::Validation(
                "升级包中的 static 目录包含不支持的文件类型".to_string(),
            ));
        }
    }

    Ok(())
}

async fn record_runtime_version(current_exe: &Path, version: &str) {
    let Some(runtime_dir) = managed_runtime_dir() else {
        return;
    };
    if !path_is_within(current_exe, &runtime_dir) {
        return;
    }

    let marker = runtime_dir.join(".installed-version");
    let content = format!("{}\n", version);
    let result = tokio::task::spawn_blocking(move || {
        crate::utils::write_file_atomic(&marker, content.as_bytes(), 0o644)
    })
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("记录运行时版本失败: {}", error),
        Err(error) => tracing::warn!("记录运行时版本任务失败: {}", error),
    }
}

async fn prune_update_backups(current_exe: &Path, target_static_dir: &Path) {
    let retention = std::env::var("SELF_UPDATE_BACKUP_RETENTION")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(3)
        .clamp(1, 20);
    let current_exe = current_exe.to_path_buf();
    let target_static_dir = target_static_dir.to_path_buf();
    let result = tokio::task::spawn_blocking(move || {
        prune_sibling_backups(&current_exe, retention)?;
        prune_sibling_backups(&target_static_dir, retention)
    })
    .await;
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!("清理旧升级备份失败: {}", error),
        Err(error) => tracing::warn!("清理旧升级备份任务失败: {}", error),
    }
}

fn prune_sibling_backups(target: &Path, retention: usize) -> Result<()> {
    let Some(parent) = target.parent() else {
        return Ok(());
    };
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let prefix = format!("{name}.bak-");
    let mut backups = std::fs::read_dir(parent)
        .map_err(|e| AppError::Internal(format!("读取升级备份目录失败: {}", e)))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&prefix))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    backups.sort_by(|left, right| right.file_name().cmp(&left.file_name()));

    for backup in backups.into_iter().skip(retention) {
        let metadata = std::fs::symlink_metadata(&backup)
            .map_err(|e| AppError::Internal(format!("读取升级备份失败: {}", e)))?;
        if metadata.is_dir() {
            std::fs::remove_dir_all(&backup)
                .map_err(|e| AppError::Internal(format!("删除旧静态资源备份失败: {}", e)))?;
        } else {
            std::fs::remove_file(&backup)
                .map_err(|e| AppError::Internal(format!("删除旧二进制备份失败: {}", e)))?;
        }
    }
    Ok(())
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
    Router::new()
        .route("/api/update/check", get(check_update))
        .route("/api/update/releases", get(list_releases))
        .route("/api/update/progress", get(update_progress))
        .route("/api/update/apply", post(apply_update))
        .route("/api/update/restart", post(restart_update))
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

    #[test]
    fn test_parse_sha256_checksum_accepts_common_formats() {
        let checksum = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let asset_name = "archive.tar.gz";
        assert_eq!(
            parse_sha256_checksum(
                format!("{}  {}\n", checksum, asset_name).as_bytes(),
                asset_name
            ),
            Some(checksum.to_string())
        );
        assert_eq!(
            parse_sha256_checksum(
                format!("{} *{}\n", checksum.to_ascii_uppercase(), asset_name).as_bytes(),
                asset_name
            ),
            Some(checksum.to_string())
        );
        assert_eq!(
            parse_sha256_checksum(
                format!("SHA256 ({}) = {}\n", asset_name, checksum).as_bytes(),
                asset_name
            ),
            Some(checksum.to_string())
        );
        assert_eq!(
            parse_sha256_checksum(
                format!("{}\n", checksum.to_ascii_uppercase()).as_bytes(),
                asset_name
            ),
            Some(checksum.to_string())
        );
        assert_eq!(
            parse_sha256_checksum(
                format!("{}  other.tar.gz\n{}  another.tar.gz\n", checksum, checksum).as_bytes(),
                asset_name
            ),
            None
        );
        assert_eq!(parse_sha256_checksum(b"not-a-checksum", asset_name), None);
    }

    #[test]
    fn test_release_response_marks_current_and_newer() {
        let release = GithubRelease {
            tag_name: "v0.9.1".to_string(),
            name: None,
            html_url: "https://example.com/release".to_string(),
            body: None,
            published_at: None,
            assets: vec![asset("my-media-sub-v0.9.1-linux-x86_64.tar.gz")],
        };
        let current = release_to_response(release.clone(), "0.9.1");
        let newer = release_to_response(release, "0.9.0");

        assert!(current.is_current);
        assert!(!current.is_newer);
        assert!(!newer.is_current);
        assert!(newer.is_newer);
        assert!(newer.asset.is_some());
    }

    #[test]
    fn test_online_update_requires_managed_docker_runtime() {
        assert!(online_update_supported_for("binary", None, false));
        assert!(!online_update_supported_for("binary", Some(false), false));
        assert!(!online_update_supported_for("docker", None, true));
        assert!(!online_update_supported_for("docker", Some(false), true));
        assert!(!online_update_supported_for("docker", Some(true), false));
        assert!(online_update_supported_for("docker", Some(true), true));
        assert!(!online_update_supported_for("unknown", Some(true), true));
    }

    /// 同一进程内不得并发跑两次升级：第二次 apply 必须被拦在下载之前，
    /// 否则两个任务会同时改写同一个二进制和 static 目录。
    #[test]
    fn concurrent_update_attempts_are_rejected_and_progress_recovers() {
        // 这些断言操作进程级的 UPDATE_PROGRESS 单例，结束前必须复位。
        assert!(try_begin_update_progress("第一次升级").is_ok());
        let running = current_update_progress();
        assert!(running.running);
        assert_eq!(running.stage, "starting");

        let rejected = try_begin_update_progress("第二次升级").unwrap_err();
        assert!(matches!(rejected, AppError::Validation(_)));
        assert!(rejected.to_string().contains("已有升级任务正在执行"));

        // 失败后必须回到非 running，否则后续升级会被永久拒绝。
        fail_update_progress("模拟失败".to_string());
        let failed = current_update_progress();
        assert!(!failed.running);
        assert_eq!(failed.error.as_deref(), Some("模拟失败"));

        assert!(try_begin_update_progress("失败后重试").is_ok());
        finish_update_progress("已复位", "idle");
        assert!(!current_update_progress().running);
    }

    #[test]
    fn test_replace_update_payload_switches_binary_and_static_together() {
        let root = std::env::temp_dir().join(format!(
            "my-media-sub-update-payload-test-{}",
            uuid::Uuid::new_v4()
        ));
        let release = root.join("release");
        let runtime = root.join("runtime");
        let new_static = release.join("static");
        let target_static = runtime.join("static");
        std::fs::create_dir_all(&new_static).unwrap();
        std::fs::create_dir_all(&target_static).unwrap();
        std::fs::write(release.join("my-media-sub"), b"new-binary").unwrap();
        for asset in REQUIRED_STATIC_ASSETS {
            std::fs::write(new_static.join(asset), format!("new-{asset}")).unwrap();
        }
        std::fs::write(runtime.join("my-media-sub"), b"old-binary").unwrap();
        std::fs::write(target_static.join("index.html"), b"old-static").unwrap();
        let backup = runtime.join("my-media-sub.bak-test");

        replace_update_payload_blocking(
            &release.join("my-media-sub"),
            &new_static,
            &runtime.join("my-media-sub"),
            &target_static,
            &backup,
        )
        .unwrap();

        assert_eq!(
            std::fs::read(runtime.join("my-media-sub")).unwrap(),
            b"new-binary"
        );
        assert_eq!(
            std::fs::read(target_static.join("index.html")).unwrap(),
            b"new-index.html"
        );
        assert_eq!(std::fs::read(backup).unwrap(), b"old-binary");
        assert!(std::fs::read_dir(&runtime).unwrap().any(|entry| {
            entry
                .ok()
                .and_then(|entry| entry.file_name().into_string().ok())
                .is_some_and(|name| name.starts_with("static.bak-"))
        }));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn test_static_payload_requires_release_shell_assets() {
        let root = std::env::temp_dir().join(format!(
            "my-media-sub-update-static-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        for asset in REQUIRED_STATIC_ASSETS {
            std::fs::write(root.join(asset), asset).unwrap();
        }
        assert!(static_payload_is_complete(&root));

        std::fs::remove_file(root.join("openapi.json")).unwrap();
        assert!(!static_payload_is_complete(&root));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn test_prune_sibling_backups_keeps_latest_named_entries() {
        let root = std::env::temp_dir().join(format!(
            "my-media-sub-update-backup-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let target = root.join("my-media-sub");
        for suffix in ["20260101", "20260201", "20260301", "20260401"] {
            std::fs::write(root.join(format!("my-media-sub.bak-{suffix}")), suffix).unwrap();
        }

        prune_sibling_backups(&target, 2).unwrap();

        let mut remaining = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        remaining.sort();
        assert_eq!(
            remaining,
            vec![
                "my-media-sub.bak-20260301".to_string(),
                "my-media-sub.bak-20260401".to_string()
            ]
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn test_safe_member_path_rejects_escape_and_absolute() {
        assert!(is_safe_member_path("my-media-sub/my-media-sub"));
        assert!(is_safe_member_path("my-media-sub/static/js/app.js"));
        assert!(!is_safe_member_path("../escape"));
        assert!(!is_safe_member_path("a/../../b"));
        assert!(!is_safe_member_path("/absolute/path"));
        assert!(!is_safe_member_path(""));
        assert!(!is_safe_member_path("a/\0/b"));
        assert!(!is_safe_member_path("./dot"));
    }

    #[test]
    fn test_parse_tar_listing_line_extracts_name_and_link_target() {
        let (name, target) =
            parse_tar_listing_line("-rw-r--r-- user/group 123 2023-01-01 12:00 a/b.txt");
        assert_eq!(name, "a/b.txt");
        assert!(target.is_none());

        let (name, target) = parse_tar_listing_line(
            "lrwxrwxrwx user/group 0 2023-01-01 12:00 a/link -> /etc/passwd",
        );
        assert_eq!(name, "a/link");
        assert_eq!(target, Some("/etc/passwd"));
    }

    /// 集成测试：构造含绝对链接目标的符号链接成员归档，verify_archive_members 必须拒绝。
    #[test]
    fn test_verify_archive_members_rejects_absolute_symlink_target() {
        let root = std::env::temp_dir().join(format!(
            "my-media-sub-update-member-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let link_dir = root.join("link-src");
        std::fs::create_dir_all(&link_dir).unwrap();
        std::os::unix::fs::symlink("/etc/passwd", link_dir.join("abs_link")).unwrap();
        let archive = root.join("abs.tar.gz");
        let status = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(&link_dir)
            .arg(".")
            .status()
            .unwrap();
        assert!(status.success());
        assert!(
            verify_archive_members(&archive).is_err(),
            "含绝对链接目标的归档必须被拒绝"
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn test_verify_archive_members_accepts_normal_payload() {
        let root = std::env::temp_dir().join(format!(
            "my-media-sub-update-member-ok-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let payload = root.join("payload.tar.gz");
        let dir = root.join("payload");
        std::fs::create_dir_all(dir.join("static")).unwrap();
        std::fs::write(dir.join("my-media-sub"), b"bin").unwrap();
        std::fs::write(dir.join("static/index.html"), b"html").unwrap();

        let status = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&payload)
            .arg("-C")
            .arg(&root)
            .arg("payload")
            .status()
            .unwrap();
        assert!(status.success());
        assert!(verify_archive_members(&payload).is_ok());

        let _ = std::fs::remove_dir_all(root);
    }
}
