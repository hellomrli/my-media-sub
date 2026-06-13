use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 通知（与 Python JSON 完全兼容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// 通知 ID
    pub id: String,

    /// 级别（info / success / warning / error）
    #[serde(default = "default_level")]
    pub level: String,

    /// 事件类型（如 subscription_updated）
    #[serde(default)]
    pub event: String,

    /// 标题
    pub title: String,

    /// 消息内容
    pub message: String,

    /// 元数据
    #[serde(default)]
    pub meta: HashMap<String, serde_json::Value>,

    /// 是否已读
    #[serde(default)]
    pub read: bool,

    /// 创建时间
    pub created_at: i64,
}

fn default_level() -> String {
    "info".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_serialize() {
        let mut meta = HashMap::new();
        meta.insert(
            "subscription_id".to_string(),
            serde_json::Value::String("sub123".to_string()),
        );

        let notif = Notification {
            id: "notif123".to_string(),
            level: "info".to_string(),
            event: "subscription_updated".to_string(),
            title: "订阅更新".to_string(),
            message: "发现新资源".to_string(),
            meta,
            read: false,
            created_at: 1718323200,
        };

        let json = serde_json::to_string_pretty(&notif).unwrap();
        println!("{}", json);

        // 验证能反序列化
        let parsed: Notification = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.level, "info");
        assert_eq!(parsed.event, "subscription_updated");
    }
}
