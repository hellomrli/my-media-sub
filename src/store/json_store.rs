use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::sync::RwLock;

/// JSON 文件存储
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

        let data: Vec<T> = serde_json::from_str(&content)
            .map_err(|e| AppError::Database(format!("Failed to parse JSON: {}", e)))?;

        let mut cache = self.cache.write().await;
        *cache = data;

        Ok(())
    }

    /// 保存数据到文件
    pub async fn save(&self) -> Result<()> {
        // 确保目录存在
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| AppError::Database(format!("Failed to create directory: {}", e)))?;
        }

        let cache = self.cache.read().await;
        let content = serde_json::to_string_pretty(&*cache)
            .map_err(|e| AppError::Database(format!("Failed to serialize JSON: {}", e)))?;

        fs::write(&self.path, content)
            .map_err(|e| AppError::Database(format!("Failed to write file: {}", e)))?;

        Ok(())
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
        drop(cache);
        self.save().await
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
        cache.iter().filter(|item| predicate(item)).cloned().collect()
    }

    /// 更新数据
    pub async fn update<F>(&self, predicate: F, updater: impl FnOnce(&mut T)) -> Result<bool>
    where
        F: Fn(&T) -> bool,
    {
        let mut cache = self.cache.write().await;
        
        if let Some(item) = cache.iter_mut().find(|item| predicate(item)) {
            updater(item);
            drop(cache);
            self.save().await?;
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
        drop(cache);
        
        if removed {
            self.save().await?;
        }
        
        Ok(removed)
    }

    /// 清空所有数据
    pub async fn clear(&self) -> Result<()> {
        let mut cache = self.cache.write().await;
        cache.clear();
        drop(cache);
        self.save().await
    }

    /// 获取数据数量
    pub async fn count(&self) -> usize {
        let cache = self.cache.read().await;
        cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestData {
        id: String,
        name: String,
    }

    #[tokio::test]
    async fn test_json_store() {
        let temp_file = std::env::temp_dir().join("test_store.json");
        let store = JsonStore::new(&temp_file);

        // 加载（空文件）
        store.load().await.unwrap();
        assert_eq!(store.count().await, 0);

        // 添加数据
        let item1 = TestData {
            id: "1".to_string(),
            name: "Test 1".to_string(),
        };
        store.add(item1.clone()).await.unwrap();
        assert_eq!(store.count().await, 1);

        // 查找数据
        let found = store.find(|item| item.id == "1").await;
        assert_eq!(found, Some(item1.clone()));

        // 更新数据
        store.update(
            |item| item.id == "1",
            |item| item.name = "Updated".to_string(),
        ).await.unwrap();

        let updated = store.find(|item| item.id == "1").await.unwrap();
        assert_eq!(updated.name, "Updated");

        // 删除数据
        store.remove(|item| item.id == "1").await.unwrap();
        assert_eq!(store.count().await, 0);

        // 清理
        let _ = fs::remove_file(&temp_file);
    }
}
