use crate::domain::backup::BackupPreview;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebDavBackupConfig {
    pub url: String,
    pub username: String,
    pub remote_dir: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBackupEntry {
    pub id: String,
    pub file_name: String,
    pub remote_path: String,
    pub created_at_ms: u64,
    pub size_bytes: u64,
    pub app_version: String,
    pub backup_version: u32,
    pub schema_version: u32,
    pub session_count: usize,
    pub title_sample_count: usize,
    pub setting_count: usize,
    pub icon_cache_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBackupUploadResult {
    pub entry: RemoteBackupEntry,
    pub index_updated: bool,
    pub index_message: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteBackupDownloadResult {
    pub path: String,
    pub preview: BackupPreview,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavTestResult {
    pub ok: bool,
}
