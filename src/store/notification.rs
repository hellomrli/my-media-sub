use crate::error::{AppError, Result};
use crate::models::Notification;
use std::path::PathBuf;
use tokio::sync::RwLock;

/// 通知存储（JSON 文件，保留最近 300 条，原子写入）
pub struct NotificationStore {
    path: PathBuf,
    items: RwLock<Vec<Notification>>,
}

const MAX_NOTIFICATIONS: usize = 300;

impl NotificationStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            items: RwLock::new(Vec::new()),
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
        *items = serde_json::from_str(&content)
            .map_err(|e| AppError::Database(format!("解析通知 JSON 失败: {}", e)))?;
        Ok(())
    }

    async fn save(&self, items: &[Notification]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Database(format!("创建目录失败: {}", e)))?;
        }
        // 只保留最近 300 条
        let slice = if items.len() > MAX_NOTIFICATIONS {
            &items[items.len() - MAX_NOTIFICATIONS..]
        } else {
            items
        };
        let content = serde_json::to_string_pretty(slice)
            .map_err(|e| AppError::Database(format!("序列化通知失败: {}", e)))?;
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, content)
            .map_err(|e| AppError::Database(format!("写入临时文件失败: {}", e)))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| AppError::Database(format!("重命名临时文件失败: {}", e)))?;
        Ok(())
    }

    /// 添加通知
    pub async fn add(&self, notif: Notification) -> Result<Notification> {
        let mut items = self.items.write().await;
        items.push(notif.clone());
        self.save(&items).await?;
        Ok(notif)
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
        let mut items = self.items.write().await;
        for item in items.iter_mut() {
            if id.is_none() || id == Some(item.id.as_str()) {
                item.read = true;
            }
        }
        self.save(&items).await?;
        Ok(())
    }

    /// 清空所有通知
    pub async fn clear(&self) -> Result<()> {
        let mut items = self.items.write().await;
        items.clear();
        self.save(&items).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

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
        let tmp = std::env::temp_dir().join("test_notif_store.json");
        let _ = std::fs::remove_file(&tmp);
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
}
