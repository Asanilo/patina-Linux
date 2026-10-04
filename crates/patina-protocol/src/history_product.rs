//! Display quantities derived from the same precise History facts. Imported
//! hourly quantities never become exact records or enter these totals.
use crate::{activity::ActivityHour, history::ExactHistorySnapshot};
use serde::{Deserialize, Serialize};

pub const MAX_HISTORY_HOUR_CATEGORIES: usize = 4096;
pub const MAX_HISTORY_HOUR_STEPS: usize = 1_000_000;

#[cfg_attr(feature = "typegen", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryProductSnapshot {
    pub history: ExactHistorySnapshot,
    pub hours: Vec<ActivityHour>,
}
