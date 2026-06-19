use std::sync::Arc;

use crate::clients::PanSouClient;
use crate::config::Config;
use crate::error::Result;
use crate::jobs::{JobQueue, JobStore};
use crate::services::{
    MetadataService, SubscriptionCheckService, SubscriptionScheduler, SubscriptionTransferService,
};
use crate::store::{NotificationStore, SettingsStore, SubscriptionStore};

/// 应用级依赖上下文。
///
/// 所有长期存活的 store、client、service 都在这里初始化并复用，避免不同路由
/// 重复创建业务服务。后续异步 worker、SSE 进度流或任务队列也应从这里挂接。
pub struct AppContext {
    pub subscription_store: Arc<SubscriptionStore>,
    pub settings_store: Arc<SettingsStore>,
    pub notification_store: Arc<NotificationStore>,
    pub job_store: Arc<JobStore>,
    pub job_queue: Arc<JobQueue>,
    pub pansou_client: Arc<PanSouClient>,
    pub metadata_service: Arc<MetadataService>,
    pub transfer_service: Arc<SubscriptionTransferService>,
    pub check_service: Arc<SubscriptionCheckService>,
    pub scheduler: Arc<SubscriptionScheduler>,
}

impl AppContext {
    pub async fn new(config: &Config) -> Result<Arc<Self>> {
        let subscription_store = Arc::new(SubscriptionStore::new(
            config.data_dir.join("subscriptions.json"),
        ));
        subscription_store.load().await?;
        tracing::info!(
            "✅ Loaded {} subscriptions",
            subscription_store.count().await
        );

        let settings_store = Arc::new(SettingsStore::new(config.data_dir.join("settings.json")));
        settings_store.load().await?;
        apply_env_overrides(&settings_store).await?;
        tracing::info!("✅ Settings loaded");

        let notification_store = Arc::new(NotificationStore::new(
            config.data_dir.join("notifications.json"),
        ));
        notification_store.load().await?;
        tracing::info!("✅ Loaded notifications");

        let job_store = Arc::new(JobStore::new(config.data_dir.join("jobs.json")));
        job_store.load().await?;
        tracing::info!("✅ Loaded jobs");

        let settings = settings_store.get().await;
        let pansou_api_url = settings.pansou_api_url.trim().to_string();
        let pansou_api_url = if pansou_api_url.is_empty() {
            None
        } else {
            Some(pansou_api_url)
        };
        let pansou_client = Arc::new(PanSouClient::new(pansou_api_url));
        let metadata_service = Arc::new(MetadataService::new());
        tracing::info!("✅ Clients initialized");

        let transfer_service = Arc::new(SubscriptionTransferService::new(
            subscription_store.clone(),
            settings_store.clone(),
            notification_store.clone(),
        ));

        let job_queue = Arc::new(JobQueue::new(
            job_store.clone(),
            settings_store.clone(),
            subscription_store.clone(),
            notification_store.clone(),
            metadata_service.clone(),
            transfer_service.clone(),
        ));

        let check_service = Arc::new(
            SubscriptionCheckService::new(
                subscription_store.clone(),
                settings_store.clone(),
                notification_store.clone(),
            )
            .with_job_queue(job_queue.clone()),
        );

        let scheduler = Arc::new(
            SubscriptionScheduler::new(check_service.clone(), settings_store.clone()).await?,
        );

        Ok(Arc::new(Self {
            subscription_store,
            settings_store,
            notification_store,
            job_store,
            job_queue,
            pansou_client,
            metadata_service,
            transfer_service,
            check_service,
            scheduler,
        }))
    }

    pub async fn start_background_services(&self) -> Result<()> {
        self.scheduler.start().await?;
        tracing::info!("✅ Services initialized");
        Ok(())
    }
}

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

const SETTINGS_ENV_KEYS: &[&str] = &[
    "APP_USERNAME",
    "SERVER_USERNAME",
    "APP_PASSWORD",
    "SERVER_PASSWORD",
    "QUARK_COOKIE",
    "WECOM_BOT_URL",
    "WXPUSHER_APP_TOKEN",
    "WXPUSHER_UIDS",
    "TELEGRAM_BOT_TOKEN",
    "TELEGRAM_CHAT_ID",
    "BARK_URL",
    "GOTIFY_URL",
    "GOTIFY_TOKEN",
    "PUSHPLUS_TOKEN",
    "SERVERCHAN_KEY",
    "ARIA2_RPC_URL",
    "ARIA2_SECRET",
    "ARIA2_MOVIE_DIR",
    "ARIA2_SERIES_DIR",
    "ARIA2_ANIME_DIR",
    "STRM_ENABLED",
    "STRM_OUTPUT_DIR",
    "STRM_PUBLIC_BASE_URL",
    "STRM_ACCESS_TOKEN",
    "TMDB_API_KEY",
    "TMDB_LANGUAGE",
    "PANSOU_API_URL",
];

async fn apply_env_overrides(settings_store: &SettingsStore) -> Result<()> {
    if !SETTINGS_ENV_KEYS
        .iter()
        .any(|key| env_non_empty(key).is_some())
    {
        return Ok(());
    }

    settings_store
        .update(|settings| {
            if let Some(value) =
                env_non_empty("APP_USERNAME").or_else(|| env_non_empty("SERVER_USERNAME"))
            {
                settings.app_username = value;
            }
            if let Some(value) =
                env_non_empty("APP_PASSWORD").or_else(|| env_non_empty("SERVER_PASSWORD"))
            {
                settings.app_password = value;
            }
            if let Some(value) = env_non_empty("QUARK_COOKIE") {
                settings.quark_cookie = value;
            }
            if let Some(value) = env_non_empty("WECOM_BOT_URL") {
                settings.wecom_bot_url = value;
            }
            if let Some(value) = env_non_empty("WXPUSHER_APP_TOKEN") {
                settings.wxpusher_app_token = value;
            }
            if let Some(value) = env_non_empty("WXPUSHER_UIDS") {
                settings.wxpusher_uids = value;
            }
            if let Some(value) = env_non_empty("TELEGRAM_BOT_TOKEN") {
                settings.telegram_bot_token = value;
            }
            if let Some(value) = env_non_empty("TELEGRAM_CHAT_ID") {
                settings.telegram_chat_id = value;
            }
            if let Some(value) = env_non_empty("BARK_URL") {
                settings.bark_url = value;
            }
            if let Some(value) = env_non_empty("GOTIFY_URL") {
                settings.gotify_url = value;
            }
            if let Some(value) = env_non_empty("GOTIFY_TOKEN") {
                settings.gotify_token = value;
            }
            if let Some(value) = env_non_empty("PUSHPLUS_TOKEN") {
                settings.pushplus_token = value;
            }
            if let Some(value) = env_non_empty("SERVERCHAN_KEY") {
                settings.serverchan_key = value;
            }
            if let Some(value) = env_non_empty("ARIA2_RPC_URL") {
                settings.aria2_rpc_url = value;
            }
            if let Some(value) = env_non_empty("ARIA2_SECRET") {
                settings.aria2_secret = value;
            }
            if let Some(value) = env_non_empty("ARIA2_MOVIE_DIR") {
                settings.aria2_movie_dir = value;
            }
            if let Some(value) = env_non_empty("ARIA2_SERIES_DIR") {
                settings.aria2_series_dir = value;
            }
            if let Some(value) = env_non_empty("ARIA2_ANIME_DIR") {
                settings.aria2_anime_dir = value;
            }
            if let Some(value) = env_non_empty("STRM_ENABLED") {
                settings.strm_enabled = parse_bool_env(&value);
            }
            if let Some(value) = env_non_empty("STRM_OUTPUT_DIR") {
                settings.strm_output_dir = value;
            }
            if let Some(value) = env_non_empty("STRM_PUBLIC_BASE_URL") {
                settings.strm_public_base_url = value;
            }
            if let Some(value) = env_non_empty("STRM_ACCESS_TOKEN") {
                settings.strm_access_token = value;
            }
            if let Some(value) = env_non_empty("TMDB_API_KEY") {
                settings.tmdb_api_key = value;
            }
            if let Some(value) = env_non_empty("TMDB_LANGUAGE") {
                settings.tmdb_language = value;
            }
            if let Some(value) = env_non_empty("PANSOU_API_URL") {
                settings.pansou_api_url = value;
            }
        })
        .await?;

    Ok(())
}

fn parse_bool_env(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use tokio::sync::Mutex;

    use super::*;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn preserve_env() -> Vec<(&'static str, Option<String>)> {
        SETTINGS_ENV_KEYS
            .iter()
            .map(|key| {
                let previous = std::env::var(key).ok();
                std::env::remove_var(key);
                (*key, previous)
            })
            .collect()
    }

    fn restore_env(previous: Vec<(&'static str, Option<String>)>) {
        for (key, value) in previous {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }

    #[tokio::test]
    async fn apply_env_overrides_applies_non_empty_values() {
        let _guard = env_lock().lock().await;
        let previous = preserve_env();
        std::env::set_var("APP_USERNAME", "env-user");
        std::env::set_var("APP_PASSWORD", "env-password");

        let path = std::env::temp_dir().join(format!(
            "my_media_sub_settings_env_override_{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = SettingsStore::new(&path);
        store.load().await.unwrap();

        apply_env_overrides(&store).await.unwrap();

        let settings = store.get().await;
        assert_eq!(settings.app_username, "env-user");
        assert_eq!(settings.app_password, "env-password");

        restore_env(previous);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn apply_env_overrides_applies_pansou_api_url_by_itself() {
        let _guard = env_lock().lock().await;
        let previous = preserve_env();
        std::env::set_var("PANSOU_API_URL", "https://example.test");

        let path = std::env::temp_dir().join(format!(
            "my_media_sub_settings_pansou_env_override_{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = SettingsStore::new(&path);
        store.load().await.unwrap();

        apply_env_overrides(&store).await.unwrap();

        let settings = store.get().await;
        assert_eq!(settings.pansou_api_url, "https://example.test");

        restore_env(previous);
        let _ = std::fs::remove_file(path);
    }
}
