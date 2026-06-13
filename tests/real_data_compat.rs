// 用真实 Python 数据验证反序列化兼容性的集成测试
use std::fs;

#[path = "../src/models/mod.rs"]
mod models;

#[test]
fn test_deserialize_real_subscriptions() {
    let content = fs::read_to_string("data/subscriptions.json")
        .expect("应该能读取 data/subscriptions.json");

    let subs: Vec<models::Subscription> = serde_json::from_str(&content)
        .expect("应该能反序列化真实订阅数据");

    println!("✅ 成功反序列化 {} 个订阅", subs.len());
    assert!(!subs.is_empty(), "应该至少有一个订阅");

    // 验证第一个订阅的字段
    let first = &subs[0];
    println!("   第一个订阅: {} ({})", first.title, first.id);

    // 重新序列化并再次反序列化（往返测试）
    let json = serde_json::to_string_pretty(&subs).unwrap();
    let _reparsed: Vec<models::Subscription> = serde_json::from_str(&json)
        .expect("往返序列化应该成功");
}

#[test]
fn test_deserialize_real_settings() {
    let content = fs::read_to_string("data/settings.json")
        .expect("应该能读取 data/settings.json");

    let settings: models::Settings = serde_json::from_str(&content)
        .expect("应该能反序列化真实设置数据");

    println!("✅ 成功反序列化设置");
    println!("   用户名: {}", settings.app_username);
    println!("   云盘类型: {:?}", settings.cloud_types);

    // 往返测试
    let json = serde_json::to_string_pretty(&settings).unwrap();
    let _reparsed: models::Settings = serde_json::from_str(&json)
        .expect("往返序列化应该成功");
}

#[test]
fn test_deserialize_real_notifications() {
    let content = fs::read_to_string("data/notifications.json")
        .expect("应该能读取 data/notifications.json");

    let notifs: Vec<models::Notification> = serde_json::from_str(&content)
        .expect("应该能反序列化真实通知数据");

    println!("✅ 成功反序列化 {} 条通知", notifs.len());
    assert!(!notifs.is_empty());
    println!("   第一条: {} ({})", notifs[0].title, notifs[0].level);

    // 往返测试
    let json = serde_json::to_string_pretty(&notifs).unwrap();
    let _reparsed: Vec<models::Notification> = serde_json::from_str(&json)
        .expect("往返序列化应该成功");
}
