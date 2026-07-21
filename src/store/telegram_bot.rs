use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

use crate::error::{AppError, Result};
use crate::store::schema::{
    backup_store_before_migration, decode_store_json, write_versioned_json_atomic_async, StoreKind,
    StoreSchemaError,
};
use crate::utils::{quarantine_corrupt_file, set_file_mode};

const MAX_PROCESSED_UPDATES: usize = 2_000;
const MAX_PROCESSED_CALLBACKS: usize = 2_000;
const MAX_ACTION_KEYS: usize = 2_000;
const MAX_AUDITS: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramCommandAudit {
    pub id: String,
    pub update_id: i64,
    #[serde(default)]
    pub callback_id: Option<String>,
    pub user_id: i64,
    pub chat_id: i64,
    pub command: String,
    #[serde(default)]
    pub target: String,
    pub result: String,
    #[serde(default)]
    pub message: String,
    pub duration_ms: u64,
    pub correlation_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelegramUserSessionRecord {
    pub user_id: i64,
    pub chat_id: i64,
    pub expires_at: i64,
    /// search | switch
    pub kind: String,
    #[serde(default)]
    pub payload: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramBotPersistentState {
    #[serde(default)]
    pub processed_update_ids: Vec<i64>,
    #[serde(default)]
    pub processed_callback_ids: Vec<String>,
    #[serde(default)]
    pub action_idempotency_keys: Vec<String>,
    #[serde(default)]
    pub audits: Vec<TelegramCommandAudit>,
    /// 搜索/换源会话（跨重启恢复）
    #[serde(default)]
    pub user_sessions: Vec<TelegramUserSessionRecord>,
}

pub struct TelegramBotStore {
    path: PathBuf,
    state: RwLock<TelegramBotPersistentState>,
    save_lock: Mutex<()>,
}

impl TelegramBotStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            state: RwLock::new(TelegramBotPersistentState::default()),
            save_lock: Mutex::new(()),
        }
    }

    pub async fn load(&self) -> Result<()> {
        if !self.path.exists() {
            self.save(&TelegramBotPersistentState::default()).await?;
            return Ok(());
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|error| AppError::Database(format!("读取 Telegram Bot 状态失败: {error}")))?;
        set_file_mode(&self.path, 0o600)?;
        match decode_store_json::<TelegramBotPersistentState>(&content, StoreKind::TelegramBot) {
            Ok(decoded) => {
                backup_store_before_migration(&self.path, &content, decoded.source_version)?;
                let mut state = decoded.data;
                trim_state(&mut state);
                if decoded.needs_write {
                    self.save(&state).await?;
                }
                *self.state.write().await = state;
                Ok(())
            }
            Err(StoreSchemaError::UnsupportedVersion { found, current }) => {
                Err(AppError::Database(format!(
                    "Telegram Bot 状态 schema 版本 {found} 高于当前支持版本 {current}"
                )))
            }
            Err(error) => {
                tracing::warn!("Telegram Bot 状态损坏，已隔离并使用空状态: {error}");
                quarantine_corrupt_file(&self.path);
                let state = TelegramBotPersistentState::default();
                self.save(&state).await?;
                *self.state.write().await = state;
                Ok(())
            }
        }
    }

    pub async fn claim_update(&self, update_id: i64) -> Result<bool> {
        self.mutate(|state| {
            if state.processed_update_ids.contains(&update_id) {
                return false;
            }
            state.processed_update_ids.push(update_id);
            true
        })
        .await
    }

    pub async fn claim_callback(&self, callback_id: &str) -> Result<bool> {
        let callback_id = callback_id.to_string();
        self.mutate(|state| {
            if state.processed_callback_ids.contains(&callback_id) {
                return false;
            }
            state.processed_callback_ids.push(callback_id);
            true
        })
        .await
    }

    pub async fn claim_action(&self, idempotency_key: &str) -> Result<bool> {
        let idempotency_key = idempotency_key.to_string();
        self.mutate(|state| {
            if state.action_idempotency_keys.contains(&idempotency_key) {
                return false;
            }
            state.action_idempotency_keys.push(idempotency_key);
            true
        })
        .await
    }

    pub async fn add_audit(&self, audit: TelegramCommandAudit) -> Result<()> {
        self.mutate(|state| {
            state.audits.push(audit);
        })
        .await
    }

    pub async fn audit_count(&self) -> usize {
        self.state.read().await.audits.len()
    }

    pub async fn list_audits(&self, limit: usize) -> Vec<TelegramCommandAudit> {
        self.state
            .read()
            .await
            .audits
            .iter()
            .rev()
            .take(limit.min(MAX_AUDITS))
            .cloned()
            .collect()
    }

    pub async fn put_user_session(&self, session: TelegramUserSessionRecord) -> Result<()> {
        self.mutate(|state| {
            let now = crate::utils::unix_now();
            state.user_sessions.retain(|item| item.expires_at >= now);
            state.user_sessions.retain(|item| {
                !(item.user_id == session.user_id && item.chat_id == session.chat_id)
            });
            state.user_sessions.push(session);
            if state.user_sessions.len() > 200 {
                let remove = state.user_sessions.len() - 200;
                state.user_sessions.drain(0..remove);
            }
        })
        .await
    }

    pub async fn get_user_session(
        &self,
        user_id: i64,
        chat_id: i64,
    ) -> Option<TelegramUserSessionRecord> {
        let now = crate::utils::unix_now();
        let state = self.state.read().await;
        state
            .user_sessions
            .iter()
            .find(|item| {
                item.user_id == user_id && item.chat_id == chat_id && item.expires_at >= now
            })
            .cloned()
    }

    async fn mutate<F, T>(&self, update: F) -> Result<T>
    where
        F: FnOnce(&mut TelegramBotPersistentState) -> T,
    {
        let _guard = self.save_lock.lock().await;
        let mut snapshot = self.state.read().await.clone();
        let result = update(&mut snapshot);
        trim_state(&mut snapshot);
        self.save(&snapshot).await?;
        *self.state.write().await = snapshot;
        Ok(result)
    }

    async fn save(&self, state: &TelegramBotPersistentState) -> Result<()> {
        write_versioned_json_atomic_async(&self.path, state, 0o600).await
    }
}

fn trim_state(state: &mut TelegramBotPersistentState) {
    trim_front(&mut state.processed_update_ids, MAX_PROCESSED_UPDATES);
    trim_front(&mut state.processed_callback_ids, MAX_PROCESSED_CALLBACKS);
    trim_front(&mut state.action_idempotency_keys, MAX_ACTION_KEYS);
    trim_front(&mut state.audits, MAX_AUDITS);
    let now = crate::utils::unix_now();
    state.user_sessions.retain(|item| item.expires_at >= now);
    if state.user_sessions.len() > 200 {
        let remove = state.user_sessions.len() - 200;
        state.user_sessions.drain(0..remove);
    }
}

fn trim_front<T>(items: &mut Vec<T>, limit: usize) {
    if items.len() > limit {
        items.drain(..items.len() - limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn claims_and_audits_survive_restart() {
        let path = std::env::temp_dir().join(format!(
            "my-media-sub-telegram-state-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = TelegramBotStore::new(&path);
        store.load().await.unwrap();
        assert!(store.claim_update(7).await.unwrap());
        assert!(!store.claim_update(7).await.unwrap());
        assert!(store.claim_callback("callback-1").await.unwrap());
        assert!(store.claim_action("action-1").await.unwrap());
        store
            .add_audit(TelegramCommandAudit {
                id: "audit-1".to_string(),
                update_id: 7,
                callback_id: None,
                user_id: 1,
                chat_id: 1,
                command: "status".to_string(),
                target: String::new(),
                result: "succeeded".to_string(),
                message: String::new(),
                duration_ms: 1,
                correlation_id: "correlation-1".to_string(),
                created_at: 1,
            })
            .await
            .unwrap();

        let reloaded = TelegramBotStore::new(&path);
        reloaded.load().await.unwrap();
        assert!(!reloaded.claim_update(7).await.unwrap());
        assert!(!reloaded.claim_callback("callback-1").await.unwrap());
        assert!(!reloaded.claim_action("action-1").await.unwrap());
        assert_eq!(reloaded.audit_count().await, 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn future_schema_is_rejected_without_overwrite() {
        let path = std::env::temp_dir().join(format!(
            "my-media-sub-telegram-future-{}.json",
            uuid::Uuid::new_v4()
        ));
        let content = r#"{"schema_version":999,"data":{}}"#;
        std::fs::write(&path, content).unwrap();
        let store = TelegramBotStore::new(&path);
        assert!(store.load().await.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn corrupt_state_is_quarantined_and_recreated_empty() {
        let path = std::env::temp_dir().join(format!(
            "my-media-sub-telegram-corrupt-{}.json",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, "{not-json").unwrap();
        let store = TelegramBotStore::new(&path);
        store.load().await.unwrap();
        assert_eq!(store.audit_count().await, 0);
        assert!(path.is_file());
        assert!(serde_json::from_str::<serde_json::Value>(
            &std::fs::read_to_string(&path).unwrap()
        )
        .is_ok());
        let parent = path.parent().unwrap();
        let prefix = format!("{}.", path.file_name().unwrap().to_string_lossy());
        let quarantined = std::fs::read_dir(parent)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .find(|candidate| {
                let name = candidate.file_name().unwrap().to_string_lossy();
                name.starts_with(&prefix) && name.contains("corrupt")
            })
            .expect("corrupt state should be quarantined");
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(quarantined);
    }
}
