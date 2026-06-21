use crate::data::sqlite_pool;
use crate::engine::api::types::{
    ApiError, ApiResponse, RouteResponse, TrendDataPoint, TrendResponse,
};
use chrono::{Datelike, TimeZone};
use sqlx::Row;
use std::collections::HashMap;

pub async fn get_trend(app: &tauri::AppHandle, query: Option<&str>) -> RouteResponse {
    let params = parse_trend_query(query);
    let range = match resolve_trend_range(
        params.period.as_deref(),
        params.granularity.as_deref(),
        chrono::Local::now().fixed_offset(),
    ) {
        Ok(range) => range,
        Err(message) => {
            return RouteResponse {
                status: 400,
                body: serde_json::to_value(ApiError::bad_request(message)).unwrap_or_default(),
            };
        }
    };

    let pool = match sqlite_pool::wait_for_sqlite_pool(app).await {
        Ok(pool) => pool,
        Err(error) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
            };
        }
    };

    let rows = match sqlx::query(
        "SELECT exe_name, start_time, end_time
         FROM sessions
         WHERE start_time <= ?
           AND COALESCE(end_time, ?) >= ?
         ORDER BY start_time ASC",
    )
    .bind(range.to_ms)
    .bind(range.to_ms)
    .bind(range.from_ms)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&error.to_string()))
                    .unwrap_or_default(),
            };
        }
    };

    let sessions = rows
        .iter()
        .map(|row| TrendSessionInput {
            exe_name: row.try_get::<String, _>("exe_name").unwrap_or_default(),
            start_time: row.try_get::<i64, _>("start_time").unwrap_or(0),
            end_time: row.try_get::<Option<i64>, _>("end_time").unwrap_or(None),
        })
        .collect();

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: build_daily_trend(range, sessions),
        })
        .unwrap_or_default(),
    }
}

#[derive(Clone, Debug)]
struct TrendQuery {
    period: Option<String>,
    granularity: Option<String>,
}

#[derive(Clone, Debug)]
struct TrendRange {
    period: String,
    granularity: String,
    from_ms: i64,
    to_ms: i64,
    day_starts: Vec<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(Clone, Debug)]
struct TrendSessionInput {
    exe_name: String,
    start_time: i64,
    end_time: Option<i64>,
}

fn parse_trend_query(query: Option<&str>) -> TrendQuery {
    let mut params = TrendQuery {
        period: None,
        granularity: None,
    };

    if let Some(query) = query {
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                match key {
                    "period" => params.period = Some(value.to_string()),
                    "granularity" => params.granularity = Some(value.to_string()),
                    _ => {}
                }
            }
        }
    }

    params
}

fn resolve_trend_range(
    period: Option<&str>,
    granularity: Option<&str>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> Result<TrendRange, &'static str> {
    let period = period.unwrap_or("week");
    let granularity = granularity.unwrap_or("day");

    if granularity != "day" {
        return Err("unsupported granularity");
    }

    let day_count = match period {
        "week" => 7,
        "month" => 30,
        _ => return Err("unsupported period"),
    };

    let today_start = now
        .offset()
        .clone()
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .unwrap_or(now);
    let range_start = today_start - chrono::Duration::days(day_count - 1);
    let day_starts = (0..day_count)
        .map(|offset_days| range_start + chrono::Duration::days(offset_days))
        .collect::<Vec<_>>();

    Ok(TrendRange {
        period: period.to_string(),
        granularity: granularity.to_string(),
        from_ms: range_start.timestamp_millis(),
        to_ms: now.timestamp_millis(),
        day_starts,
    })
}

fn build_daily_trend(range: TrendRange, sessions: Vec<TrendSessionInput>) -> TrendResponse {
    let mut active_by_day = vec![0_i64; range.day_starts.len()];
    let mut app_totals_by_day = vec![HashMap::<String, i64>::new(); range.day_starts.len()];

    for session in sessions {
        let session_start = session.start_time.max(range.from_ms);
        let session_end = session.end_time.unwrap_or(range.to_ms).min(range.to_ms);
        if session_end <= session_start {
            continue;
        }

        for (index, day_start) in range.day_starts.iter().enumerate() {
            let day_start_ms = day_start.timestamp_millis();
            let day_end_ms = (*day_start + chrono::Duration::days(1))
                .timestamp_millis()
                .min(range.to_ms);
            let overlap_start = session_start.max(day_start_ms);
            let overlap_end = session_end.min(day_end_ms);
            let overlap = overlap_end.saturating_sub(overlap_start);
            if overlap <= 0 {
                continue;
            }

            active_by_day[index] += overlap;
            *app_totals_by_day[index]
                .entry(session.exe_name.clone())
                .or_insert(0) += overlap;
        }
    }

    let data_points = range
        .day_starts
        .iter()
        .enumerate()
        .map(|(index, day_start)| TrendDataPoint {
            date: day_start.format("%Y-%m-%d").to_string(),
            active_ms: active_by_day[index],
            top_app: resolve_top_app(&app_totals_by_day[index]),
        })
        .collect();

    TrendResponse {
        period: range.period,
        granularity: range.granularity,
        from_ms: range.from_ms,
        to_ms: range.to_ms,
        data_points,
    }
}

fn resolve_top_app(app_totals: &HashMap<String, i64>) -> Option<String> {
    app_totals
        .iter()
        .filter(|(_, duration)| **duration > 0)
        .max_by(|(left_app, left_duration), (right_app, right_duration)| {
            left_duration
                .cmp(right_duration)
                .then_with(|| right_app.cmp(left_app))
        })
        .map(|(app, _)| app.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    #[test]
    fn daily_trend_splits_cross_day_sessions_on_local_boundaries() {
        let offset = FixedOffset::east_opt(8 * 3600).unwrap();
        let now = offset.with_ymd_and_hms(2026, 6, 21, 14, 30, 0).unwrap();
        let range = resolve_trend_range(Some("week"), Some("day"), now).unwrap();
        let sessions = vec![TrendSessionInput {
            exe_name: "ghostty".to_string(),
            start_time: offset
                .with_ymd_and_hms(2026, 6, 20, 23, 50, 0)
                .unwrap()
                .timestamp_millis(),
            end_time: Some(
                offset
                    .with_ymd_and_hms(2026, 6, 21, 0, 10, 0)
                    .unwrap()
                    .timestamp_millis(),
            ),
        }];

        let trend = build_daily_trend(range, sessions);

        let june_20 = trend
            .data_points
            .iter()
            .find(|point| point.date == "2026-06-20")
            .unwrap();
        let june_21 = trend
            .data_points
            .iter()
            .find(|point| point.date == "2026-06-21")
            .unwrap();

        assert_eq!(june_20.active_ms, 10 * 60 * 1000);
        assert_eq!(june_20.top_app.as_deref(), Some("ghostty"));
        assert_eq!(june_21.active_ms, 10 * 60 * 1000);
        assert_eq!(june_21.top_app.as_deref(), Some("ghostty"));
    }

    #[test]
    fn daily_trend_counts_active_sessions_until_now() {
        let offset = FixedOffset::east_opt(8 * 3600).unwrap();
        let now = offset.with_ymd_and_hms(2026, 6, 21, 14, 30, 0).unwrap();
        let range = resolve_trend_range(Some("week"), Some("day"), now).unwrap();
        let sessions = vec![TrendSessionInput {
            exe_name: "obsidian".to_string(),
            start_time: offset
                .with_ymd_and_hms(2026, 6, 21, 14, 0, 0)
                .unwrap()
                .timestamp_millis(),
            end_time: None,
        }];

        let trend = build_daily_trend(range, sessions);

        let today = trend
            .data_points
            .iter()
            .find(|point| point.date == "2026-06-21")
            .unwrap();
        assert_eq!(today.active_ms, 30 * 60 * 1000);
        assert_eq!(today.top_app.as_deref(), Some("obsidian"));
    }

    #[test]
    fn trend_range_rejects_unsupported_granularity() {
        let offset = FixedOffset::east_opt(8 * 3600).unwrap();
        let now = offset.with_ymd_and_hms(2026, 6, 21, 14, 30, 0).unwrap();

        let error = resolve_trend_range(Some("week"), Some("hour"), now).unwrap_err();

        assert_eq!(error, "unsupported granularity");
    }
}
