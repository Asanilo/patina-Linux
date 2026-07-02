use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageTargetKind {
    Data,
    Webview,
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
