use crate::engine::api::{
    auth::ApiCredentialStore,
    handlers,
    http::{self, ApiRequest},
    types::ApiError,
    types::RouteResponse,
};
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;
use tokio::net::TcpStream;

pub async fn handle_connection(
    stream: TcpStream,
    app: tauri::AppHandle,
    credentials: ApiCredentialStore,
) {
    http::serve_connection(stream, credentials, move |request| async move {
        let method = request.method.clone();
        let path = request.path.clone();
        match AssertUnwindSafe(route_request(request, &app))
            .catch_unwind()
            .await
        {
            Ok(response) => response,
            Err(_) => {
                eprintln!("[api] handler panicked while serving {method} {path}");
                RouteResponse {
                    status: 500,
                    body: serde_json::to_value(ApiError::internal("handler panicked"))
                        .unwrap_or_default(),
                }
            }
        }
    })
    .await;
}

pub fn route_minimal_request(method: &str, path: &str) -> RouteResponse {
    match (method, path) {
        ("GET", "/api/v1/health") => handlers::health::get_health_for_runtime(
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
        ),
        ("GET", "/api/v1/openapi.json") => handlers::openapi::get_openapi(),
        _ => RouteResponse {
            status: 404,
            body: serde_json::to_value(ApiError::not_found(
                "endpoint not available in patinad stage-1",
            ))
            .unwrap_or_default(),
        },
    }
}

pub(crate) async fn handle_minimal_connection(stream: TcpStream, credentials: ApiCredentialStore) {
    http::serve_connection(stream, credentials, |request| async move {
        route_minimal_request(&request.method, &request.path)
    })
    .await;
}

async fn route_request(request: ApiRequest, app: &tauri::AppHandle) -> RouteResponse {
    let method = request.method.as_str();
    let path = request.path.as_str();
    let query = request.query.as_deref();
    let body = request.body.as_slice();
    match (method, path) {
        ("GET", "/api/v1/health") => handlers::health::get_health(app),
        ("GET", "/api/v1/openapi.json") => handlers::openapi::get_openapi(),
        ("GET", "/api/v1/diagnostics") => handlers::diagnostics::get_diagnostics(app).await,
        ("GET", "/api/v1/current") => handlers::health::get_current(app),
        ("GET", "/api/v1/sessions") => handlers::sessions::get_sessions(app, query).await,
        ("GET", "/api/v1/sessions/active") => handlers::sessions::get_active_session(app).await,
        ("GET", "/api/v1/summary/today") => handlers::sessions::get_summary_today(app).await,
        ("GET", "/api/v1/summary/range") => handlers::sessions::get_summary_range(app, query).await,
        ("GET", "/api/v1/summary/week") => handlers::sessions::get_summary_week(app).await,
        ("GET", "/api/v1/trend") => handlers::trend::get_trend(app, query).await,
        ("GET", "/api/v1/web-activity") => {
            handlers::web_activity::get_web_activity(app, query).await
        }
        ("GET", "/api/v1/ai/activity-context") => handlers::ai::get_activity_context(app).await,
        ("GET", "/api/v1/apps") => handlers::apps::get_apps(app).await,
        ("POST", path) if path.starts_with("/api/v1/apps/") => {
            handlers::apps::handle_app_action(app, path, body).await
        }
        ("GET", "/api/v1/settings/tracker") => handlers::settings::get_tracker_settings(app).await,
        ("POST", "/api/v1/settings/tracker/afk-threshold") => {
            handlers::settings::set_afk_threshold(app, body).await
        }
        ("GET", "/api/v1/tools/snapshot") => handlers::tools::get_tools_snapshot(app).await,
        _ => RouteResponse {
            status: 404,
            body: serde_json::to_value(ApiError::not_found("endpoint not found"))
                .unwrap_or_default(),
        },
    }
}
