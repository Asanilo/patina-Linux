use crate::data::repositories::activity_read_model;
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ActiveSessionResponse, ApiError, ApiResponse, AppSummaryEntry, CategorySummaryEntry,
    RouteResponse, SessionEntry, SessionQueryParams, SessionsResponse, SummaryQueryParams,
    SummaryResponse,
};
use chrono::{Datelike, TimeZone};
use sqlx::Row;
use std::collections::HashMap;

pub async fn get_sessions(context: &ApiRuntimeContext, query: Option<&str>) -> RouteResponse {
    let pool = context.pool();
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

    let rows = match sqlx::query(&sql).fetch_all(pool).await {
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

pub async fn get_active_session(context: &ApiRuntimeContext) -> RouteResponse {
    let pool = context.pool();
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
    .fetch_optional(pool)
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

    let sampled_at_ms = context.now_ms();
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

pub async fn get_summary_today(context: &ApiRuntimeContext) -> RouteResponse {
    let range = local_today_range(local_now(context));
    build_summary_response(context, range.from_ms, range.to_ms, &range.label).await
}

pub async fn get_summary_range(context: &ApiRuntimeContext, query: Option<&str>) -> RouteResponse {
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
    if to <= from {
        return RouteResponse {
            status: 400,
            body: serde_json::to_value(ApiError::bad_request("'to' must be greater than 'from'"))
                .unwrap_or_default(),
        };
    }

    let label = format!(
        "{}..{}",
        chrono::DateTime::from_timestamp_millis(from)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default(),
        chrono::DateTime::from_timestamp_millis(to)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default()
    );

    build_summary_response(context, from, to, &label).await
}

pub async fn get_summary_week(context: &ApiRuntimeContext) -> RouteResponse {
    let range = local_week_range(local_now(context));
    build_summary_response(context, range.from_ms, range.to_ms, &range.label).await
}

async fn build_summary_response(
    context: &ApiRuntimeContext,
    from_ms: i64,
    to_ms: i64,
    label: &str,
) -> RouteResponse {
    let pool = context.pool();
    let sampled_at_ms = context.now_ms();
    // A calendar period has no elapsed time at its exact opening boundary.
    let contributions = if from_ms == to_ms {
        Vec::new()
    } else {
        match activity_read_model::load_snapshot(pool, from_ms, to_ms, sampled_at_ms).await {
            Ok(snapshot) => snapshot.contributions(from_ms, to_ms),
            Err(error) => {
                return RouteResponse {
                    status: 500,
                    body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
                };
            }
        }
    };

    let app_semantics = match activity_read_model::load_app_semantics(pool).await {
        Ok(semantics) => semantics,
        Err(error) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
            };
        }
    };
    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: build_summary_from_contributions(label, contributions, &app_semantics),
        })
        .unwrap_or_default(),
    }
}

fn build_summary_from_contributions(
    label: &str,
    contributions: Vec<
        crate::domain::activity_read_model::ActivityContribution<activity_read_model::ActivityFact>,
    >,
    app_semantics: &activity_read_model::ActivityAppSemantics,
) -> SummaryResponse {
    let mut app_totals = HashMap::<String, i64>::new();
    let mut category_totals = HashMap::<String, i64>::new();

    for contribution in contributions {
        if contribution.duration_ms <= 0 {
            continue;
        }
        let fact = contribution.value;
        if app_semantics.is_excluded(&fact.exe_name) {
            continue;
        }
        let app_key = normalize_app_key(&fact.exe_name);
        *app_totals.entry(app_key.clone()).or_insert(0) += contribution.duration_ms;
        if let Some(category) = app_semantics
            .category_for(&app_key)
            .map(str::to_string)
            .or(fact.source_category)
            .filter(|category| !category.trim().is_empty())
        {
            *category_totals.entry(category).or_insert(0) += contribution.duration_ms;
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

fn normalize_app_key(exe_name: &str) -> String {
    exe_name.trim().to_ascii_lowercase()
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

fn local_now(context: &ApiRuntimeContext) -> chrono::DateTime<chrono::FixedOffset> {
    chrono::Local
        .timestamp_millis_opt(context.now_ms())
        .single()
        .unwrap_or_else(chrono::Local::now)
        .fixed_offset()
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

    #[tokio::test]
    async fn calendar_summary_at_midnight_is_empty_but_explicit_empty_range_is_rejected() {
        let root = std::env::temp_dir().join(format!(
            "patina-summary-midnight-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(
            &root.join("patina.db"),
            true,
        )
        .await
        .unwrap();
        let runtime = crate::engine::runtime_context::RuntimeContext::system(pool.clone());
        let context = ApiRuntimeContext::new(runtime);
        for seconds in [0, 8 * 3600, -5 * 3600] {
            let midnight = FixedOffset::east_opt(seconds)
                .unwrap()
                .with_ymd_and_hms(2026, 6, 22, 0, 0, 0)
                .unwrap();
            for range in [local_today_range(midnight), local_week_range(midnight)] {
                assert_eq!(range.from_ms, range.to_ms);
                let response =
                    build_summary_response(&context, range.from_ms, range.to_ms, &range.label)
                        .await;
                assert_eq!(response.status, 200, "{}", response.body);
                assert_eq!(response.body["data"]["total_active_ms"], 0);
            }
        }
        assert_eq!(
            get_summary_range(&context, Some("from=1&to=1"))
                .await
                .status,
            400
        );
        pool.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }

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
    use crate::domain::activity_read_model::{ActivityContribution, ActivityOrigin};
    use std::collections::HashMap;

    fn contribution(
        exe_name: &str,
        duration_ms: i64,
        source_category: Option<&str>,
    ) -> ActivityContribution<activity_read_model::ActivityFact> {
        ActivityContribution {
            origin: ActivityOrigin::Native,
            duration_ms,
            value: activity_read_model::ActivityFact {
                record_id: 1,
                app_name: exe_name.to_string(),
                exe_name: exe_name.to_string(),
                window_title: String::new(),
                source_category: source_category.map(str::to_string),
            },
        }
    }

    fn semantics(
        categories: HashMap<String, String>,
        excluded: &[&str],
    ) -> activity_read_model::ActivityAppSemantics {
        activity_read_model::ActivityAppSemantics {
            categories,
            excluded: excluded.iter().map(|value| value.to_string()).collect(),
        }
    }

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
    fn summary_aggregates_resolved_contributions_and_categories() {
        let contributions = vec![
            contribution("ghostty", 500, None),
            contribution("ghostty", 500, None),
            contribution("obsidian", 1_500, Some("Imported writing")),
        ];
        let categories = HashMap::from([
            ("ghostty".to_string(), "Development".to_string()),
            ("obsidian".to_string(), "Writing".to_string()),
        ]);

        let summary =
            build_summary_from_contributions("range", contributions, &semantics(categories, &[]));

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
    fn summary_uses_imported_category_when_no_local_override_exists() {
        let summary = build_summary_from_contributions(
            "range",
            vec![contribution("imported", 1_000, Some("Imported category"))],
            &semantics(HashMap::new(), &[]),
        );

        assert_eq!(summary.total_active_ms, 1_000);
        assert_eq!(summary.apps[0].total_ms, 1_000);
        assert_eq!(summary.categories[0].name, "Imported category");
        assert_eq!(summary.categories[0].total_ms, 1_000);
    }

    #[test]
    fn summary_omits_excluded_apps() {
        let summary = build_summary_from_contributions(
            "range",
            vec![
                contribution("ghostty", 1_000, None),
                contribution("obsidian", 2_000, None),
            ],
            &semantics(HashMap::new(), &["ghostty"]),
        );

        assert_eq!(summary.total_active_ms, 2_000);
        assert_eq!(summary.apps.len(), 1);
        assert_eq!(summary.apps[0].exe_name, "obsidian");
    }
}
