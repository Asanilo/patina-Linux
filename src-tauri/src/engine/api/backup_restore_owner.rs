use crate::domain::backup::RestoreStrategy;
use crate::engine::api::runtime_control::DaemonServiceRuntimeSnapshot;
use std::future::Future;
use std::pin::Pin;

pub type BackupRestoreOwnerFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BackupRestoreOwnerError>> + Send + 'a>>;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct BackupRestoreScheduleInput {
    pub ticket: String,
    pub expected_sha256: String,
    pub expected_size_bytes: u64,
    pub strategy: RestoreStrategy,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct BackupRestoreSnapshot {
    pub request_id: String,
    pub status: String,
    pub strategy: RestoreStrategy,
    pub archive_sha256: String,
    pub size_bytes: u64,
    pub requested_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub restart_request_id: Option<String>,
    pub error: Option<String>,
    pub cleanup_warning: Option<String>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct BackupRestoreScheduleResult {
    pub restore: BackupRestoreSnapshot,
    pub service: DaemonServiceRuntimeSnapshot,
    pub reconnect_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackupRestoreOwnerError {
    InvalidInput(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

pub trait BackupRestoreOwner: Send + Sync {
    fn schedule(
        &self,
        input: BackupRestoreScheduleInput,
    ) -> BackupRestoreOwnerFuture<'_, BackupRestoreScheduleResult>;

    fn snapshot(
        &self,
        request_id: Option<String>,
    ) -> BackupRestoreOwnerFuture<'_, Option<BackupRestoreSnapshot>>;

    fn cancel(&self, request_id: String) -> BackupRestoreOwnerFuture<'_, BackupRestoreSnapshot>;

    fn references_staged_ticket(&self, _ticket: String) -> BackupRestoreOwnerFuture<'_, bool> {
        Box::pin(async { Ok(false) })
    }
}
