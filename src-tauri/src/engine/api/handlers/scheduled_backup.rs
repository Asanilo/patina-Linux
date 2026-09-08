use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::scheduled_backup_owner::ScheduledBackupOwnerError;
use crate::engine::api::types::{
    ApiError, ApiResponse, RouteResponse, ScheduledBackupConfigRequest,
};

pub async fn get_snapshot(context: &ApiRuntimeContext) -> RouteResponse {
    let Some(owner) = context.scheduled_backup_owner() else {
        return unavailable("scheduled backup owner is unavailable");
    };
    match owner.snapshot().await {
        Ok(snapshot) => ok(snapshot),
        Err(error) => owner_error(error),
    }
}

pub async fn save_config(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: ScheduledBackupConfigRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !request.confirmed {
        return bad_request("scheduled backup configuration requires confirmed=true");
    }
    if let Err(error) = request.config.validate() {
        return bad_request(&error);
    }
    let Some(owner) = context.scheduled_backup_owner() else {
        return unavailable("scheduled backup owner is unavailable");
    };
    match owner.save_config(request.config).await {
        Ok(snapshot) => ok(snapshot),
        Err(error) => owner_error(error),
    }
}

fn owner_error(error: ScheduledBackupOwnerError) -> RouteResponse {
    match error {
        ScheduledBackupOwnerError::InvalidInput(message) => bad_request(&message),
        ScheduledBackupOwnerError::Internal(message) => internal_error(&message),
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

    #[tokio::test]
    async fn schedule_replacement_requires_explicit_confirmation() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let context = ApiRuntimeContext::new(
            crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
        );
        let body = serde_json::to_vec(&serde_json::json!({
            "config": {
                "enabled": true,
                "cadence": "daily",
                "weekday": null,
                "localTimeMinutes": 1260,
                "targetDir": "/tmp/patina-test-backups"
            },
            "confirmed": false
        }))
        .unwrap();

        let response = save_config(&context, &body).await;

        assert_eq!(response.status, 400);
        assert_eq!(
            response
                .body
                .pointer("/error/code")
                .and_then(|value| value.as_str()),
            Some("bad_request")
        );
        pool.close().await;
    }
}
