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

pub async fn delete_canonical_app_history(
    context: &ApiRuntimeContext,
    body: &[u8],
) -> RouteResponse {
    let request: patina_protocol::maintenance::CanonicalAppCleanupRequest =
        match serde_json::from_slice(body) {
            Ok(r) => r,
            Err(_) => return bad_request("invalid canonical cleanup request"),
        };
    if let Err(error) = crate::domain::data_maintenance::canonical_cleanup_key(&request) {
        return bad_request(&error);
    }
    match crate::data::maintenance::canonical::delete_canonical_app(
        context.pool(),
        &request,
        context.now_ms(),
    )
    .await
    {
        Ok(result) => {
            if result.matched_executables > 0 {
                context.emit_tracking_data_changed("application-tracking-data-deleted");
            }
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

pub async fn delete_web_domain_history(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: crate::engine::api::types::WebDomainCleanupRequest =
        match serde_json::from_slice(body) {
            Ok(request) => request,
            Err(_) => return bad_request("invalid JSON body"),
        };
    if !request.confirmed {
        return bad_request("web history cleanup requires confirmed=true");
    }
    let domain =
        match crate::domain::data_maintenance::normalize_web_domain_cleanup(&request.domain) {
            Ok(domain) => domain,
            Err(error) => return bad_request(&error),
        };
    match crate::data::maintenance::delete_web_activity_segments_by_domain(context.pool(), &domain)
        .await
    {
        Ok(result) => {
            context.emit_tracking_data_changed(
                crate::domain::web_activity::WEB_ACTIVITY_CHANGED_REASON,
            );
            ok(result)
        }
        Err(error) => internal_error(&error),
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
