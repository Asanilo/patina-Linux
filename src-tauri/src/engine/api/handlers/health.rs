use crate::engine::api::types::{
    ApiResponse, CurrentWindowResponse, HealthResponse, RouteResponse,
};
use crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState;
use tauri::Manager;

pub fn get_health(app: &tauri::AppHandle) -> RouteResponse {
    let version = app.package_info().version.to_string();
    let platform = std::env::consts::OS.to_string();

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: HealthResponse {
                status: "ok".to_string(),
                version,
                platform,
            },
        })
        .unwrap_or_default(),
    }
}

pub fn get_current(app: &tauri::AppHandle) -> RouteResponse {
    let snapshot_state = app.state::<TrackingRuntimeSnapshotState>();
    let Some(snapshot) = snapshot_state.snapshot() else {
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
            },
        })
        .unwrap_or_default(),
    }
}
