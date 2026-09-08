use crate::domain::remote_backup::{RemoteBackupUploadResult, WebDavBackupConfig};
use std::future::Future;
use std::pin::Pin;

pub type RemoteBackupOwnerFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, RemoteBackupOwnerError>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteBackupOwnerError {
    InvalidInput(String),
    Unavailable(String),
    Internal(String),
}

pub trait RemoteBackupOwner: Send + Sync {
    fn upload(
        &self,
        config: WebDavBackupConfig,
    ) -> RemoteBackupOwnerFuture<'_, RemoteBackupUploadResult>;
}
