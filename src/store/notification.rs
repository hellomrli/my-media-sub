use crate::error::{AppError, Result};
use crate::models::Notification;
use crate::store::schema::{
    backup_store_before_migration, decode_store_json, write_versioned_json_atomic_async, StoreKind,
    StoreSchemaError,
};
use crate::utils::{quarantine_corrupt_file, set_file_mode};
use std::path::PathBuf;
use tokio::sync::{Mutex, RwLock};

/// 通知存储（JSON 文件，保留最近 300 条，原子写入）
pub struct NotificationStore {
    path: PathBuf,
    items: RwLock<Vec<Notification>>,
    save_lock: Mutex<()>,
}

const MAX_NOTIFICATIONS: usize = 300;

impl NotificationStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            items: RwLock::new(Vec::new()),
            save_lock: Mutex::new(()),
        }
    }

    pub async fn load(&self) -> Result<()> {
        let mut items = self.items.write().await;
        if !self.path.exists() {
            *items = Vec::new();
            return Ok(());
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| AppError::Database(format!("读取通知文件失败: {}", e)))?;
        set_file_mode(&self.path, 0o600)?;
        match decode_store_json::<Vec<Notification>>(&content, StoreKind::Notifications) {
            Ok(decoded) => {
                backup_store_before_migration(&self.path, &content, decoded.source_version)?;
                let mut parsed = decoded.data;
                let original_len = parsed.len();
                truncate_notifications(&mut parsed);
                if decoded.needs_write || parsed.len() != original_len {
                    write_versioned_json_atomic_async(&self.path, &parsed, 0o600).await?;
                }
                *items = parsed;
            }
            Err(StoreSchemaError::UnsupportedVersion { found, current }) => {
                return Err(AppError::Database(format!(
                    "通知存储 schema 版本 {} 高于当前支持版本 {}，请升级程序后重试",
                    found, current
                )));
            }
            Err(error) => {
                tracing::warn!("解析通知 JSON 失败，已隔离损坏文件并使用空通知: {}", error);
                quarantine_corrupt_file(&self.path);
                *items = Vec::new();
            }
        }
        Ok(())
    }

    async fn save(&self, items: &[Notification]) -> Result<()> {
        write_versioned_json_atomic_async(&self.path, &items, 0o600).await
    }

    /// 添加通知
    pub async fn add(&self, notif: Notification) -> Result<Notification> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = {
            let items = self.items.read().await;
            let mut snapshot = items.clone();
            snapshot.push(notif.clone());
            truncate_notifications(&mut snapshot);
            snapshot
        };
        self.save(&snapshot).await?;
        *self.items.write().await = snapshot;
        Ok(notif)
    }

    pub async fn update<F>(&self, id: &str, updater: F) -> Result<Option<Notification>>
    where
        F: FnOnce(&mut Notification),
    {
        let _save_guard = self.save_lock.lock().await;
        let updated = {
            let items = self.items.read().await;
            let mut snapshot = items.clone();
            if let Some(item) = snapshot.iter_mut().find(|item| item.id == id) {
                updater(item);
                let updated = item.clone();
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

    /// 列出通知（倒序，最新在前）
    pub async fn list(&self, include_read: bool) -> Vec<Notification> {
        let items = self.items.read().await;
        items
            .iter()
            .rev()
            .filter(|n| include_read || !n.read)
            .cloned()
            .collect()
    }

    /// 标记已读（None 表示全部）
    pub async fn mark_read(&self, id: Option<&str>) -> Result<()> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = {
            let items = self.items.read().await;
            let mut snapshot = items.clone();
            for item in snapshot.iter_mut() {
                if id.is_none() || id == Some(item.id.as_str()) {
                    item.read = true;
                }
            }
            snapshot
        };
        self.save(&snapshot).await?;
        *self.items.write().await = snapshot;
        Ok(())
    }

    pub async fn compact(&self) -> Result<usize> {
        let _guard = self.save_lock.lock().await;
        let mut snapshot = self.items.read().await.clone();
        let before = snapshot.len();
        truncate_notifications(&mut snapshot);
        self.save(&snapshot).await?;
        *self.items.write().await = snapshot;
        Ok(before.saturating_sub(self.items.read().await.len()))
    }

    /// 清空所有通知
    pub async fn clear(&self) -> Result<()> {
        let _save_guard = self.save_lock.lock().await;
        let snapshot = Vec::new();
        self.save(&snapshot).await?;
        *self.items.write().await = snapshot;
        Ok(())
    }
}

fn truncate_notifications(items: &mut Vec<Notification>) {
    if items.len() > MAX_NOTIFICATIONS {
        let remove_count = items.len() - MAX_NOTIFICATIONS;
        items.drain(0..remove_count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::schema::migration_backup_path;
    use std::collections::HashMap;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "my-media-sub-{}-{}.json",
            name,
            uuid::Uuid::new_v4()
        ))
    }

    fn make_notif(id: &str, read: bool) -> Notification {
        Notification {
            id: id.to_string(),
            level: "info".to_string(),
            event: "test".to_string(),
            title: "标题".to_string(),
            message: "消息".to_string(),
            meta: HashMap::new(),
            read,
            created_at: 1,
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

    fn quarantine_path(path: &std::path::Path) -> Option<PathBuf> {
        let file_name = path.file_name()?.to_string_lossy();
        std::fs::read_dir(path.parent()?)
            .ok()?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .find(|candidate| {
                candidate.file_name().is_some_and(|name| {
                    name.to_string_lossy()
                        .starts_with(&format!("{}.corrupt-", file_name))
                })
            })
    }

    #[tokio::test]
    async fn test_notification_store() {
        let tmp = temp_path("notif-store");
        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        // 添加
        store.add(make_notif("n1", false)).await.unwrap();
        store.add(make_notif("n2", false)).await.unwrap();

        // 列出（倒序）
        let all = store.list(true).await;
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "n2"); // 最新在前

        // 只看未读
        let unread = store.list(false).await;
        assert_eq!(unread.len(), 2);

        // 标记单个已读
        store.mark_read(Some("n1")).await.unwrap();
        let unread = store.list(false).await;
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].id, "n2");

        // 全部已读
        store.mark_read(None).await.unwrap();
        assert_eq!(store.list(false).await.len(), 0);

        // 清空
        store.clear().await.unwrap();
        assert_eq!(store.list(true).await.len(), 0);

        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn add_truncates_to_recent_notifications() {
        let tmp = temp_path("notif-truncate-add");
        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        for index in 0..(MAX_NOTIFICATIONS + 5) {
            store
                .add(make_notif(&format!("n{}", index), false))
                .await
                .unwrap();
        }

        let all = store.list(true).await;
        assert_eq!(all.len(), MAX_NOTIFICATIONS);
        assert_eq!(
            all.first().unwrap().id,
            format!("n{}", MAX_NOTIFICATIONS + 4)
        );
        assert_eq!(all.last().unwrap().id, "n5");

        let persisted = std::fs::read_to_string(&tmp).unwrap();
        let decoded =
            decode_store_json::<Vec<Notification>>(&persisted, StoreKind::Notifications).unwrap();
        assert!(!decoded.needs_write);
        assert_eq!(decoded.data.len(), MAX_NOTIFICATIONS);
        assert_eq!(decoded.data.first().unwrap().id, "n5");

        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn load_truncates_existing_notification_file() {
        let tmp = temp_path("notif-truncate-load");
        let items = (0..(MAX_NOTIFICATIONS + 2))
            .map(|index| make_notif(&format!("n{}", index), false))
            .collect::<Vec<_>>();
        std::fs::write(&tmp, serde_json::to_vec(&items).unwrap()).unwrap();

        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        let all = store.list(true).await;
        assert_eq!(all.len(), MAX_NOTIFICATIONS);
        assert_eq!(
            all.first().unwrap().id,
            format!("n{}", MAX_NOTIFICATIONS + 1)
        );
        assert_eq!(all.last().unwrap().id, "n2");

        let _ = std::fs::remove_file(migration_backup_path(&tmp, 0));
        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn load_migrates_legacy_notifications_to_envelope() {
        let tmp = temp_path("notif-legacy");
        let original = serde_json::to_vec_pretty(&vec![make_notif("legacy", false)]).unwrap();
        std::fs::write(&tmp, &original).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        assert_eq!(store.list(true).await[0].id, "legacy");
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&tmp).unwrap()).unwrap();
        assert_eq!(persisted["schema_version"], 1);
        assert_eq!(persisted["data"][0]["id"], "legacy");
        assert_private_file_mode(&tmp);

        let backup = migration_backup_path(&tmp, 0);
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        assert_private_file_mode(&backup);

        let _ = std::fs::remove_file(tmp);
        let _ = std::fs::remove_file(backup);
    }

    #[tokio::test]
    async fn future_notification_schema_is_preserved() {
        let tmp = temp_path("notif-future");
        let original = serde_json::json!({"schema_version": 99, "data": []}).to_string();
        std::fs::write(&tmp, &original).unwrap();

        let store = NotificationStore::new(&tmp);
        let error = store.load().await.unwrap_err();

        assert!(matches!(error, AppError::Database(_)));
        assert_eq!(std::fs::read_to_string(&tmp).unwrap(), original);
        assert!(quarantine_path(&tmp).is_none());
        assert_private_file_mode(&tmp);

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn load_quarantines_corrupt_notification_file() {
        let tmp = temp_path("notif-corrupt");
        std::fs::write(&tmp, b"{not-valid-json").unwrap();

        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();

        assert!(store.list(true).await.is_empty());
        assert!(!tmp.exists());
        let quarantined =
            quarantine_path(&tmp).expect("corrupt notification file was not quarantined");
        let _ = std::fs::remove_file(quarantined);
    }

    #[tokio::test]
    async fn add_keeps_notifications_unchanged_when_save_fails() {
        let blocker = temp_path("notif-save-blocker");
        std::fs::write(&blocker, b"not-a-directory").unwrap();
        let path = blocker.join("notifications.json");
        let store = NotificationStore::new(&path);
        store.load().await.unwrap();

        let result = store.add(make_notif("should-not-stick", false)).await;

        assert!(matches!(result, Err(AppError::Database(_))));
        assert!(store.list(true).await.is_empty());

        let _ = std::fs::remove_file(blocker);
    }

    #[tokio::test]
    async fn update_modifies_existing_notification() {
        let tmp = temp_path("notif-update");
        let store = NotificationStore::new(&tmp);
        store.load().await.unwrap();
        store.add(make_notif("n1", false)).await.unwrap();

        let updated = store
            .update("n1", |notification| {
                notification.read = true;
                notification
                    .meta
                    .insert("push".to_string(), serde_json::json!({"success_count": 1}));
            })
            .await
            .unwrap()
            .unwrap();

        assert!(updated.read);
        assert_eq!(updated.meta["push"]["success_count"], serde_json::json!(1));
        assert!(store.update("missing", |_| {}).await.unwrap().is_none());

        let _ = std::fs::remove_file(&tmp);
    }
}
