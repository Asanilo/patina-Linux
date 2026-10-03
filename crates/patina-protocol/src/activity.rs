//! Product activity projections; clients do not classify or exclude these totals again.
use serde::{Deserialize, Serialize};

pub fn is_product_category(value: &str) -> bool {
    value.len() <= 1024 && (matches!(value, "ai" | "development" | "office" | "browser" |
        "communication" | "video" | "music" | "game" | "design" | "utility" | "other")
        || value.strip_prefix("custom:").is_some_and(|label| !label.is_empty()))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DailyProductSnapshot {
    pub sampled_at_ms: i64,
    pub configuration_revision: String,
    pub days: Vec<DailyProductDay>,
    pub applications: Vec<ProductAppIdentity>,
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
