#![allow(dead_code)]

use crate::error::{AppError, Result};
use crate::utils::{quarantine_corrupt_file, write_json_atomic_async};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

/// JSON 文件存储（原子写入）
pub struct JsonStore<T> {
    /// 数据文件路径
    path: PathBuf,
    /// 内存缓存
    cache: RwLock<Vec<T>>,
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
        let cache = self.cache.write().await;
        self.save_locked(&cache).await
    }

    async fn save_locked(&self, cache: &[T]) -> Result<()> {
        write_json_atomic_async(&self.path, &cache, 0o600).await
    }

    /// 获取所有数据
    pub async fn all(&self) -> Vec<T> {
        let cache = self.cache.read().await;
        cache.clone()
    }

    /// 添加数据
    pub async fn add(&self, item: T) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.push(item);
        self.save_locked(&cache).await
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
        let mut cache = self.cache.write().await;

        if let Some(item) = cache.iter_mut().find(|item| predicate(item)) {
            updater(item);
            self.save_locked(&cache).await?;
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
        let mut cache = self.cache.write().await;
        let initial_len = cache.len();
        cache.retain(|item| !predicate(item));
        let removed = cache.len() != initial_len;

        if removed {
            self.save_locked(&cache).await?;
        }

        Ok(removed)
    }

    /// 清空所有数据
    pub async fn clear(&self) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.clear();
        self.save_locked(&cache).await
    }

    /// 获取数据数量
    pub async fn count(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }
}
