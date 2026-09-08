use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::remote_backup_owner::RemoteBackupOwnerError;
use crate::engine::api::types::{
    ApiError, ApiResponse, RemoteBackupListRequest, RemoteBackupRestoreRequest,
    RemoteBackupUploadRequest, RouteResponse,
};

pub async fn list(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: RemoteBackupListRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    let Some(owner) = context.remote_backup_owner() else {
        return unavailable("remote backup owner is unavailable");
    };
    match owner.list(request.config).await {
        Ok(entries) => RouteResponse {
            status: 200,
            body: serde_json::to_value(ApiResponse { data: entries }).unwrap_or_default(),
        },
        Err(error) => owner_error(error),
    }
}

pub async fn upload(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: RemoteBackupUploadRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !request.confirmed {
        return bad_request("remote backup upload requires confirmed=true");
    }
    let Some(owner) = context.remote_backup_owner() else {
        return unavailable("remote backup owner is unavailable");
    };
    match owner.upload(request.config).await {
        Ok(result) => RouteResponse {
            status: 200,
            body: serde_json::to_value(ApiResponse { data: result }).unwrap_or_default(),
        },
        Err(error) => owner_error(error),
    }
}

pub async fn restore(context: &ApiRuntimeContext, body: &[u8]) -> RouteResponse {
    let request: RemoteBackupRestoreRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => return bad_request("invalid JSON body"),
    };
    if !request.confirmed {
        return bad_request("remote backup restore requires confirmed=true");
    }
    let Some(owner) = context.remote_backup_owner() else {
        return unavailable("remote backup owner is unavailable");
    };
    match owner
        .restore(request.config, request.id, request.strategy)
        .await
    {
        Ok(result) => RouteResponse {
            status: 202,
            body: serde_json::to_value(ApiResponse { data: result }).unwrap_or_default(),
        },
        Err(error) => owner_error(error),
    }
}

fn owner_error(error: RemoteBackupOwnerError) -> RouteResponse {
    match error {
        RemoteBackupOwnerError::InvalidInput(message) => bad_request(&message),
        RemoteBackupOwnerError::NotFound(message) => RouteResponse {
            status: 404,
            body: serde_json::to_value(ApiError::not_found(&message)).unwrap_or_default(),
        },
        RemoteBackupOwnerError::Conflict(message) => RouteResponse {
            status: 409,
            body: serde_json::to_value(ApiError::conflict(&message)).unwrap_or_default(),
        },
        RemoteBackupOwnerError::Unavailable(message) => unavailable(&message),
        RemoteBackupOwnerError::Internal(message) => RouteResponse {
            status: 500,
            body: serde_json::to_value(ApiError::internal(&message)).unwrap_or_default(),
        },
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::remote_backup::{
        RemoteBackupEntry, RemoteBackupUploadResult, WebDavBackupConfig,
    };
    use crate::engine::api::remote_backup_owner::{RemoteBackupOwner, RemoteBackupOwnerFuture};
    use std::sync::Arc;

    struct FakeRemoteBackupOwner;

    impl RemoteBackupOwner for FakeRemoteBackupOwner {
        fn upload(
            &self,
            config: WebDavBackupConfig,
        ) -> RemoteBackupOwnerFuture<'_, RemoteBackupUploadResult> {
            Box::pin(async move {
                assert_eq!(config.username, "arin");
                Ok(RemoteBackupUploadResult {
                    entry: RemoteBackupEntry {
                        id: "backup-id".to_string(),
                        file_name: "Patina-backup-backup-id.zip".to_string(),
                        remote_path: "/Patina/Patina-backup-backup-id.zip".to_string(),
                        created_at_ms: 1,
                        size_bytes: 2,
                        app_version: "1.8.4".to_string(),
                        backup_version: 1,
                        schema_version: 1,
                        session_count: 3,
                        title_sample_count: 4,
                        setting_count: 5,
                        icon_cache_count: 6,
                    },
                    index_updated: true,
                    index_message: None,
                })
            })
        }

        fn list(
            &self,
            _config: WebDavBackupConfig,
        ) -> RemoteBackupOwnerFuture<'_, Vec<RemoteBackupEntry>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn restore(
            &self,
            _config: WebDavBackupConfig,
            _id: String,
            _strategy: crate::domain::backup::RestoreStrategy,
        ) -> RemoteBackupOwnerFuture<
            '_,
            crate::engine::api::backup_restore_owner::BackupRestoreScheduleResult,
        > {
            Box::pin(async {
                Err(RemoteBackupOwnerError::Conflict(
                    "restore unavailable in upload test".to_string(),
                ))
            })
        }
    }

    #[tokio::test]
    async fn upload_requires_explicit_confirmation_before_owner_lookup() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let context = ApiRuntimeContext::new(
            crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
        );
        let body = serde_json::to_vec(&serde_json::json!({
            "config": {
                "url": "https://dav.example.test",
                "username": "arin",
                "remoteDir": "/Patina"
            },
            "confirmed": false
        }))
        .unwrap();

        let response = upload(&context, &body).await;

        assert_eq!(response.status, 400);
        assert_eq!(response.body["error"]["code"], "bad_request");
        pool.close().await;
    }

    #[tokio::test]
    async fn upload_returns_the_daemon_owner_result() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let context = ApiRuntimeContext::new(
            crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
        )
        .with_remote_backup_owner(Arc::new(FakeRemoteBackupOwner));
        let body = serde_json::to_vec(&serde_json::json!({
            "config": {
                "url": "https://dav.example.test",
                "username": "arin",
                "remoteDir": "/Patina"
            },
            "confirmed": true
        }))
        .unwrap();

        let response = upload(&context, &body).await;

        assert_eq!(response.status, 200);
        assert_eq!(response.body["data"]["entry"]["id"], "backup-id");
        assert_eq!(response.body["data"]["indexUpdated"], true);
        pool.close().await;
    }

    #[tokio::test]
    async fn restore_requires_explicit_confirmation_before_owner_lookup() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let context = ApiRuntimeContext::new(
            crate::engine::runtime_context::RuntimeContext::system(pool.clone()),
        );
        let body = serde_json::to_vec(&serde_json::json!({
            "config": {
                "url": "https://dav.example.test",
                "username": "arin",
                "remoteDir": "/Patina"
            },
            "id": "backup-id",
            "strategy": "replace",
            "confirmed": false
        }))
        .unwrap();

        let response = restore(&context, &body).await;

        assert_eq!(response.status, 400);
        assert_eq!(response.body["error"]["code"], "bad_request");
        pool.close().await;
    }
}
