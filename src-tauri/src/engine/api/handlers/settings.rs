use crate::data::sqlite_pool;
use crate::engine::api::types::{
    AfkThresholdRequest, ApiError, ApiResponse, RouteResponse, TrackerSettingsResponse,
};

const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 180;
const DEFAULT_TIMELINE_MERGE_GAP_SECS: u64 = 30;

pub async fn get_tracker_settings(app: &tauri::AppHandle) -> RouteResponse {
    let pool = match sqlite_pool::wait_for_sqlite_pool(app).await {
        Ok(p) => p,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e)).unwrap_or_default(),
            };
        }
    };

    let idle_timeout = match crate::data::repositories::tracker_settings::load_idle_timeout_secs(
        &pool,
        DEFAULT_IDLE_TIMEOUT_SECS,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e.to_string())).unwrap_or_default(),
            };
        }
    };

    let merge_gap = match crate::data::repositories::tracker_settings::load_timeline_merge_gap_secs(
        &pool,
        DEFAULT_TIMELINE_MERGE_GAP_SECS,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&e.to_string())).unwrap_or_default(),
            };
        }
    };

    let tracking_paused =
        match crate::data::repositories::tracker_settings::load_tracking_paused_setting(&pool).await
        {
            Ok(v) => v,
            Err(e) => {
                return RouteResponse {
                    status: 500,
                    body: serde_json::to_value(ApiError::internal(&e.to_string()))
                        .unwrap_or_default(),
                };
            }
        };

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: TrackerSettingsResponse {
                idle_timeout_secs: idle_timeout,
                timeline_merge_gap_secs: merge_gap,
                tracking_paused,
            },
        })
        .unwrap_or_default(),
    }
}

pub async fn set_afk_threshold(app: &tauri::AppHandle, body: &[u8]) -> RouteResponse {
    let req: AfkThresholdRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(_) => {
            return RouteResponse {
                status: 400,
                body: serde_json::to_value(ApiError::bad_request("invalid JSON body"))
                    .unwrap_or_default(),
            };
        }
    };

    // Update in-memory atomic
    crate::commands::tracking::cmd_set_afk_threshold(req.seconds);

    // Persist to database
    if let Ok(pool) = sqlite_pool::wait_for_sqlite_pool(app).await {
        let key = "idle_timeout_secs";
        let value = req.seconds.to_string();
        let _ = crate::data::repositories::tracker_settings::save_setting_value(&pool, key, &value)
            .await;
    }

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: serde_json::json!({"ok": true}),
        })
        .unwrap_or_default(),
    }
}
