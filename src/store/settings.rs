use crate::error::{AppError, Result};
use crate::models::{settings::normalize_check_interval_minutes, Settings};
use crate::store::schema::{
    backup_store_before_migration, decode_store_json, write_versioned_json_atomic_async, StoreKind,
    StoreSchemaError,
};
use crate::utils::{quarantine_corrupt_file, set_file_mode};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use p256::elliptic_curve::sec1::ToEncodedPoint;
use std::path::PathBuf;
use tokio::sync::{Mutex, RwLock};

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
    "browser_push_vapid_private_key",
    "browser_push_subscriptions",
    "webhook_secret",
    "webhook_previous_secret",
];

/// 支持的云盘类型
pub const SUPPORTED_CLOUD_TYPES: &[&str] = &["quark"];

/// 设置存储（单个 JSON 对象，原子写入）
pub struct SettingsStore {
    path: PathBuf,
    settings: RwLock<Settings>,
    save_lock: Mutex<()>,
}

fn ensure_browser_push_keys(settings: &mut Settings) -> Result<bool> {
    if !settings.browser_push_vapid_private_key.is_empty()
        && !settings.browser_push_vapid_public_key.is_empty()
    {
        return Ok(false);
    }
    let secret = p256::SecretKey::random(&mut p256::elliptic_curve::rand_core::OsRng);
    let public = secret.public_key().to_encoded_point(false);
    settings.browser_push_vapid_private_key = URL_SAFE_NO_PAD.encode(secret.to_bytes());
    settings.browser_push_vapid_public_key = URL_SAFE_NO_PAD.encode(public.as_bytes());
    Ok(true)
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            settings: RwLock::new(Settings::default()),
            save_lock: Mutex::new(()),
        }
    }

    /// 从文件加载（不存在则写入默认值）
    pub async fn load(&self) -> Result<()> {
        let mut settings = self.settings.write().await;
        if !self.path.exists() {
            ensure_browser_push_keys(&mut settings)?;
            self.write_to_disk(&settings).await?;
            return Ok(());
        }
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| AppError::Database(format!("读取设置文件失败: {}", e)))?;
        set_file_mode(&self.path, 0o600)?;
        let decoded = match decode_store_json::<Settings>(&content, StoreKind::Settings) {
            Ok(decoded) => decoded,
            Err(StoreSchemaError::UnsupportedVersion { found, current }) => {
                return Err(AppError::Database(format!(
                    "设置存储 schema 版本 {} 高于当前支持版本 {}，请升级程序后重试",
                    found, current
                )));
            }
            Err(error) => {
                tracing::error!("设置文件解析失败，已隔离损坏文件并停止启动: {}", error);
                quarantine_corrupt_file(&self.path);
                return Err(AppError::Database(
                    "设置文件解析失败，已隔离损坏文件；请检查配置后重启".to_string(),
                ));
            }
        };
        backup_store_before_migration(&self.path, &content, decoded.source_version)?;
        let mut should_write = decoded.needs_write;
        *settings = decoded.data;
        if settings.strm_access_token.trim().is_empty() {
            settings.strm_access_token = uuid::Uuid::new_v4().to_string();
            should_write = true;
        }
        should_write |= ensure_browser_push_keys(&mut settings)?;
        if should_write {
            self.write_to_disk(&settings).await?;
        }
        Ok(())
    }

    async fn write_to_disk(&self, settings: &Settings) -> Result<()> {
        write_versioned_json_atomic_async(&self.path, settings, 0o600).await
    }

    pub async fn compact(&self) -> Result<()> {
        let _guard = self.save_lock.lock().await;
        let settings = self.settings.read().await.clone();
        self.write_to_disk(&settings).await
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
        let _save_guard = self.save_lock.lock().await;
        let updated = {
            let settings = self.settings.read().await;
            let mut settings = settings.clone();
            updater(&mut settings);
            settings.subscription_check_interval_minutes = normalize_check_interval_minutes(
                i64::from(settings.subscription_check_interval_minutes),
            );
            settings.subscription_check_max_concurrency =
                settings.subscription_check_max_concurrency.clamp(1, 32);
            settings.external_api_max_concurrency =
                settings.external_api_max_concurrency.clamp(1, 64);
            settings.job_max_concurrency = settings.job_max_concurrency.clamp(1, 32);
            settings.job_transfer_max_concurrency =
                settings.job_transfer_max_concurrency.clamp(1, 32);
            settings.job_metadata_max_concurrency =
                settings.job_metadata_max_concurrency.clamp(1, 32);
            settings.job_push_max_concurrency = settings.job_push_max_concurrency.clamp(1, 32);
            settings.aria2_batch_submit_limit = settings.aria2_batch_submit_limit.clamp(1, 100);
            settings.push_quiet_start_hour = settings.push_quiet_start_hour.min(23);
            settings.push_quiet_end_hour = settings.push_quiet_end_hour.min(23);
            settings.push_dedup_window_seconds =
                settings.push_dedup_window_seconds.clamp(0, 86_400);
            settings.push_digest_window_minutes =
                settings.push_digest_window_minutes.clamp(1, 1_440);
            if !matches!(
                settings.push_min_level.as_str(),
                "info" | "success" | "warning" | "error"
            ) {
                settings.push_min_level = "info".to_string();
            }
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
            settings
        };
        self.write_to_disk(&updated).await?;
        *self.settings.write().await = updated.clone();
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::schema::migration_backup_path;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "my-media-sub-{}-{}.json",
            name,
            uuid::Uuid::new_v4()
        ))
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
    async fn test_settings_store() {
        let tmp = temp_path("settings-store");
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

        let updated = store
            .update(|s| {
                s.subscription_check_max_concurrency = 0;
                s.external_api_max_concurrency = usize::MAX;
                s.job_max_concurrency = 0;
                s.job_transfer_max_concurrency = usize::MAX;
                s.job_metadata_max_concurrency = 0;
                s.job_push_max_concurrency = usize::MAX;
                s.aria2_batch_submit_limit = 0;
            })
            .await
            .unwrap();
        assert_eq!(updated.subscription_check_max_concurrency, 1);
        assert_eq!(updated.external_api_max_concurrency, 64);
        assert_eq!(updated.job_max_concurrency, 1);
        assert_eq!(updated.job_transfer_max_concurrency, 32);
        assert_eq!(updated.job_metadata_max_concurrency, 1);
        assert_eq!(updated.job_push_max_concurrency, 32);
        assert_eq!(updated.aria2_batch_submit_limit, 1);

        // 校验：无效云盘类型被过滤
        let updated = store
            .update(|s| s.cloud_types = vec!["invalid".to_string()])
            .await
            .unwrap();
        assert_eq!(updated.cloud_types, vec!["quark"]);

        // 持久化验证
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&tmp).unwrap()).unwrap();
        assert_eq!(persisted["schema_version"], 1);
        assert_eq!(persisted["data"]["app_username"], "lain");
        let store2 = SettingsStore::new(&tmp);
        store2.load().await.unwrap();
        assert_eq!(store2.get().await.app_username, "lain");

        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn load_migrates_legacy_settings_object_to_envelope() {
        let tmp = temp_path("settings-legacy");
        let legacy = Settings {
            app_username: "legacy-user".to_string(),
            strm_access_token: "existing-token".to_string(),
            ..Settings::default()
        };
        let original = serde_json::to_vec_pretty(&legacy).unwrap();
        std::fs::write(&tmp, &original).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644)).unwrap();
        }

        let store = SettingsStore::new(&tmp);
        store.load().await.unwrap();

        assert_eq!(store.get().await.app_username, "legacy-user");
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&tmp).unwrap()).unwrap();
        assert_eq!(persisted["schema_version"], 1);
        assert_eq!(persisted["data"]["app_username"], "legacy-user");
        assert_private_file_mode(&tmp);

        let backup = migration_backup_path(&tmp, 0);
        assert_eq!(std::fs::read(&backup).unwrap(), original);
        assert_private_file_mode(&backup);

        let _ = std::fs::remove_file(tmp);
        let _ = std::fs::remove_file(backup);
    }

    #[tokio::test]
    async fn future_settings_schema_is_preserved() {
        let tmp = temp_path("settings-future");
        let original = serde_json::json!({"schema_version": 99, "data": {}}).to_string();
        std::fs::write(&tmp, &original).unwrap();

        let store = SettingsStore::new(&tmp);
        let error = store.load().await.unwrap_err();

        assert!(matches!(error, AppError::Database(_)));
        assert_eq!(std::fs::read_to_string(&tmp).unwrap(), original);
        assert_private_file_mode(&tmp);

        let file_name = tmp.file_name().unwrap().to_string_lossy();
        let quarantined = std::fs::read_dir(tmp.parent().unwrap())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{}.corrupt-", file_name))
            });
        assert!(!quarantined);

        let _ = std::fs::remove_file(tmp);
    }

    #[tokio::test]
    async fn update_keeps_settings_unchanged_when_save_fails() {
        let blocker = temp_path("settings-save-blocker");
        std::fs::write(&blocker, b"not-a-directory").unwrap();
        let path = blocker.join("settings.json");
        let store = SettingsStore::new(&path);

        let before = store.get().await;
        let result = store
            .update(|settings| settings.app_username = "should-not-stick".to_string())
            .await;

        assert!(matches!(result, Err(AppError::Database(_))));
        assert_eq!(store.get().await.app_username, before.app_username);

        let _ = std::fs::remove_file(blocker);
    }

    #[tokio::test]
    async fn load_quarantines_corrupt_settings_file() {
        let tmp = temp_path("settings-corrupt");
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
    #[tokio::test]
    async fn generated_vapid_key_is_accepted_by_web_push() {
        let path = temp_path("vapid-key");
        let store = SettingsStore::new(&path);
        store.load().await.unwrap();
        let settings = store.get().await;
        let private = settings.browser_push_vapid_private_key.clone();
        let info = web_push::SubscriptionInfo::new(
            "https://updates.push.services.mozilla.com/wpush/v1/test",
            "BH1HTeKM7-NwaLGHEqxeu2IamQaVVLkcsFHPIHmsCnqxcBHPQBprF41bEMOr3O1hUQ2jU1opNEm1F_lZV_sxMP8",
            "sBXU5_tIYz-5w7G2B25BEw",
        );
        assert!(web_push::VapidSignatureBuilder::from_base64(&private, &info).is_ok());
        let _ = std::fs::remove_file(path);
    }
}
