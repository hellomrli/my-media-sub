use super::*;

/// 升级进度复位 guard：正常路径（成功/失败）已显式结束进度；只有 panic
/// 展开经过 drop 时 running 仍为 true，此处负责复位互斥状态。
pub(super) struct UpdateProgressResetGuard;

impl Drop for UpdateProgressResetGuard {
    fn drop(&mut self) {
        if let Ok(mut progress) = UPDATE_PROGRESS.lock() {
            if progress.running {
                progress.running = false;
                progress.error = Some("升级任务异常中止，请重试".to_string());
                progress.updated_at = Utc::now().to_rfc3339();
            }
        }
    }
}

pub(super) fn current_update_progress() -> UpdateProgressResponse {
    UPDATE_PROGRESS
        .lock()
        .map(|progress| progress.clone())
        .unwrap_or_else(|_| UpdateProgressResponse::idle())
}

pub(super) fn try_begin_update_progress(message: impl Into<String>) -> Result<()> {
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

pub(super) fn set_update_progress(percent: u8, stage: &str, message: impl Into<String>) {
    if let Ok(mut progress) = UPDATE_PROGRESS.lock() {
        progress.running = true;
        progress.percent = percent.min(100);
        progress.stage = stage.to_string();
        progress.message = message.into();
        progress.error = None;
        progress.updated_at = Utc::now().to_rfc3339();
    }
}

pub(super) fn set_download_progress(downloaded_bytes: u64, total_bytes: Option<u64>) {
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

pub(super) fn finish_update_progress(message: impl Into<String>, stage: &str) {
    if let Ok(mut progress) = UPDATE_PROGRESS.lock() {
        progress.running = false;
        progress.percent = 100;
        progress.stage = stage.to_string();
        progress.message = message.into();
        progress.error = None;
        progress.updated_at = Utc::now().to_rfc3339();
    }
}

pub(super) fn fail_update_progress(message: impl Into<String>) {
    let message = message.into();
    if let Ok(mut progress) = UPDATE_PROGRESS.lock() {
        progress.running = false;
        progress.stage = "failed".to_string();
        progress.message = message.clone();
        progress.error = Some(message);
        progress.updated_at = Utc::now().to_rfc3339();
    }
}

pub(super) fn store_pending_restart(plan: RestartPlan) -> Result<()> {
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

pub(super) fn ensure_no_pending_restart() -> Result<()> {
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
