//! Product activity projections; clients do not classify or exclude these totals again.
use serde::{Deserialize, Serialize};

pub fn is_product_category(value: &str) -> bool {
    value.len() <= 1024
        && (matches!(
            value,
            "ai" | "development"
                | "office"
                | "browser"
                | "communication"
                | "video"
                | "music"
                | "game"
                | "design"
                | "utility"
                | "other"
        ) || value
            .strip_prefix("custom:")
            .is_some_and(|label| !label.is_empty()))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyProductSnapshot {
    pub sampled_at_ms: i64,
    pub tracking_health: ActivityReadHealth,
    pub configuration_revision: String,
    pub days: Vec<DailyProductDay>,
    pub applications: Vec<ProductAppIdentity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityReadStatus {
    Healthy,
    Stale,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityReadHealth {
    pub status: ActivityReadStatus,
    pub last_heartbeat_ms: Option<i64>,
    pub live_cutoff_ms: i64,
    pub stale_after_ms: i64,
}

impl ActivityReadHealth {
    pub fn is_valid_at(&self, sampled_at_ms: i64) -> bool {
        if sampled_at_ms < 0
            || self.stale_after_ms <= 0
            || self.live_cutoff_ms < 0
            || self.live_cutoff_ms > sampled_at_ms
        {
            return false;
        }
        match (self.status, self.last_heartbeat_ms) {
            (ActivityReadStatus::Unavailable, None) => self.live_cutoff_ms == 0,
            (ActivityReadStatus::Healthy, Some(heartbeat)) => {
                heartbeat > 0
                    && heartbeat <= sampled_at_ms
                    && sampled_at_ms - heartbeat <= self.stale_after_ms
                    && self.live_cutoff_ms == sampled_at_ms
            }
            (ActivityReadStatus::Stale, Some(heartbeat)) => {
                heartbeat > 0
                    && heartbeat <= sampled_at_ms
                    && sampled_at_ms - heartbeat > self.stale_after_ms
                    && self.live_cutoff_ms == heartbeat
            }
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyProductDay {
    pub start_ms: i64,
    pub end_ms: i64,
    pub active_ms: i64,
    pub apps: Vec<DailyProductAppTotal>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyProductAppTotal {
    pub app_key: String,
    pub active_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductAppIdentity {
    pub app_key: String,
    pub app_name: String,
    pub exe_name: String,
    pub category: String,
    pub display_name_override: Option<String>,
}
