use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::runtime_control::RuntimeControlError;
use crate::engine::api::types::{ApiError, ApiResponse, LocalApiPortRequest, RouteResponse};

pub async fn get_configuration(context: &ApiRuntimeContext) -> RouteResponse {
    let Some(control) = context.runtime_control() else {
        return unavailable();
    };
    match control.local_api_snapshot().await {
        Ok(snapshot) => ok(snapshot),
        Err(error) => control_error(error),
    }
}

pub async fn apply_port(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: LocalApiPortRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    let Some(control) = context.runtime_control() else {
        return unavailable();
    };
    match control
        .apply_local_api_port(context.clone(), request.port)
        .await
    {
        Ok(result) => ok(result),
        Err(error) => control_error(error),
    }
}

pub async fn rotate_token(context: &ApiRuntimeContext) -> RouteResponse {
    let Some(control) = context.runtime_control() else {
        return unavailable();
    };
    match control.rotate_local_api_token().await {
        Ok(result) => ok(result),
        Err(error) => control_error(error),
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

fn unavailable() -> RouteResponse {
    RouteResponse {
        status: 503,
        body: serde_json::to_value(ApiError::unavailable(
            "local API configuration is not owned by this API host",
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
