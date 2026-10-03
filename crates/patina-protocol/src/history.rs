//! Bounded precise activity facts. Hour buckets never appear in this contract.
use crate::activity::ActivityReadHealth;
use serde::{Deserialize, Serialize};

pub const MAX_HISTORY_RANGE_MS: i64 = 32 * 24 * 60 * 60 * 1000;
pub const MAX_HISTORY_FACTS: usize = 20_000;
pub const MAX_HISTORY_RECORDS: usize = 40_000;
pub const MAX_HISTORY_TITLE_SAMPLES: usize = 50_000;
pub const MAX_HISTORY_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_HISTORY_TITLE_BYTES: usize = 16_384;
pub const MAX_HISTORY_NAME_BYTES: usize = 1024;
pub const MAX_SAFE_TIMESTAMP: i64 = 9_007_199_254_740_991;

pub fn valid_range(from_ms: i64, to_ms: i64) -> bool {
    from_ms >= -MAX_SAFE_TIMESTAMP
        && to_ms <= MAX_SAFE_TIMESTAMP
        && matches!(to_ms.checked_sub(from_ms), Some(span) if span > 0 && span <= MAX_HISTORY_RANGE_MS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExactActivityOrigin {
    Native,
    ImportExact,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactHistorySnapshot {
    pub from_ms: i64,
    pub to_ms: i64,
    pub sampled_at_ms: i64,
    pub configuration_revision: String,
    pub tracking_health: ActivityReadHealth,
    pub records: Vec<ExactActivityRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactActivityRecord {
    pub origin: ExactActivityOrigin,
    pub record_id: i64,
    pub app_key: String,
    pub app_name: String,
    pub exe_name: String,
    pub category: String,
    pub display_name_override: Option<String>,
    pub window_title: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub continuity_start_ms: i64,
    pub is_open: bool,
    pub title_samples: Vec<ExactTitleSample>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactTitleSample {
    pub title: String,
    pub start_ms: i64,
    pub end_ms: i64,
}
