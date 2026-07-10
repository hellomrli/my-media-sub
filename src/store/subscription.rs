use crate::error::{AppError, Result};
use crate::models::Subscription;
use crate::store::schema::{
    backup_store_before_migration, decode_store_json, write_versioned_json_atomic_async, StoreKind,
    StoreSchemaError,
};
use crate::utils::{quarantine_corrupt_file, set_file_mode};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, RwLock};

/// 订阅存储（JSON 文件，原子写入）
pub struct SubscriptionStore {
    path: Option<PathBuf>,
    items: RwLock<Vec<Subscription>>,
    save_lock: Mutex<()>,
    save_count: AtomicU64,
}

impl SubscriptionStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
            items: RwLock::new(Vec::new()),
            save_lock: Mutex::new(()),
            save_count: AtomicU64::new(0),
        }
    }

    /// 创建仅驻留内存的快照 Store，用于批量计算后一次性提交。
    pub fn from_snapshot(items: Vec<Subscription>) -> Self {
        Self {
            path: None,
            items: RwLock::new(items),
            save_lock: Mutex::new(()),
            save_count: AtomicU64::new(0),
        }
    }

    /// 从文件加载
    pub async fn load(&self) -> Result<()> {
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };
        let mut items = self.items.write().await;
        if !path.exists() {
            *items = Vec::new();
            return Ok(());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| AppError::Database(format!("读取订阅文件失败: {}", e)))?;
        set_file_mode(path, 0o600)?;
        match decode_store_json::<Vec<Subscription>>(&content, StoreKind::Subscriptions) {
            Ok(decoded) => {
                backup_store_before_migration(path, &content, decoded.source_version)?;
                if decoded.needs_write {
                    write_versioned_json_atomic_async(path, &decoded.data, 0o600).await?;
                }
                *items = decoded.data;
            }
            Err(StoreSchemaError::UnsupportedVersion { found, current }) => {
                return Err(AppError::Database(format!(
                    "订阅存储 schema 版本 {} 高于当前支持版本 {}，请升级程序后重试",
                    found, current
                )));
            }
            Err(error) => {
                tracing::warn!("解析订阅 JSON 失败，已隔离损坏文件并使用空订阅: {}", error);
                quarantine_corrupt_file(path);
                *items = Vec::new();
            }
        }
        Ok(())
    }

    /// 原子保存到文件。内存快照 Store 不执行磁盘写入。
    async fn save(&self, items: &[Subscription]) -> Result<()> {
        let Some(path) = self.path.as_ref() else {
            return Ok(());
        };
        write_versioned_json_atomic_async(path, &items, 0o600).await?;
        self.save_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// 已成功执行的持久化次数。
    pub fn save_count(&self) -> u64 {
        self.save_count.load(Ordering::Relaxed)
    }

    /// 在独占保存锁内修改完整快照；磁盘写入成功后才替换内存。
    pub async fn mutate_snapshot<F, R>(&self, mutate: F) -> Result<R>
    where
        F: FnOnce(&mut Vec<Subscription>) -> Result<R>,
    {
        let _save_guard = self.save_lock.lock().await;
        let mut snapshot = self.items.read().await.clone();
        let result = mutate(&mut snapshot)?;
        self.save(&snapshot).await?;
        *self.items.write().await = snapshot;
        Ok(result)
    }

    /// 原子替换多个订阅；未包含的订阅保持当前值，整批只落盘一次。
    pub async fn update_many(&self, updates: Vec<Subscription>) -> Result<Vec<Subscription>> {
        if updates.is_empty() {
            return Ok(Vec::new());
        }
        self.mutate_snapshot(|snapshot| {
            let mut applied = Vec::with_capacity(updates.len());
            for update in updates {
                let current = snapshot
                    .iter_mut()
                    .find(|item| item.id == update.id)
                    .ok_or_else(|| AppError::NotFound(format!("订阅不存在: {}", update.id)))?;
                *current = update.clone();
                applied.push(update);
            }
            Ok(applied)
        })
        .await
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
    use crate::store::schema::migration_backup_path;

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
            manual_schedule: None,
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
            source_candidates: vec![],
            last_source_search_time: None,
            previous_share_links: vec![],
            source_failure_count: 0,
            last_source_switch_at: None,
            source_switch_history: vec![],
        }
    }

    #[cfg(unix)]
    fn assert_private_file_mode(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;

        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(not(unix))]
    fn assert_private_file_mode(_path: &std::path::Path) {}

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
    async fn load_migrates_legacy_subscription_array_to_envelope() {
        let tmp = temp_path("subs-legacy");
        let original = serde_json::to_vec(&vec![make_sub("legacy")]).unwrap();
        std::fs::write(&tmp, &original).unwrap();

        let store = SubscriptionStore::new(&tmp);
        store.load().await.unwrap();
        assert!(store.get("legacy").await.is_some());

        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&tmp).unwrap()).unwrap();
        assert_eq!(persisted["schema_version"], 1);
        assert_eq!(persisted["data"].as_array().unwrap().len(), 1);
        assert_private_file_mode(&tmp);

        let backup = migration_backup_path(&tmp, 0);
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        assert_private_file_mode(&backup);

        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(backup);
    }

    #[tokio::test]
    async fn future_subscription_schema_is_preserved() {
        let tmp = temp_path("subs-future");
        let original = serde_json::json!({"schema_version": 99, "data": []}).to_string();
        std::fs::write(&tmp, &original).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        let store = SubscriptionStore::new(&tmp);
        let error = store.load().await.unwrap_err();
        assert!(matches!(error, AppError::Database(_)));
        assert_eq!(std::fs::read_to_string(&tmp).unwrap(), original);
        assert_private_file_mode(&tmp);
        let _ = std::fs::remove_file(&tmp);
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

    #[tokio::test]
    async fn update_many_persists_once_and_updates_all_items_atomically() {
        let tmp = temp_path("subs-update-many");
        let store = SubscriptionStore::new(&tmp);
        store.load().await.unwrap();
        store.create(make_sub("a1")).await.unwrap();
        store.create(make_sub("a2")).await.unwrap();
        let before = store.save_count();

        let mut a1 = store.get("a1").await.unwrap();
        let mut a2 = store.get("a2").await.unwrap();
        a1.title = "A1 updated".to_string();
        a2.title = "A2 updated".to_string();
        store.update_many(vec![a1, a2]).await.unwrap();

        assert_eq!(store.save_count(), before + 1);
        assert_eq!(store.get("a1").await.unwrap().title, "A1 updated");
        assert_eq!(store.get("a2").await.unwrap().title, "A2 updated");
        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn mutate_snapshot_failure_does_not_change_memory_or_disk() {
        let tmp = temp_path("subs-mutate-fail");
        let store = SubscriptionStore::new(&tmp);
        store.load().await.unwrap();
        store.create(make_sub("a1")).await.unwrap();
        let before_bytes = std::fs::read(&tmp).unwrap();
        let before_saves = store.save_count();

        let error = store
            .mutate_snapshot(|items| {
                items[0].title = "should not persist".to_string();
                Err::<(), _>(AppError::Validation("abort".to_string()))
            })
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::Validation(_)));
        assert_eq!(store.get("a1").await.unwrap().title, "测试");
        assert_eq!(std::fs::read(&tmp).unwrap(), before_bytes);
        assert_eq!(store.save_count(), before_saves);
        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn concurrent_update_many_preserves_disjoint_updates() {
        let tmp = temp_path("subs-update-many-concurrent");
        let store = std::sync::Arc::new(SubscriptionStore::new(&tmp));
        store.load().await.unwrap();
        store.create(make_sub("a1")).await.unwrap();
        store.create(make_sub("a2")).await.unwrap();

        let mut a1 = store.get("a1").await.unwrap();
        let mut a2 = store.get("a2").await.unwrap();
        a1.title = "first".to_string();
        a2.title = "second".to_string();
        let left = {
            let store = store.clone();
            tokio::spawn(async move { store.update_many(vec![a1]).await })
        };
        let right = {
            let store = store.clone();
            tokio::spawn(async move { store.update_many(vec![a2]).await })
        };
        left.await.unwrap().unwrap();
        right.await.unwrap().unwrap();

        assert_eq!(store.get("a1").await.unwrap().title, "first");
        assert_eq!(store.get("a2").await.unwrap().title, "second");
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&tmp).unwrap()).unwrap();
        assert_eq!(persisted["data"].as_array().unwrap().len(), 2);
        let _ = std::fs::remove_file(tmp);
    }
}
