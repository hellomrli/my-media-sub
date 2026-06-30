use crate::error::{AppError, Result};
use crate::models::Subscription;
use crate::utils::{quarantine_corrupt_file, write_json_atomic_async};
use std::path::PathBuf;
use tokio::sync::{Mutex, RwLock};

/// 订阅存储（JSON 文件，原子写入）
pub struct SubscriptionStore {
    path: PathBuf,
    items: RwLock<Vec<Subscription>>,
    save_lock: Mutex<()>,
}

impl SubscriptionStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            items: RwLock::new(Vec::new()),
            save_lock: Mutex::new(()),
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
        match serde_json::from_str(&content) {
            Ok(parsed) => *items = parsed,
            Err(e) => {
                tracing::warn!("解析订阅 JSON 失败，已隔离损坏文件并使用空订阅: {}", e);
                quarantine_corrupt_file(&self.path);
                *items = Vec::new();
            }
        }
        Ok(())
    }

    /// 原子保存到文件
    async fn save(&self, items: &[Subscription]) -> Result<()> {
        write_json_atomic_async(&self.path, &items, 0o600).await
    }

    /// 列出所有订阅
    pub async fn list(&self) -> Vec<Subscription> {
        self.items.read().await.clone()
    }

    /// 分页列出订阅，保持与 list() 相同顺序。
    pub async fn list_paginated(&self, offset: usize, limit: usize) -> Vec<Subscription> {
        self.items
            .read()
            .await
            .iter()
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    /// 在读锁内访问订阅，适合统计等不需要克隆整表的场景。
    pub async fn with_items<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[Subscription]) -> R,
    {
        let items = self.items.read().await;
        f(&items)
    }

    /// 按 ID 获取
    pub async fn get(&self, id: &str) -> Option<Subscription> {
        self.items.read().await.iter().find(|s| s.id == id).cloned()
    }

    /// 创建订阅
    pub async fn create(&self, sub: Subscription) -> Result<Subscription> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = {
            let items = self.items.read().await;
            if items.iter().any(|s| s.id == sub.id) {
                return Err(AppError::Validation(format!(
                    "订阅已存在（相同链接和标题）: {}",
                    sub.title
                )));
            }
            let mut snapshot = items.clone();
            snapshot.push(sub.clone());
            snapshot
        };
        self.save(&snapshot).await?;
        *self.items.write().await = snapshot;
        Ok(sub)
    }

    /// 更新订阅（通过闭包修改）
    pub async fn update<F>(&self, id: &str, updater: F) -> Result<Option<Subscription>>
    where
        F: FnOnce(&mut Subscription),
    {
        let _save_guard = self.save_lock.lock().await;
        let updated = {
            let items = self.items.read().await;
            let mut snapshot = items.clone();
            if let Some(sub) = snapshot.iter_mut().find(|s| s.id == id) {
                updater(sub);
                let updated = sub.clone();
                Some((updated, snapshot))
            } else {
                None
            }
        };

        if let Some((updated, snapshot)) = updated {
            self.save(&snapshot).await?;
            *self.items.write().await = snapshot;
            Ok(Some(updated))
        } else {
            Ok(None)
        }
    }

    /// 删除订阅，返回是否删除成功
    pub async fn delete(&self, id: &str) -> Result<bool> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = {
            let items = self.items.read().await;
            let mut snapshot = items.clone();
            let before = snapshot.len();
            snapshot.retain(|s| s.id != id);
            if snapshot.len() != before {
                Some(snapshot)
            } else {
                None
            }
        };

        if let Some(snapshot) = snapshot {
            self.save(&snapshot).await?;
            *self.items.write().await = snapshot;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 数量
    pub async fn count(&self) -> usize {
        self.with_items(|items| items.len()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Subscription;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "my-media-sub-{}-{}.json",
            name,
            uuid::Uuid::new_v4()
        ))
    }

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
            sync_download_enabled: false,
            sync_download_dir: String::new(),
            strm_enabled: false,
            enabled: true,
            completed: false,
            rules: Default::default(),
            rule_preset_id: String::new(),
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
        let tmp = temp_path("subs-store");
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

    #[tokio::test]
    async fn create_rejects_duplicate_subscription_id() {
        let tmp = temp_path("subs-duplicate");
        let store = SubscriptionStore::new(&tmp);
        store.load().await.unwrap();

        store.create(make_sub("same")).await.unwrap();
        let error = store.create(make_sub("same")).await.unwrap_err();

        assert!(matches!(error, AppError::Validation(_)));
        assert_eq!(store.count().await, 1);

        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn create_keeps_cache_unchanged_when_save_fails() {
        let dir = std::env::temp_dir().join(format!(
            "my-media-sub-subs-save-fail-{}",
            uuid::Uuid::new_v4()
        ));
        let path = dir.join("subscriptions.json");
        let store = SubscriptionStore::new(&path);
        store.load().await.unwrap();
        store.create(make_sub("a1")).await.unwrap();

        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        std::fs::write(&dir, b"not a directory").unwrap();

        let error = store.create(make_sub("a2")).await.unwrap_err();

        assert!(matches!(error, AppError::Database(_)));
        assert_eq!(store.count().await, 1);
        assert!(store.get("a1").await.is_some());
        assert!(store.get("a2").await.is_none());

        let _ = std::fs::remove_file(dir);
    }

    #[tokio::test]
    async fn load_quarantines_corrupt_subscription_file() {
        let tmp = temp_path("subs-corrupt");
        std::fs::write(&tmp, b"{not-valid-json").unwrap();

        let store = SubscriptionStore::new(&tmp);
        store.load().await.unwrap();

        assert_eq!(store.count().await, 0);
        assert!(!tmp.exists());

        let parent = tmp.parent().unwrap();
        let file_name = tmp.file_name().unwrap().to_string_lossy();
        let quarantined = std::fs::read_dir(parent)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{}.corrupt-", file_name))
            });
        assert!(quarantined);

        for entry in std::fs::read_dir(parent)
            .unwrap()
            .filter_map(|entry| entry.ok())
        {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{}.corrupt-", file_name))
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}
