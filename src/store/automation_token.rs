use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::sync::{Mutex, RwLock};

use crate::error::{AppError, Result};
use crate::store::schema::{decode_store_json, write_versioned_json_atomic_async, StoreKind};
use crate::utils::{constant_time_eq, set_file_mode, unix_now};

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

        // 与其他 Store 一致走 schema_version 信封（`decode_store_json` 同时兼容
        // 旧的无信封裸记录，并会在读到旧格式时标记需要回写）。
        let content = String::from_utf8_lossy(&bytes);
        let decoded =
            decode_store_json::<AutomationTokenRecord>(&content, StoreKind::AutomationToken)
                .map_err(|error| AppError::Database(format!("解析自动化 Token 失败: {error}")))?;
        let needs_write = decoded.needs_write;
        *self.record.write().await = Some(decoded.data);
        if needs_write {
            let _guard = self.save_lock.lock().await;
            if let Some(record) = self.record.read().await.clone() {
                self.save_locked(&record).await?;
            }
        }
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
        // 持锁覆盖「落盘 + 内存更新」整个区间，避免与 authenticate 的
        // 读-改-写交错：否则 authenticate 可能把旧记录（含 last_used_at）
        // 覆写回盘，新 token 在重启后凭空消失。
        let _guard = self.save_lock.lock().await;
        self.commit_locked(record).await?;
        Ok((token, self.status().await))
    }

    pub async fn revoke(&self) -> Result<AutomationTokenStatus> {
        let _guard = self.save_lock.lock().await;
        let Some(mut record) = self.record.read().await.clone() else {
            return Err(AppError::NotFound("尚未配置自动化 Token".into()));
        };
        record.revoked_at = Some(unix_now());
        self.commit_locked(record).await?;
        Ok(self.status().await)
    }

    pub async fn authenticate(&self, token: &str, required_scope: &str) -> bool {
        // 持锁覆盖整个读-改-写，与 rotate/revoke 串行化，避免写回旧记录
        // 覆盖新轮换的 token。
        let _guard = self.save_lock.lock().await;
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
            if self.save_locked(&record).await.is_ok() {
                *self.record.write().await = Some(record);
            }
        }
        true
    }

    /// 在持有 save_lock 的前提下落盘（阻塞的 fsync/rename 放到 spawn_blocking）。
    /// 提交参数记录：**先更新内存，再落盘**，失败时回滚内存。调用方须已持
    /// `save_lock`。
    ///
    /// 理由同 `SubscriptionStore::commit`：落盘走 spawn_blocking 不可取消，
    /// 「先落盘再改内存」在 future 被 abort（例如撤销请求被中断）时会留下
    /// 「磁盘已撤销、内存仍有效」，而内存是鉴权时读取的来源，下一次
    /// `authenticate` 的写回会把撤销状态覆盖掉——**撤销静默失效**。
    async fn commit_locked(&self, record: AutomationTokenRecord) -> Result<()> {
        let previous = self.record.read().await.clone();
        *self.record.write().await = Some(record.clone());
        match self.save_locked(&record).await {
            Ok(()) => Ok(()),
            Err(error) => {
                *self.record.write().await = previous;
                Err(error)
            }
        }
    }

    async fn save_locked(&self, record: &AutomationTokenRecord) -> Result<()> {
        write_versioned_json_atomic_async(&self.path, record, 0o600).await
    }
}

fn token_hash(token: &str) -> String {
    digest(&SHA256, token.as_bytes())
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// scope 必须**精确匹配**。
///
/// 旧实现写成 `scope == "read" && required.ends_with(":read") || scope == required`，
/// 按优先级等价于 `(read && ends_with(":read")) || exact`，于是只授 `read` 的 Token
/// 可以满足任意以 `:read` 结尾的 scope——包括 `diagnostics:read`，而
/// `/api/telegram/audits` 的 `target` 字段记录的是 Telegram 命令的原始参数，
/// 里面就有用户粘贴的分享链接与提取码。`TOKEN_SCOPES` 把 `read` 与
/// `diagnostics:read` 并列为独立选项，因此这是与设计意图不符的隐性提权。
///
/// 若将来确实需要「超级只读」scope，必须新增一个显式名字并同时从
/// `TOKEN_SCOPES` 移除 `diagnostics:read`，不能靠后缀匹配隐式扩大授权。
fn scope_allows(scopes: &[String], required: &str) -> bool {
    scopes.iter().any(|scope| scope == required)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归测试：`read` 不得隐式满足任何 `*:read` scope。
    ///
    /// 修复前 `scope_allows` 是 `(read && required.ends_with(":read")) || exact`，
    /// 于是只授 `read` 的 Token 拿到 `diagnostics:read`，可读 `/api/telegram/audits`
    /// ——其中的 `target` 就是用户粘贴的分享链接与提取码。
    #[test]
    fn read_scope_does_not_imply_namespaced_read_scopes() {
        let read_only = vec!["read".to_string()];
        assert!(scope_allows(&read_only, "read"));
        for escalated in [
            "diagnostics:read",
            "jobs:read",
            "notifications:read",
            "subscriptions:read",
        ] {
            assert!(
                !scope_allows(&read_only, escalated),
                "scope `read` 不应满足 `{escalated}`"
            );
        }
    }

    #[test]
    fn scopes_require_exact_match() {
        let scopes = vec!["jobs:read".to_string()];
        assert!(scope_allows(&scopes, "jobs:read"));
        assert!(!scope_allows(&scopes, "jobs:write"));
        assert!(!scope_allows(&scopes, "diagnostics:read"));
        assert!(!scope_allows(&scopes, "read"));
        assert!(!scope_allows(&[], "read"));
    }

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

    /// 回归测试：并发 authenticate 与 rotate 交错时，不得让旧记录（含
    /// last_used_at）覆写掉新轮换的 token。修复前此测试在最后断言
    /// persisted.hash == memory.hash 时可能失败。
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_authenticate_and_rotate_keep_memory_and_disk_in_sync() {
        let path = std::env::temp_dir().join(format!(
            "automation-token-race-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = std::sync::Arc::new(AutomationTokenStore::new(&path));
        let (token_a, _) = store.rotate(vec!["read".into()], None).await.unwrap();

        let mut tasks = Vec::new();
        for _ in 0..8 {
            let store = store.clone();
            let token_a = token_a.clone();
            tasks.push(tokio::spawn(async move {
                for _ in 0..20 {
                    // rotate 可能已替换 token，authenticate 结果可能为假；
                    // 这里只制造并发交错，不校验返回值。
                    let _ = store.authenticate(&token_a, "read").await;
                }
            }));
        }
        let store_rotate = store.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..5 {
                let _ = store_rotate
                    .rotate(vec!["read".into()], None)
                    .await
                    .unwrap();
            }
        }));
        for task in tasks {
            task.await.unwrap();
        }

        let (final_token, _) = store.rotate(vec!["read".into()], None).await.unwrap();
        assert!(store.authenticate(&final_token, "read").await);

        // 磁盘现在是 schema_version 信封，必须走同一个解码器读取。
        let raw = std::fs::read_to_string(&path).unwrap();
        let persisted =
            decode_store_json::<AutomationTokenRecord>(&raw, StoreKind::AutomationToken)
                .unwrap()
                .data;
        assert!(
            raw.contains("\"schema_version\""),
            "Token 存储应与其他 Store 一样带 schema_version 信封: {raw}"
        );
        let memory = store.record.read().await.clone().unwrap();
        assert_eq!(
            persisted.hash, memory.hash,
            "并发交错后磁盘与内存记录不一致"
        );
        let _ = std::fs::remove_file(path);
    }
}
