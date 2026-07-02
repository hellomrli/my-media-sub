use crate::error::{AppError, Result};
use serde::Deserialize;
use std::path::PathBuf;

/// 应用配置
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// 服务器配置
    pub server: ServerConfig,
    /// 数据目录
    pub data_dir: PathBuf,
}

/// 服务器配置
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// 监听地址
    pub host: String,
    /// 监听端口
    pub port: u16,
    /// HTTP Basic Auth 用户名
    pub username: String,
    /// HTTP Basic Auth 密码
    pub password: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 56001,
                username: "admin".to_string(),
                password: "change-me".to_string(),
            },
            data_dir: PathBuf::from("./data"),
        }
    }
}

impl Config {
    /// 从环境变量加载配置
    pub fn load() -> Result<Self> {
        let mut config = Self::default();

        // 环境变量覆盖
        if let Ok(host) = std::env::var("SERVER_HOST") {
            config.server.host = host;
        }
        if let Ok(port) = std::env::var("SERVER_PORT") {
            config.server.port = port
                .parse()
                .map_err(|e| AppError::Config(format!("Invalid SERVER_PORT: {}", e)))?;
        }
        if let Ok(username) =
            std::env::var("APP_USERNAME").or_else(|_| std::env::var("SERVER_USERNAME"))
        {
            config.server.username = username;
        }
        if let Ok(password) =
            std::env::var("APP_PASSWORD").or_else(|_| std::env::var("SERVER_PASSWORD"))
        {
            config.server.password = password;
        }
        if let Ok(data_dir) = std::env::var("DATA_DIR") {
            config.data_dir = PathBuf::from(data_dir);
        }

        Ok(config)
    }
}
