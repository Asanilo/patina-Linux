use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageTargetKind {
    Data,
    Webview,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageDirectoryKind {
    Data,
    Backups,
    Webview,
    RetainedData,
    RetainedWebview,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageMigrationPreview {
    pub current_data_root: PathBuf,
    pub target_data_root: PathBuf,
    pub current_webview_root: PathBuf,
    pub target_webview_root: PathBuf,
    pub payload_size_bytes: u64,
    pub available_space_bytes: u64,
    pub required_space_bytes: u64,
    pub requires_restart: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoragePathSnapshot {
    pub data_root: PathBuf,
    pub default_data_root: PathBuf,
    pub database_path: PathBuf,
    pub backup_dir: PathBuf,
    pub webview_root: PathBuf,
    pub default_webview_root: PathBuf,
    pub is_custom_data_root: bool,
    pub is_custom_webview_root: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSizeSnapshot {
    pub data_bytes: u64,
    pub webview_profile_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebviewCacheSnapshot {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub clear_on_restart: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageMaintenanceSnapshot {
    pub last_webview_cache_clear_at_ms: Option<u64>,
    pub last_error: Option<String>,
    pub last_migration_status: Option<String>,
    pub retained_previous_data_root: Option<PathBuf>,
    pub retained_previous_webview_root: Option<PathBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoragePendingMigrationSnapshot {
    pub id: String,
    pub source_data_root: PathBuf,
    pub target_data_root: PathBuf,
    pub source_webview_root: PathBuf,
    pub target_webview_root: PathBuf,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSnapshot {
    pub paths: StoragePathSnapshot,
    pub sizes: StorageSizeSnapshot,
    pub webview_cache: WebviewCacheSnapshot,
    pub maintenance: StorageMaintenanceSnapshot,
    pub pending_migration: Option<StoragePendingMigrationSnapshot>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_snapshot_serializes_a_stable_camel_case_contract() {
        let snapshot = StorageSnapshot {
            paths: StoragePathSnapshot {
                data_root: PathBuf::from("/data/Patina"),
                default_data_root: PathBuf::from("/default/Patina"),
                database_path: PathBuf::from("/data/Patina/patina.db"),
                backup_dir: PathBuf::from("/data/Patina/backups"),
                webview_root: PathBuf::from("/webview/Patina"),
                default_webview_root: PathBuf::from("/default/Patina"),
                is_custom_data_root: true,
                is_custom_webview_root: true,
            },
            sizes: StorageSizeSnapshot {
                data_bytes: 10,
                webview_profile_bytes: 20,
            },
            webview_cache: WebviewCacheSnapshot {
                path: PathBuf::from("/webview/Patina/WebKitCache"),
                size_bytes: 5,
                clear_on_restart: false,
            },
            maintenance: StorageMaintenanceSnapshot {
                last_webview_cache_clear_at_ms: None,
                last_error: None,
                last_migration_status: Some("succeeded".to_string()),
                retained_previous_data_root: Some(PathBuf::from("/old/Patina")),
                retained_previous_webview_root: None,
            },
            pending_migration: None,
        };

        let value = serde_json::to_value(snapshot).unwrap();

        assert_eq!(value["paths"]["databasePath"], "/data/Patina/patina.db");
        assert_eq!(value["sizes"]["webviewProfileBytes"], 20);
        assert_eq!(value["webviewCache"]["clearOnRestart"], false);
        assert_eq!(value["maintenance"]["lastMigrationStatus"], "succeeded");
    }
}
