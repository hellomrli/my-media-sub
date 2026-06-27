use crate::error::{AppError, Result};
use crate::models::{settings::normalize_check_interval_minutes, Settings};
use crate::utils::{quarantine_corrupt_file, set_file_mode, write_json_atomic_async};
use std::path::PathBuf;
use tokio::sync::RwLock;

/// 受保护的密钥字段（public() 视图中会被脱敏）
pub const SECRET_KEYS: &[&str] = &[
    "app_password",
    "aria2_secret",
    "quark_cookie",
    "quark_signin_cookie",
    "strm_access_token",
    "pansou_api_url",
    "tmdb_api_key",
    "wecom_bot_url",
    "bark_url",
    "wxpusher_app_token",
    "telegram_bot_token",
    "gotify_token",
    "pushplus_token",
    "serverchan_key",
];

/// 支持的云盘类型
pub const SUPPORTED_CLOUD_TYPES: &[&str] = &["quark"];

/// 设置存储（单个 JSON 对象，原子写入）
pub struct SettingsStore {
    path: PathBuf,
    settings: RwLock<Settings>,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            settings: RwLock::new(Settings::default()),
        }
    }

    /// 从文件加载（不存在则写入默认值）
    pub async fn load(&self) -> Result<()> {
        let mut settings = self.settings.write().await;
        if !self.path.exists() {
            // 写默认值
            self.write_to_disk(&settings).await?;
            return Ok(());
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| AppError::Database(format!("读取设置文件失败: {}", e)))?;
        set_file_mode(&self.path, 0o600)?;
        let strm_token_missing = serde_json::from_str::<serde_json::Value>(&content)
            .ok()
            .and_then(|value| {
                value
                    .get("strm_access_token")
                    .and_then(|token| token.as_str())
                    .map(|token| token.trim().is_empty())
            })
            .unwrap_or(true);
        let mut should_write = strm_token_missing;
        *settings = match serde_json::from_str::<Settings>(&content) {
            Ok(settings) => settings,
            Err(e) => {
                tracing::error!("设置文件解析失败，已隔离损坏文件并停止启动: {}", e);
                quarantine_corrupt_file(&self.path);
                return Err(AppError::Database(
                    "设置文件解析失败，已隔离损坏文件；请检查配置后重启".to_string(),
                ));
            }
        };
        if settings.strm_access_token.trim().is_empty() {
            settings.strm_access_token = uuid::Uuid::new_v4().to_string();
            should_write = true;
        }
        if should_write {
            self.write_to_disk(&settings).await?;
        }
        Ok(())
    }

    async fn write_to_disk(&self, settings: &Settings) -> Result<()> {
        write_json_atomic_async(&self.path, settings, 0o600).await
    }

    /// 获取完整设置（含密钥，仅内部使用）
    pub async fn get(&self) -> Settings {
        self.settings.read().await.clone()
    }

    /// 更新设置（通过闭包修改）
    pub async fn update<F>(&self, updater: F) -> Result<Settings>
    where
        F: FnOnce(&mut Settings),
    {
        let mut settings = self.settings.write().await;
        updater(&mut settings);
        settings.subscription_check_interval_minutes = normalize_check_interval_minutes(i64::from(
            settings.subscription_check_interval_minutes,
        ));
        // 校验：cloud_types 只保留支持的类型，为空则默认 quark
        settings
            .cloud_types
            .retain(|t| SUPPORTED_CLOUD_TYPES.contains(&t.as_str()));
        if settings.cloud_types.is_empty() {
            settings.cloud_types = vec!["quark".to_string()];
        }
        if settings.strm_access_token.trim().is_empty() {
            settings.strm_access_token = uuid::Uuid::new_v4().to_string();
        }
        let updated = settings.clone();
        self.write_to_disk(&updated).await?;
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_settings_store() {
        let tmp = std::env::temp_dir().join("test_settings_store.json");
        let _ = std::fs::remove_file(&tmp);
        let store = SettingsStore::new(&tmp);
        store.load().await.unwrap();

        // 默认值
        let s = store.get().await;
        assert_eq!(s.app_username, "admin");

        // 更新
        let updated = store
            .update(|s| {
                s.app_username = "lain".to_string();
                s.quark_cookie = "cookie123".to_string();
            })
            .await
            .unwrap();
        assert_eq!(updated.app_username, "lain");

        // 校验：检查间隔下限
        let updated = store
            .update(|s| s.subscription_check_interval_minutes = 1)
            .await
            .unwrap();
        assert_eq!(updated.subscription_check_interval_minutes, 5);

        // 校验：无效云盘类型被过滤
        let updated = store
            .update(|s| s.cloud_types = vec!["invalid".to_string()])
            .await
            .unwrap();
        assert_eq!(updated.cloud_types, vec!["quark"]);

        // 持久化验证
        let store2 = SettingsStore::new(&tmp);
        store2.load().await.unwrap();
        assert_eq!(store2.get().await.app_username, "lain");

        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn load_quarantines_corrupt_settings_file() {
        let tmp = std::env::temp_dir().join(format!(
            "test_settings_corrupt_{}.json",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&tmp, b"{not-valid-json").unwrap();

        let store = SettingsStore::new(&tmp);
        let error = store.load().await.unwrap_err();

        assert!(matches!(error, AppError::Database(_)));
        assert!(!tmp.exists());

        let parent = tmp.parent().unwrap();
        let file_name = tmp.file_name().unwrap().to_string_lossy();
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
                return;
            }
        }

        panic!("corrupt settings file was not quarantined");
    }
}
