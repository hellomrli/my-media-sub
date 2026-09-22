use super::*;

pub(super) fn detect_runtime() -> String {
    if std::path::Path::new("/.dockerenv").exists() {
        "docker".to_string()
    } else {
        "binary".to_string()
    }
}

pub(super) fn online_update_supported(runtime: &str) -> bool {
    online_update_supported_for(
        runtime,
        optional_env_flag("SELF_UPDATE_ENABLED"),
        managed_docker_runtime_layout(),
    )
}

pub(super) fn online_update_supported_for(
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

pub(super) fn optional_env_flag(key: &str) -> Option<bool> {
    std::env::var(key)
        .ok()
        .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
}

pub(super) fn managed_runtime_dir() -> Option<PathBuf> {
    std::env::var("APP_RUNTIME_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub(super) fn managed_docker_runtime_layout() -> bool {
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

pub(super) fn path_is_within(path: &Path, directory: &Path) -> bool {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let directory = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    path.starts_with(directory)
}

pub(super) fn directory_is_writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // Replacing entries requires both write and search permission on the
    // containing directory. `access` evaluates the real uid/gid of the app.
    unsafe { libc::access(path.as_ptr(), libc::W_OK | libc::X_OK) == 0 }
}

#[cfg(not(unix))]
pub(super) fn directory_is_writable(_path: &Path) -> bool {
    true
}

pub(super) fn online_update_unavailable_message(runtime: &str) -> String {
    if runtime == "docker" {
        "当前 Docker 容器未启用可写的持久化运行目录，不能在线替换程序；请升级到新版 Compose 配置，或在宿主机执行：docker compose pull && docker compose up -d"
            .to_string()
    } else {
        "当前运行环境不支持在线替换程序，请手工升级二进制和完整 static 目录".to_string()
    }
}
