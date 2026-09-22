//! 单实例保护：对 `DATA_DIR` 加排他 advisory lock。
//!
//! 为什么必须要有
//! ==============
//! 业务状态是"整份 JSON 读进内存 → 改 → 整份写回"的模型，写盘虽然原子，
//! 但**没有跨进程协调**：两个进程共用同一个 `DATA_DIR` 时各持一份内存快照，
//! 后写的那个会整份覆盖前者的写入（last writer wins），并且同一个 `Queued`
//! 作业可以被两边同时领取执行——表现为重复转存、重复推送、订阅进度回退。
//!
//! `docker-compose.yml` 的 `container_name` 只能挡住同名的 compose 服务，
//! 挡不住 `docker run`、宿主机上另跑一个二进制、或误加的 `--scale`。
//!
//! 实现要点
//! ========
//! - 用 `flock(LOCK_EX | LOCK_NB)`：advisory 锁，进程退出（fd 关闭）时由内核
//!   自动释放，因此崩溃不会留下死锁；`LOCK_NB` 保证第二个实例立刻失败而不是挂住。
//! - 锁文件权限 0600，内容是持有者 pid，便于排查"谁占着"。
//! - **注意**：`flock` 在 NFS 上的语义依赖服务端实现（多数现代 NFS 支持，
//!   但不如本地文件系统可靠）。文档已提示 `DATA_DIR` 应放在本地卷或块存储上。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{AppError, Result};

/// 锁文件相对 `DATA_DIR` 的名字。
pub const LOCK_FILE_NAME: &str = ".lock";

/// 持有期间保持打开的文件描述符；drop 时释放锁。
///
/// `_file` 从不被读取——**持有它**就是全部意义所在：`flock` 绑定在打开文件
/// 描述上，fd 关闭（含进程退出、fd 泄漏）时由内核自动释放。
#[derive(Debug)]
pub struct InstanceLock {
    _file: std::fs::File,
    path: PathBuf,
}

impl InstanceLock {
    /// 尝试独占 `data_dir`。已有实例持有时返回可读的错误。
    #[cfg(unix)]
    pub fn acquire(data_dir: &Path) -> Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        use std::os::unix::io::AsRawFd;

        std::fs::create_dir_all(data_dir).map_err(|error| {
            AppError::Database(format!("创建数据目录 {} 失败: {error}", data_dir.display()))
        })?;
        let path = data_dir.join(LOCK_FILE_NAME);
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .map_err(|error| {
                AppError::Database(format!("打开实例锁 {} 失败: {error}", path.display()))
            })?;

        // SAFETY: `file` 持有有效的文件描述符，且在整个调用期间存活。
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            let error = std::io::Error::last_os_error();
            return match error.raw_os_error() {
                Some(libc::EWOULDBLOCK) => Err(AppError::Validation(format!(
                    "数据目录 {} 已被另一个 my-media-sub 实例占用。\
                     业务状态是整份 JSON 覆盖写，两个实例共用同一 DATA_DIR 会互相覆盖数据\
                     并可能重复执行同一个作业。请先停止另一个实例；\
                     若确认没有其它实例，可删除 {} 后重试。",
                    data_dir.display(),
                    path.display()
                ))),
                _ => Err(AppError::Database(format!(
                    "锁定数据目录 {} 失败: {error}",
                    data_dir.display()
                ))),
            };
        }

        // 写入持有者 pid，便于排查。写失败不影响锁语义，只记日志。
        let _ = file.set_len(0);
        if let Err(error) = writeln!(file, "{}", std::process::id()) {
            tracing::warn!("写入实例锁 pid 失败（不影响锁定）: {}", error);
        }
        let _ = file.flush();

        Ok(Self { _file: file, path })
    }

    /// 非 Unix 平台没有 flock；退化为只写 pid 的标记文件（不提供互斥）。
    #[cfg(not(unix))]
    pub fn acquire(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir).map_err(|error| {
            AppError::Database(format!("创建数据目录 {} 失败: {error}", data_dir.display()))
        })?;
        let path = data_dir.join(LOCK_FILE_NAME);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)
            .map_err(|error| {
                AppError::Database(format!("打开实例锁 {} 失败: {error}", path.display()))
            })?;
        let _ = writeln!(file, "{}", std::process::id());
        tracing::warn!("当前平台不支持 flock，未启用单实例互斥保护");
        Ok(Self { file, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 显式释放（正常退出路径）。drop 也会释放，这里只是让意图更清晰。
    pub fn release(self) {
        drop(self);
    }

    /// 真实探测锁是否生效：另开一个 fd 再尝试 `flock`。
    ///
    /// `flock` 绑定在**打开文件描述**而非进程上，因此同一个进程里的第二次
    /// 独立 open + LOCK_EX 也会失败——这让我们能真正验证互斥，而不是只断言
    /// 「拿锁没报错」。
    #[cfg(all(unix, test))]
    pub(crate) fn is_actually_held(&self) -> bool {
        use std::os::unix::io::AsRawFd;
        let Ok(probe) = std::fs::File::open(&self.path) else {
            return false;
        };
        // SAFETY: probe 持有有效 fd，且在调用期间存活。
        unsafe { libc::flock(probe.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) != 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-lock-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[cfg(unix)]
    #[test]
    fn second_instance_on_same_data_dir_is_rejected() {
        let dir = temp_dir("exclusive");
        let first = InstanceLock::acquire(&dir).expect("第一个实例应当拿到锁");
        assert!(
            first.is_actually_held(),
            "acquire 成功之后锁必须真的处于持有状态"
        );

        let second = InstanceLock::acquire(&dir);
        let error = second.expect_err("第二个实例必须被拒绝");
        let message = error.to_string();
        assert!(
            message.contains("已被另一个") && message.contains("DATA_DIR"),
            "错误信息需要给出可操作的说明，实际为: {message}"
        );

        // 释放后应当可以重新获取（模拟重启）
        first.release();
        InstanceLock::acquire(&dir).expect("锁释放后应当可以重新获取");
    }

    #[cfg(unix)]
    #[test]
    fn different_data_dirs_do_not_conflict() {
        let dir_a = temp_dir("a");
        let dir_b = temp_dir("b");
        let _a = InstanceLock::acquire(&dir_a).unwrap();
        let _b = InstanceLock::acquire(&dir_b).expect("不同数据目录不应互相阻塞");
        assert!(_b.is_actually_held());
    }

    #[cfg(unix)]
    #[test]
    fn lock_file_records_owner_pid_with_restrictive_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir("pid");
        let lock = InstanceLock::acquire(&dir).unwrap();
        let content = std::fs::read_to_string(lock.path()).unwrap();
        assert_eq!(content.trim(), std::process::id().to_string());

        let mode = std::fs::metadata(lock.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "锁文件不应被其他用户读取");
    }
}
