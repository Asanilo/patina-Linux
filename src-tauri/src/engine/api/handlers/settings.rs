use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    AfkThresholdRequest, ApiError, ApiResponse, RouteResponse, TrackerSettingsResponse,
    TrackingPausedRequest,
};

const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 180;
const DEFAULT_TIMELINE_MERGE_GAP_SECS: u64 = 30;

pub async fn get_tracker_settings(context: &ApiRuntimeContext) -> RouteResponse {
    let pool = context.pool();

    let idle_timeout = match crate::data::repositories::tracker_settings::load_idle_timeout_secs(
        pool,
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
        pool,
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
        match crate::data::repositories::tracker_settings::load_tracking_paused_setting(pool).await
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

pub async fn set_afk_threshold(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
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
    if !(60..=86_400).contains(&req.seconds) {
        return RouteResponse {
            status: 400,
            body: serde_json::to_value(ApiError::bad_request(
                "seconds must be between 60 and 86400",
            ))
            .unwrap_or_default(),
        };
    }

    let key = "idle_timeout_secs";
    let value = req.seconds.to_string();
    if let Err(error) =
        crate::data::repositories::tracker_settings::save_setting_value(context.pool(), key, &value)
            .await
    {
        return RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&error.to_string())).unwrap_or_default(),
        };
    }
    crate::engine::tracking::runtime_settings::set_idle_threshold(req.seconds);
    context.emit_tracking_data_changed(
        crate::domain::tracking::TRACKING_REASON_TRACKER_SETTINGS_CHANGED,
    );

    ok_response()
}

pub async fn set_tracking_paused(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: TrackingPausedRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => {
            return RouteResponse {
                status: 400,
                body: serde_json::to_value(ApiError::bad_request("invalid JSON body"))
                    .unwrap_or_default(),
            }
        }
    };
    if let Err(error) = crate::data::repositories::tracker_settings::save_tracking_paused_setting(
        context.pool(),
        request.paused,
    )
    .await
    {
        return RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&error.to_string())).unwrap_or_default(),
        };
    }
    context.emit_tracking_data_changed(if request.paused {
        "tracking-paused"
    } else {
        "tracking-resumed"
    });
    ok_response()
}

fn ok_response() -> RouteResponse {
    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: serde_json::json!({"ok": true}),
        })
        .unwrap_or_default(),
    }
}
