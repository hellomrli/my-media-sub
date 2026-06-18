use crate::error::{AppError, Result};
use crate::models::Subscription;
use std::path::PathBuf;
use tokio::sync::RwLock;

/// 订阅存储（JSON 文件，原子写入）
pub struct SubscriptionStore {
    path: PathBuf,
    items: RwLock<Vec<Subscription>>,
}

impl SubscriptionStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            items: RwLock::new(Vec::new()),
        }
    }

    /// 从文件加载
    pub async fn load(&self) -> Result<()> {
        let mut items = self.items.write().await;
        if !self.path.exists() {
            *items = Vec::new();
            return Ok(());
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| AppError::Database(format!("读取订阅文件失败: {}", e)))?;
        *items = serde_json::from_str(&content)
            .map_err(|e| AppError::Database(format!("解析订阅 JSON 失败: {}", e)))?;
        Ok(())
    }

    /// 原子保存到文件
    async fn save(&self, items: &[Subscription]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Database(format!("创建目录失败: {}", e)))?;
        }
        let content = serde_json::to_string_pretty(items)
            .map_err(|e| AppError::Database(format!("序列化订阅失败: {}", e)))?;
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, content)
            .map_err(|e| AppError::Database(format!("写入临时文件失败: {}", e)))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| AppError::Database(format!("重命名临时文件失败: {}", e)))?;
        Ok(())
    }

    /// 列出所有订阅
    pub async fn list(&self) -> Vec<Subscription> {
        self.items.read().await.clone()
    }

    /// 按 ID 获取
    pub async fn get(&self, id: &str) -> Option<Subscription> {
        self.items.read().await.iter().find(|s| s.id == id).cloned()
    }

    /// 创建订阅
    pub async fn create(&self, sub: Subscription) -> Result<Subscription> {
        let mut items = self.items.write().await;
        items.push(sub.clone());
        self.save(&items).await?;
        Ok(sub)
    }

    /// 更新订阅（通过闭包修改）
    pub async fn update<F>(&self, id: &str, updater: F) -> Result<Option<Subscription>>
    where
        F: FnOnce(&mut Subscription),
    {
        let mut items = self.items.write().await;
        let found = items.iter_mut().find(|s| s.id == id);
        match found {
            Some(sub) => {
                updater(sub);
                let updated = sub.clone();
                self.save(&items).await?;
                Ok(Some(updated))
            }
            None => Ok(None),
        }
    }

    /// 删除订阅，返回是否删除成功
    pub async fn delete(&self, id: &str) -> Result<bool> {
        let mut items = self.items.write().await;
        let before = items.len();
        items.retain(|s| s.id != id);
        let changed = items.len() != before;
        if changed {
            self.save(&items).await?;
        }
        Ok(changed)
    }

    /// 数量
    pub async fn count(&self) -> usize {
        self.items.read().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Subscription;

    fn make_sub(id: &str) -> Subscription {
        Subscription {
            id: id.to_string(),
            title: "测试".to_string(),
            source_title: String::new(),
            media_type: "series".to_string(),
            season: 1,
            start_episode_number: None,
            current_episode_number: 0,
            total_episode_number: None,
            source_group: String::new(),
            metadata: None,
            cloud_type: "quark".to_string(),
            url: "https://pan.quark.cn/s/test".to_string(),
            password: String::new(),
            known_files: vec![],
            known_file_keys: vec![],
            known_episodes: vec![],
            transferred_files: vec![],
            transferred_file_keys: vec![],
            last_probe: None,
            last_plan_summary: String::new(),
            notify_only: false,
            enabled: true,
            completed: false,
            rules: Default::default(),
            created_at: 1,
            updated_at: 1,
            last_checked_at: 1,
            last_new_files: vec![],
            last_new_episodes: vec![],
            last_check_summary: String::new(),
            check_history: vec![],
            status: "active".to_string(),
            invalid_since: None,
            last_error: String::new(),
            rule_summary: String::new(),
        }
    }

    #[tokio::test]
    async fn test_subscription_store_crud() {
        let tmp = std::env::temp_dir().join("test_subs_store.json");
        let _ = std::fs::remove_file(&tmp);
        let store = SubscriptionStore::new(&tmp);
        store.load().await.unwrap();

        assert_eq!(store.count().await, 0);

        // 创建
        store.create(make_sub("a1")).await.unwrap();
        assert_eq!(store.count().await, 1);

        // 获取
        let got = store.get("a1").await.unwrap();
        assert_eq!(got.id, "a1");

        // 更新
        let updated = store
            .update("a1", |s| s.current_episode_number = 5)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.current_episode_number, 5);

        // 更新不存在的
        let none = store.update("nope", |_| {}).await.unwrap();
        assert!(none.is_none());

        // 删除
        assert!(store.delete("a1").await.unwrap());
        assert_eq!(store.count().await, 0);
        assert!(!store.delete("a1").await.unwrap());

        // 验证持久化（重新加载）
        let store2 = SubscriptionStore::new(&tmp);
        store2.load().await.unwrap();
        assert_eq!(store2.count().await, 0);

        let _ = std::fs::remove_file(&tmp);
    }
}
