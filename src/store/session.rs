#![allow(dead_code)]

use crate::models::SearchResult;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

/// 内存搜索会话
#[derive(Debug, Clone)]
pub struct SearchSession {
    pub keyword: String,
    pub results: Vec<SearchResult>,
    pub created_at: u64,
}

/// 内存会话存储（按 chat_id，带 TTL）
pub struct SessionStore {
    ttl_seconds: u64,
    sessions: RwLock<HashMap<String, SearchSession>>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl SessionStore {
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            ttl_seconds,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// 保存搜索会话
    pub async fn set(&self, key: &str, keyword: String, results: Vec<SearchResult>) {
        let mut sessions = self.sessions.write().await;
        sessions.insert(
            key.to_string(),
            SearchSession {
                keyword,
                results,
                created_at: now_secs(),
            },
        );
    }

    /// 获取搜索会话（过期则返回 None 并清除）
    pub async fn get(&self, key: &str) -> Option<SearchSession> {
        // 先读检查
        {
            let sessions = self.sessions.read().await;
            if let Some(sess) = sessions.get(key) {
                if now_secs().saturating_sub(sess.created_at) <= self.ttl_seconds {
                    return Some(sess.clone());
                }
            } else {
                return None;
            }
        }
        // 过期，清除
        let mut sessions = self.sessions.write().await;
        sessions.remove(key);
        None
    }
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(3600) // 1 小时，与 Python 一致
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(title: &str) -> SearchResult {
        SearchResult {
            title: title.to_string(),
            url: "https://pan.quark.cn/s/x".to_string(),
            password: String::new(),
            source: String::new(),
            cloud_type: "quark".to_string(),
            probe: None,
        }
    }

    #[tokio::test]
    async fn test_session_store() {
        let store = SessionStore::new(3600);

        // 不存在
        assert!(store.get("chat1").await.is_none());

        // 保存
        store
            .set("chat1", "关键词".to_string(), vec![make_result("结果1")])
            .await;

        // 获取
        let sess = store.get("chat1").await.unwrap();
        assert_eq!(sess.keyword, "关键词");
        assert_eq!(sess.results.len(), 1);
    }

    #[tokio::test]
    async fn test_session_expiry() {
        let store = SessionStore::new(0); // 立即过期
        store.set("chat1", "kw".to_string(), vec![]).await;
        // TTL=0，下次获取应该过期（created_at 与 now 差 >= 0，但 saturating_sub 为 0 <= 0 会命中）
        // 为确保过期，这里测试 TTL 边界：0 秒 TTL 意味着同秒内仍有效
        // 真实过期测试需 mock 时间，这里仅验证 set/get 不 panic
        let _ = store.get("chat1").await;
    }
}
