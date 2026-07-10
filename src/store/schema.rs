use std::fmt;
use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use crate::error::{AppError, Result};
use crate::utils::{set_file_mode, write_file_atomic, write_json_atomic_async};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreKind {
    Settings,
    Subscriptions,
    Notifications,
    Jobs,
    AutomationEvents,
}

impl fmt::Display for StoreKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            StoreKind::Settings => "settings",
            StoreKind::Subscriptions => "subscriptions",
            StoreKind::Notifications => "notifications",
            StoreKind::Jobs => "jobs",
            StoreKind::AutomationEvents => "automation_events",
        })
    }
}

#[derive(Debug)]
pub enum StoreSchemaError {
    Invalid(String),
    UnsupportedVersion { found: u32, current: u32 },
}

impl fmt::Display for StoreSchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreSchemaError::Invalid(message) => f.write_str(message),
            StoreSchemaError::UnsupportedVersion { found, current } => {
                write!(f, "存储 schema 版本 {} 高于当前支持版本 {}", found, current)
            }
        }
    }
}

impl std::error::Error for StoreSchemaError {}

#[derive(Debug)]
pub struct DecodedStore<T> {
    pub data: T,
    pub needs_write: bool,
    pub source_version: u32,
}

#[derive(Serialize)]
struct StoreEnvelope<'a, T> {
    schema_version: u32,
    data: &'a T,
}

pub fn decode_store_json<T: DeserializeOwned>(
    content: &str,
    kind: StoreKind,
) -> std::result::Result<DecodedStore<T>, StoreSchemaError> {
    let value: Value = serde_json::from_str(content)
        .map_err(|error| StoreSchemaError::Invalid(error.to_string()))?;
    let (source_version, data, legacy) = match value {
        Value::Object(mut object) if object.contains_key("schema_version") => {
            let version = object
                .remove("schema_version")
                .and_then(|value| value.as_u64())
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| {
                    StoreSchemaError::Invalid("schema_version 必须是非负整数".to_string())
                })?;
            let data = object
                .remove("data")
                .ok_or_else(|| StoreSchemaError::Invalid("版本化存储缺少 data 字段".to_string()))?;
            (version, data, false)
        }
        legacy_data => (0, legacy_data, true),
    };

    if source_version > CURRENT_SCHEMA_VERSION {
        return Err(StoreSchemaError::UnsupportedVersion {
            found: source_version,
            current: CURRENT_SCHEMA_VERSION,
        });
    }

    let migrated = migrate_store_data(kind, source_version, data)?;
    let data = serde_json::from_value(migrated)
        .map_err(|error| StoreSchemaError::Invalid(error.to_string()))?;
    Ok(DecodedStore {
        data,
        needs_write: legacy || source_version < CURRENT_SCHEMA_VERSION,
        source_version,
    })
}

fn migrate_store_data(
    kind: StoreKind,
    mut version: u32,
    mut data: Value,
) -> std::result::Result<Value, StoreSchemaError> {
    while version < CURRENT_SCHEMA_VERSION {
        data = match version {
            // v0 was the historical bare array/object. v1 wraps the same payload in an envelope.
            0 => migrate_v0_to_v1(kind, data),
            other => {
                return Err(StoreSchemaError::Invalid(format!(
                    "没有 {} 存储从 schema {} 开始的迁移路径",
                    kind, other
                )))
            }
        };
        version += 1;
    }
    Ok(data)
}

fn migrate_v0_to_v1(_kind: StoreKind, data: Value) -> Value {
    data
}

pub fn migration_backup_path(path: &Path, source_version: u32) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("store.json");
    path.with_file_name(format!("{}.schema-v{}.bak", file_name, source_version))
}

/// Preserve the exact pre-migration bytes once before rewriting a store.
/// Existing backups are never overwritten so they remain useful for manual rollback.
pub fn backup_store_before_migration(
    path: &Path,
    content: &str,
    source_version: u32,
) -> Result<Option<PathBuf>> {
    if source_version >= CURRENT_SCHEMA_VERSION || !path.exists() {
        return Ok(None);
    }

    let backup = migration_backup_path(path, source_version);
    if backup.exists() && !backup.is_file() {
        return Err(AppError::Database(format!(
            "迁移备份路径不是文件: {}",
            backup.display()
        )));
    }
    if !backup.exists() {
        write_file_atomic(&backup, content.as_bytes(), 0o600)?;
    }
    set_file_mode(&backup, 0o600)?;
    Ok(Some(backup))
}

pub async fn write_versioned_json_atomic_async<T: Serialize>(
    path: &Path,
    data: &T,
    mode: u32,
) -> Result<()> {
    let envelope = StoreEnvelope {
        schema_version: CURRENT_SCHEMA_VERSION,
        data,
    };
    write_json_atomic_async(path, &envelope, mode).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("my-media-sub-{}-{}", name, uuid::Uuid::new_v4()))
    }

    #[test]
    fn legacy_payload_is_migrated_to_current_schema() {
        let decoded = decode_store_json::<Vec<i32>>("[1,2,3]", StoreKind::Subscriptions).unwrap();
        assert_eq!(decoded.data, vec![1, 2, 3]);
        assert!(decoded.needs_write);
        assert_eq!(decoded.source_version, 0);
    }

    #[test]
    fn current_envelope_does_not_require_rewrite() {
        let decoded =
            decode_store_json::<Vec<i32>>(r#"{"schema_version":1,"data":[1,2]}"#, StoreKind::Jobs)
                .unwrap();
        assert_eq!(decoded.data, vec![1, 2]);
        assert!(!decoded.needs_write);
        assert_eq!(decoded.source_version, 1);
    }

    #[test]
    fn future_schema_is_rejected_without_treating_it_as_corruption() {
        let error = decode_store_json::<Value>(
            &json!({"schema_version": 99, "data": {}}).to_string(),
            StoreKind::Settings,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            StoreSchemaError::UnsupportedVersion {
                found: 99,
                current: CURRENT_SCHEMA_VERSION
            }
        ));
    }

    #[test]
    fn migration_backup_preserves_original_bytes_once() {
        let path = temp_path("schema-backup").with_extension("json");
        let original = "[1,2,3]\n";
        std::fs::write(&path, original).unwrap();

        let backup = backup_store_before_migration(&path, original, 0)
            .unwrap()
            .unwrap();
        assert_eq!(backup, migration_backup_path(&path, 0));
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);

        std::fs::write(&path, "[4,5,6]").unwrap();
        backup_store_before_migration(&path, "[4,5,6]", 0).unwrap();
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), original);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(backup);
    }
}
