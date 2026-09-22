//! `api::update` 的单元测试。
//!
//! 原本内联在母文件里（317 行），外移后母文件只保留生产代码。
//! `use super::*;` 的解析目标与原先一致（都指向 `api::update`）。

use super::*;

fn asset(name: &str) -> GithubAsset {
    GithubAsset {
        name: name.to_string(),
        size: 42,
        browser_download_url: format!("https://example.com/{}", name),
    }
}

#[test]
fn test_version_compare_handles_tags() {
    assert!(is_newer_version("v0.7.15", "0.7.14"));
    assert!(is_newer_version("0.8.0", "0.7.99"));
    assert!(!is_newer_version("0.7.14", "0.7.14"));
    assert!(!is_newer_version("0.7.13", "0.7.14"));
}

#[test]
fn test_find_release_assets() {
    let assets = vec![
        asset("my-media-sub-v0.7.15-linux-x86_64.tar.gz"),
        asset("my-media-sub-v0.7.15-linux-x86_64.tar.gz.sha256"),
    ];

    let archive = find_asset(&assets, "linux-x86_64.tar.gz").unwrap();
    let checksum = find_asset(&assets, "linux-x86_64.tar.gz.sha256").unwrap();

    assert_eq!(archive.name, "my-media-sub-v0.7.15-linux-x86_64.tar.gz");
    assert_eq!(
        checksum.name,
        "my-media-sub-v0.7.15-linux-x86_64.tar.gz.sha256"
    );
}

#[test]
fn test_parse_sha256_checksum_accepts_common_formats() {
    let checksum = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let asset_name = "archive.tar.gz";
    assert_eq!(
        parse_sha256_checksum(
            format!("{}  {}\n", checksum, asset_name).as_bytes(),
            asset_name
        ),
        Some(checksum.to_string())
    );
    assert_eq!(
        parse_sha256_checksum(
            format!("{} *{}\n", checksum.to_ascii_uppercase(), asset_name).as_bytes(),
            asset_name
        ),
        Some(checksum.to_string())
    );
    assert_eq!(
        parse_sha256_checksum(
            format!("SHA256 ({}) = {}\n", asset_name, checksum).as_bytes(),
            asset_name
        ),
        Some(checksum.to_string())
    );
    assert_eq!(
        parse_sha256_checksum(
            format!("{}\n", checksum.to_ascii_uppercase()).as_bytes(),
            asset_name
        ),
        Some(checksum.to_string())
    );
    assert_eq!(
        parse_sha256_checksum(
            format!("{}  other.tar.gz\n{}  another.tar.gz\n", checksum, checksum).as_bytes(),
            asset_name
        ),
        None
    );
    assert_eq!(parse_sha256_checksum(b"not-a-checksum", asset_name), None);
}

#[test]
fn test_release_response_marks_current_and_newer() {
    let release = GithubRelease {
        tag_name: "v0.9.1".to_string(),
        name: None,
        html_url: "https://example.com/release".to_string(),
        body: None,
        published_at: None,
        assets: vec![asset("my-media-sub-v0.9.1-linux-x86_64.tar.gz")],
    };
    let current = release_to_response(release.clone(), "0.9.1");
    let newer = release_to_response(release, "0.9.0");

    assert!(current.is_current);
    assert!(!current.is_newer);
    assert!(!newer.is_current);
    assert!(newer.is_newer);
    assert!(newer.asset.is_some());
}

#[test]
fn test_online_update_requires_managed_docker_runtime() {
    assert!(online_update_supported_for("binary", None, false));
    assert!(!online_update_supported_for("binary", Some(false), false));
    assert!(!online_update_supported_for("docker", None, true));
    assert!(!online_update_supported_for("docker", Some(false), true));
    assert!(!online_update_supported_for("docker", Some(true), false));
    assert!(online_update_supported_for("docker", Some(true), true));
    assert!(!online_update_supported_for("unknown", Some(true), true));
}

/// 同一进程内不得并发跑两次升级：第二次 apply 必须被拦在下载之前，
/// 否则两个任务会同时改写同一个二进制和 static 目录。
#[test]
fn concurrent_update_attempts_are_rejected_and_progress_recovers() {
    // 这些断言操作进程级的 UPDATE_PROGRESS 单例，结束前必须复位。
    assert!(try_begin_update_progress("第一次升级").is_ok());
    let running = current_update_progress();
    assert!(running.running);
    assert_eq!(running.stage, "starting");

    let rejected = try_begin_update_progress("第二次升级").unwrap_err();
    assert!(matches!(rejected, AppError::Validation(_)));
    assert!(rejected.to_string().contains("已有升级任务正在执行"));

    // 失败后必须回到非 running，否则后续升级会被永久拒绝。
    fail_update_progress("模拟失败".to_string());
    let failed = current_update_progress();
    assert!(!failed.running);
    assert_eq!(failed.error.as_deref(), Some("模拟失败"));

    assert!(try_begin_update_progress("失败后重试").is_ok());
    finish_update_progress("已复位", "idle");
    assert!(!current_update_progress().running);
}

#[test]
fn test_replace_update_payload_switches_binary_and_static_together() {
    let root = std::env::temp_dir().join(format!(
        "my-media-sub-update-payload-test-{}",
        uuid::Uuid::new_v4()
    ));
    let release = root.join("release");
    let runtime = root.join("runtime");
    let new_static = release.join("static");
    let target_static = runtime.join("static");
    std::fs::create_dir_all(&new_static).unwrap();
    std::fs::create_dir_all(&target_static).unwrap();
    std::fs::write(release.join("my-media-sub"), b"new-binary").unwrap();
    for asset in REQUIRED_STATIC_ASSETS {
        std::fs::write(new_static.join(asset), format!("new-{asset}")).unwrap();
    }
    std::fs::write(runtime.join("my-media-sub"), b"old-binary").unwrap();
    std::fs::write(target_static.join("index.html"), b"old-static").unwrap();
    let backup = runtime.join("my-media-sub.bak-test");

    replace_update_payload_blocking(
        &release.join("my-media-sub"),
        &new_static,
        &runtime.join("my-media-sub"),
        &target_static,
        &backup,
    )
    .unwrap();

    assert_eq!(
        std::fs::read(runtime.join("my-media-sub")).unwrap(),
        b"new-binary"
    );
    assert_eq!(
        std::fs::read(target_static.join("index.html")).unwrap(),
        b"new-index.html"
    );
    assert_eq!(std::fs::read(backup).unwrap(), b"old-binary");
    assert!(std::fs::read_dir(&runtime).unwrap().any(|entry| {
        entry
            .ok()
            .and_then(|entry| entry.file_name().into_string().ok())
            .is_some_and(|name| name.starts_with("static.bak-"))
    }));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_static_payload_requires_release_shell_assets() {
    let root = std::env::temp_dir().join(format!(
        "my-media-sub-update-static-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();

    for asset in REQUIRED_STATIC_ASSETS {
        std::fs::write(root.join(asset), asset).unwrap();
    }
    assert!(static_payload_is_complete(&root));

    std::fs::remove_file(root.join("openapi.json")).unwrap();
    assert!(!static_payload_is_complete(&root));

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_prune_sibling_backups_keeps_latest_named_entries() {
    let root = std::env::temp_dir().join(format!(
        "my-media-sub-update-backup-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let target = root.join("my-media-sub");
    for suffix in ["20260101", "20260201", "20260301", "20260401"] {
        std::fs::write(root.join(format!("my-media-sub.bak-{suffix}")), suffix).unwrap();
    }

    prune_sibling_backups(&target, 2).unwrap();

    let mut remaining = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    remaining.sort();
    assert_eq!(
        remaining,
        vec![
            "my-media-sub.bak-20260301".to_string(),
            "my-media-sub.bak-20260401".to_string()
        ]
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_safe_member_path_rejects_escape_and_absolute() {
    assert!(is_safe_member_path("my-media-sub/my-media-sub"));
    assert!(is_safe_member_path("my-media-sub/static/js/app.js"));
    assert!(!is_safe_member_path("../escape"));
    assert!(!is_safe_member_path("a/../../b"));
    assert!(!is_safe_member_path("/absolute/path"));
    assert!(!is_safe_member_path(""));
    assert!(!is_safe_member_path("a/\0/b"));
    assert!(!is_safe_member_path("./dot"));
}

#[test]
fn test_parse_tar_listing_line_extracts_name_and_link_target() {
    let (name, target) =
        parse_tar_listing_line("-rw-r--r-- user/group 123 2023-01-01 12:00 a/b.txt");
    assert_eq!(name, "a/b.txt");
    assert!(target.is_none());

    let (name, target) =
        parse_tar_listing_line("lrwxrwxrwx user/group 0 2023-01-01 12:00 a/link -> /etc/passwd");
    assert_eq!(name, "a/link");
    assert_eq!(target, Some("/etc/passwd"));
}

/// 集成测试：构造含绝对链接目标的符号链接成员归档，verify_archive_members 必须拒绝。
#[test]
fn test_verify_archive_members_rejects_absolute_symlink_target() {
    let root = std::env::temp_dir().join(format!(
        "my-media-sub-update-member-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();

    let link_dir = root.join("link-src");
    std::fs::create_dir_all(&link_dir).unwrap();
    std::os::unix::fs::symlink("/etc/passwd", link_dir.join("abs_link")).unwrap();
    let archive = root.join("abs.tar.gz");
    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&link_dir)
        .arg(".")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(
        verify_archive_members(&archive).is_err(),
        "含绝对链接目标的归档必须被拒绝"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn test_verify_archive_members_accepts_normal_payload() {
    let root = std::env::temp_dir().join(format!(
        "my-media-sub-update-member-ok-test-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let payload = root.join("payload.tar.gz");
    let dir = root.join("payload");
    std::fs::create_dir_all(dir.join("static")).unwrap();
    std::fs::write(dir.join("my-media-sub"), b"bin").unwrap();
    std::fs::write(dir.join("static/index.html"), b"html").unwrap();

    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&payload)
        .arg("-C")
        .arg(&root)
        .arg("payload")
        .status()
        .unwrap();
    assert!(status.success());
    assert!(verify_archive_members(&payload).is_ok());

    let _ = std::fs::remove_dir_all(root);
}

// ─── 下载地址的主机白名单 ────────────────────────────────────────────────────
//
// `browser_download_url` 直接来自 GitHub API 的 JSON，属于**外部输入**。旧实现
// 逐字采纳它且不限制重定向，因此 API 响应被篡改时可以把下载引到任意地址，
// 而下载内容会被解包并**覆盖运行中的二进制**。

#[test]
fn update_download_accepts_github_asset_hosts() {
    for url in [
        "https://github.com/owner/repo/releases/download/v1.0.0/asset.tar.gz",
        "https://objects.githubusercontent.com/github-production-release-asset/x/y",
        "https://release-assets.githubusercontent.com/github-production-release-asset/x",
        "https://api.github.com/repos/owner/repo/releases/assets/1",
    ] {
        assert!(
            ensure_allowed_update_url(url).is_ok(),
            "{url} 应被接受（GitHub 资产下载会 302 到 objects./release-assets. 主机）"
        );
    }
}

#[test]
fn update_download_rejects_non_github_hosts() {
    for url in [
        "https://evil.example/asset.tar.gz",
        // 后缀混淆：精确匹配必须挡住它，否则 github.com.evil.example 会被放行
        "https://github.com.evil.example/asset.tar.gz",
        "https://notgithub.com/asset.tar.gz",
        // 明文 HTTP 一律拒绝
        "http://github.com/owner/repo/releases/download/v1/asset.tar.gz",
        // 用户信息混淆
        "https://github.com@evil.example/asset.tar.gz",
        // 其他协议
        "file:///etc/passwd",
    ] {
        assert!(ensure_allowed_update_url(url).is_err(), "{url} 必须被拒绝");
    }
}

#[test]
fn update_download_rejects_unparsable_urls() {
    assert!(ensure_allowed_update_url("not a url").is_err());
    assert!(ensure_allowed_update_url("").is_err());
}

/// 客户端的重定向策略与前置校验用的是同一个判定函数，因此这里直接验证它，
/// 保证「初始 URL」与「重定向目标」的口径不会漂移。
#[test]
fn host_allowlist_is_shared_by_pre_check_and_redirect_policy() {
    let allowed = reqwest::Url::parse("https://objects.githubusercontent.com/a/b").unwrap();
    assert!(crate::clients::http_pool::is_allowed_update_host(&allowed));
    let blocked = reqwest::Url::parse("https://evil.example/a").unwrap();
    assert!(!crate::clients::http_pool::is_allowed_update_host(&blocked));
}
