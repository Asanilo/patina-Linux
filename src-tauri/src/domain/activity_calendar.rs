//! Actual time partitions for a host-local day, including UTC-offset changes.
use chrono::{Offset, TimeZone, Timelike};

pub fn hour_boundaries<T: TimeZone>(
    timezone: &T,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<i64>, String> {
    bounded_hour_partitions(timezone, start_ms, end_ms, 48 * 3_600_000, 96)
}

pub fn history_hour_boundaries<T: TimeZone>(
    timezone: &T,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<i64>, String> {
    bounded_hour_partitions(
        timezone,
        start_ms,
        end_ms,
        patina_protocol::history::MAX_HISTORY_RANGE_MS,
        32 * 96,
    )
}

fn bounded_hour_partitions<T: TimeZone>(
    timezone: &T,
    start_ms: i64,
    end_ms: i64,
    max_width: i64,
    max_boundaries: usize,
) -> Result<Vec<i64>, String> {
    if !matches!(end_ms.checked_sub(start_ms), Some(width) if width > 0 && width <= max_width) {
        return Err("invalid activity day boundaries".into());
    }
    let at = |ms| {
        timezone
            .timestamp_millis_opt(ms)
            .single()
            .ok_or("invalid activity timestamp")
    };
    let offset = |ms| at(ms).map(|time| time.offset().fix().local_minus_utc());
    let mut boundaries = vec![start_ms];
    let mut cursor = start_ms;
    while cursor < end_ms {
        if boundaries.len() > max_boundaries {
            return Err("activity day has too many clock transitions".into());
        }
        let local = at(cursor)?;
        let elapsed = i64::from(local.minute()) * 60_000
            + i64::from(local.second()) * 1000
            + i64::from(local.timestamp_subsec_millis());
        let mut next = cursor.saturating_add(3_600_000 - elapsed).min(end_ms);
        let original_offset = offset(cursor)?;
        if offset(next)? != original_offset {
            // Locate the offset transition exactly, including half-hour changes.
            let mut low = cursor + 1;
            let mut high = next;
            while low < high {
                let middle = low + (high - low) / 2;
                if offset(middle)? == original_offset {
                    low = middle + 1;
                } else {
                    high = middle;
                }
            }
            next = low;
        }
        if next <= cursor {
            return Err("activity clock did not advance".into());
        }
        boundaries.push(next);
        cursor = next;
    }
    Ok(boundaries)
}

pub fn dashboard_boundaries(date: &str) -> Result<Vec<i64>, String> {
    let parsed = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| "Dashboard date must use YYYY-MM-DD")?;
    if date.len() != 10 || parsed.format("%Y-%m-%d").to_string() != date {
        return Err("Dashboard date must use YYYY-MM-DD".into());
    }
    let previous = parsed.pred_opt().ok_or("Dashboard date out of range")?;
    let next = parsed.succ_opt().ok_or("Dashboard date out of range")?;
    crate::domain::daily_activity::local_day_boundaries(
        &previous.format("%Y-%m-%d").to_string(),
        &next.format("%Y-%m-%d").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_offset_partitions_cover_every_millisecond() {
        let tz = chrono::FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap();
        let start = tz
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        let boundaries = hour_boundaries(&tz, start, start + 24 * 3_600_000).unwrap();
        assert_eq!(boundaries.len(), 25);
        assert!(boundaries.windows(2).all(|p| p[1] - p[0] == 3_600_000));
    }

    // Run explicitly in separate processes with TZ=America/New_York and
    // TZ=Australia/Lord_Howe; do not mutate process timezone in parallel tests.
    #[test]
    fn host_local_partitions_cover_dst_days() {
        for (date, expected_new_york, expected_lord_howe) in [
            ((2026, 3, 8), 23 * 3_600_000, 24 * 3_600_000),
            ((2026, 11, 1), 25 * 3_600_000, 24 * 3_600_000),
            ((2026, 4, 5), 24 * 3_600_000, 24 * 3_600_000 + 1_800_000),
            ((2026, 10, 4), 24 * 3_600_000, 24 * 3_600_000 - 1_800_000),
        ] {
            let day = chrono::NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap();
            let start = chrono::Local
                .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
                .earliest()
                .unwrap()
                .timestamp_millis();
            let end = chrono::Local
                .from_local_datetime(&day.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap())
                .earliest()
                .unwrap()
                .timestamp_millis();
            let boundaries = hour_boundaries(&chrono::Local, start, end).unwrap();
            assert_eq!(boundaries.first(), Some(&start));
            assert_eq!(boundaries.last(), Some(&end));
            assert!(boundaries
                .windows(2)
                .all(|p| p[1] > p[0] && p[1] - p[0] <= 3_600_000));
            let expected = match std::env::var("TZ").as_deref() {
                Ok("America/New_York") => Some(expected_new_york),
                Ok("Australia/Lord_Howe") => Some(expected_lord_howe),
                _ => None,
            };
            if let Some(expected) = expected {
                assert_eq!(end - start, expected);
            }
        }
    }
}
