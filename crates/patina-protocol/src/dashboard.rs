//! Dashboard quantities. Hourly imported quantities are not exact session spans.
use crate::activity::{ActivityReadHealth, DailyProductDay, ProductAppIdentity};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardProductSnapshot {
    pub sampled_at_ms: i64,
    pub tracking_health: ActivityReadHealth,
    pub configuration_revision: String,
    pub current: DailyProductDay,
    pub previous: DailyProductDay,
    pub applications: Vec<ProductAppIdentity>,
    pub hours: Vec<DashboardHour>,
}

// Preserve the existing Rust names; the wire representation is unchanged.
pub use crate::activity::{ActivityHour as DashboardHour, ActivityCategoryTotal as CategoryTotal};
