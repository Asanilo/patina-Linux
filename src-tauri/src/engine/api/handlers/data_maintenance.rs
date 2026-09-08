use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ApiError, ApiResponse, AppTrackingDataCleanupRequest, ConfirmedActionRequest, RouteResponse,
    TrackingDataCleanupRequest,
};

pub async fn delete_tracking_data_before(
    context: &ApiRuntimeContext,
    body: &[u8],
) -> RouteResponse {
    let request: TrackingDataCleanupRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !request.confirmed {
        return bad_request("tracking data cleanup requires confirmed=true");
    }
    match crate::data::maintenance::delete_tracking_data_before(
        context.pool(),
        request.cutoff_time_ms,
    )
    .await
    {
        Ok(result) => {
            context.emit_tracking_data_changed("tracking-data-cleaned");
            ok(result)
        }
        Err(error) if request.cutoff_time_ms < 0 => bad_request(&error),
        Err(error) => internal_error(&error),
    }
}

pub async fn clear_window_titles(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: ConfirmedActionRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !request.confirmed {
        return bad_request("window title cleanup requires confirmed=true");
    }
    match crate::data::maintenance::clear_all_window_titles(context.pool()).await {
        Ok(result) => {
            context.emit_tracking_data_changed("window-titles-cleared");
            ok(result)
        }
        Err(error) => internal_error(&error),
    }
}

pub async fn delete_app_tracking_data(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: AppTrackingDataCleanupRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !request.confirmed {
        return bad_request("application data cleanup requires confirmed=true");
    }
    if let Err(error) = crate::domain::data_maintenance::validate_app_tracking_data_cleanup(
        &request.exe_names,
        request.start_time_ms,
        request.end_time_ms,
    ) {
        return bad_request(&error);
    }
    match crate::data::maintenance::delete_app_tracking_data(
        context.pool(),
        &request.exe_names,
        request.start_time_ms,
        request.end_time_ms,
    )
    .await
    {
        Ok(result) => {
            context.emit_tracking_data_changed("application-tracking-data-deleted");
            ok(result)
        }
        Err(error) => internal_error(&error),
    }
}

fn ok(data: impl serde::Serialize) -> RouteResponse {
    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse { data }).unwrap_or_default(),
    }
}

fn bad_request(message: &str) -> RouteResponse {
    RouteResponse {
        status: 400,
        body: serde_json::to_value(ApiError::bad_request(message)).unwrap_or_default(),
    }
}

fn internal_error(message: &str) -> RouteResponse {
    RouteResponse {
        status: 500,
        body: serde_json::to_value(ApiError::internal(message)).unwrap_or_default(),
    }
}
