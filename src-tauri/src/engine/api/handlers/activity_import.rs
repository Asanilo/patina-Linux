use crate::engine::api::activity_import_owner::ActivityImportOwnerError;
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ApiError, ApiResponse, ConfirmedActionRequest, RouteResponse, StagedActivityImportCommitRequest,
};

const MAX_SOURCE_NAME_BYTES: usize = 512;

pub async fn list_batches(context: &ApiRuntimeContext) -> RouteResponse {
    match crate::data::repositories::activity_import::list(context.pool()).await {
        Ok(batches) => ok(batches),
        Err(error) => internal_error(&error),
    }
}

pub async fn commit_staged(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: StagedActivityImportCommitRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !valid_ticket(&request.ticket) {
        return bad_request("activity import staging ticket is invalid");
    }
    if !valid_fingerprint(&request.expected_fingerprint) {
        return bad_request("activity import preview fingerprint is invalid");
    }
    if !valid_source_name(&request.source_name) {
        return bad_request("activity import source name is invalid");
    }
    let Some(owner) = context.activity_import_owner() else {
        return unavailable("activity import owner is unavailable");
    };

    match owner
        .commit_staged(
            request.ticket,
            request.source_name,
            request.expected_fingerprint,
        )
        .await
    {
        Ok(report) => {
            if report.imported_records > 0 {
                context.emit_tracking_data_changed("external-data-imported");
            }
            ok(report)
        }
        Err(error) => owner_error(error),
    }
}

pub async fn delete_batch(context: &ApiRuntimeContext, path: &str, body: &[u8]) -> RouteResponse {
    let Some(batch_id) = path
        .strip_prefix("/api/v1/imports/")
        .and_then(|value| value.strip_suffix("/delete"))
        .filter(|value| !value.is_empty() && !value.contains('/'))
    else {
        return not_found("activity import action not found");
    };
    let request: ConfirmedActionRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !request.confirmed {
        return bad_request("activity import deletion requires confirmed=true");
    }
    let Some(owner) = context.activity_import_owner() else {
        return unavailable("activity import owner is unavailable");
    };

    match owner.delete_batch(batch_id.to_string()).await {
        Ok(report) => {
            context.emit_tracking_data_changed("external-import-deleted");
            ok(report)
        }
        Err(error) => owner_error(error),
    }
}

fn valid_ticket(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_source_name(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= MAX_SOURCE_NAME_BYTES
        && !value.contains(['/', '\\', '\0'])
        && !value.chars().any(char::is_control)
}

fn owner_error(error: ActivityImportOwnerError) -> RouteResponse {
    match error {
        ActivityImportOwnerError::InvalidInput(message) => bad_request(&message),
        ActivityImportOwnerError::NotFound(message) => not_found(&message),
        ActivityImportOwnerError::Conflict(message) => conflict(&message),
        ActivityImportOwnerError::Internal(message) => internal_error(&message),
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

fn not_found(message: &str) -> RouteResponse {
    RouteResponse {
        status: 404,
        body: serde_json::to_value(ApiError::not_found(message)).unwrap_or_default(),
    }
}

fn conflict(message: &str) -> RouteResponse {
    RouteResponse {
        status: 409,
        body: serde_json::to_value(ApiError::conflict(message)).unwrap_or_default(),
    }
}

fn unavailable(message: &str) -> RouteResponse {
    RouteResponse {
        status: 503,
        body: serde_json::to_value(ApiError::unavailable(message)).unwrap_or_default(),
    }
}

fn internal_error(message: &str) -> RouteResponse {
    RouteResponse {
        status: 500,
        body: serde_json::to_value(ApiError::internal(message)).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_identifiers_and_source_names_are_strictly_bounded() {
        assert!(valid_ticket("0123456789abcdef0123456789abcdef"));
        assert!(!valid_ticket("../0123456789abcdef0123456789abcdef"));
        assert!(valid_fingerprint(&"a".repeat(64)));
        assert!(!valid_fingerprint(&"A".repeat(64)));
        assert!(valid_source_name("activity.csv"));
        assert!(!valid_source_name("../activity.csv"));
        assert!(!valid_source_name("activity\ncsv"));
    }
}
