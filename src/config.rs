use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::error::{AppError, Result};

/// 应用配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 服务器配置
    pub server: ServerConfig,
    /// 夸克网盘配置
    pub quark: QuarkConfig,
    /// 推送配置
    pub push: PushConfig,
    /// 数据目录
    pub data_dir: PathBuf,
}

/// 服务器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// 监听地址
    pub host: String,
    /// 监听端口
    pub port: u16,
}

/// 夸克网盘配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarkConfig {
    /// Cookie
    #[serde(default)]
    pub cookie: String,
    /// 保存根目录 fid
    #[serde(default)]
    pub save_root_fid: String,
}

/// 推送配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushConfig {
    /// 是否启用推送
    #[serde(default)]
    pub enabled: bool,
    /// Telegram 配置
    #[serde(default)]
    pub telegram: Option<TelegramConfig>,
    /// Bark 配置
    #[serde(default)]
    pub bark: Option<BarkConfig>,
}

/// Telegram 推送配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
}

/// Bark 推送配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarkConfig {
    pub device_key: String,
    pub server_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 50001,
            },
            quark: QuarkConfig {
                cookie: String::new(),
                save_root_fid: String::new(),
            },
            push: PushConfig {
                enabled: false,
                telegram: None,
                bark: None,
            },
            data_dir: PathBuf::from("data"),
        }
    }
}

impl Config {
    /// 从文件加载配置
    pub fn from_file(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| AppError::Config(format!("Failed to read config file: {}", e)))?;
        
        let config: Config = serde_json::from_str(&content)
            .map_err(|e| AppError::Config(format!("Failed to parse config: {}", e)))?;
        
        Ok(config)
    }

    /// 保存配置到文件
    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| AppError::Config(format!("Failed to serialize config: {}", e)))?;
        
        std::fs::write(path, content)
            .map_err(|e| AppError::Config(format!("Failed to write config file: {}", e)))?;
        
        Ok(())
    }

    /// 从环境变量和文件加载配置
    pub fn load() -> Result<Self> {
        // 尝试从配置文件加载
        let config_path = std::env::var("CONFIG_FILE")
            .unwrap_or_else(|_| "data/config.json".to_string());

        let mut config = if std::path::Path::new(&config_path).exists() {
            Self::from_file(&config_path)?
        } else {
            tracing::info!("Config file not found, using defaults");
            Self::default()
        };

        // 环境变量覆盖
        if let Ok(host) = std::env::var("SERVER_HOST") {
            config.server.host = host;
        }
        if let Ok(port) = std::env::var("SERVER_PORT") {
            config.server.port = port.parse()
                .map_err(|e| AppError::Config(format!("Invalid PORT: {}", e)))?;
        }
        if let Ok(cookie) = std::env::var("QUARK_COOKIE") {
            config.quark.cookie = cookie;
        }

        Ok(config)
    }
}
