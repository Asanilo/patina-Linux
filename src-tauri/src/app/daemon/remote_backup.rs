use crate::domain::backup::RestoreStrategy;
use crate::domain::remote_backup::{
    RemoteBackupEntry, RemoteBackupUploadResult, WebDavBackupConfig,
};
use crate::engine::api::backup_restore_owner::{
    BackupRestoreOwner, BackupRestoreOwnerError, BackupRestoreScheduleInput,
    BackupRestoreScheduleResult,
};
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
    restore_staging_dir: PathBuf,
    profile: AppProfile,
    backup_restore_owner: std::sync::Arc<dyn BackupRestoreOwner>,
    operation_lock: Mutex<()>,
}

impl DaemonRemoteBackupOwner {
    pub fn new(
        runtime: RuntimeContext,
        temp_dir: PathBuf,
        restore_staging_dir: PathBuf,
        profile: AppProfile,
        backup_restore_owner: std::sync::Arc<dyn BackupRestoreOwner>,
    ) -> Self {
        Self {
            runtime,
            temp_dir,
            restore_staging_dir,
            profile,
            backup_restore_owner,
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

    async fn list_inner(
        &self,
        config: WebDavBackupConfig,
    ) -> Result<Vec<RemoteBackupEntry>, RemoteBackupOwnerError> {
        let _guard = self.operation_lock.lock().await;
        crate::data::remote_backup::list_webdav_backups(self.profile, config)
            .await
            .map_err(classify_remote_error)
    }

    async fn restore_inner(
        &self,
        config: WebDavBackupConfig,
        id: String,
        strategy: RestoreStrategy,
    ) -> Result<BackupRestoreScheduleResult, RemoteBackupOwnerError> {
        let _guard = self.operation_lock.lock().await;
        let staged = crate::data::remote_backup::stage_webdav_backup_for_restore(
            self.profile,
            config,
            id,
            &self.temp_dir,
            &self.restore_staging_dir,
        )
        .await
        .map_err(classify_remote_error)?;
        let ticket = staged.ticket.clone();
        let result = self
            .backup_restore_owner
            .schedule(BackupRestoreScheduleInput {
                ticket: staged.ticket,
                expected_sha256: staged.sha256,
                expected_size_bytes: staged.size_bytes,
                strategy,
            })
            .await;
        match result {
            Ok(result) => Ok(result),
            Err(error) => {
                let is_reserved = self
                    .backup_restore_owner
                    .references_staged_ticket(ticket.clone())
                    .await
                    .unwrap_or(true);
                let cleanup_error = if is_reserved {
                    None
                } else {
                    crate::platform::backup_restore_staging::discard(
                        &self.restore_staging_dir,
                        &ticket,
                    )
                    .err()
                };
                Err(map_restore_error(error, cleanup_error))
            }
        }
    }
}

impl RemoteBackupOwner for DaemonRemoteBackupOwner {
    fn upload(
        &self,
        config: WebDavBackupConfig,
    ) -> RemoteBackupOwnerFuture<'_, RemoteBackupUploadResult> {
        Box::pin(async move { self.upload_inner(config).await })
    }

    fn list(
        &self,
        config: WebDavBackupConfig,
    ) -> RemoteBackupOwnerFuture<'_, Vec<RemoteBackupEntry>> {
        Box::pin(async move { self.list_inner(config).await })
    }

    fn restore(
        &self,
        config: WebDavBackupConfig,
        id: String,
        strategy: RestoreStrategy,
    ) -> RemoteBackupOwnerFuture<'_, BackupRestoreScheduleResult> {
        Box::pin(async move { self.restore_inner(config, id, strategy).await })
    }
}

fn classify_upload_error(error: String) -> RemoteBackupOwnerError {
    classify_remote_error(error)
}

fn classify_remote_error(error: String) -> RemoteBackupOwnerError {
    if error.contains("cannot be empty")
        || error.contains("is invalid")
        || error.contains("invalid WebDAV")
        || error.contains("failed to parse WebDAV backup index")
        || error.contains("unsupported WebDAV backup index")
        || error.contains("belongs to another product")
        || error.contains("does not match its validated index metadata")
        || error.contains("unsupported path")
        || error.contains("control characters")
        || error.contains("unsafe backup path")
        || error.contains("duplicate backup ids")
        || error.contains("size limit")
        || error.contains("oversized backup")
        || error.starts_with("Backup format version")
        || error.starts_with("Backup schema version")
    {
        RemoteBackupOwnerError::InvalidInput(error)
    } else if error.contains("was not found") {
        RemoteBackupOwnerError::NotFound(error)
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

fn map_restore_error(
    error: BackupRestoreOwnerError,
    cleanup_error: Option<String>,
) -> RemoteBackupOwnerError {
    let append_cleanup = |message: String| match cleanup_error.as_deref() {
        Some(cleanup) => format!("{message}; failed to discard unreserved staging file: {cleanup}"),
        None => message,
    };
    match error {
        BackupRestoreOwnerError::InvalidInput(message) => {
            RemoteBackupOwnerError::InvalidInput(append_cleanup(message))
        }
        BackupRestoreOwnerError::NotFound(message) => {
            RemoteBackupOwnerError::NotFound(append_cleanup(message))
        }
        BackupRestoreOwnerError::Conflict(message) => {
            RemoteBackupOwnerError::Conflict(append_cleanup(message))
        }
        BackupRestoreOwnerError::Internal(message) => {
            RemoteBackupOwnerError::Internal(append_cleanup(message))
        }
    }
}
