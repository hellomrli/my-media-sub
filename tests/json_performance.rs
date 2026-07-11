use my_media_sub::services::storage::{evaluate_storage, StorageDecisionInput};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, Instant};

fn envelope(data: Value) -> Value {
    json!({"schema_version": 1, "data": data})
}

fn subscription(index: usize) -> Value {
    json!({
        "id": format!("sub-{index:04}"),
        "title": format!("基线媒体 {index}"),
        "media_type": "series",
        "season": 1,
        "url": format!("https://pan.quark.cn/s/fixture-{index}"),
        "created_at": 1,
        "updated_at": 1,
        "known_files": [format!("Show.S01E{:02}.mkv", index % 24 + 1)],
        "known_episodes": [index % 24 + 1]
    })
}

fn job(index: usize) -> Value {
    json!({
        "id": format!("job-{index:05}"),
        "kind": if index.is_multiple_of(2) { "subscription_transfer" } else { "metadata_scrape" },
        "status": if index.is_multiple_of(20) { "failed" } else { "succeeded" },
        "progress": 100,
        "title": "规模基线任务",
        "message": "任务执行完成，用于测量 JSON 解析、序列化和索引开销",
        "payload": {"subscription_id": format!("sub-{:04}", index % 500)},
        "created_at": 1,
        "updated_at": 2
    })
}

fn notification(index: usize) -> Value {
    json!({
        "id": format!("notification-{index:05}"),
        "level": if index.is_multiple_of(20) { "error" } else { "info" },
        "event": "performance_baseline",
        "title": "性能基线通知",
        "message": "用于验证一万条通知的紧凑 JSON 大小和解析耗时",
        "meta": {"subscription_id": format!("sub-{:04}", index % 500)},
        "read": index.is_multiple_of(3),
        "created_at": 1
    })
}

fn automation_event(index: usize) -> Value {
    json!({
        "id": format!("event-{index:05}"),
        "correlation_id": format!("correlation-{:04}", index % 500),
        "subscription_id": format!("sub-{:04}", index % 500),
        "job_id": format!("job-{index:05}"),
        "stage": "source_check",
        "status": if index.is_multiple_of(20) { "failed" } else { "succeeded" },
        "attempt": 1,
        "message": "自动化阶段性能基线",
        "error": if index.is_multiple_of(20) { "fixture failure" } else { "" },
        "metadata": {},
        "created_at": 1,
        "updated_at": 2
    })
}

fn measure(name: &str, value: &Value) -> (usize, Duration, Duration, Value) {
    let write_started = Instant::now();
    let bytes = serde_json::to_vec(value).expect("serialize baseline");
    let write_duration = write_started.elapsed();
    let parse_started = Instant::now();
    let parsed: Value = serde_json::from_slice(&bytes).expect("parse baseline");
    let parse_duration = parse_started.elapsed();
    println!(
        "P9 baseline {name}: {} records, {} bytes, serialize {:?}, parse {:?}",
        value["data"].as_array().map(Vec::len).unwrap_or_default(),
        bytes.len(),
        write_duration,
        parse_duration
    );
    (bytes.len(), write_duration, parse_duration, parsed)
}

#[test]
fn json_scale_baseline_500_subscriptions_and_10000_histories() {
    let subscriptions = envelope((0..500).map(subscription).collect());
    let jobs = envelope((0..10_000).map(job).collect());
    let notifications = envelope((0..10_000).map(notification).collect());
    let events = envelope((0..10_000).map(automation_event).collect());

    let started = Instant::now();
    let reports = [
        measure("subscriptions", &subscriptions),
        measure("jobs", &jobs),
        measure("notifications", &notifications),
        measure("automation_events", &events),
    ];
    assert!(started.elapsed() < Duration::from_secs(15));
    assert!(reports.iter().all(|(bytes, write, parse, _)| {
        *bytes < 32 * 1024 * 1024
            && *write < Duration::from_secs(5)
            && *parse < Duration::from_secs(5)
    }));

    let index_started = Instant::now();
    let subscription_index: HashMap<_, _> = reports[0].3["data"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, item)| (item["id"].as_str().unwrap().to_string(), index))
        .collect();
    let job_index: HashMap<_, _> = reports[1].3["data"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, item)| (item["id"].as_str().unwrap().to_string(), index))
        .collect();
    assert_eq!(subscription_index.get("sub-0499"), Some(&499));
    assert_eq!(job_index.get("job-09999"), Some(&9_999));
    assert!(index_started.elapsed() < Duration::from_secs(2));

    let largest_store_bytes = reports.iter().map(|report| report.0 as u64).max().unwrap();
    let decision = evaluate_storage(StorageDecisionInput {
        subscriptions: 500,
        jobs: 10_000,
        notifications: 10_000,
        automation_events: 10_000,
        largest_store_bytes,
        complex_query_required: false,
    });
    assert!(
        decision.sqlite_required,
        "stress fixture must cross the explicit migration gate"
    );
}

#[test]
fn compact_store_encoding_is_smaller_than_pretty_encoding() {
    let value = envelope((0..500).map(subscription).collect());
    let compact = serde_json::to_vec(&value).unwrap();
    let pretty = serde_json::to_vec_pretty(&value).unwrap();
    assert!(compact.len() < pretty.len());
    assert!(compact.len() * 100 / pretty.len() < 90);
}
