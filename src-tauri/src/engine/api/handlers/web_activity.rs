use crate::data::repositories::web_activity::{
    query_segments, WebActivitySegmentQuery, WebActivitySegmentRecord,
};
use crate::data::sqlite_pool;
use crate::domain::settings::WebActivityUrlPrivacyMode;
use crate::engine::api::types::{
    ApiError, ApiResponse, RouteResponse, WebActivityEntry, WebActivityResponse,
};

pub async fn get_web_activity(app: &tauri::AppHandle, query: Option<&str>) -> RouteResponse {
    let pool = match sqlite_pool::wait_for_sqlite_pool(app).await {
        Ok(pool) => pool,
        Err(error) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
            };
        }
    };

    let query = parse_web_activity_query(query);
    let rows = match query_segments(&pool, &query, now_ms()).await {
        Ok(rows) => rows,
        Err(error) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&error.to_string()))
                    .unwrap_or_default(),
            };
        }
    };

    let settings =
        match crate::data::repositories::app_settings::load_web_activity_settings(&pool).await {
            Ok(settings) => settings,
            Err(error) => {
                return RouteResponse {
                    status: 500,
                    body: serde_json::to_value(ApiError::internal(&error.to_string()))
                        .unwrap_or_default(),
                };
            }
        };

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: build_web_activity_response(rows, settings.url_privacy),
        })
        .unwrap_or_default(),
    }
}

fn parse_web_activity_query(query: Option<&str>) -> WebActivitySegmentQuery {
    let mut parsed = WebActivitySegmentQuery::default();

    if let Some(query) = query {
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                match key {
                    "from" => parsed.from = value.parse().ok(),
                    "to" => parsed.to = value.parse().ok(),
                    "domain" => {
                        let trimmed = value.trim();
                        if !trimmed.is_empty() {
                            parsed.domain = Some(trimmed.to_string());
                        }
                    }
                    "limit" => parsed.limit = value.parse().ok(),
                    _ => {}
                }
            }
        }
    }

    parsed
}

fn build_web_activity_response(
    rows: Vec<WebActivitySegmentRecord>,
    url_privacy: WebActivityUrlPrivacyMode,
) -> WebActivityResponse {
    WebActivityResponse {
        items: rows
            .into_iter()
            .map(|row| WebActivityEntry {
                id: row.id,
                browser_client_id: row.browser_client_id,
                browser_kind: row.browser_kind,
                browser_exe_name: row.browser_exe_name,
                domain: row.domain,
                normalized_domain: row.normalized_domain,
                url: apply_url_privacy(row.url, url_privacy),
                title: row.title,
                favicon_url: row.favicon_url,
                start_time: row.start_time,
                end_time: row.end_time,
                duration: row.duration,
                source: row.source,
            })
            .collect(),
    }
}

fn apply_url_privacy(
    url: Option<String>,
    url_privacy: WebActivityUrlPrivacyMode,
) -> Option<String> {
    match url_privacy {
        WebActivityUrlPrivacyMode::Full => url,
        WebActivityUrlPrivacyMode::StripQuery => url.map(|value| strip_query_and_fragment(&value)),
        WebActivityUrlPrivacyMode::DomainOnly => None,
    }
}

fn strip_query_and_fragment(url: &str) -> String {
    let query_index = url.find('?');
    let fragment_index = url.find('#');
    let truncate_at = [query_index, fragment_index]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(url.len());
    url[..truncate_at].to_string()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_web_activity_query_accepts_range_domain_and_limit() {
        let query = parse_web_activity_query(Some(
            "from=1000&to=2000&domain=GitHub.com&limit=25&ignored=true",
        ));

        assert_eq!(query.from, Some(1000));
        assert_eq!(query.to, Some(2000));
        assert_eq!(query.domain.as_deref(), Some("GitHub.com"));
        assert_eq!(query.limit, Some(25));
    }

    #[test]
    fn web_activity_response_preserves_full_url_fields() {
        let response = build_web_activity_response(
            vec![WebActivitySegmentRecord {
                id: 1,
                browser_client_id: "client".into(),
                browser_kind: "chrome".into(),
                browser_exe_name: "chrome.exe".into(),
                domain: "github.com".into(),
                normalized_domain: "github.com".into(),
                url: Some("https://github.com/Ceceliaee/patina?tab=readme#install".into()),
                title: Some("Patina".into()),
                favicon_url: None,
                start_time: 1000,
                end_time: Some(2000),
                duration: 1000,
                source: "browser-extension".into(),
            }],
            WebActivityUrlPrivacyMode::Full,
        );

        assert_eq!(response.items.len(), 1);
        assert_eq!(
            response.items[0].url.as_deref(),
            Some("https://github.com/Ceceliaee/patina?tab=readme#install")
        );
        assert_eq!(response.items[0].domain, "github.com");
        assert_eq!(response.items[0].duration, 1000);
    }

    #[test]
    fn web_activity_response_can_strip_query_and_fragment_from_urls() {
        let response = build_web_activity_response(
            vec![WebActivitySegmentRecord {
                id: 1,
                browser_client_id: "client".into(),
                browser_kind: "chrome".into(),
                browser_exe_name: "chrome.exe".into(),
                domain: "github.com".into(),
                normalized_domain: "github.com".into(),
                url: Some("https://github.com/Ceceliaee/patina?tab=readme#install".into()),
                title: Some("Patina".into()),
                favicon_url: None,
                start_time: 1000,
                end_time: Some(2000),
                duration: 1000,
                source: "browser-extension".into(),
            }],
            WebActivityUrlPrivacyMode::StripQuery,
        );

        assert_eq!(
            response.items[0].url.as_deref(),
            Some("https://github.com/Ceceliaee/patina")
        );
    }

    #[test]
    fn web_activity_response_can_hide_urls_for_domain_only_api_access() {
        let response = build_web_activity_response(
            vec![WebActivitySegmentRecord {
                id: 1,
                browser_client_id: "client".into(),
                browser_kind: "firefox".into(),
                browser_exe_name: "zen".into(),
                domain: "example.com".into(),
                normalized_domain: "example.com".into(),
                url: Some("https://example.com/private/path?token=secret".into()),
                title: Some("Example".into()),
                favicon_url: None,
                start_time: 1000,
                end_time: Some(2000),
                duration: 1000,
                source: "browser-extension".into(),
            }],
            WebActivityUrlPrivacyMode::DomainOnly,
        );

        assert_eq!(response.items[0].url, None);
        assert_eq!(response.items[0].domain, "example.com");
    }
}
