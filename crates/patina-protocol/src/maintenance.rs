//! Explicitly confirmed canonical application cleanup. Writes are not retried.
use serde::{Deserialize, Serialize};
pub const MAX_CLEANUP_APP_KEY_BYTES: usize = 1024;
pub const MAX_CLEANUP_EXECUTABLES: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCleanupScope {
    All,
    Today,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalAppCleanupRequest {
    pub app_key: String,
    pub scope: AppCleanupScope,
    pub confirmed: bool,
}
impl CanonicalAppCleanupRequest {
    pub fn validate(&self) -> Result<(), String> {
        if !self.confirmed {
            return Err("application cleanup requires confirmed=true".into());
        }
        if self.app_key.trim().is_empty()
            || self.app_key.len() > MAX_CLEANUP_APP_KEY_BYTES
            || self.app_key.chars().any(char::is_control)
        {
            return Err("application cleanup requires a valid app key".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct AppTrackingDataCleanupResult {
    pub sessions_deleted: u64,
    pub imported_exact_sessions_deleted: u64,
    pub imported_time_buckets_deleted: u64,
    pub import_batches_deleted: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CanonicalAppCleanupResult {
    pub app_key: String,
    pub matched_executables: usize,
    pub deleted: AppTrackingDataCleanupResult,
}
