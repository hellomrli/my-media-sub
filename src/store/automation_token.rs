use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::{Mutex, RwLock};

use crate::error::{AppError, Result};
use crate::utils::{constant_time_eq, set_file_mode, unix_now, write_file_atomic};

pub const TOKEN_SCOPES: &[&str] = &[
    "read",
    "subscriptions:read",
    "subscriptions:write",
    "subscriptions:check",
    "jobs:read",
    "jobs:write",
    "notifications:read",
    "notifications:write",
    "quark:signin",
    "diagnostics:read",
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AutomationTokenRecord {
    pub prefix: String,
    pub hash: String,
    pub scopes: Vec<String>,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutomationTokenStatus {
    pub configured: bool,
    pub prefix: Option<String>,
    pub scopes: Vec<String>,
    pub created_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

pub struct AutomationTokenStore {
    path: PathBuf,
    record: RwLock<Option<AutomationTokenRecord>>,
    save_lock: Mutex<()>,
}

impl AutomationTokenStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            record: RwLock::new(None),
            save_lock: Mutex::new(()),
        }
    }

    pub async fn load(&self) -> Result<()> {
        if !self.path.exists() {
            return Ok(());
        }
        let bytes = std::fs::read(&self.path)
            .map_err(|e| AppError::Database(format!("读取自动化 Token 失败: {e}")))?;
        set_file_mode(&self.path, 0o600)?;
        *self.record.write().await = Some(
            serde_json::from_slice(&bytes)
                .map_err(|e| AppError::Database(format!("解析自动化 Token 失败: {e}")))?,
        );
        Ok(())
    }

    pub async fn status(&self) -> AutomationTokenStatus {
        let record = self.record.read().await.clone();
        AutomationTokenStatus {
            configured: record.is_some(),
            prefix: record.as_ref().map(|r| r.prefix.clone()),
            scopes: record
                .as_ref()
                .map(|r| r.scopes.clone())
                .unwrap_or_default(),
            created_at: record.as_ref().map(|r| r.created_at),
            expires_at: record.as_ref().and_then(|r| r.expires_at),
            last_used_at: record.as_ref().and_then(|r| r.last_used_at),
            revoked_at: record.as_ref().and_then(|r| r.revoked_at),
        }
    }

    pub async fn rotate(
        &self,
        scopes: Vec<String>,
        expires_days: Option<u64>,
    ) -> Result<(String, AutomationTokenStatus)> {
        if scopes.is_empty()
            || scopes
                .iter()
                .any(|scope| !TOKEN_SCOPES.contains(&scope.as_str()))
        {
            return Err(AppError::Validation("Token scopes 无效或为空".into()));
        }
        let token = format!(
            "mms_{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let now = unix_now();
        let record = AutomationTokenRecord {
            prefix: token.chars().take(12).collect(),
            hash: token_hash(&token),
            scopes,
            created_at: now,
            expires_at: expires_days
                .map(|days| now.saturating_add((days.clamp(1, 3650) * 86400) as i64)),
            last_used_at: None,
            revoked_at: None,
        };
        self.save(&record).await?;
        *self.record.write().await = Some(record);
        Ok((token, self.status().await))
    }

    pub async fn revoke(&self) -> Result<AutomationTokenStatus> {
        let _guard = self.save_lock.lock().await;
        let Some(mut record) = self.record.read().await.clone() else {
            return Err(AppError::NotFound("尚未配置自动化 Token".into()));
        };
        record.revoked_at = Some(unix_now());
        self.save_unlocked(&record)?;
        *self.record.write().await = Some(record);
        Ok(self.status().await)
    }

    pub async fn authenticate(&self, token: &str, required_scope: &str) -> bool {
        let Some(mut record) = self.record.read().await.clone() else {
            return false;
        };
        let now = unix_now();
        if record.revoked_at.is_some()
            || record.expires_at.is_some_and(|expires| expires <= now)
            || !scope_allows(&record.scopes, required_scope)
            || !constant_time_eq(&record.hash, &token_hash(token))
        {
            return false;
        }
        if record
            .last_used_at
            .is_none_or(|last| now.saturating_sub(last) >= 60)
        {
            record.last_used_at = Some(now);
            if self.save(&record).await.is_ok() {
                *self.record.write().await = Some(record);
            }
        }
        true
    }

    async fn save(&self, record: &AutomationTokenRecord) -> Result<()> {
        let _guard = self.save_lock.lock().await;
        self.save_unlocked(record)
    }
    fn save_unlocked(&self, record: &AutomationTokenRecord) -> Result<()> {
        write_file_atomic(&self.path, &serde_json::to_vec_pretty(record)?, 0o600)
    }
}

fn token_hash(token: &str) -> String {
    digest(&SHA256, token.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn scope_allows(scopes: &[String], required: &str) -> bool {
    scopes
        .iter()
        .any(|scope| scope == "read" && required.ends_with(":read") || scope == required)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn token_is_hashed_scoped_and_revocable() {
        let path =
            std::env::temp_dir().join(format!("automation-token-{}.json", uuid::Uuid::new_v4()));
        let store = AutomationTokenStore::new(&path);
        let (token, status) = store
            .rotate(vec!["subscriptions:read".into()], Some(30))
            .await
            .unwrap();
        assert!(status.configured);
        let persisted = std::fs::read_to_string(&path).unwrap();
        assert!(!persisted.contains(&token));
        assert!(store.authenticate(&token, "subscriptions:read").await);
        assert!(!store.authenticate(&token, "jobs:read").await);
        store.revoke().await.unwrap();
        assert!(!store.authenticate(&token, "subscriptions:read").await);
        let _ = std::fs::remove_file(path);
    }
}
