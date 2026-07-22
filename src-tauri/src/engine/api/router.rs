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
        ("GET", "/api/v1/tools/snapshot") => handlers::tools::get_tools_snapshot(context).await,
        _ => RouteResponse {
            status: 404,
            body: serde_json::to_value(ApiError::not_found("endpoint not found"))
                .unwrap_or_default(),
        },
    }
}
