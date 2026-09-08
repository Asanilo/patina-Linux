use crate::domain::remote_backup::{RemoteBackupUploadResult, WebDavBackupConfig};
use crate::engine::api::remote_backup_owner::{
    RemoteBackupOwner, RemoteBackupOwnerError, RemoteBackupOwnerFuture,
};
use crate::engine::runtime_context::RuntimeContext;
use crate::platform::app_paths::AppProfile;
use std::path::PathBuf;
use tokio::sync::Mutex;

pub struct DaemonRemoteBackupOwner {
    runtime: RuntimeContext,
    temp_dir: PathBuf,
    profile: AppProfile,
    operation_lock: Mutex<()>,
}

impl DaemonRemoteBackupOwner {
    pub fn new(runtime: RuntimeContext, temp_dir: PathBuf, profile: AppProfile) -> Self {
        Self {
            runtime,
            temp_dir,
            profile,
            operation_lock: Mutex::new(()),
        }
    }

    async fn upload_inner(
        &self,
        config: WebDavBackupConfig,
    ) -> Result<RemoteBackupUploadResult, RemoteBackupOwnerError> {
        let _guard = self.operation_lock.lock().await;
        crate::data::remote_backup::upload_webdav_backup_from_pool(
            self.runtime.pool(),
            &self.temp_dir,
            self.profile,
            config,
        )
        .await
        .map_err(classify_upload_error)
    }
}

impl RemoteBackupOwner for DaemonRemoteBackupOwner {
    fn upload(
        &self,
        config: WebDavBackupConfig,
    ) -> RemoteBackupOwnerFuture<'_, RemoteBackupUploadResult> {
        Box::pin(async move { self.upload_inner(config).await })
    }
}

fn classify_upload_error(error: String) -> RemoteBackupOwnerError {
    if error.contains("cannot be empty")
        || error.contains("invalid WebDAV")
        || error.contains("unsupported path")
        || error.contains("control characters")
    {
        RemoteBackupOwnerError::InvalidInput(error)
    } else if error.contains("password is missing")
        || error.contains("Secret Service")
        || error.contains("keyring")
        || error.contains("WebDAV")
    {
        RemoteBackupOwnerError::Unavailable(error)
    } else {
        RemoteBackupOwnerError::Internal(error)
    }
}
