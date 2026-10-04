use chrono::{TimeZone, Timelike};
use patina_protocol::{
    dashboard::{CategoryTotal, DashboardHour},
    history::*,
    history_product::*,
};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
mod tests;

pub async fn load_history_product(
    pool: &sqlx::SqlitePool,
    from_ms: i64,
    to_ms: i64,
    sampled_at_ms: i64,
    language: &str,
) -> Result<HistoryProductSnapshot, String> {
    super::load_with_projection(pool, from_ms, to_ms, sampled_at_ms, language, |history| {
        let hours = project_hours(&chrono::Local, &history)?;
        let result = HistoryProductSnapshot { history, hours };
        if serde_json::to_vec(&result)
            .map_err(|e| e.to_string())?
            .len()
            + 512
            > MAX_HISTORY_RESPONSE_BYTES
        {
            return Err("History product response exceeds budget".into());
        }
        Ok(result)
    })
    .await
}

fn project_hours<T: TimeZone>(
    timezone: &T,
    history: &ExactHistorySnapshot,
) -> Result<Vec<DashboardHour>, String> {
    let boundaries = crate::domain::activity_calendar::history_hour_boundaries(
        timezone,
        history.from_ms,
        history.to_ms,
    )?;
    let hour_indices = boundaries[..boundaries.len() - 1]
        .iter()
        .map(|&ms| {
            timezone
                .timestamp_millis_opt(ms)
                .single()
                .map(|time| time.hour() as usize)
                .ok_or("invalid History hour")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut buckets: Vec<BTreeMap<String, i64>> = (0..24).map(|_| BTreeMap::new()).collect();
    let mut categories = BTreeSet::new();
    let mut steps = 0;
    let mut bytes = 1024usize;
    for record in &history.records {
        categories.insert(record.category.as_str());
        if categories.len() > MAX_HISTORY_HOUR_CATEGORIES {
            return Err("History hourly category budget exceeded".into());
        }
        let mut index = boundaries
            .partition_point(|&ms| ms <= record.start_ms)
            .saturating_sub(1);
        while index + 1 < boundaries.len() && boundaries[index] < record.end_ms {
            steps += 1;
            if steps > MAX_HISTORY_HOUR_STEPS {
                return Err("History hourly projection step budget exceeded".into());
            }
            let start = record.start_ms.max(boundaries[index]);
            let end = record.end_ms.min(boundaries[index + 1]);
            if end > start {
                let bucket = &mut buckets[hour_indices[index]];
                if !bucket.contains_key(&record.category) {
                    bytes += record.category.len() + 128;
                    if bytes > MAX_HISTORY_RESPONSE_BYTES {
                        return Err("History hourly projection byte budget exceeded".into());
                    }
                    bucket.insert(record.category.clone(), 0);
                }
                let value = bucket
                    .get_mut(&record.category)
                    .ok_or("missing History category")?;
                *value = value
                    .checked_add(end - start)
                    .ok_or("History hourly quantity overflow")?;
            }
            index += 1;
        }
    }
    buckets
        .into_iter()
        .enumerate()
        .map(|(hour, values)| {
            let active_ms = values
                .values()
                .try_fold(0i64, |sum, value| sum.checked_add(*value))
                .ok_or("History hourly total overflow")?;
            Ok(DashboardHour {
                hour: hour as u8,
                active_ms,
                categories: values
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
