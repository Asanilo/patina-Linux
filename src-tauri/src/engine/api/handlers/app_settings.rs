use crate::data::repositories::app_settings::{
    commit_app_setting_mutations, validate_app_setting_mutations, AppSettingMutation,
};
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ApiError, ApiResponse, AppSettingsMutationsRequest, RouteResponse,
};

const MAX_APP_SETTING_MUTATIONS: usize = 256;

pub async fn commit_app_settings(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: AppSettingsMutationsRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if request.mutations.len() > MAX_APP_SETTING_MUTATIONS {
        return bad_request("too many app setting mutations");
    }
    let mutations = request
        .mutations
        .into_iter()
        .map(|mutation| AppSettingMutation {
            key: mutation.key,
            value: mutation.value,
        })
        .collect::<Vec<_>>();
    if mutations
        .iter()
        .any(|mutation| !is_non_resource_setting(&mutation.key))
    {
        return bad_request("runtime resource settings require their dedicated endpoint");
    }
    if let Err(error) = validate_app_setting_mutations(&mutations) {
        return bad_request(&error);
    }

    match commit_app_setting_mutations(context.pool(), &mutations).await {
        Ok(()) => RouteResponse {
            status: 200,
            body: serde_json::to_value(ApiResponse {
                data: serde_json::json!({"ok": true}),
            })
            .unwrap_or_default(),
        },
        Err(error) => RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
        },
    }
}

fn is_non_resource_setting(key: &str) -> bool {
    matches!(
        key,
        "timeline_merge_gap_secs"
            | "refresh_interval_secs"
            | "min_session_secs"
            | "close_behavior"
            | "minimize_behavior"
            | "theme_mode"
            | "language"
            | "hourly_activity_chart_mode"
            | "color_scheme_light"
            | "color_scheme_dark"
            | "launch_at_login"
            | "start_minimized"
            | "background_optimization"
            | "onboarding_completed"
            | "remote_status_bridge_enabled"
            | "remote_status_bridge_url"
            | "remote_status_bridge_token"
            | "remote_status_bridge_machine_id"
    )
}

fn bad_request(message: &str) -> RouteResponse {
    RouteResponse {
        status: 400,
        body: serde_json::to_value(ApiError::bad_request(message)).unwrap_or_default(),
    }
}
