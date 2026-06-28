use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{AppError, Result};

pub mod metrics;

pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn write_file_atomic(path: &Path, content: &[u8], mode: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Database(format!("创建目录失败: {}", e)))?;
    }

    let tmp = unique_tmp_path(path);
    let write_result = (|| -> Result<()> {
        let mut file = open_tmp_file(&tmp, mode)?;
        file.write_all(content)
            .map_err(|e| AppError::Database(format!("写入临时文件失败: {}", e)))?;
        file.sync_all()
            .map_err(|e| AppError::Database(format!("同步临时文件失败: {}", e)))?;
        drop(file);

        fs::rename(&tmp, path)
            .map_err(|e| AppError::Database(format!("重命名临时文件失败: {}", e)))?;
        sync_parent_dir(path)?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&tmp);
    }

    write_result
}

/// 异步原子写入 JSON：序列化在当前线程完成（CPU 操作，开销小），
/// 阻塞的文件系统操作（写入 + fsync + rename）放到 `spawn_blocking`，
/// 避免阻塞 tokio executor 线程。调用方应在持有写锁时序列化以保证写入顺序，
/// 但实际落盘发生在阻塞线程池中。
pub async fn write_json_atomic_async<T: serde::Serialize>(
    path: &Path,
    value: &T,
    mode: u32,
) -> Result<()> {
    let content = serde_json::to_vec_pretty(value)
        .map_err(|e| AppError::Database(format!("序列化 JSON 失败: {}", e)))?;
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || write_file_atomic(&path, &content, mode))
        .await
        .map_err(|e| AppError::Database(format!("写盘任务执行失败: {}", e)))?
}

pub fn set_file_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .map_err(|e| AppError::Database(format!("设置文件权限失败: {}", e)))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

pub fn quarantine_corrupt_file(path: &Path) {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let quarantine = path.with_file_name(format!("{}.corrupt-{}", file_name, unix_now()));
    if let Err(error) = fs::rename(path, &quarantine) {
        tracing::warn!("隔离损坏文件 {} 失败: {}", path.display(), error);
    } else {
        tracing::warn!(
            "已将损坏文件 {} 移动到 {}",
            path.display(),
            quarantine.display()
        );
    }
}

pub fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let max_len = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();

    for index in 0..max_len {
        let l = left.get(index).copied().unwrap_or(0);
        let r = right.get(index).copied().unwrap_or(0);
        diff |= usize::from(l ^ r);
    }

    diff == 0
}

fn unique_tmp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("data");
    path.with_file_name(format!(
        ".{}.{}.{}.tmp",
        file_name,
        std::process::id(),
        uuid::Uuid::new_v4()
    ))
}

#[cfg(unix)]
fn open_tmp_file(path: &Path, mode: u32) -> Result<File> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)
        .map_err(|e| AppError::Database(format!("创建临时文件失败: {}", e)))
}

#[cfg(not(unix))]
fn open_tmp_file(path: &Path, _mode: u32) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| AppError::Database(format!("创建临时文件失败: {}", e)))
}

fn sync_parent_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        if let Some(parent) = path.parent() {
            File::open(parent)
                .and_then(|dir| dir.sync_all())
                .map_err(|e| AppError::Database(format!("同步目录失败: {}", e)))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("my-media-sub-{}-{}", name, uuid::Uuid::new_v4()))
    }

    #[test]
    fn constant_time_eq_matches_exact_bytes_only() {
        assert!(constant_time_eq("abcdef", "abcdef"));
        assert!(!constant_time_eq("abcdef", "abcdeg"));
        assert!(!constant_time_eq("abcdef", "abc"));
        assert!(!constant_time_eq("abc", "abcdef"));
    }

    #[test]
    fn write_file_atomic_overwrites_existing_file() {
        let dir = temp_path("atomic-write");
        let path = dir.join("nested").join("settings.json");

        write_file_atomic(&path, b"{\"version\":1}", 0o600).unwrap();
        write_file_atomic(&path, b"{\"version\":2}", 0o600).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"version\":2}");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let leftovers = fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);

        let _ = fs::remove_dir_all(dir);
    }
}
