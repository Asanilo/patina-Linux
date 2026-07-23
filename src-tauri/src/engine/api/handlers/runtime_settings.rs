use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::runtime_control::{
    BrowserActivityRuntimeConfiguration, RuntimeControlError,
};
use crate::engine::api::types::{
    ApiError, ApiResponse, AudioParticipationRequest, BrowserActivitySettingsResponse,
    RouteResponse, RuntimeSettingsResponse,
};

pub async fn get_runtime_settings(context: &ApiRuntimeContext) -> RouteResponse {
    let settings = match crate::data::repositories::app_settings::load_runtime_activity_settings(
        context.pool(),
    )
    .await
    {
        Ok(settings) => settings,
        Err(error) => {
            return RouteResponse {
                status: 500,
                body: serde_json::to_value(ApiError::internal(&error.to_string()))
                    .unwrap_or_default(),
            };
        }
    };

    ok(RuntimeSettingsResponse {
        audio_participation_enabled: settings.audio_participation_enabled,
        browser_activity: BrowserActivitySettingsResponse {
            enabled: settings.web_activity_bridge.enabled,
            port: settings.web_activity_bridge.port,
            token_present: !settings.web_activity_bridge.token.is_empty(),
            url_privacy: settings.web_activity_url_privacy,
        },
    })
}

pub async fn set_audio_participation(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: AudioParticipationRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    let Some(control) = context.runtime_control() else {
        return unavailable();
    };
    match control
        .set_audio_participation_enabled(request.enabled)
        .await
    {
        Ok(enabled) => ok(serde_json::json!({ "enabled": enabled })),
        Err(error) => control_error(error),
    }
}

pub async fn configure_browser_activity(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: BrowserActivityRuntimeConfiguration = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    let Some(control) = context.runtime_control() else {
        return unavailable();
    };
    match control.configure_browser_activity(request).await {
        Ok(configuration) => ok(BrowserActivitySettingsResponse {
            enabled: configuration.enabled,
            port: configuration.port,
            token_present: !configuration.token.is_empty(),
            url_privacy: configuration.url_privacy,
        }),
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
            "runtime settings are not owned by this API host",
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
