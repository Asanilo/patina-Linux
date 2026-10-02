use crate::data::repositories::classification_settings::{
    validate_classification_setting_mutations, ClassificationSettingMutation,
};
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ApiError, ApiResponse, ClassificationMutationsRequest, RouteResponse,
};

pub async fn get_classification_snapshot(context: &ApiRuntimeContext) -> RouteResponse {
    match crate::data::repositories::classification_settings::load_classification_snapshot(
        context.pool(),
        context.now_ms(),
    )
    .await
    {
        Ok(snapshot) => RouteResponse {
            status: 200,
            body: serde_json::to_value(ApiResponse { data: snapshot }).unwrap_or_default(),
        },
        Err(error) => RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&error)).unwrap_or_default(),
        },
    }
}

use patina_protocol::configuration::MAX_CLASSIFICATION_MUTATIONS;

pub async fn commit_classification_settings(
    context: &ApiRuntimeContext,
    body: &[u8],
) -> RouteResponse {
    commit(context, body, false).await
}

pub async fn commit_conditional(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    commit(context, body, true).await
}

async fn commit(context: &ApiRuntimeContext, body: &[u8], require_revision: bool) -> RouteResponse {
    let request: ClassificationMutationsRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if require_revision && request.expected_revision.is_none() {
        return bad_request("conditional classification commit requires expected_revision");
    }
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

    let result = if let Some(revision) = request.expected_revision.as_deref() {
        use crate::data::repositories::classification_settings::{
            commit_classification_if_revision, ConditionalCommitError,
        };
        match commit_classification_if_revision(
            context.pool(),
            &mutations,
            revision,
            context.now_ms(),
        )
        .await
        {
            Ok(result) => Ok(result),
            Err(ConditionalCommitError::Conflict) => {
                return RouteResponse {
                    status: 409,
                    body: serde_json::to_value(ApiError::conflict(
                        "classification configuration changed; reload before retrying",
                    ))
                    .unwrap_or_default(),
                }
            }
            Err(ConditionalCommitError::InvalidInput(error)) => return bad_request(&error),
            Err(ConditionalCommitError::Storage(error)) => Err(error),
        }
    } else {
        crate::data::repositories::classification_settings::commit_classification_setting_mutations(
            context.pool(),
            &mutations,
        )
        .await
        .map(
            |()| patina_protocol::configuration::ClassificationCommitResult {
                ok: true,
                revision: None,
            },
        )
    };
    match result {
        Ok(result) => {
            if !mutations.is_empty() {
                context.emit_tracking_data_changed(
                    crate::domain::tracking::TRACKING_REASON_CLASSIFICATION_CHANGED,
                );
            }
            RouteResponse {
                status: 200,
                body: serde_json::to_value(ApiResponse { data: result }).unwrap_or_default(),
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
