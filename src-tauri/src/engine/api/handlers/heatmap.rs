use crate::data::repositories::daily_activity::load_daily_activity;
use crate::domain::daily_activity::local_day_boundaries;
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{ApiError, ApiResponse, RouteResponse};

pub async fn get_heatmap(context: &ApiRuntimeContext, query: Option<&str>) -> RouteResponse {
    let boundaries = match parse_boundaries(query) {
        Ok(boundaries) => boundaries,
        Err(message) => return error_response(400, ApiError::bad_request(&message)),
    };
    match load_daily_activity(context.pool(), &boundaries, context.now_ms()).await {
        Ok(data) => RouteResponse {
            status: 200,
            body: serde_json::to_value(ApiResponse { data }).unwrap_or_default(),
        },
        Err(message) => error_response(500, ApiError::internal(&message)),
    }
}

fn error_response(status: u16, error: ApiError) -> RouteResponse {
    RouteResponse {
        status,
        body: serde_json::to_value(error).unwrap_or_default(),
    }
}

fn parse_boundaries(query: Option<&str>) -> Result<Vec<i64>, String> {
    let mut from = None;
    let mut to = None;
    for (key, value) in url::form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        let target = match key.as_ref() {
            "from" => &mut from,
            "to" => &mut to,
            _ => return Err("unsupported heatmap query parameter".to_string()),
        };
        if target.is_some() {
            return Err("duplicate heatmap query parameter".to_string());
        }
        *target = Some(value.into_owned());
    }
    local_day_boundaries(
        &from.ok_or("heatmap from date is required")?,
        &to.ok_or("heatmap exclusive to date is required")?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_real_local_calendar_boundaries() {
        const CHILD: &str = "PATINA_HEATMAP_CALENDAR_TEST";
        if let Ok(zone) = std::env::var(CHILD) {
            let cases = match zone.as_str() {
                "America/New_York" => vec![
                    ("2026-03-08", "2026-03-09", 23.0),
                    ("2026-11-01", "2026-11-02", 25.0),
                ],
                "Australia/Lord_Howe" => vec![
                    ("2026-04-05", "2026-04-06", 24.5),
                    ("2026-10-04", "2026-10-05", 23.5),
                ],
                "Pacific/Apia" => {
                    assert!(parse_boundaries(Some("from=2011-12-29&to=2011-12-31")).is_err());
                    return;
                }
                _ => vec![
                    ("2026-03-08", "2026-03-09", 24.0),
                    ("2026-11-01", "2026-11-02", 24.0),
                ],
            };
            for (from, to, hours) in cases {
                let boundaries = parse_boundaries(Some(&format!("from={from}&to={to}"))).unwrap();
                assert_eq!(boundaries.len(), 2);
                assert_eq!(boundaries[1] - boundaries[0], (hours * 3_600_000.0) as i64);
            }
            return;
        }
        // A child process owns TZ; parallel tests never observe a mutated timezone.
        for zone in [
            "UTC",
            "Asia/Singapore",
            "America/New_York",
            "Australia/Lord_Howe",
            "Pacific/Apia",
        ] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "engine::api::handlers::heatmap::tests::resolves_real_local_calendar_boundaries"])
                .env(CHILD, zone).env("TZ", zone).output().unwrap();
            assert!(
                output.status.success(),
                "{zone}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[test]
    fn requires_bounded_unambiguous_calendar_dates() {
        for query in [
            "",
            "from=2026-01-01",
            "from=2026-01-01&to=2026-01-01",
            "from=2026-01-01&to=2028-01-01",
            "from=2026-02-30&to=2026-03-03",
            "from=2026-1-1&to=2026-01-03",
            "from=2026-01-01&to=2026-01-03&from=2026-01-02",
            "from=2026-01-01&to=2026-01-03&timezone=UTC",
        ] {
            assert!(parse_boundaries(Some(query)).is_err(), "{query}");
        }
        let boundaries = parse_boundaries(Some("from=2026-01-01&to=2026-01-03")).unwrap();
        assert_eq!(boundaries.len(), 3);
        assert!(boundaries.windows(2).all(|pair| pair[1] > pair[0]));
    }
}
