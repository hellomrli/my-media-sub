// 用真实 Python 数据验证反序列化兼容性的集成测试
#![allow(unused_imports)]

use std::fs;

#[path = "../src/models/mod.rs"]
mod models;

#[allow(dead_code)]
#[path = "../src/jobs/model.rs"]
mod job_model;

#[test]
fn test_deserialize_real_subscriptions() {
    let Ok(content) = fs::read_to_string("data/subscriptions.json") else {
        eprintln!("跳过真实订阅数据兼容性测试：data/subscriptions.json 不存在");
        return;
    };

    let subs: Vec<models::Subscription> =
        serde_json::from_str(&content).expect("应该能反序列化真实订阅数据");

    println!("✅ 成功反序列化 {} 个订阅", subs.len());
    assert!(!subs.is_empty(), "应该至少有一个订阅");

    // 验证第一个订阅的字段
    let first = &subs[0];
    println!("   第一个订阅: {} ({})", first.title, first.id);

    // 重新序列化并再次反序列化（往返测试）
    let json = serde_json::to_string_pretty(&subs).unwrap();
    let _reparsed: Vec<models::Subscription> =
        serde_json::from_str(&json).expect("往返序列化应该成功");
}

#[test]
fn test_deserialize_real_settings() {
    let Ok(content) = fs::read_to_string("data/settings.json") else {
        eprintln!("跳过真实设置数据兼容性测试：data/settings.json 不存在");
        return;
    };

    let settings: models::Settings =
        serde_json::from_str(&content).expect("应该能反序列化真实设置数据");

    println!("✅ 成功反序列化设置");
    println!("   用户名: {}", settings.app_username);
    println!("   云盘类型: {:?}", settings.cloud_types);

    // 往返测试
    let json = serde_json::to_string_pretty(&settings).unwrap();
    let _reparsed: models::Settings = serde_json::from_str(&json).expect("往返序列化应该成功");
}

#[test]
fn test_deserialize_real_notifications() {
    let Ok(content) = fs::read_to_string("data/notifications.json") else {
        eprintln!("跳过真实通知数据兼容性测试：data/notifications.json 不存在");
        return;
    };

    let notifs: Vec<models::Notification> =
        serde_json::from_str(&content).expect("应该能反序列化真实通知数据");

    println!("✅ 成功反序列化 {} 条通知", notifs.len());
    assert!(!notifs.is_empty());
    println!("   第一条: {} ({})", notifs[0].title, notifs[0].level);

    // 往返测试
    let json = serde_json::to_string_pretty(&notifs).unwrap();
    let _reparsed: Vec<models::Notification> =
        serde_json::from_str(&json).expect("往返序列化应该成功");
}

#[test]
fn test_deserialize_jobs_fixture_with_all_current_kinds() {
    let content =
        fs::read_to_string("tests/fixtures/jobs.json").expect("应该能读取任务兼容性测试样例");

    let jobs: Vec<job_model::Job> = serde_json::from_str(&content).expect("应该能反序列化任务样例");

    assert_eq!(jobs.len(), 3);
    assert!(jobs
        .iter()
        .any(|job| job.kind == job_model::JobKind::ManualTransfer));
    assert!(jobs
        .iter()
        .any(|job| job.kind == job_model::JobKind::SubscriptionTransfer));
    assert!(jobs
        .iter()
        .any(|job| job.kind == job_model::JobKind::MetadataScrape));

    let json = serde_json::to_string_pretty(&jobs).unwrap();
    let _reparsed: Vec<job_model::Job> =
        serde_json::from_str(&json).expect("任务样例往返序列化应该成功");
}
