#![allow(dead_code)]

use crate::error::{AppError, Result};
use crate::utils::{quarantine_corrupt_file, write_json_atomic_async};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::sync::{Mutex, RwLock};

/// JSON 文件存储（原子写入）
pub struct JsonStore<T> {
    /// 数据文件路径
    path: PathBuf,
    /// 内存缓存
    cache: RwLock<Vec<T>>,
    /// 串行化文件写入，避免并发写快照乱序落盘。
    save_lock: Mutex<()>,
}

impl<T> JsonStore<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Clone,
{
    /// 创建新的 JSON 存储
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            cache: RwLock::new(Vec::new()),
            save_lock: Mutex::new(()),
        }
    }

    /// 从文件加载数据
    pub async fn load(&self) -> Result<()> {
        if !self.path.exists() {
            // 文件不存在，初始化为空数组
            let mut cache = self.cache.write().await;
            *cache = Vec::new();
            return Ok(());
        }

        let content = fs::read_to_string(&self.path)
            .map_err(|e| AppError::Database(format!("Failed to read file: {}", e)))?;

        let mut cache = self.cache.write().await;
        match serde_json::from_str(&content) {
            Ok(data) => *cache = data,
            Err(e) => {
                tracing::warn!("Failed to parse JSON, quarantining corrupt file: {}", e);
                quarantine_corrupt_file(&self.path);
                *cache = Vec::new();
            }
        }

        Ok(())
    }

    /// 保存数据到文件（原子写入：先写 .tmp，再 replace）
    pub async fn save(&self) -> Result<()> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = self.cache.read().await.clone();
        self.write_snapshot(&snapshot).await
    }

    async fn write_snapshot(&self, cache: &[T]) -> Result<()> {
        write_json_atomic_async(&self.path, &cache, 0o600).await
    }

    /// 获取所有数据
    pub async fn all(&self) -> Vec<T> {
        let cache = self.cache.read().await;
        cache.clone()
    }

    /// 在读锁内访问所有数据，避免调用方为了统计等只读操作克隆整表。
    pub async fn with_all<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[T]) -> R,
    {
        let cache = self.cache.read().await;
        f(&cache)
    }

    /// 分页获取数据。
    pub async fn paginate(&self, offset: usize, limit: usize) -> Vec<T> {
        let cache = self.cache.read().await;
        cache.iter().skip(offset).take(limit).cloned().collect()
    }

    /// 添加数据
    pub async fn add(&self, item: T) -> Result<()> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = {
            let cache = self.cache.read().await;
            let mut snapshot = cache.clone();
            snapshot.push(item);
            snapshot
        };
        self.write_snapshot(&snapshot).await?;
        *self.cache.write().await = snapshot;
        Ok(())
    }

    /// 查找数据
    pub async fn find<F>(&self, predicate: F) -> Option<T>
    where
        F: Fn(&T) -> bool,
    {
        let cache = self.cache.read().await;
        cache.iter().find(|item| predicate(item)).cloned()
    }

    /// 过滤数据
    pub async fn filter<F>(&self, predicate: F) -> Vec<T>
    where
        F: Fn(&T) -> bool,
    {
        let cache = self.cache.read().await;
        cache
            .iter()
            .filter(|item| predicate(item))
            .cloned()
            .collect()
    }

    /// 更新数据
    pub async fn update<F>(&self, predicate: F, updater: impl FnOnce(&mut T)) -> Result<bool>
    where
        F: Fn(&T) -> bool,
    {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = {
            let cache = self.cache.read().await;
            let mut snapshot = cache.clone();
            if let Some(item) = snapshot.iter_mut().find(|item| predicate(item)) {
                updater(item);
                Some(snapshot)
            } else {
                None
            }
        };

        if let Some(snapshot) = snapshot {
            self.write_snapshot(&snapshot).await?;
            *self.cache.write().await = snapshot;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 删除数据
    pub async fn remove<F>(&self, predicate: F) -> Result<bool>
    where
        F: Fn(&T) -> bool,
    {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = {
            let cache = self.cache.read().await;
            let mut snapshot = cache.clone();
            let initial_len = snapshot.len();
            snapshot.retain(|item| !predicate(item));
            if snapshot.len() != initial_len {
                Some(snapshot)
            } else {
                None
            }
        };

        if let Some(snapshot) = snapshot {
            self.write_snapshot(&snapshot).await?;
            *self.cache.write().await = snapshot;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 清空所有数据
    pub async fn clear(&self) -> Result<()> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = Vec::new();
        self.write_snapshot(&snapshot).await?;
        *self.cache.write().await = snapshot;
        Ok(())
    }

    /// 获取数据数量
    pub async fn count(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }
}
