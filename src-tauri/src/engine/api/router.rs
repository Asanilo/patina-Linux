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
        ("GET", "/api/v1/imports") => handlers::activity_import::list_batches(context).await,
        ("POST", "/api/v1/imports/canonical/commit") => {
            handlers::activity_import::commit_staged(context, body).await
        }
        ("POST", path) if path.starts_with("/api/v1/imports/") => {
            handlers::activity_import::delete_batch(context, path, body).await
        }
        ("GET", "/api/v1/backups/schedule") => {
            handlers::scheduled_backup::get_snapshot(context).await
        }
        ("POST", "/api/v1/backups/schedule") => {
            handlers::scheduled_backup::save_config(context, body).await
        }
        ("POST", path) if path.starts_with("/api/v1/apps/") => {
            handlers::apps::handle_app_action(context, path, body).await
        }
        ("GET", "/api/v1/settings/tracker") => {
            handlers::settings::get_tracker_settings(context).await
        }
        ("GET", "/api/v1/settings/runtime") => {
            handlers::runtime_settings::get_runtime_settings(context).await
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
        ("POST", "/api/v1/settings/app") => {
            handlers::app_settings::commit_app_settings(context, body).await
        }
        ("POST", "/api/v1/settings/runtime/audio-participation") => {
            handlers::runtime_settings::set_audio_participation(context, body).await
        }
        ("POST", "/api/v1/settings/runtime/browser-activity") => {
            handlers::runtime_settings::configure_browser_activity(context, body).await
        }
        ("GET", "/api/v1/settings/local-api") => {
            handlers::local_api::get_configuration(context).await
        }
        ("POST", "/api/v1/settings/local-api/port") => {
            handlers::local_api::apply_port(context, body).await
        }
        ("POST", "/api/v1/settings/local-api/token/rotate") => {
            handlers::local_api::rotate_token(context).await
        }
        ("POST", "/api/v1/data/cleanup") => {
            handlers::data_maintenance::delete_tracking_data_before(context, body).await
        }
        ("POST", "/api/v1/data/window-titles/clear") => {
            handlers::data_maintenance::clear_window_titles(context, body).await
        }
        ("GET", "/api/v1/system/service") => handlers::service::get_service(context).await,
        ("POST", "/api/v1/system/service/restart") => {
            handlers::service::restart_service(context, body).await
        }
        ("GET", "/api/v1/tools/snapshot") => handlers::tools::get_tools_snapshot(context).await,
        ("POST", path) if path.starts_with("/api/v1/tools/") => {
            handlers::tools::handle_tools_action(context, path, body).await
        }
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

    #[derive(Default)]
    struct TestRuntimeControl {
        audio_enabled: std::sync::Mutex<Option<bool>>,
        browser_configuration: std::sync::Mutex<
            Option<crate::engine::api::runtime_control::BrowserActivityRuntimeConfiguration>,
        >,
    }

    impl crate::engine::api::runtime_control::ApiRuntimeControl for TestRuntimeControl {
        fn daemon_service_managed(&self) -> bool {
            true
        }

        fn daemon_service_snapshot(
            &self,
        ) -> crate::engine::api::runtime_control::RuntimeControlFuture<
            '_,
            crate::engine::api::runtime_control::DaemonServiceRuntimeSnapshot,
        > {
            Box::pin(async { Ok(test_service_snapshot(None)) })
        }

        fn request_daemon_service_restart(
            &self,
        ) -> crate::engine::api::runtime_control::RuntimeControlFuture<
            '_,
            crate::engine::api::runtime_control::DaemonServiceRestartResult,
        > {
            Box::pin(async {
                Ok(
                    crate::engine::api::runtime_control::DaemonServiceRestartResult {
                        service: test_service_snapshot(Some("pending")),
                        reconnect_required: true,
                    },
                )
            })
        }

        fn set_audio_participation_enabled(
            &self,
            enabled: bool,
        ) -> crate::engine::api::runtime_control::RuntimeControlFuture<'_, bool> {
            Box::pin(async move {
                *self.audio_enabled.lock().unwrap() = Some(enabled);
                Ok(enabled)
            })
        }

        fn configure_browser_activity(
            &self,
            configuration: crate::engine::api::runtime_control::BrowserActivityRuntimeConfiguration,
        ) -> crate::engine::api::runtime_control::RuntimeControlFuture<
            '_,
            crate::engine::api::runtime_control::BrowserActivityRuntimeConfiguration,
        > {
            Box::pin(async move {
                *self.browser_configuration.lock().unwrap() = Some(configuration.clone());
                Ok(configuration)
            })
        }

        fn local_api_snapshot(
            &self,
        ) -> crate::engine::api::runtime_control::RuntimeControlFuture<
            '_,
            crate::engine::api::runtime_control::LocalApiRuntimeSnapshot,
        > {
            Box::pin(async {
                Ok(
                    crate::engine::api::runtime_control::LocalApiRuntimeSnapshot {
                        port: 14_840,
                        base_url: "http://127.0.0.1:14840".to_string(),
                        token_path: "/tmp/patina-test-token".to_string(),
                        token_present: true,
                    },
                )
            })
        }

        fn apply_local_api_port(
            &self,
            _context: crate::engine::api::context::ApiRuntimeContext,
            port: u16,
        ) -> crate::engine::api::runtime_control::RuntimeControlFuture<
            '_,
            crate::engine::api::runtime_control::LocalApiPortApplyResult,
        > {
            Box::pin(async move {
                Ok(
                    crate::engine::api::runtime_control::LocalApiPortApplyResult {
                        configuration:
                            crate::engine::api::runtime_control::LocalApiRuntimeSnapshot {
                                port,
                                base_url: format!("http://127.0.0.1:{port}"),
                                token_path: "/tmp/patina-test-token".to_string(),
                                token_present: true,
                            },
                        previous_port: 14_840,
                        reconnect_required: port != 14_840,
                    },
                )
            })
        }

        fn rotate_local_api_token(
            &self,
        ) -> crate::engine::api::runtime_control::RuntimeControlFuture<
            '_,
            crate::engine::api::runtime_control::LocalApiTokenRotationResult,
        > {
            Box::pin(async {
                Ok(
                    crate::engine::api::runtime_control::LocalApiTokenRotationResult {
                        configuration:
                            crate::engine::api::runtime_control::LocalApiRuntimeSnapshot {
                                port: 14_840,
                                base_url: "http://127.0.0.1:14840".to_string(),
                                token_path: "/tmp/patina-test-token".to_string(),
                                token_present: true,
                            },
                        reauthentication_required: true,
                    },
                )
            })
        }
    }

    fn test_service_snapshot(
        restart_status: Option<&str>,
    ) -> crate::engine::api::runtime_control::DaemonServiceRuntimeSnapshot {
        crate::engine::api::runtime_control::DaemonServiceRuntimeSnapshot {
            service_name: "patinad.service".to_string(),
            managed_by_systemd: true,
            instance_id: "instance_test".to_string(),
            restart: restart_status.map(|status| {
                crate::engine::api::runtime_control::DaemonServiceRestartSnapshot {
                    request_id: "restart_test".to_string(),
                    status: status.to_string(),
                    requested_at_ms: 1_000,
                    requested_instance_id: "instance_test".to_string(),
                    completed_at_ms: None,
                    completed_instance_id: None,
                }
            }),
        }
    }

    #[derive(Default)]
    struct TestToolsSink;

    struct TestRuntimeState;

    impl crate::engine::api::context::ApiRuntimeStateProvider for TestRuntimeState {
        fn tracking_snapshot(
            &self,
        ) -> Option<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshot> {
            None
        }

        fn web_activity_snapshot(
            &self,
            _settings: &crate::domain::settings::WebActivitySettings,
            _now_ms: i64,
        ) -> Option<crate::domain::web_activity::WebActivityBridgeSnapshot> {
            None
        }

        fn tools_runtime_ready(&self) -> bool {
            true
        }
    }

    impl crate::engine::tools::ToolsRuntimeSink for TestToolsSink {
        fn snapshot_changed(&self, _snapshot: &crate::domain::tools::ToolsRuntimeSnapshot) {}

        fn alert(&self, _alert: &crate::domain::tools::ToolAlert) {}
    }

    async fn test_context() -> (
        sqlx::SqlitePool,
        ApiRuntimeContext,
        Arc<crate::engine::runtime_event::MemoryRuntimeEventSink>,
        Arc<TestRuntimeControl>,
    ) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        pool.execute(crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(crate::data::schema::TOOLS_TABLES_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(crate::data::schema::SOFTWARE_REMINDER_RULES_SCHEMA_SQL)
            .await
            .unwrap();
        pool.execute(crate::data::schema::ACTIVITY_IMPORT_SCHEMA_SQL)
            .await
            .unwrap();
        let sink = Arc::new(crate::engine::runtime_event::MemoryRuntimeEventSink::default());
        let event_sink: Arc<dyn RuntimeEventSink> = sink.clone();
        let control = Arc::new(TestRuntimeControl::default());
        let runtime_control: Arc<dyn crate::engine::api::runtime_control::ApiRuntimeControl> =
            control.clone();
        let runtime = crate::engine::runtime_context::RuntimeContext::system(pool.clone());
        let tools_owner = Arc::new(crate::engine::tools::ToolsRuntimeOwner::new(
            runtime.clone(),
            Arc::new(TestToolsSink),
        ));
        let context = ApiRuntimeContext::with_state_and_events(
            runtime,
            "1.8.3",
            "linux",
            Arc::new(TestRuntimeState),
            Some(event_sink),
        )
        .with_runtime_control(runtime_control)
        .with_tools_owner(tools_owner);
        (pool, context, sink, control)
    }

    #[tokio::test]
    async fn daemon_tools_writes_validate_inputs_and_return_complete_snapshots() {
        let (pool, context, _sink, _control) = test_context().await;
        let surface = crate::engine::api::surface::ApiSurface::DaemonTracking;

        let starting_context = ApiRuntimeContext::new(
            crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
        );
        let starting = route(
            &starting_context,
            surface,
            "POST",
            "/api/v1/tools/timer/pause",
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(starting.status, 503);

        let rejected = route(
            &context,
            surface,
            "POST",
            "/api/v1/tools/timer/start",
            serde_json::json!({"mode": "countdown", "duration_ms": 1}),
        )
        .await;
        assert_eq!(rejected.status, 400);

        let reminder = route(
            &context,
            surface,
            "POST",
            "/api/v1/tools/reminders",
            serde_json::json!({
                "label": "Review",
                "scheduled_at": 2_000_000_000_000_i64
            }),
        )
        .await;
        assert_eq!(reminder.status, 200);
        assert_eq!(reminder.body["data"]["reminders"][0]["label"], "Review");
        assert!(reminder.body["data"]["settings"].is_object());

        let paused = route(
            &context,
            surface,
            "POST",
            "/api/v1/tools/timer/pause",
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(paused.status, 200);
        assert!(paused.body["data"]["sampled_at_ms"].is_number());

        let unavailable = route(
            &context,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
            "POST",
            "/api/v1/tools/timer/pause",
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(unavailable.status, 404);
        pool.close().await;
    }

    #[tokio::test]
    async fn daemon_local_api_configuration_routes_are_sanitized_and_owner_gated() {
        let (pool, context, _sink, _control) = test_context().await;
        let surface = crate::engine::api::surface::ApiSurface::DaemonTracking;

        let snapshot = route(
            &context,
            surface,
            "GET",
            "/api/v1/settings/local-api",
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(snapshot.status, 200);
        assert_eq!(snapshot.body["data"]["port"], 14_840);
        assert!(snapshot.body["data"].get("token").is_none());

        let changed = route(
            &context,
            surface,
            "POST",
            "/api/v1/settings/local-api/port",
            serde_json::json!({"port": 15_555}),
        )
        .await;
        assert_eq!(changed.status, 200);
        assert_eq!(changed.body["data"]["configuration"]["port"], 15_555);
        assert_eq!(changed.body["data"]["reconnect_required"], true);
        assert!(changed.body["data"]["configuration"].get("token").is_none());

        let rotated = route(
            &context,
            surface,
            "POST",
            "/api/v1/settings/local-api/token/rotate",
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(rotated.status, 200);
        assert_eq!(rotated.body["data"]["reauthentication_required"], true);
        assert!(rotated.body["data"]["configuration"].get("token").is_none());

        let unavailable = route(
            &context,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
            "GET",
            "/api/v1/settings/local-api",
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(unavailable.status, 404);
        pool.close().await;
    }

    #[tokio::test]
    async fn daemon_service_restart_requires_confirmation_and_returns_a_ticket() {
        let (pool, context, _sink, _control) = test_context().await;
        let surface = crate::engine::api::surface::ApiSurface::DaemonTracking;

        let snapshot = route(
            &context,
            surface,
            "GET",
            "/api/v1/system/service",
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(snapshot.status, 200);
        assert_eq!(snapshot.body["data"]["service_name"], "patinad.service");
        assert_eq!(snapshot.body["data"]["managed_by_systemd"], true);

        let rejected = route(
            &context,
            surface,
            "POST",
            "/api/v1/system/service/restart",
            serde_json::json!({"confirmed": false}),
        )
        .await;
        assert_eq!(rejected.status, 400);

        let accepted = route(
            &context,
            surface,
            "POST",
            "/api/v1/system/service/restart",
            serde_json::json!({"confirmed": true}),
        )
        .await;
        assert_eq!(accepted.status, 202);
        assert_eq!(
            accepted.body["data"]["service"]["restart"]["request_id"],
            "restart_test"
        );
        assert_eq!(accepted.body["data"]["reconnect_required"], true);

        let unavailable = route(
            &context,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
            "GET",
            "/api/v1/system/service",
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(unavailable.status, 404);
        pool.close().await;
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
        let (pool, context, sink, _control) = test_context().await;

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
        let (pool, context, sink, control) = test_context().await;
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
        assert_eq!(capabilities.body["data"]["protocol"]["current"], 2);
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

        let app_settings = route(
            &context,
            surface,
            "POST",
            "/api/v1/settings/app",
            serde_json::json!({
                "mutations": [{"key": "theme_mode", "value": "dark"}]
            }),
        )
        .await;
        assert_eq!(app_settings.status, 200);
        assert_eq!(
            crate::data::repositories::tracker_settings::load_setting_value(&pool, "theme_mode")
                .await
                .unwrap(),
            Some("dark".to_string())
        );
        assert_eq!(
            route(
                &context,
                surface,
                "POST",
                "/api/v1/settings/app",
                serde_json::json!({
                    "mutations": [{"key": "not_allowed", "value": "1"}]
                }),
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
                "/api/v1/settings/app",
                serde_json::json!({
                    "mutations": [{"key": "local_api_port", "value": "14841"}]
                }),
            )
            .await
            .status,
            400
        );

        let read_only_runtime_write = route(
            &context,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
            "POST",
            "/api/v1/settings/runtime/audio-participation",
            serde_json::json!({"enabled": false}),
        )
        .await;
        assert_eq!(read_only_runtime_write.status, 404);
        assert_eq!(
            route(
                &context,
                surface,
                "POST",
                "/api/v1/settings/runtime/audio-participation",
                serde_json::json!({"enabled": false}),
            )
            .await
            .status,
            200
        );
        assert_eq!(*control.audio_enabled.lock().unwrap(), Some(false));

        let browser = route(
            &context,
            surface,
            "POST",
            "/api/v1/settings/runtime/browser-activity",
            serde_json::json!({
                "enabled": true,
                "port": 12345,
                "token": "browser-token",
                "url_privacy": "domain_only"
            }),
        )
        .await;
        assert_eq!(browser.status, 200);
        assert_eq!(browser.body["data"]["port"], 12_345);
        assert_eq!(browser.body["data"]["token_present"], true);
        assert!(browser.body["data"].get("token").is_none());
        assert!(!serde_json::to_string(&browser.body)
            .unwrap()
            .contains("browser-token"));
        assert_eq!(
            control
                .browser_configuration
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .url_privacy,
            crate::domain::settings::WebActivityUrlPrivacyMode::DomainOnly
        );

        crate::data::repositories::app_settings::save_audio_participation_enabled(&pool, false)
            .await
            .unwrap();
        crate::data::repositories::app_settings::save_web_activity_runtime_settings(
            &pool,
            &crate::domain::settings::WebActivityBridgeSettings {
                enabled: true,
                port: 18_080,
                token: "stored-browser-token".to_string(),
            },
            crate::domain::settings::WebActivityUrlPrivacyMode::StripQuery,
        )
        .await
        .unwrap();
        let runtime_settings = route(
            &context,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
            "GET",
            "/api/v1/settings/runtime",
            serde_json::Value::Null,
        )
        .await;
        assert_eq!(runtime_settings.status, 200);
        assert_eq!(
            runtime_settings.body["data"]["audio_participation_enabled"],
            false
        );
        assert_eq!(
            runtime_settings.body["data"]["browser_activity"]["port"],
            18_080
        );
        assert_eq!(
            runtime_settings.body["data"]["browser_activity"]["token_present"],
            true
        );
        assert_eq!(
            runtime_settings.body["data"]["browser_activity"]["url_privacy"],
            "strip_query"
        );
        assert!(runtime_settings.body["data"]["browser_activity"]
            .get("token")
            .is_none());
        assert!(!serde_json::to_string(&runtime_settings.body)
            .unwrap()
            .contains("stored-browser-token"));

        pool.close().await;
    }

    #[tokio::test]
    async fn daemon_data_maintenance_is_bounded_transactional_and_emits_refresh_events() {
        let (pool, context, sink, _control) = test_context().await;
        pool.execute(crate::data::schema::WEB_ACTIVITY_SCHEMA_SQL)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, app_name, exe_name, window_title, start_time, end_time, duration)
             VALUES (1, 'Old', 'old', 'Old title', 1000, 2000, 1000),
                    (2, 'New', 'new', 'New title', 3000, 4000, 1000)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO session_title_samples (session_id, title, start_time, end_time)
             VALUES (1, 'Old title', 1000, 2000), (2, 'New title', 3000, 4000)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let surface = crate::engine::api::surface::ApiSurface::DaemonTracking;
        assert_eq!(
            route(
                &context,
                crate::engine::api::surface::ApiSurface::DaemonReadOnly,
                "POST",
                "/api/v1/data/cleanup",
                serde_json::json!({"cutoff_time_ms": 2500, "confirmed": true}),
            )
            .await
            .status,
            404
        );
        assert_eq!(
            route(
                &context,
                surface,
                "POST",
                "/api/v1/data/cleanup",
                serde_json::json!({"cutoff_time_ms": 2500, "confirmed": false}),
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
                "/api/v1/data/cleanup",
                serde_json::json!({"cutoff_time_ms": -1, "confirmed": true}),
            )
            .await
            .status,
            400
        );

        let cleanup = route(
            &context,
            surface,
            "POST",
            "/api/v1/data/cleanup",
            serde_json::json!({"cutoff_time_ms": 2500, "confirmed": true}),
        )
        .await;
        assert_eq!(cleanup.status, 200);
        assert_eq!(cleanup.body["data"]["sessions_deleted"], 1);
        assert_eq!(cleanup.body["data"]["title_samples_deleted"], 1);

        let redaction = route(
            &context,
            surface,
            "POST",
            "/api/v1/data/window-titles/clear",
            serde_json::json!({"confirmed": true}),
        )
        .await;
        assert_eq!(redaction.status, 200);
        assert_eq!(redaction.body["data"]["sessions_redacted"], 1);
        assert_eq!(redaction.body["data"]["title_samples_deleted"], 1);
        assert_eq!(sink.events().len(), 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sessions WHERE COALESCE(window_title, '') <> ''",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );

        pool.close().await;
    }
}
