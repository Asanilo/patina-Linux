use crate::domain::activity_import::{ImportCommitReportDto, ImportDeleteReportDto};
use std::future::Future;
use std::pin::Pin;

pub type ActivityImportOwnerFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ActivityImportOwnerError>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivityImportOwnerError {
    InvalidInput(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

pub trait ActivityImportOwner: Send + Sync {
    fn commit_staged(
        &self,
        ticket: String,
        source_name: String,
        expected_fingerprint: String,
    ) -> ActivityImportOwnerFuture<'_, ImportCommitReportDto>;

    fn delete_batch(
        &self,
        batch_id: String,
    ) -> ActivityImportOwnerFuture<'_, ImportDeleteReportDto>;
}
