use crate::error::{AppError, Result};
use crate::models::Notification;
use crate::utils::{quarantine_corrupt_file, write_json_atomic_async};
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
        match serde_json::from_str(&content) {
            Ok(mut parsed) => {
                truncate_notifications(&mut parsed);
                *items = parsed;
            }
            Err(e) => {
                tracing::warn!("解析通知 JSON 失败，已隔离损坏文件并使用空通知: {}", e);
                quarantine_corrupt_file(&self.path);
                *items = Vec::new();
            }
        }
        Ok(())
    }

    async fn save(&self, items: &[Notification]) -> Result<()> {
        write_json_atomic_async(&self.path, &items, 0o600).await
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
        let persisted_items: Vec<Notification> = serde_json::from_str(&persisted).unwrap();
        assert_eq!(persisted_items.len(), MAX_NOTIFICATIONS);
        assert_eq!(persisted_items.first().unwrap().id, "n5");

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

        let _ = std::fs::remove_file(&tmp);
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
