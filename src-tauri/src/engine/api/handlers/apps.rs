use crate::data::repositories::app_mappings::{AppOverrideUpdate, AppOverrideUpdateError};
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ApiError, ApiResponse, AppEntry, AppsResponse, ClassifyRequest, ExcludeRequest, RenameRequest,
    RouteResponse,
};

pub async fn get_apps(context: &ApiRuntimeContext) -> RouteResponse {
    match crate::data::repositories::app_mappings::load_observed_app_configurations(context.pool())
        .await
    {
        Ok(apps) => RouteResponse {
            status: 200,
            body: serde_json::to_value(ApiResponse {
                data: AppsResponse {
                    apps: apps
                        .into_iter()
                        .map(|app| AppEntry {
                            exe_name: app.exe_name,
                            display_name: app.display_name,
                            category: app.category,
                            excluded: app.excluded,
                        })
                        .collect(),
                },
            })
            .unwrap_or_default(),
        },
        Err(error) => internal_error(&error),
    }
}

pub async fn handle_app_action(
    context: &ApiRuntimeContext,
    path: &str,
    body: &[u8],
) -> RouteResponse {
    let Some((encoded_exe_name, action)) = app_action_parts(path) else {
        return bad_request("missing app action");
    };
    let exe_name = match percent_encoding::percent_decode_str(encoded_exe_name).decode_utf8() {
        Ok(value) => value.into_owned(),
        Err(_) => return bad_request("invalid app executable name encoding"),
    };
    let update = match action {
        "classify" => match parse_body::<ClassifyRequest>(body) {
            Ok(request) => AppOverrideUpdate::Category(request.category),
            Err(response) => return response,
        },
        "rename" => match parse_body::<RenameRequest>(body) {
            Ok(request) => AppOverrideUpdate::DisplayName(request.display_name),
            Err(response) => return response,
        },
        "exclude" => match parse_body::<ExcludeRequest>(body) {
            Ok(request) => AppOverrideUpdate::Excluded(request.excluded),
            Err(response) => return response,
        },
        _ => {
            return RouteResponse {
                status: 404,
                body: serde_json::to_value(ApiError::not_found("unknown app action"))
                    .unwrap_or_default(),
            }
        }
    };

    match crate::data::repositories::app_mappings::update_app_override(
        context.pool(),
        &exe_name,
        update,
        context.now_ms(),
    )
    .await
    {
        Ok(()) => {
            context.emit_tracking_data_changed(
                crate::domain::tracking::TRACKING_REASON_CLASSIFICATION_CHANGED,
            );
            ok_response()
        }
        Err(AppOverrideUpdateError::InvalidInput(message)) => bad_request(&message),
        Err(AppOverrideUpdateError::Storage(message)) => internal_error(&message),
    }
}

fn app_action_parts(path: &str) -> Option<(&str, &str)> {
    let remainder = path.strip_prefix("/api/v1/apps/")?;
    let (exe_name, action) = remainder.split_once('/')?;
    if exe_name.is_empty() || action.is_empty() || action.contains('/') {
        return None;
    }
    Some((exe_name, action))
}

fn parse_body<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, RouteResponse> {
    serde_json::from_slice(body).map_err(|_| bad_request("invalid JSON body"))
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
