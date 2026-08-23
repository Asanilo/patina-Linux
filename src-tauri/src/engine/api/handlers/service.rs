use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::runtime_control::RuntimeControlError;
use crate::engine::api::types::{ApiError, ApiResponse, ConfirmedActionRequest, RouteResponse};

pub async fn get_service(context: &ApiRuntimeContext) -> RouteResponse {
    let Some(control) = context.runtime_control() else {
        return unavailable();
    };
    match control.daemon_service_snapshot().await {
        Ok(snapshot) => response(200, snapshot),
        Err(error) => control_error(error),
    }
}

pub async fn restart_service(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: ConfirmedActionRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !request.confirmed {
        return bad_request("service restart requires confirmed=true");
    }
    let Some(control) = context.runtime_control() else {
        return unavailable();
    };
    match control.request_daemon_service_restart().await {
        Ok(result) => response(202, result),
        Err(error) => control_error(error),
    }
}

fn response(status: u16, data: impl serde::Serialize) -> RouteResponse {
    RouteResponse {
        status,
        body: serde_json::to_value(ApiResponse { data }).unwrap_or_default(),
    }
}

fn bad_request(message: &str) -> RouteResponse {
    RouteResponse {
        status: 400,
        body: serde_json::to_value(ApiError::bad_request(message)).unwrap_or_default(),
    }
}

fn unavailable() -> RouteResponse {
    RouteResponse {
        status: 503,
        body: serde_json::to_value(ApiError::unavailable(
            "daemon service lifecycle is not owned by this API host",
        ))
        .unwrap_or_default(),
    }
}

fn control_error(error: RuntimeControlError) -> RouteResponse {
    let (status, body) = match error {
        RuntimeControlError::InvalidInput(message) => (400, ApiError::bad_request(&message)),
        RuntimeControlError::Conflict(message) => (409, ApiError::conflict(&message)),
        RuntimeControlError::Internal(message) => (500, ApiError::internal(&message)),
    };
    RouteResponse {
        status,
        body: serde_json::to_value(body).unwrap_or_default(),
    }
}
