// 反序列化兼容性测试：用 tests/fixtures/ 中的最小样本确保 CI 中每次都执行。
// 若 data/*.json 存在（本机开发环境），同时验证真实数据，以便捕获 schema 漂移。
#![allow(unused_imports)]

use std::fs;

#[path = "../src/models/mod.rs"]
mod models;

#[allow(dead_code)]
#[path = "../src/jobs/model.rs"]
mod job_model;

// ─── fixture 测试（总是运行）───────────────────────────────────────────────

#[test]
fn test_deserialize_subscriptions_fixture() {
    let content = fs::read_to_string("tests/fixtures/subscriptions.json")
        .expect("应该能读取订阅兼容性测试样例");

    let subs: Vec<models::Subscription> =
        serde_json::from_str(&content).expect("应该能反序列化订阅样例");

    assert_eq!(subs.len(), 2);
    assert_eq!(subs[0].id, "fixture-sub-001");
    assert_eq!(subs[1].media_type, "movie");

    let json = serde_json::to_string_pretty(&subs).unwrap();
    let _reparsed: Vec<models::Subscription> =
        serde_json::from_str(&json).expect("订阅样例往返序列化应该成功");
}

#[test]
fn test_deserialize_settings_fixture() {
    let content =
        fs::read_to_string("tests/fixtures/settings.json").expect("应该能读取设置兼容性测试样例");

    let settings: models::Settings =
        serde_json::from_str(&content).expect("应该能反序列化设置样例");

    assert_eq!(settings.app_username, "admin");
    assert_eq!(settings.cloud_types, vec!["quark"]);

    let json = serde_json::to_string_pretty(&settings).unwrap();
    let _reparsed: models::Settings =
        serde_json::from_str(&json).expect("设置样例往返序列化应该成功");
}

#[test]
fn test_deserialize_notifications_fixture() {
    let content = fs::read_to_string("tests/fixtures/notifications.json")
        .expect("应该能读取通知兼容性测试样例");

    let notifs: Vec<models::Notification> =
        serde_json::from_str(&content).expect("应该能反序列化通知样例");

    assert_eq!(notifs.len(), 2);
    assert_eq!(notifs[0].level, "success");

    let json = serde_json::to_string_pretty(&notifs).unwrap();
    let _reparsed: Vec<models::Notification> =
        serde_json::from_str(&json).expect("通知样例往返序列化应该成功");
}

#[test]
fn test_deserialize_jobs_fixture_with_all_current_kinds() {
    let content =
        fs::read_to_string("tests/fixtures/jobs.json").expect("应该能读取任务兼容性测试样例");

    let jobs: Vec<job_model::Job> = serde_json::from_str(&content).expect("应该能反序列化任务样例");

    assert_eq!(jobs.len(), 4);
    assert!(jobs
        .iter()
        .any(|job| job.kind == job_model::JobKind::ManualTransfer));
    assert!(jobs
        .iter()
        .any(|job| job.kind == job_model::JobKind::SubscriptionTransfer));
    assert!(jobs
        .iter()
        .any(|job| job.kind == job_model::JobKind::MetadataScrape));
    assert!(jobs
        .iter()
        .any(|job| job.kind == job_model::JobKind::PushDispatch));

    let json = serde_json::to_string_pretty(&jobs).unwrap();
    let _reparsed: Vec<job_model::Job> =
        serde_json::from_str(&json).expect("任务样例往返序列化应该成功");
}

// ─── 真实数据测试（仅本机有数据时运行）──────────────────────────────────────

#[test]
fn test_deserialize_real_subscriptions_if_present() {
    let Ok(content) = fs::read_to_string("data/subscriptions.json") else {
        return; // CI 中无真实数据，跳过
    };

    let subs: Vec<models::Subscription> =
        serde_json::from_str(&content).expect("应该能反序列化真实订阅数据");

    assert!(!subs.is_empty(), "应该至少有一个订阅");
    let json = serde_json::to_string_pretty(&subs).unwrap();
    let _reparsed: Vec<models::Subscription> =
        serde_json::from_str(&json).expect("往返序列化应该成功");
}

#[test]
fn test_deserialize_real_settings_if_present() {
    let Ok(content) = fs::read_to_string("data/settings.json") else {
        return;
    };

    let settings: models::Settings =
        serde_json::from_str(&content).expect("应该能反序列化真实设置数据");

    let json = serde_json::to_string_pretty(&settings).unwrap();
    let _reparsed: models::Settings = serde_json::from_str(&json).expect("往返序列化应该成功");
}

#[test]
fn test_deserialize_real_notifications_if_present() {
    let Ok(content) = fs::read_to_string("data/notifications.json") else {
        return;
    };

    let notifs: Vec<models::Notification> =
        serde_json::from_str(&content).expect("应该能反序列化真实通知数据");

    assert!(!notifs.is_empty());
    let json = serde_json::to_string_pretty(&notifs).unwrap();
    let _reparsed: Vec<models::Notification> =
        serde_json::from_str(&json).expect("往返序列化应该成功");
}
