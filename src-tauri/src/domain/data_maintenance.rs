use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct TrackingDataCleanupResult {
    pub title_samples_deleted: u64,
    pub sessions_deleted: u64,
    pub web_activity_segments_deleted: u64,
    pub imported_exact_sessions_deleted: u64,
    pub imported_time_buckets_deleted: u64,
    pub import_batches_deleted: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WindowTitleCleanupResult {
    pub title_samples_deleted: u64,
    pub sessions_redacted: u64,
    pub imported_exact_sessions_redacted: u64,
}
