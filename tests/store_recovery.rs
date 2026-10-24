//! Run each boot in a separate process so an in-memory recovery flag cannot pass.
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn assert_boot_refused(dir: &Path, allow_empty: bool) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_my-media-sub"));
    command
        .env_clear()
        .env("DATA_DIR", dir)
        .env("SERVER_HOST", "127.0.0.1")
        .env("SERVER_PORT", "0")
        .env("SERVER_PASSWORD", "isolated-recovery-test-password")
        .env("RUST_LOG", "error")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if allow_empty {
        command.env("ALLOW_QUARANTINE_STARTUP", "true");
    }
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let output = child.wait_with_output().unwrap();
            assert!(!status.success(), "corrupt store must prevent startup");
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(
                error.contains(".corrupt-"),
                "unexpected startup failure: {error}"
            );
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "service started despite an unresolved quarantine in {}",
                dir.display()
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn automatic_restarts_cannot_bypass_quarantine() {
    for name in [
        "subscriptions.json",
        "settings.json",
        "notifications.json",
        "automation_events.json",
        "telegram_bot.json",
        "jobs.json",
    ] {
        let dir = std::env::temp_dir().join(format!("mms-recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, b"{broken-json").unwrap();
        assert_boot_refused(&dir, false);
        assert!(!path.exists());
        assert_boot_refused(&dir, false);
        assert_boot_refused(&dir, false);
        if name == "settings.json" {
            assert_boot_refused(&dir, true);
        }
        assert!(!path.exists(), "restart must not recreate the store");
        let quarantined = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|entry| {
                entry
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(&format!("{name}.corrupt-"))
            })
            .unwrap();
        assert_eq!(std::fs::read(quarantined).unwrap(), b"{broken-json");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
