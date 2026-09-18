use serde::{Deserialize, Serialize};

pub const MAX_OBSERVED_APP_RANGE_MS: i64 = 366 * 24 * 60 * 60 * 1000;
pub const MAX_OBSERVED_APPS: usize = 4096;
pub const MAX_OBSERVED_APPS_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ObservedAppStat {
    pub exe_name: String,
    pub app_name: String,
    pub total_duration_ms: i64,
    pub last_seen_ms: i64,
}

pub fn validate_range(from_ms: i64, to_ms: i64) -> Result<(), String> {
    if from_ms < 0 || to_ms <= from_ms || to_ms - from_ms > MAX_OBSERVED_APP_RANGE_MS {
        return Err(
            "observed apps require an increasing nonnegative range of at most 366 days".into(),
        );
    }
    Ok(())
}
