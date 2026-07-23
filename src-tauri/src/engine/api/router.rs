use crate::engine::api::{handlers, types::ApiError, types::RouteResponse};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiRequest {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub body: Vec<u8>,
}

pub(crate) async fn route_request(
    request: ApiRequest,
    context: &crate::engine::api::context::ApiRuntimeContext,
    surface: crate::engine::api::surface::ApiSurface,
) -> RouteResponse {
    let method = request.method.as_str();
    let path = request.path.as_str();
    let query = request.query.as_deref();
    let body = request.body.as_slice();
    if !surface.allows_request(method, path) {
        return RouteResponse {
            status: 404,
            body: serde_json::to_value(ApiError::not_found("endpoint not available"))
                .unwrap_or_default(),
        };
    }
    match (method, path) {
        ("GET", "/api/v1/health") => handlers::health::get_health(context),
        ("GET", "/api/v1/capabilities") => {
            handlers::capabilities::get_capabilities(context, surface)
        }
        ("GET", "/api/v1/openapi.json") => handlers::openapi::get_openapi(surface),
        ("GET", "/api/v1/diagnostics") => handlers::diagnostics::get_diagnostics(context).await,
        ("GET", "/api/v1/current") => handlers::health::get_current(context),
        ("GET", "/api/v1/sessions") => handlers::sessions::get_sessions(context, query).await,
        ("GET", "/api/v1/sessions/active") => handlers::sessions::get_active_session(context).await,
        ("GET", "/api/v1/summary/today") => handlers::sessions::get_summary_today(context).await,
        ("GET", "/api/v1/summary/range") => {
            handlers::sessions::get_summary_range(context, query).await
        }
        ("GET", "/api/v1/summary/week") => handlers::sessions::get_summary_week(context).await,
        ("GET", "/api/v1/trend") => handlers::trend::get_trend(context, query).await,
        ("GET", "/api/v1/web-activity") => {
            handlers::web_activity::get_web_activity(context, query).await
        }
        ("GET", "/api/v1/ai/activity-context") => handlers::ai::get_activity_context(context).await,
        ("GET", "/api/v1/apps") => handlers::apps::get_apps(context).await,
        ("POST", path) if path.starts_with("/api/v1/apps/") => {
            handlers::apps::handle_app_action(context, path, body).await
        }
        ("GET", "/api/v1/settings/tracker") => {
            handlers::settings::get_tracker_settings(context).await
        }
        ("POST", "/api/v1/settings/tracker/afk-threshold") => {
            handlers::settings::set_afk_threshold(context, body).await
        }
        ("POST", "/api/v1/settings/tracker/pause") => {
            handlers::settings::set_tracking_paused(context, body).await
        }
        ("POST", "/api/v1/settings/classification") => {
            handlers::classification::commit_classification_settings(context, body).await
        }
        ("GET", "/api/v1/tools/snapshot") => handlers::tools::get_tools_snapshot(context).await,
        _ => RouteResponse {
            status: 404,
            body: serde_json::to_value(ApiError::not_found("endpoint not found"))
                .unwrap_or_default(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::api::context::ApiRuntimeContext;
    use crate::engine::runtime_event::RuntimeEventSink;
    use sqlx::Executor;
    use std::sync::Arc;

    async fn test_context() -> (
        sqlx::SqlitePool,
        ApiRuntimeContext,
        Arc<crate::engine::runtime_event::MemoryRuntimeEventSink>,
    ) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        let sink = Arc::new(crate::engine::runtime_event::MemoryRuntimeEventSink::default());
        let event_sink: Arc<dyn RuntimeEventSink> = sink.clone();
        let context = ApiRuntimeContext::with_state_and_events(
            crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
            "1.8.3",
            "linux",
            Arc::new(crate::engine::api::context::UnavailableApiRuntimeState),
            Some(event_sink),
        );
        (pool, context, sink)
    }

    async fn route(
        context: &ApiRuntimeContext,
        surface: crate::engine::api::surface::ApiSurface,
        method: &str,
        path: &str,
        body: serde_json::Value,
    ) -> RouteResponse {
        route_request(
            ApiRequest {
                method: method.to_string(),
                path: path.to_string(),
                query: None,
                body: if body.is_null() {
                    Vec::new()
                } else {
                    serde_json::to_vec(&body).unwrap()
                },
            },
            context,
            surface,
        )
        .await
    }

    #[tokio::test]
    async fn daemon_tracking_writes_preserve_app_override_shape_and_emit_refresh_events() {
        let (pool, context, sink) = test_context().await;

        let rejected = route(
            &context,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
            "POST",
            "/api/v1/apps/my%20app/rename",
            serde_json::json!({"display_name": "My App"}),
        )
        .await;
        assert_eq!(rejected.status, 404);

        let surface = crate::engine::api::surface::ApiSurface::DaemonTracking;
        assert_eq!(
            route(
                &context,
                surface,
                "POST",
                "/api/v1/apps/my%20app/rename",
                serde_json::json!({"display_name": "My App"}),
            )
            .await
            .status,
            200
        );
        assert_eq!(
            route(
                &context,
                surface,
                "POST",
                "/api/v1/apps/my%20app/classify",
                serde_json::json!({"category": "development"}),
            )
            .await
            .status,
            200
        );
        assert_eq!(
            route(
                &context,
                surface,
                "POST",
                "/api/v1/apps/my%20app/exclude",
                serde_json::json!({"excluded": true}),
            )
            .await
            .status,
            200
        );

        let stored = crate::data::repositories::tracker_settings::load_setting_value(
            &pool,
            "__app_override::my app",
        )
        .await
        .unwrap()
        .unwrap();
        let stored: serde_json::Value = serde_json::from_str(&stored).unwrap();
        assert_eq!(stored["displayName"], "My App");
        assert_eq!(stored["category"], "development");
        assert_eq!(stored["track"], false);
        assert!(stored.get("display_name").is_none());
        assert_eq!(sink.events().len(), 3);
        assert!(sink.events().iter().all(|event| matches!(
            event,
            crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged { reason, .. }
                if reason == crate::domain::tracking::TRACKING_REASON_CLASSIFICATION_CHANGED
        )));

        pool.close().await;
    }

    #[tokio::test]
    async fn daemon_tracker_and_classification_writes_are_validated_and_advertised() {
        let (pool, context, sink) = test_context().await;
        let surface = crate::engine::api::surface::ApiSurface::DaemonTracking;

        let capabilities = route(
            &context,
            surface,
            "GET",
            "/api/v1/capabilities",
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(capabilities.body["data"]["server_version"], "1.8.3");
        assert_eq!(capabilities.body["data"]["protocol"]["current"], 1);
        assert_eq!(capabilities.body["data"]["write_api"]["available"], true);

        assert_eq!(
            route(
                &context,
                surface,
                "POST",
                "/api/v1/settings/tracker/afk-threshold",
                serde_json::json!({"seconds": 30}),
            )
            .await
            .status,
            400
        );
        assert_eq!(
            route(
                &context,
                surface,
                "POST",
                "/api/v1/settings/tracker/afk-threshold",
                serde_json::json!({"seconds": 900}),
            )
            .await
            .status,
            200
        );
        assert_eq!(
            route(
                &context,
                surface,
                "POST",
                "/api/v1/settings/tracker/pause",
                serde_json::json!({"paused": true}),
            )
            .await
            .status,
            200
        );
        assert_eq!(
            crate::data::repositories::tracker_settings::load_tracking_paused_setting(&pool)
                .await
                .unwrap(),
            true
        );

        let invalid_batch = route(
            &context,
            surface,
            "POST",
            "/api/v1/settings/classification",
            serde_json::json!({
                "mutations": [{"key": "tracking_paused", "value": "0"}]
            }),
        )
        .await;
        assert_eq!(invalid_batch.status, 400);
        assert_eq!(
            crate::data::repositories::tracker_settings::load_tracking_paused_setting(&pool)
                .await
                .unwrap(),
            true
        );

        let valid_batch = route(
            &context,
            surface,
            "POST",
            "/api/v1/settings/classification",
            serde_json::json!({
                "mutations": [{
                    "key": "__web_domain_override::example.com",
                    "value": "{\"category\":\"research\",\"enabled\":true}"
                }]
            }),
        )
        .await;
        assert_eq!(valid_batch.status, 200);
        assert_eq!(
            crate::data::repositories::tracker_settings::load_setting_value(
                &pool,
                "__web_domain_override::example.com",
            )
            .await
            .unwrap(),
            Some("{\"category\":\"research\",\"enabled\":true}".to_string())
        );
        assert_eq!(sink.events().len(), 3);

        pool.close().await;
    }
}
