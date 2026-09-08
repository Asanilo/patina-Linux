use crate::domain::backup::RestoreStrategy;
use crate::domain::remote_backup::{
    RemoteBackupEntry, RemoteBackupUploadResult, WebDavBackupConfig,
};
use crate::engine::api::backup_restore_owner::BackupRestoreScheduleResult;
use std::future::Future;
use std::pin::Pin;

pub type RemoteBackupOwnerFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, RemoteBackupOwnerError>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteBackupOwnerError {
    InvalidInput(String),
    NotFound(String),
    Conflict(String),
    Unavailable(String),
    Internal(String),
}

pub trait RemoteBackupOwner: Send + Sync {
    fn upload(
        &self,
        config: WebDavBackupConfig,
    ) -> RemoteBackupOwnerFuture<'_, RemoteBackupUploadResult>;

    fn list(
        &self,
        config: WebDavBackupConfig,
    ) -> RemoteBackupOwnerFuture<'_, Vec<RemoteBackupEntry>>;

    fn restore(
        &self,
        config: WebDavBackupConfig,
        id: String,
        strategy: RestoreStrategy,
    ) -> RemoteBackupOwnerFuture<'_, BackupRestoreScheduleResult>;
}
