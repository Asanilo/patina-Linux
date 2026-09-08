use crate::domain::backup_schedule::{ScheduledBackupConfigInput, ScheduledBackupSnapshot};
use std::future::Future;
use std::pin::Pin;

pub type ScheduledBackupOwnerFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ScheduledBackupOwnerError>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScheduledBackupOwnerError {
    InvalidInput(String),
    Internal(String),
}

pub trait ScheduledBackupOwner: Send + Sync {
    fn snapshot(&self) -> ScheduledBackupOwnerFuture<'_, ScheduledBackupSnapshot>;

    fn save_config(
        &self,
        input: ScheduledBackupConfigInput,
    ) -> ScheduledBackupOwnerFuture<'_, ScheduledBackupSnapshot>;
}
