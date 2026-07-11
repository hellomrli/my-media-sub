use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use tokio::sync::{Mutex, RwLock};

use crate::error::{AppError, Result};
use crate::models::{AutomationEvent, AutomationStatus};
use crate::store::schema::{
    backup_store_before_migration, decode_store_json, write_versioned_json_atomic_async, StoreKind,
    StoreSchemaError,
};
use crate::utils::{quarantine_corrupt_file, set_file_mode, unix_now};

const NORMAL_RETENTION_SECONDS: i64 = 30 * 24 * 3600;
const FAILED_RETENTION_SECONDS: i64 = 90 * 24 * 3600;
const MAX_EVENTS: usize = 5_000;

#[derive(Default)]
struct EventIndexes {
    subscriptions: HashMap<String, Vec<String>>,
    correlations: HashMap<String, Vec<String>>,
    jobs: HashMap<String, Vec<String>>,
}

pub struct AutomationEventStore {
    path: PathBuf,
    items: RwLock<Vec<AutomationEvent>>,
    indexes: RwLock<EventIndexes>,
    save_lock: Mutex<()>,
}

impl AutomationEventStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            items: RwLock::new(Vec::new()),
            indexes: RwLock::new(EventIndexes::default()),
            save_lock: Mutex::new(()),
        }
    }

    pub async fn load(&self) -> Result<()> {
        if !self.path.exists() {
            self.replace_memory(Vec::new()).await;
            return Ok(());
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|error| AppError::Database(format!("读取自动化事件失败: {}", error)))?;
        set_file_mode(&self.path, 0o600)?;
        match decode_store_json::<Vec<AutomationEvent>>(&content, StoreKind::AutomationEvents) {
            Ok(decoded) => {
                backup_store_before_migration(&self.path, &content, decoded.source_version)?;
                let mut events = decoded.data;
                let original_len = events.len();
                prune_events(&mut events, unix_now());
                if decoded.needs_write || events.len() != original_len {
                    self.save(&events).await?;
                }
                self.replace_memory(events).await;
                Ok(())
            }
            Err(StoreSchemaError::UnsupportedVersion { found, current }) => {
                Err(AppError::Database(format!(
                    "自动化事件 schema 版本 {} 高于当前支持版本 {}，请升级程序后重试",
                    found, current
                )))
            }
            Err(error) => {
                tracing::warn!("自动化事件 JSON 损坏，已隔离并使用空事件: {}", error);
                quarantine_corrupt_file(&self.path);
                self.replace_memory(Vec::new()).await;
                Ok(())
            }
        }
    }

    pub async fn add(&self, event: AutomationEvent) -> Result<AutomationEvent> {
        let _guard = self.save_lock.lock().await;
        let snapshot = {
            let items = self.items.read().await;
            if let Some(existing) = items.iter().find(|item| item.id == event.id) {
                return Ok(existing.clone());
            }
            let mut snapshot = items.clone();
            snapshot.push(event.clone());
            prune_events(&mut snapshot, unix_now());
            snapshot
        };
        self.save(&snapshot).await?;
        self.replace_memory(snapshot).await;
        Ok(event)
    }

    pub async fn upsert(&self, mut event: AutomationEvent) -> Result<AutomationEvent> {
        let _guard = self.save_lock.lock().await;
        let snapshot = {
            let items = self.items.read().await;
            let mut snapshot = items.clone();
            if let Some(current) = snapshot.iter_mut().find(|item| item.id == event.id) {
                if !current.status.can_transition_to(event.status) {
                    return Err(AppError::Validation(format!(
                        "自动化事件不能从 {} 转换到 {}",
                        current.status.as_str(),
                        event.status.as_str()
                    )));
                }
                if current.created_at > 0
                    && (event.created_at == 0 || current.created_at < event.created_at)
                {
                    event.created_at = current.created_at;
                }
                if event.started_at.is_none() {
                    event.started_at = current.started_at;
                }
                if event.finished_at.is_none() && event.status.is_terminal() {
                    event.finished_at = Some(event.updated_at.max(event.created_at));
                }
                *current = event.clone();
            } else {
                snapshot.push(event.clone());
            }
            prune_events(&mut snapshot, unix_now());
            snapshot
        };
        self.save(&snapshot).await?;
        self.replace_memory(snapshot).await;
        Ok(event)
    }

    pub async fn get(&self, id: &str) -> Option<AutomationEvent> {
        self.items
            .read()
            .await
            .iter()
            .find(|event| event.id == id)
            .cloned()
    }

    pub async fn list(&self, limit: usize) -> Vec<AutomationEvent> {
        self.items
            .read()
            .await
            .iter()
            .rev()
            .take(limit.clamp(1, 1_000))
            .cloned()
            .collect()
    }

    pub async fn list_by_subscription(&self, id: &str, limit: usize) -> Vec<AutomationEvent> {
        self.list_by_index(|indexes| indexes.subscriptions.get(id), limit)
            .await
    }

    pub async fn list_by_correlation(&self, id: &str, limit: usize) -> Vec<AutomationEvent> {
        self.list_by_index(|indexes| indexes.correlations.get(id), limit)
            .await
    }

    pub async fn list_by_job(&self, id: &str, limit: usize) -> Vec<AutomationEvent> {
        self.list_by_index(|indexes| indexes.jobs.get(id), limit)
            .await
    }

    async fn list_by_index<F>(&self, select: F, limit: usize) -> Vec<AutomationEvent>
    where
        F: FnOnce(&EventIndexes) -> Option<&Vec<String>>,
    {
        let items = self.items.read().await;
        let indexes = self.indexes.read().await;
        let Some(ids) = select(&indexes) else {
            return Vec::new();
        };
        let wanted = ids
            .iter()
            .rev()
            .take(limit.clamp(1, 1_000))
            .cloned()
            .collect::<HashSet<_>>();
        items
            .iter()
            .rev()
            .filter(|event| wanted.contains(&event.id))
            .take(limit.clamp(1, 1_000))
            .cloned()
            .collect()
    }

    pub async fn counts_by_status(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for event in self.items.read().await.iter() {
            *counts.entry(event.status.as_str().to_string()).or_default() += 1;
        }
        counts
    }

    async fn save(&self, events: &[AutomationEvent]) -> Result<()> {
        write_versioned_json_atomic_async(&self.path, &events, 0o600).await
    }

    pub async fn compact(&self) -> Result<usize> {
        let _guard = self.save_lock.lock().await;
        let mut snapshot = self.items.read().await.clone();
        let before = snapshot.len();
        prune_events(&mut snapshot, unix_now());
        self.save(&snapshot).await?;
        let removed = before.saturating_sub(snapshot.len());
        self.replace_memory(snapshot).await;
        Ok(removed)
    }

    async fn replace_memory(&self, events: Vec<AutomationEvent>) {
        let indexes = build_indexes(&events);
        let mut current_items = self.items.write().await;
        let mut current_indexes = self.indexes.write().await;
        *current_items = events;
        *current_indexes = indexes;
    }
}

fn build_indexes(events: &[AutomationEvent]) -> EventIndexes {
    let mut indexes = EventIndexes::default();
    for event in events {
        if let Some(subscription_id) = &event.subscription_id {
            indexes
                .subscriptions
                .entry(subscription_id.clone())
                .or_default()
                .push(event.id.clone());
        }
        indexes
            .correlations
            .entry(event.correlation_id.clone())
            .or_default()
            .push(event.id.clone());
        if let Some(job_id) = &event.job_id {
            indexes
                .jobs
                .entry(job_id.clone())
                .or_default()
                .push(event.id.clone());
        }
    }
    indexes
}

fn prune_events(events: &mut Vec<AutomationEvent>, now: i64) {
    events.retain(|event| {
        let retention = if event.status == AutomationStatus::Failed {
            FAILED_RETENTION_SECONDS
        } else {
            NORMAL_RETENTION_SECONDS
        };
        now.saturating_sub(event.updated_at.max(event.created_at)) <= retention
    });
    if events.len() > MAX_EVENTS {
        let remove = events.len() - MAX_EVENTS;
        events.drain(0..remove);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AutomationStage, AutomationStatus};

    fn event(id: &str, status: AutomationStatus, now: i64) -> AutomationEvent {
        let mut event =
            AutomationEvent::new(id, "correlation", AutomationStage::SourceCheck, status, now);
        event.subscription_id = Some("sub".to_string());
        event.job_id = Some("job".to_string());
        event
    }

    #[tokio::test]
    async fn add_is_idempotent_and_indexes_are_rebuilt_after_load() {
        let path = std::env::temp_dir().join(format!("events-{}.json", uuid::Uuid::new_v4()));
        let store = AutomationEventStore::new(&path);
        store.load().await.unwrap();
        let now = unix_now();
        store
            .add(event("one", AutomationStatus::Pending, now))
            .await
            .unwrap();
        store
            .add(event("one", AutomationStatus::Pending, now))
            .await
            .unwrap();
        assert_eq!(store.list(10).await.len(), 1);
        assert_eq!(store.list_by_subscription("sub", 10).await.len(), 1);

        let reloaded = AutomationEventStore::new(&path);
        reloaded.load().await.unwrap();
        assert_eq!(
            reloaded.list_by_correlation("correlation", 10).await.len(),
            1
        );
        assert_eq!(reloaded.list_by_job("job", 10).await.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn upsert_keeps_one_lifecycle_and_preserves_started_time() {
        let path = std::env::temp_dir().join(format!("events-{}.json", uuid::Uuid::new_v4()));
        let store = AutomationEventStore::new(&path);
        let now = unix_now();
        store
            .upsert(event("one", AutomationStatus::Running, now))
            .await
            .unwrap();
        store
            .upsert(event("one", AutomationStatus::Succeeded, now + 5))
            .await
            .unwrap();

        let items = store.list(10).await;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].status, AutomationStatus::Succeeded);
        assert_eq!(items[0].created_at, now);
        assert_eq!(items[0].started_at, Some(now));
        assert_eq!(items[0].finished_at, Some(now + 5));
        assert_eq!(items[0].duration_seconds(), Some(5));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn invalid_terminal_transition_does_not_overwrite_store() {
        let path = std::env::temp_dir().join(format!("events-{}.json", uuid::Uuid::new_v4()));
        let store = AutomationEventStore::new(&path);
        let now = unix_now();
        store
            .add(event("one", AutomationStatus::Succeeded, now))
            .await
            .unwrap();
        let changed = event("one", AutomationStatus::Running, now + 1);
        assert!(store.upsert(changed).await.is_err());
        assert_eq!(
            store.get("one").await.unwrap().status,
            AutomationStatus::Succeeded
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn failed_events_have_extended_retention() {
        let now = 100 * 24 * 3600;
        let mut events = vec![
            event("normal", AutomationStatus::Succeeded, now - 40 * 24 * 3600),
            event("failed", AutomationStatus::Failed, now - 40 * 24 * 3600),
        ];
        prune_events(&mut events, now);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "failed");
    }
}
