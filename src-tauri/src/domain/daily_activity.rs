use chrono::{Local, NaiveDate, TimeZone};

pub const MAX_DAILY_ACTIVITY_DAYS: usize = 378;
pub const MAX_DAILY_ACTIVITY_APPS: usize = 4096;
pub const MAX_DAILY_ACTIVITY_APP_ROWS: usize = 50_000;
pub const MAX_DAILY_APPS_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DailyAppTotal {
    pub app_key: String,
    pub active_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DailyAppActivityDay {
    pub start_ms: i64,
    pub end_ms: i64,
    pub active_ms: i64,
    pub apps: Vec<DailyAppTotal>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DailyAppActivitySnapshot {
    pub sampled_at_ms: i64,
    pub days: Vec<DailyAppActivityDay>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DailyActivityTotal {
    pub start_ms: i64,
    pub end_ms: i64,
    pub active_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DailyActivitySnapshot {
    pub sampled_at_ms: i64,
    pub earliest_start_ms: Option<i64>,
    pub days: Vec<DailyActivityTotal>,
}

pub fn local_day_boundaries(from: &str, to: &str) -> Result<Vec<i64>, String> {
    fn date(value: &str) -> Result<NaiveDate, String> {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map_err(|_| "heatmap dates must use YYYY-MM-DD".to_string())?;
        if value.len() != 10 || date.format("%Y-%m-%d").to_string() != value {
            return Err("heatmap dates must use YYYY-MM-DD".to_string());
        }
        Ok(date)
    }
    let from = date(from)?;
    let to = date(to)?;
    let count = (to - from).num_days();
    if !(1..=MAX_DAILY_ACTIVITY_DAYS as i64).contains(&count) {
        return Err("heatmap range must contain 1 to 378 local days".to_string());
    }
    (0..=count)
        .map(|offset| {
            let midnight = (from + chrono::Duration::days(offset))
                .and_hms_opt(0, 0, 0)
                .unwrap();
            // Resolve each midnight independently and reject nonexistent local dates.
            Local
                .from_local_datetime(&midnight)
                .earliest()
                .filter(|time| {
                    Local
                        .timestamp_millis_opt(time.timestamp_millis())
                        .single()
                        .is_some_and(|resolved| resolved.naive_local() == midnight)
                })
                .map(|time| time.timestamp_millis())
                .ok_or_else(|| "heatmap range contains a nonexistent local midnight".to_string())
        })
        .collect()
}
