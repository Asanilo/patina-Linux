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
    let sampled_at_ms = now_ms();
    let active_cutoff_ms = sampled_at_ms.min(to_ms);

    let rows = match sqlx::query(
        "SELECT exe_name, start_time, end_time
         FROM sessions
         WHERE start_time < ?
           AND COALESCE(end_time, ?) > ?
         ORDER BY start_time ASC",
    )
    .bind(to_ms)
    .bind(active_cutoff_ms)
    .bind(from_ms)
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

    let sessions = rows
        .iter()
        .map(|row| SummarySessionInput {
            exe_name: row.try_get::<String, _>("exe_name").unwrap_or_default(),
            start_time: row.try_get::<i64, _>("start_time").unwrap_or(0),
            end_time: row.try_get::<Option<i64>, _>("end_time").unwrap_or(None),
        })
        .collect();

    // Category aggregation: load category overrides from settings
    let category_rows =
        sqlx::query("SELECT key, value FROM settings WHERE key LIKE '__app_category::%'")
            .fetch_all(&pool)
            .await
            .unwrap_or_default();

    let mut exe_to_category: HashMap<String, String> = HashMap::new();

    for row in &category_rows {
        let key: String = row.try_get("key").unwrap_or_default();
        let value: String = row.try_get("value").unwrap_or_default();
        if let Some(exe) = key.strip_prefix("__app_category::") {
            exe_to_category.insert(exe.to_string(), value);
        }
    }

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: build_summary_from_sessions(
                label,
                from_ms,
                to_ms,
                sampled_at_ms,
                sessions,
                &exe_to_category,
            ),
        })
        .unwrap_or_default(),
    }
}

#[derive(Clone, Debug)]
struct SummarySessionInput {
    exe_name: String,
    start_time: i64,
    end_time: Option<i64>,
}

fn build_summary_from_sessions(
    label: &str,
    from_ms: i64,
    to_ms: i64,
    sampled_at_ms: i64,
    sessions: Vec<SummarySessionInput>,
    exe_to_category: &HashMap<String, String>,
) -> SummaryResponse {
    let mut app_totals = HashMap::<String, i64>::new();

    for session in sessions {
        let overlap_start = session.start_time.max(from_ms);
        let overlap_end = session.end_time.unwrap_or(sampled_at_ms).min(to_ms);
        let duration = overlap_end.saturating_sub(overlap_start);
        if duration > 0 {
            *app_totals.entry(session.exe_name).or_insert(0) += duration;
        }
    }

    let total_active_ms = app_totals.values().copied().sum();
    let mut apps = app_totals
        .iter()
        .map(|(exe_name, total_ms)| AppSummaryEntry {
            exe_name: exe_name.clone(),
            total_ms: *total_ms,
            percentage: if total_active_ms > 0 {
                (*total_ms as f64 / total_active_ms as f64) * 100.0
            } else {
                0.0
            },
        })
        .collect::<Vec<_>>();
    apps.sort_by(|left, right| {
        right
            .total_ms
            .cmp(&left.total_ms)
            .then_with(|| left.exe_name.cmp(&right.exe_name))
    });

    let mut category_totals = HashMap::<String, i64>::new();
    for (exe_name, total_ms) in &app_totals {
        if let Some(category) = exe_to_category.get(exe_name) {
            *category_totals.entry(category.clone()).or_insert(0) += total_ms;
        }
    }
    let mut categories = category_totals
        .into_iter()
        .map(|(name, total_ms)| CategorySummaryEntry { name, total_ms })
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| {
        right
            .total_ms
            .cmp(&left.total_ms)
            .then_with(|| left.name.cmp(&right.name))
    });

    SummaryResponse {
        date: label.to_string(),
        total_active_ms,
        apps,
        categories,
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
    use std::collections::HashMap;

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

    #[test]
    fn summary_clips_cross_boundary_sessions_and_counts_active_session_until_sample_time() {
        let sessions = vec![
            SummarySessionInput {
                exe_name: "ghostty".to_string(),
                start_time: 500,
                end_time: Some(1_500),
            },
            SummarySessionInput {
                exe_name: "ghostty".to_string(),
                start_time: 2_500,
                end_time: None,
            },
            SummarySessionInput {
                exe_name: "obsidian".to_string(),
                start_time: 1_500,
                end_time: Some(3_500),
            },
        ];
        let categories = HashMap::from([
            ("ghostty".to_string(), "Development".to_string()),
            ("obsidian".to_string(), "Writing".to_string()),
        ]);

        let summary =
            build_summary_from_sessions("range", 1_000, 3_000, 3_000, sessions, &categories);

        assert_eq!(summary.total_active_ms, 2_500);
        assert_eq!(summary.apps.len(), 2);
        assert_eq!(
            summary
                .apps
                .iter()
                .find(|app| app.exe_name == "ghostty")
                .map(|app| app.total_ms),
            Some(1_000),
        );
        assert_eq!(
            summary
                .apps
                .iter()
                .find(|app| app.exe_name == "obsidian")
                .map(|app| app.total_ms),
            Some(1_500),
        );
        assert_eq!(
            summary
                .categories
                .iter()
                .find(|category| category.name == "Development")
                .map(|category| category.total_ms),
            Some(1_000),
        );
        assert_eq!(
            summary
                .categories
                .iter()
                .find(|category| category.name == "Writing")
                .map(|category| category.total_ms),
            Some(1_500),
        );
    }

    #[test]
    fn summary_does_not_count_active_session_past_sample_time() {
        let summary = build_summary_from_sessions(
            "range",
            1_000,
            5_000,
            3_000,
            vec![SummarySessionInput {
                exe_name: "ghostty".to_string(),
                start_time: 2_000,
                end_time: None,
            }],
            &HashMap::new(),
        );

        assert_eq!(summary.total_active_ms, 1_000);
        assert_eq!(summary.apps[0].total_ms, 1_000);
    }
}
