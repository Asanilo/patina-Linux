use crate::data::repositories::classification_settings::{
    validate_classification_setting_mutations, ClassificationSettingMutation,
};
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ApiError, ApiResponse, ClassificationMutationsRequest, RouteResponse,
};

const MAX_CLASSIFICATION_MUTATIONS: usize = 256;

pub async fn commit_classification_settings(
    context: &ApiRuntimeContext,
    body: &[u8],
) -> RouteResponse {
    let request: ClassificationMutationsRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if request.mutations.len() > MAX_CLASSIFICATION_MUTATIONS {
        return bad_request("too many classification mutations");
    }
    let mutations = request
        .mutations
        .into_iter()
        .map(|mutation| ClassificationSettingMutation {
            key: mutation.key,
            value: mutation.value,
        })
        .collect::<Vec<_>>();
    if let Err(error) = validate_classification_setting_mutations(&mutations) {
        return bad_request(&error);
    }

    match crate::data::repositories::classification_settings::commit_classification_setting_mutations(
        context.pool(),
        &mutations,
    )
    .await
    {
        Ok(()) => {
            if !mutations.is_empty() {
                context.emit_tracking_data_changed(
                    crate::domain::tracking::TRACKING_REASON_CLASSIFICATION_CHANGED,
                );
            }
            RouteResponse {
                status: 200,
                body: serde_json::to_value(ApiResponse {
                    data: serde_json::json!({"ok": true}),
                })
                .unwrap_or_default(),
            }
        }
        Err(error) => RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
        },
    }
}

fn bad_request(message: &str) -> RouteResponse {
    RouteResponse {
        status: 400,
        body: serde_json::to_value(ApiError::bad_request(message)).unwrap_or_default(),
    }
}
