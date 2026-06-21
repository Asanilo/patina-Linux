use crate::data::sqlite_pool;
use crate::engine::api::types::{
    ActiveSessionResponse, ApiError, ApiResponse, AppSummaryEntry, CategorySummaryEntry,
    RouteResponse, SessionEntry, SessionQueryParams, SessionsResponse, SummaryQueryParams,
    SummaryResponse,
};
use chrono::{Datelike, TimeZone};
use sqlx::Row;
use std::collections::HashMap;

pub async fn get_sessions(app: &tauri::AppHandle, query: Option<&str>) -> RouteResponse {
    let pool = match sqlite_pool::wait_for_sqlite_pool(app).await {
        Ok(p) => p,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e)).unwrap_or_default(),
            };
        }
    };

    let params = parse_session_query(query);

    let mut sql = String::from(
        "SELECT id, app_name, exe_name, window_title, start_time, end_time, duration
         FROM sessions WHERE end_time IS NOT NULL",
    );

    if let Some(from) = params.from {
        sql.push_str(&format!(" AND start_time >= {from}"));
    }
    if let Some(to) = params.to {
        sql.push_str(&format!(" AND start_time <= {to}"));
    }
    if let Some(ref app_filter) = params.app {
        sql.push_str(&format!(
            " AND exe_name = '{}'",
            app_filter.replace('\'', "''")
        ));
    }

    sql.push_str(" ORDER BY start_time DESC");

    if let Some(limit) = params.limit {
        sql.push_str(&format!(" LIMIT {limit}"));
    } else {
        sql.push_str(" LIMIT 100");
    }

    let rows = match sqlx::query(&sql).fetch_all(&pool).await {
        Ok(r) => r,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e.to_string())).unwrap_or_default(),
            };
        }
    };

    let sessions: Vec<SessionEntry> = rows
        .iter()
        .map(|row| SessionEntry {
            id: row.try_get::<i64, _>("id").unwrap_or(0),
            app_name: row.try_get::<String, _>("app_name").unwrap_or_default(),
            exe_name: row.try_get::<String, _>("exe_name").unwrap_or_default(),
            window_title: row
                .try_get::<Option<String>, _>("window_title")
                .unwrap_or(None),
            start_time: row.try_get::<i64, _>("start_time").unwrap_or(0),
            end_time: row.try_get::<Option<i64>, _>("end_time").unwrap_or(None),
            duration: row.try_get::<Option<i64>, _>("duration").unwrap_or(None),
        })
        .collect();

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: SessionsResponse { sessions },
        })
        .unwrap_or_default(),
    }
}

pub async fn get_active_session(app: &tauri::AppHandle) -> RouteResponse {
    let pool = match sqlite_pool::wait_for_sqlite_pool(app).await {
        Ok(p) => p,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e)).unwrap_or_default(),
            };
        }
    };

    let row = match sqlx::query(
        "SELECT id,
                app_name,
                exe_name,
                window_title,
                start_time,
                COALESCE(continuity_group_start_time, start_time) AS continuity_group_start_time
         FROM sessions
         WHERE end_time IS NULL
         ORDER BY start_time DESC, id DESC
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(row)) => row,
        Ok(None) => {
            return RouteResponse {
                status: 200,
                body: serde_json::to_value(ApiResponse {
                    data: serde_json::Value::Null,
                })
                .unwrap_or_default(),
            };
        }
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e.to_string())).unwrap_or_default(),
            };
        }
    };

    let sampled_at_ms = now_ms();
    let active = build_active_session_response(
        row.try_get::<i64, _>("id").unwrap_or(0),
        row.try_get::<String, _>("app_name").unwrap_or_default(),
        row.try_get::<String, _>("exe_name").unwrap_or_default(),
        row.try_get::<Option<String>, _>("window_title")
            .unwrap_or(None),
        row.try_get::<i64, _>("start_time").unwrap_or(0),
        row.try_get::<i64, _>("continuity_group_start_time")
            .unwrap_or(0),
        sampled_at_ms,
    );

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse { data: active }).unwrap_or_default(),
    }
}

fn build_active_session_response(
    id: i64,
    app_name: String,
    exe_name: String,
    window_title: Option<String>,
    start_time: i64,
    continuity_group_start_time: i64,
    sampled_at_ms: i64,
) -> ActiveSessionResponse {
    ActiveSessionResponse {
        id,
        app_name,
        exe_name,
        window_title,
        start_time,
        end_time: None,
        duration: sampled_at_ms.saturating_sub(start_time).max(0),
        continuity_group_start_time,
        sampled_at_ms,
    }
}

pub async fn get_summary_today(app: &tauri::AppHandle) -> RouteResponse {
    let range = local_today_range(chrono::Local::now().fixed_offset());
    build_summary_response(app, range.from_ms, range.to_ms, &range.label).await
}

pub async fn get_summary_range(app: &tauri::AppHandle, query: Option<&str>) -> RouteResponse {
    let params = parse_summary_query(query);
    let Some(from) = params.from else {
        return RouteResponse {
            status: 400,
            body: serde_json::to_value(ApiError::bad_request("missing 'from' parameter"))
                .unwrap_or_default(),
        };
    };
    let Some(to) = params.to else {
        return RouteResponse {
            status: 400,
            body: serde_json::to_value(ApiError::bad_request("missing 'to' parameter"))
                .unwrap_or_default(),
        };
    };

    let label = format!(
        "{}..{}",
        chrono::DateTime::from_timestamp_millis(from)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default(),
        chrono::DateTime::from_timestamp_millis(to)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    );

    build_summary_response(app, from, to, &label).await
}

pub async fn get_summary_week(app: &tauri::AppHandle) -> RouteResponse {
    let range = local_week_range(chrono::Local::now().fixed_offset());
    build_summary_response(app, range.from_ms, range.to_ms, &range.label).await
}

async fn build_summary_response(
    app: &tauri::AppHandle,
    from_ms: i64,
    to_ms: i64,
    label: &str,
) -> RouteResponse {
    let pool = match sqlite_pool::wait_for_sqlite_pool(app).await {
        Ok(p) => p,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e)).unwrap_or_default(),
            };
        }
    };

    let rows = match sqlx::query(
        "SELECT exe_name, SUM(duration) as total_ms
         FROM sessions
         WHERE end_time IS NOT NULL
           AND start_time >= ? AND start_time <= ?
         GROUP BY exe_name
         ORDER BY total_ms DESC",
    )
    .bind(from_ms)
    .bind(to_ms)
    .fetch_all(&pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e.to_string())).unwrap_or_default(),
            };
        }
    };

    let mut total_active_ms: i64 = 0;
    let mut app_map: Vec<(String, i64)> = Vec::new();

    for row in &rows {
        let exe: String = row.try_get("exe_name").unwrap_or_default();
        let ms: i64 = row.try_get("total_ms").unwrap_or(0);
        total_active_ms += ms;
        app_map.push((exe, ms));
    }

    let apps: Vec<AppSummaryEntry> = app_map
        .iter()
        .map(|(exe, ms)| AppSummaryEntry {
            exe_name: exe.clone(),
            total_ms: *ms,
            percentage: if total_active_ms > 0 {
                (*ms as f64 / total_active_ms as f64) * 100.0
            } else {
                0.0
            },
        })
        .collect();

    // Category aggregation: load category overrides from settings
    let category_rows =
        sqlx::query("SELECT key, value FROM settings WHERE key LIKE '__app_category::%'")
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

    let mut category_map: HashMap<String, i64> = HashMap::new();
    let mut exe_to_category: HashMap<String, String> = HashMap::new();

    for row in &category_rows {
        let key: String = row.try_get("key").unwrap_or_default();
        let value: String = row.try_get("value").unwrap_or_default();
        if let Some(exe) = key.strip_prefix("__app_category::") {
            exe_to_category.insert(exe.to_string(), value);
        }
    }

    for (exe, ms) in &app_map {
        if let Some(cat) = exe_to_category.get(exe) {
            *category_map.entry(cat.clone()).or_insert(0) += ms;
        }
    }

    let categories: Vec<CategorySummaryEntry> = category_map
        .into_iter()
        .map(|(name, ms)| CategorySummaryEntry { name, total_ms: ms })
        .collect();

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: SummaryResponse {
                date: label.to_string(),
                total_active_ms,
                apps,
                categories,
            },
        })
        .unwrap_or_default(),
    }
}

fn parse_session_query(query: Option<&str>) -> SessionQueryParams {
    let mut params = SessionQueryParams {
        from: None,
        to: None,
        app: None,
        limit: None,
    };
    if let Some(q) = query {
        for pair in q.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                match k {
                    "from" => params.from = v.parse().ok(),
                    "to" => params.to = v.parse().ok(),
                    "app" => params.app = Some(v.to_string()),
                    "limit" => params.limit = v.parse().ok(),
                    _ => {}
                }
            }
        }
    }
    params
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SummaryTimeRange {
    from_ms: i64,
    to_ms: i64,
    label: String,
}

fn local_today_range(now: chrono::DateTime<chrono::FixedOffset>) -> SummaryTimeRange {
    let start = now
        .offset()
        .clone()
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .unwrap_or(now);

    SummaryTimeRange {
        from_ms: start.timestamp_millis(),
        to_ms: now.timestamp_millis(),
        label: now.format("%Y-%m-%d").to_string(),
    }
}

fn local_week_range(now: chrono::DateTime<chrono::FixedOffset>) -> SummaryTimeRange {
    let days_from_monday = i64::from(now.weekday().num_days_from_monday());
    let week_start_date = now.date_naive() - chrono::Duration::days(days_from_monday);
    let start = now
        .offset()
        .clone()
        .with_ymd_and_hms(
            week_start_date.year(),
            week_start_date.month(),
            week_start_date.day(),
            0,
            0,
            0,
        )
        .single()
        .unwrap_or(now);

    SummaryTimeRange {
        from_ms: start.timestamp_millis(),
        to_ms: now.timestamp_millis(),
        label: "week".to_string(),
    }
}

fn parse_summary_query(query: Option<&str>) -> SummaryQueryParams {
    let mut params = SummaryQueryParams {
        from: None,
        to: None,
    };
    if let Some(q) = query {
        for pair in q.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                match k {
                    "from" => params.from = v.parse().ok(),
                    "to" => params.to = v.parse().ok(),
                    _ => {}
                }
            }
        }
    }
    params
}

#[cfg(test)]
mod local_summary_range_tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    #[test]
    fn local_today_range_uses_local_midnight_instead_of_utc_midnight() {
        let offset = FixedOffset::east_opt(8 * 3600).unwrap();
        let now = offset.with_ymd_and_hms(2026, 6, 21, 14, 30, 0).unwrap();

        let range = local_today_range(now);

        assert_eq!(range.label, "2026-06-21");
        assert_eq!(
            range.from_ms,
            offset
                .with_ymd_and_hms(2026, 6, 21, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(range.to_ms, now.timestamp_millis());
    }

    #[test]
    fn local_week_range_starts_on_local_monday() {
        let offset = FixedOffset::east_opt(8 * 3600).unwrap();
        let now = offset.with_ymd_and_hms(2026, 6, 21, 14, 30, 0).unwrap();

        let range = local_week_range(now);

        assert_eq!(range.label, "week");
        assert_eq!(
            range.from_ms,
            offset
                .with_ymd_and_hms(2026, 6, 15, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(range.to_ms, now.timestamp_millis());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_session_response_includes_realtime_duration_and_window_fields() {
        let response = build_active_session_response(
            42,
            "Ghostty".to_string(),
            "ghostty".to_string(),
            Some("patina".to_string()),
            1_000,
            800,
            4_500,
        );

        assert_eq!(response.id, 42);
        assert_eq!(response.app_name, "Ghostty");
        assert_eq!(response.exe_name, "ghostty");
        assert_eq!(response.window_title.as_deref(), Some("patina"));
        assert_eq!(response.start_time, 1_000);
        assert_eq!(response.end_time, None);
        assert_eq!(response.duration, 3_500);
        assert_eq!(response.continuity_group_start_time, 800);
        assert_eq!(response.sampled_at_ms, 4_500);
    }

    #[test]
    fn active_session_response_clamps_negative_realtime_duration() {
        let response = build_active_session_response(
            42,
            "Ghostty".to_string(),
            "ghostty".to_string(),
            None,
            5_000,
            5_000,
            4_500,
        );

        assert_eq!(response.duration, 0);
    }
}
