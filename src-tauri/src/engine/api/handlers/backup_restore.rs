use crate::engine::api::backup_restore_owner::{
    BackupRestoreOwnerError, BackupRestoreScheduleInput,
};
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ApiError, ApiResponse, CancelBackupRestoreRequest, RouteResponse, StagedBackupRestoreRequest,
};

pub async fn get_status(context: &ApiRuntimeContext, query: Option<&str>) -> RouteResponse {
    let request_id = query.and_then(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .find(|(key, _)| key == "request_id")
            .map(|(_, value)| value.into_owned())
    });
    if request_id
        .as_deref()
        .is_some_and(|value| !valid_request_id(value))
    {
        return bad_request("backup restore request id is invalid");
    }
    let Some(owner) = context.backup_restore_owner() else {
        return unavailable();
    };
    match owner.snapshot(request_id).await {
        Ok(snapshot) => response(200, snapshot),
        Err(error) => owner_error(error),
    }
}

pub async fn schedule(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: StagedBackupRestoreRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !request.confirmed {
        return bad_request("backup restore requires confirmed=true");
    }
    let Some(owner) = context.backup_restore_owner() else {
        return unavailable();
    };
    match owner
        .schedule(BackupRestoreScheduleInput {
            ticket: request.ticket,
            expected_sha256: request.expected_sha256,
            expected_size_bytes: request.expected_size_bytes,
            strategy: request.strategy,
        })
        .await
    {
        Ok(result) => response(202, result),
        Err(error) => owner_error(error),
    }
}

pub async fn cancel(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: CancelBackupRestoreRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !request.confirmed {
        return bad_request("backup restore cancellation requires confirmed=true");
    }
    if !valid_request_id(&request.request_id) {
        return bad_request("backup restore request id is invalid");
    }
    let Some(owner) = context.backup_restore_owner() else {
        return unavailable();
    };
    match owner.cancel(request.request_id).await {
        Ok(snapshot) => response(200, snapshot),
        Err(error) => owner_error(error),
    }
}

fn valid_request_id(value: &str) -> bool {
    value.strip_prefix("restore_").is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn owner_error(error: BackupRestoreOwnerError) -> RouteResponse {
    match error {
        BackupRestoreOwnerError::InvalidInput(message) => bad_request(&message),
        BackupRestoreOwnerError::NotFound(message) => RouteResponse {
            status: 404,
            body: serde_json::to_value(ApiError::not_found(&message)).unwrap_or_default(),
        },
        BackupRestoreOwnerError::Conflict(message) => RouteResponse {
            status: 409,
            body: serde_json::to_value(ApiError::conflict(&message)).unwrap_or_default(),
        },
        BackupRestoreOwnerError::Internal(message) => RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&message)).unwrap_or_default(),
        },
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
        body: serde_json::to_value(ApiError::unavailable("backup restore owner is unavailable"))
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::backup::RestoreStrategy;
    use crate::engine::api::backup_restore_owner::{
        BackupRestoreOwner, BackupRestoreOwnerFuture, BackupRestoreScheduleResult,
        BackupRestoreSnapshot,
    };
    use crate::engine::api::runtime_control::{
        DaemonServiceRestartSnapshot, DaemonServiceRuntimeSnapshot,
    };
    use std::sync::{Arc, Mutex};

    const REQUEST_ID: &str = "restore_0123456789abcdef0123456789abcdef";

    #[derive(Default)]
    struct TestBackupRestoreOwner {
        scheduled: Mutex<Option<BackupRestoreScheduleInput>>,
        cancelled: Mutex<Option<String>>,
    }

    impl TestBackupRestoreOwner {
        fn restore_snapshot(status: &str) -> BackupRestoreSnapshot {
            BackupRestoreSnapshot {
                request_id: REQUEST_ID.to_string(),
                status: status.to_string(),
                strategy: RestoreStrategy::Replace,
                archive_sha256: "a".repeat(64),
                size_bytes: 128,
                requested_at_ms: 1_000,
                started_at_ms: None,
                completed_at_ms: None,
                restart_request_id: Some("restart_test".to_string()),
                error: None,
                cleanup_warning: None,
            }
        }
    }

    impl BackupRestoreOwner for TestBackupRestoreOwner {
        fn schedule(
            &self,
            input: BackupRestoreScheduleInput,
        ) -> BackupRestoreOwnerFuture<'_, BackupRestoreScheduleResult> {
            Box::pin(async move {
                *self.scheduled.lock().unwrap() = Some(input);
                Ok(BackupRestoreScheduleResult {
                    restore: Self::restore_snapshot("pending_restart"),
                    service: DaemonServiceRuntimeSnapshot {
                        service_name: "patinad.service".to_string(),
                        managed_by_systemd: true,
                        instance_id: "instance_test".to_string(),
                        restart: Some(DaemonServiceRestartSnapshot {
                            request_id: "restart_test".to_string(),
                            status: "pending".to_string(),
                            requested_at_ms: 1_000,
                            requested_instance_id: "instance_test".to_string(),
                            completed_at_ms: None,
                            completed_instance_id: None,
                        }),
                    },
                    reconnect_required: true,
                })
            })
        }

        fn snapshot(
            &self,
            request_id: Option<String>,
        ) -> BackupRestoreOwnerFuture<'_, Option<BackupRestoreSnapshot>> {
            Box::pin(async move {
                if request_id
                    .as_deref()
                    .is_some_and(|value| value != REQUEST_ID)
                {
                    return Err(BackupRestoreOwnerError::NotFound(
                        "backup restore request was not found".to_string(),
                    ));
                }
                Ok(Some(Self::restore_snapshot("completed")))
            })
        }

        fn cancel(
            &self,
            request_id: String,
        ) -> BackupRestoreOwnerFuture<'_, BackupRestoreSnapshot> {
            Box::pin(async move {
                *self.cancelled.lock().unwrap() = Some(request_id);
                Ok(Self::restore_snapshot("cancelled"))
            })
        }
    }

    async fn test_context() -> (ApiRuntimeContext, Arc<TestBackupRestoreOwner>) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let owner = Arc::new(TestBackupRestoreOwner::default());
        let context =
            ApiRuntimeContext::new(crate::engine::runtime_context::RuntimeContext::system(pool))
                .with_backup_restore_owner(owner.clone());
        (context, owner)
    }

    #[test]
    fn restore_request_ids_are_strictly_bounded() {
        assert!(valid_request_id("restore_0123456789abcdef0123456789abcdef"));
        assert!(!valid_request_id("../restore_0123456789abcdef"));
        assert!(!valid_request_id(
            "restore_0123456789ABCDEF0123456789ABCDEF"
        ));
    }

    #[tokio::test]
    async fn restore_handlers_require_confirmation_and_delegate_to_the_owner() {
        let (context, owner) = test_context().await;
        let rejected = schedule(
            &context,
            serde_json::to_vec(&serde_json::json!({
                "ticket": "b".repeat(32),
                "expected_sha256": "a".repeat(64),
                "expected_size_bytes": 128,
                "strategy": "replace",
                "confirmed": false
            }))
            .unwrap()
            .as_slice(),
        )
        .await;
        assert_eq!(rejected.status, 400);
        assert!(owner.scheduled.lock().unwrap().is_none());

        let accepted = schedule(
            &context,
            serde_json::to_vec(&serde_json::json!({
                "ticket": "b".repeat(32),
                "expected_sha256": "a".repeat(64),
                "expected_size_bytes": 128,
                "strategy": "replace",
                "confirmed": true
            }))
            .unwrap()
            .as_slice(),
        )
        .await;
        assert_eq!(accepted.status, 202);
        assert_eq!(accepted.body["data"]["restore"]["request_id"], REQUEST_ID);
        assert_eq!(
            owner.scheduled.lock().unwrap().as_ref().unwrap().strategy,
            RestoreStrategy::Replace
        );

        let status = get_status(&context, Some(&format!("request_id={REQUEST_ID}"))).await;
        assert_eq!(status.status, 200);
        assert_eq!(status.body["data"]["status"], "completed");

        let cancelled = cancel(
            &context,
            serde_json::to_vec(&serde_json::json!({
                "request_id": REQUEST_ID,
                "confirmed": true
            }))
            .unwrap()
            .as_slice(),
        )
        .await;
        assert_eq!(cancelled.status, 200);
        assert_eq!(owner.cancelled.lock().unwrap().as_deref(), Some(REQUEST_ID));
    }
}
