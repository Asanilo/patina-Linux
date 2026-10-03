use super::*;
use chrono::{TimeZone, Timelike};
use patina_protocol::dashboard::{CategoryTotal, DashboardHour, DashboardProductSnapshot};
use std::collections::BTreeMap;

pub async fn load_dashboard_product(
    pool: &SqlitePool,
    boundaries: &[i64],
    sampled_at_ms: i64,
    language: &str,
) -> Result<DashboardProductSnapshot, String> {
    if boundaries.len() != 3 {
        return Err("Dashboard requires yesterday and the selected day".into());
    }
    let mode = match language {
        "en-US" => ReadMode::DashboardEnglish,
        "zh-CN" => ReadMode::DashboardChinese,
        _ => return Err("unsupported product language".into()),
    };
    let mut source = load_bounded_snapshot(pool, boundaries, sampled_at_ms, mode).await?;
    let hours = std::mem::take(&mut source.hours);
    let mut product = project_product(source)?;
    let current = product.days.pop().ok_or("missing selected day")?;
    let previous = product.days.pop().ok_or("missing previous day")?;
    let result = DashboardProductSnapshot {
        sampled_at_ms,
        configuration_revision: product.configuration_revision,
        tracking_health: product.tracking_health,
        applications: product.applications,
        current,
        previous,
        hours,
    };
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        + 512
        > MAX_DAILY_APPS_RESPONSE_BYTES
    {
        return Err("Dashboard response exceeds budget".into());
    }
    Ok(result)
}

pub(super) fn build_hours(
    records: &[OwnedActivityRange<DayFact>],
    day: &[i64],
    policy: &crate::domain::product_classification::ProductClassification,
) -> Result<Vec<DashboardHour>, String> {
    let boundaries =
        crate::domain::activity_calendar::hour_boundaries(&chrono::Local, day[0], day[1])?;
    let partitions =
        crate::domain::activity_read_model::summarize_activity_partitions(records, &boundaries);
    let mut categories: Vec<BTreeMap<String, i64>> = (0..24).map(|_| BTreeMap::new()).collect();
    for (index, values) in partitions.into_iter().enumerate() {
        let hour = chrono::Local
            .timestamp_millis_opt(boundaries[index])
            .single()
            .ok_or("invalid Dashboard hour")?
            .hour() as usize;
        for value in values.into_iter().filter(|v| v.value.included) {
            let app = value
                .value
                .app
                .as_deref()
                .ok_or("missing Dashboard application identity")?;
            let entry = categories[hour]
                .entry(policy.category(app).to_owned())
                .or_default();
            *entry = entry
                .checked_add(value.duration_ms)
                .ok_or("Dashboard duration overflow")?;
        }
    }
    categories
        .into_iter()
        .enumerate()
        .map(|(hour, categories)| {
            let active_ms = categories
                .values()
                .try_fold(0i64, |sum, value| sum.checked_add(*value))
                .ok_or("Dashboard hour overflow")?;
            Ok(DashboardHour {
                hour: hour as u8,
                active_ms,
                categories: categories
                    .into_iter()
                    .map(|(category, active_ms)| CategoryTotal {
                        category,
                        active_ms,
                    })
                    .collect(),
            })
        })
        .collect()
}
