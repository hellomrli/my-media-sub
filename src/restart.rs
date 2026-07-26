use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{LazyLock, Mutex};

use tokio::sync::Notify;

/// A process restart requested by the online updater.
///
/// The plan is executed by `main` only after Axum has stopped accepting new
/// requests and the persistent job queue has completed its graceful shutdown.
#[derive(Debug, Clone)]
pub struct RestartPlan {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub current_dir: Option<PathBuf>,
}

impl RestartPlan {
    pub fn for_executable(executable: &Path) -> Self {
        Self {
            executable: executable.to_path_buf(),
            args: std::env::args_os().skip(1).collect(),
            current_dir: std::env::current_dir().ok(),
        }
    }
}

static REQUESTED_RESTART: LazyLock<Mutex<Option<RestartPlan>>> = LazyLock::new(|| Mutex::new(None));
static RESTART_NOTIFY: LazyLock<Notify> = LazyLock::new(Notify::new);

pub fn request(plan: RestartPlan) -> std::result::Result<(), String> {
    let mut requested = REQUESTED_RESTART
        .lock()
        .map_err(|_| "保存重启请求失败".to_string())?;
    if requested.is_some() {
        return Err("服务重启已在进行中".to_string());
    }
    *requested = Some(plan);
    drop(requested);
    RESTART_NOTIFY.notify_waiters();
    Ok(())
}

pub async fn wait_for_request() {
    loop {
        let notified = RESTART_NOTIFY.notified();
        if REQUESTED_RESTART
            .lock()
            .map(|requested| requested.is_some())
            .unwrap_or(false)
        {
            return;
        }
        notified.await;
    }
}

pub fn take_request() -> Option<RestartPlan> {
    REQUESTED_RESTART
        .lock()
        .ok()
        .and_then(|mut requested| requested.take())
}

#[cfg(unix)]
pub fn execute(plan: RestartPlan) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;

    let mut command = Command::new(&plan.executable);
    command.args(&plan.args);
    if let Some(current_dir) = plan.current_dir {
        command.current_dir(current_dir);
    }
    Err(command.exec())
}

#[cfg(not(unix))]
pub fn execute(plan: RestartPlan) -> std::io::Result<()> {
    let mut command = Command::new(&plan.executable);
    command.args(&plan.args);
    if let Some(current_dir) = plan.current_dir {
        command.current_dir(current_dir);
    }
    command.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn restart_request_is_observable_and_taken_once() {
        let _ = take_request();
        let plan = RestartPlan {
            executable: PathBuf::from("/tmp/my-media-sub-test"),
            args: vec![OsString::from("--test")],
            current_dir: Some(PathBuf::from("/tmp")),
        };

        request(plan.clone()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), wait_for_request())
            .await
            .unwrap();
        let requested = take_request().unwrap();
        assert_eq!(requested.executable, plan.executable);
        assert_eq!(requested.args, plan.args);
        assert!(take_request().is_none());
    }
}
