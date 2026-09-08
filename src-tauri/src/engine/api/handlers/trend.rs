use crate::data::repositories::activity_read_model;
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ApiError, ApiResponse, RouteResponse, TrendDataPoint, TrendResponse,
};
use chrono::{Datelike, TimeZone};
use std::collections::HashMap;

pub async fn get_trend(context: &ApiRuntimeContext, query: Option<&str>) -> RouteResponse {
    let params = parse_trend_query(query);
    let range = match resolve_trend_range(
        params.period.as_deref(),
        params.granularity.as_deref(),
        chrono::Local
            .timestamp_millis_opt(context.now_ms())
            .single()
            .unwrap_or_else(chrono::Local::now)
            .fixed_offset(),
    ) {
        Ok(range) => range,
        Err(message) => {
            return RouteResponse {
                status: 400,
                body: serde_json::to_value(ApiError::bad_request(message)).unwrap_or_default(),
            };
        }
    };

    let pool = context.pool();

    let snapshot =
        match activity_read_model::load_snapshot(pool, range.from_ms, range.to_ms, range.to_ms)
            .await
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return RouteResponse {
                    status: 500,
                    body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
                };
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
    let daily_contributions = range
        .day_starts
        .iter()
        .map(|day_start| {
            let day_start_ms = day_start.timestamp_millis();
            let day_end_ms = (*day_start + chrono::Duration::days(1))
                .timestamp_millis()
                .min(range.to_ms);
            snapshot
                .contributions(day_start_ms, day_end_ms)
                .into_iter()
                .filter(|contribution| !app_semantics.is_excluded(&contribution.value.exe_name))
                .map(|contribution| TrendContributionInput {
                    exe_name: contribution.value.exe_name,
                    duration_ms: contribution.duration_ms,
                })
                .collect()
        })
        .collect();

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: build_daily_trend(range, daily_contributions),
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
struct TrendContributionInput {
    exe_name: String,
    duration_ms: i64,
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

fn build_daily_trend(
    range: TrendRange,
    daily_contributions: Vec<Vec<TrendContributionInput>>,
) -> TrendResponse {
    let data_points = range
        .day_starts
        .iter()
        .enumerate()
        .map(|(index, day_start)| {
            let mut app_totals = HashMap::<String, i64>::new();
            for contribution in daily_contributions.get(index).into_iter().flatten() {
                if contribution.duration_ms > 0 {
                    *app_totals
                        .entry(normalize_app_key(&contribution.exe_name))
                        .or_insert(0) += contribution.duration_ms;
                }
            }
            TrendDataPoint {
                date: day_start.format("%Y-%m-%d").to_string(),
                active_ms: app_totals.values().copied().sum(),
                top_app: resolve_top_app(&app_totals),
            }
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

fn normalize_app_key(exe_name: &str) -> String {
    exe_name.trim().to_ascii_lowercase()
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
    fn daily_trend_uses_resolved_contributions_for_each_local_day() {
        let offset = FixedOffset::east_opt(8 * 3600).unwrap();
        let now = offset.with_ymd_and_hms(2026, 6, 21, 14, 30, 0).unwrap();
        let range = resolve_trend_range(Some("week"), Some("day"), now).unwrap();
        let mut daily = (0..7).map(|_| Vec::new()).collect::<Vec<_>>();
        daily[5].push(TrendContributionInput {
            exe_name: "ghostty".to_string(),
            duration_ms: 10 * 60 * 1000,
        });
        daily[6].push(TrendContributionInput {
            exe_name: "ghostty".to_string(),
            duration_ms: 10 * 60 * 1000,
        });

        let trend = build_daily_trend(range, daily);

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
    fn daily_trend_sums_multiple_contributions_and_selects_top_app() {
        let offset = FixedOffset::east_opt(8 * 3600).unwrap();
        let now = offset.with_ymd_and_hms(2026, 6, 21, 14, 30, 0).unwrap();
        let range = resolve_trend_range(Some("week"), Some("day"), now).unwrap();
        let mut daily = (0..7).map(|_| Vec::new()).collect::<Vec<_>>();
        daily[6] = vec![
            TrendContributionInput {
                exe_name: "obsidian".to_string(),
                duration_ms: 30 * 60 * 1000,
            },
            TrendContributionInput {
                exe_name: "ghostty".to_string(),
                duration_ms: 10 * 60 * 1000,
            },
        ];

        let trend = build_daily_trend(range, daily);

        let today = trend
            .data_points
            .iter()
            .find(|point| point.date == "2026-06-21")
            .unwrap();
        assert_eq!(today.active_ms, 40 * 60 * 1000);
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
