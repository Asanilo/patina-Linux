use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ApiResponse, CurrentWindowResponse, HealthResponse, RouteResponse,
};

pub fn get_health(context: &ApiRuntimeContext) -> RouteResponse {
    get_health_for_runtime(context.version(), context.platform())
}

pub fn get_health_for_runtime(
    version: impl Into<String>,
    platform: impl Into<String>,
) -> RouteResponse {
    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: HealthResponse {
                status: "ok".to_string(),
                version: version.into(),
                platform: platform.into(),
            },
        })
        .unwrap_or_default(),
    }
}

pub fn get_current(context: &ApiRuntimeContext) -> RouteResponse {
    let Some(snapshot) = context.tracking_snapshot() else {
        return RouteResponse {
            status: 503,
            body: serde_json::to_value(crate::engine::api::types::ApiError::internal(
                "tracking runtime snapshot is not ready",
            ))
            .unwrap_or_default(),
        };
    };

    let window = &snapshot.window;
    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: CurrentWindowResponse {
                exe_name: window.exe_name.clone(),
                title: window.title.clone(),
                process_id: window.process_id,
                is_afk: window.is_afk,
                idle_time_ms: window.idle_time_ms,
                process_path: window.process_path.clone(),
                sampled_at_ms: snapshot.sampled_at_ms,
            },
        })
        .unwrap_or_default(),
    }
}
