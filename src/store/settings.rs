use crate::error::{AppError, Result};
use crate::models::Settings;
use std::path::PathBuf;
use tokio::sync::RwLock;

/// 受保护的密钥字段（public() 视图中会被脱敏）
pub const SECRET_KEYS: &[&str] = &[
    "app_password",
    "aria2_secret",
    "quark_cookie",
    "wxpusher_app_token",
    "telegram_bot_token",
    "gotify_token",
    "pushplus_token",
    "serverchan_key",
];

/// 支持的云盘类型
pub const SUPPORTED_CLOUD_TYPES: &[&str] = &[
    "quark", "baidu", "aliyun", "uc", "tianyi", "mobile", "115", "pikpak", "xunlei", "123",
    "magnet", "ed2k", "others",
];

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
        // 容错：解析失败时保留默认值（与 Python 行为一致）
        match serde_json::from_str::<Settings>(&content) {
            Ok(s) => *settings = s,
            Err(e) => {
                tracing::warn!("设置文件解析失败，使用默认值: {}", e);
            }
        }
        Ok(())
    }

    async fn write_to_disk(&self, settings: &Settings) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::Database(format!("创建目录失败: {}", e)))?;
        }
        let content = serde_json::to_string_pretty(settings)
            .map_err(|e| AppError::Database(format!("序列化设置失败: {}", e)))?;
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, content)
            .map_err(|e| AppError::Database(format!("写入临时文件失败: {}", e)))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|e| AppError::Database(format!("重命名临时文件失败: {}", e)))?;
        Ok(())
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
        // 校验：检查间隔最小 5 分钟
        if settings.subscription_check_interval_minutes < 5 {
            settings.subscription_check_interval_minutes = 5;
        }
        // 校验：cloud_types 只保留支持的类型，为空则默认 quark
        settings
            .cloud_types
            .retain(|t| SUPPORTED_CLOUD_TYPES.contains(&t.as_str()));
        if settings.cloud_types.is_empty() {
            settings.cloud_types = vec!["quark".to_string()];
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
}
