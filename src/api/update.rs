use axum::{
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use ring::digest;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

use super::response::ApiResponse as Response;
use crate::clients::http_pool;
use crate::error::{AppError, Result};
use crate::utils::constant_time_eq;

const GITHUB_REPO: &str = "hellomrli/my-media-sub";
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

#[derive(Debug, Clone)]
struct RestartPlan {
    executable: PathBuf,
    args: Vec<OsString>,
    current_dir: Option<PathBuf>,
}

async fn check_update() -> Result<impl IntoResponse> {
    let release = fetch_latest_release().await?;
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let latest_version = normalize_version(&release.tag_name);
    let update_available = is_newer_version(&latest_version, &current_version);
    let linux_x86_64_asset = find_asset(&release.assets, "linux-x86_64.tar.gz").map(Into::into);

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

    finish_update_progress("服务正在重启，请稍后刷新页面", "restarting");
    schedule_restart(plan);

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

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", size, UNITS[unit])
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
    let restart_plan = restart_plan(&current_exe);
    let backup_path = backup_path(&current_exe);
    let work_dir = std::env::temp_dir().join(format!(
        "my-media-sub-update-{}-{}",
        target_version,
        uuid::Uuid::new_v4()
    ));
    tokio::fs::create_dir_all(&work_dir)
        .await
        .map_err(|e| AppError::Internal(format!("创建升级临时目录失败: {}", e)))?;

    let archive_path = work_dir.join(&asset.name);
    set_update_progress(8, "checksum", "正在下载校验文件");
    let checksum_content = download_asset_bytes(&checksum_asset.browser_download_url).await?;
    download_asset(&asset.browser_download_url, &archive_path, asset.size).await?;
    set_update_progress(69, "checksum", "正在校验升级包 SHA256");
    verify_sha256(&archive_path, &asset.name, &checksum_content).await?;
    set_update_progress(70, "extracting", "正在解压升级包");
    extract_archive(&archive_path, &work_dir).await?;
    set_update_progress(82, "locating", "正在查找新版本二进制");
    let new_binary = find_binary(&work_dir)
        .ok_or_else(|| AppError::Internal("升级包中未找到 my-media-sub 二进制".to_string()))?;
    set_update_progress(86, "assets", "正在更新静态资源");
    if let Some(static_dir) = find_static_dir(&work_dir) {
        let target_static_dir = static_target_dir(&restart_plan, &current_exe);
        replace_static_dir(&static_dir, &target_static_dir).await?;
    }
    set_update_progress(90, "replacing", "正在备份并替换当前二进制");
    replace_binary(&new_binary, &current_exe, &backup_path).await?;

    set_update_progress(97, "cleanup", "正在清理升级临时文件");
    let _ = tokio::fs::remove_dir_all(&work_dir).await;
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

fn store_pending_restart(plan: RestartPlan) -> Result<()> {
    let mut pending = PENDING_RESTART
        .lock()
        .map_err(|_| AppError::Internal("保存重启计划失败".to_string()))?;
    *pending = Some(plan);
    Ok(())
}

fn restart_plan(executable: &Path) -> RestartPlan {
    RestartPlan {
        executable: executable.to_path_buf(),
        args: std::env::args_os().skip(1).collect(),
        current_dir: std::env::current_dir().ok(),
    }
}

fn schedule_restart(plan: RestartPlan) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        restart_process(plan);
    });
}

#[cfg(unix)]
fn restart_process(plan: RestartPlan) {
    use std::os::unix::process::CommandExt;

    tracing::info!("升级完成，正在自动重启服务");
    let mut command = Command::new(&plan.executable);
    command.args(&plan.args);
    if let Some(current_dir) = plan.current_dir {
        command.current_dir(current_dir);
    }
    let error = command.exec();
    tracing::error!("自动重启服务失败: {}", error);
}

#[cfg(not(unix))]
fn restart_process(plan: RestartPlan) {
    tracing::info!("升级完成，正在自动重启服务");
    let mut command = Command::new(&plan.executable);
    command.args(&plan.args);
    if let Some(current_dir) = plan.current_dir {
        command.current_dir(current_dir);
    }
    match command.spawn() {
        Ok(_) => std::process::exit(0),
        Err(error) => tracing::error!("自动重启服务失败: {}", error),
    }
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
        let output = std::process::Command::new("tar")
            .arg("-xzf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&output_dir)
            .output()
            .map_err(|e| AppError::Internal(format!("执行 tar 解压失败: {}", e)))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(AppError::Internal(format!(
                "解压升级包失败: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    })
    .await
    .map_err(|e| AppError::Internal(format!("解压任务失败: {}", e)))?
}

fn backup_path(current_exe: &Path) -> PathBuf {
    let file_name = current_exe
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("my-media-sub");
    current_exe.with_file_name(format!(
        "{}.bak-{}",
        file_name,
        Utc::now().format("%Y%m%d%H%M%S")
    ))
}

async fn replace_binary(new_binary: &Path, current_exe: &Path, backup_path: &Path) -> Result<()> {
    tokio::fs::copy(current_exe, backup_path)
        .await
        .map_err(|e| AppError::Internal(format!("备份当前二进制失败: {}", e)))?;
    let metadata = tokio::fs::metadata(current_exe)
        .await
        .map_err(|e| AppError::Internal(format!("读取当前二进制权限失败: {}", e)))?;
    let staging_path = current_exe.with_file_name(format!(
        ".{}.new-{}",
        current_exe
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("my-media-sub"),
        uuid::Uuid::new_v4()
    ));
    tokio::fs::copy(new_binary, &staging_path)
        .await
        .map_err(|e| AppError::Internal(format!("写入新二进制失败: {}", e)))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(
            &staging_path,
            std::fs::Permissions::from_mode(metadata.permissions().mode()),
        )
        .await
        .map_err(|e| AppError::Internal(format!("设置新二进制权限失败: {}", e)))?;
    }

    tokio::fs::rename(&staging_path, current_exe)
        .await
        .map_err(|e| AppError::Internal(format!("替换当前二进制失败: {}", e)))?;

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

fn static_target_dir(restart_plan: &RestartPlan, current_exe: &Path) -> PathBuf {
    restart_plan
        .current_dir
        .clone()
        .or_else(|| current_exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("static")
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
                && path.join("index.html").is_file()
            {
                return Some(path);
            }
            stack.push(path);
        }
    }
    None
}

async fn replace_static_dir(new_static_dir: &Path, target_static_dir: &Path) -> Result<()> {
    let new_static_dir = new_static_dir.to_path_buf();
    let target_static_dir = target_static_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        replace_static_dir_blocking(&new_static_dir, &target_static_dir)
    })
    .await
    .map_err(|e| AppError::Internal(format!("更新静态资源任务失败: {}", e)))?
}

fn replace_static_dir_blocking(new_static_dir: &Path, target_static_dir: &Path) -> Result<()> {
    if !new_static_dir.is_dir() {
        return Err(AppError::Internal("升级包中的 static 不是目录".to_string()));
    }

    let backup_dir = target_static_dir.with_file_name(format!(
        "{}.bak-{}",
        target_static_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("static"),
        Utc::now().format("%Y%m%d%H%M%S")
    ));

    if target_static_dir.exists() {
        std::fs::rename(target_static_dir, &backup_dir)
            .map_err(|e| AppError::Internal(format!("备份静态资源失败: {}", e)))?;
    }

    if let Err(error) = copy_dir_all(new_static_dir, target_static_dir) {
        let _ = std::fs::remove_dir_all(target_static_dir);
        if backup_dir.exists() {
            let _ = std::fs::rename(&backup_dir, target_static_dir);
        }
        return Err(error);
    }

    Ok(())
}

fn copy_dir_all(source: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)
        .map_err(|e| AppError::Internal(format!("创建静态资源目录失败: {}", e)))?;

    for entry in std::fs::read_dir(source)
        .map_err(|e| AppError::Internal(format!("读取静态资源目录失败: {}", e)))?
    {
        let entry = entry.map_err(|e| AppError::Internal(format!("读取静态资源项失败: {}", e)))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_all(&source_path, &target_path)?;
        } else {
            std::fs::copy(&source_path, &target_path)
                .map_err(|e| AppError::Internal(format!("复制静态资源失败: {}", e)))?;
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
}
