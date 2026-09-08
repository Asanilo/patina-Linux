use super::service_lifecycle::DaemonServiceLifecycleOwner;
use crate::domain::backup::RestoreStrategy;
use crate::engine::api::backup_restore_owner::{
    BackupRestoreOwner, BackupRestoreOwnerError, BackupRestoreOwnerFuture,
    BackupRestoreScheduleInput, BackupRestoreScheduleResult, BackupRestoreSnapshot,
};
use crate::engine::api::runtime_control::RuntimeControlError;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const RESERVATION_FILE_NAME: &str = "backup-restore-reservation.json";
const RESERVATION_VERSION: u32 = 1;
const MAX_RESERVATION_BYTES: u64 = 64 * 1024;
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ReservationStatus {
    Prepared,
    PendingRestart,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl ReservationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::PendingRestart => "pending_restart",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn is_active(self) -> bool {
        matches!(self, Self::Prepared | Self::PendingRestart | Self::Running)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RestoreReservation {
    version: u32,
    request_id: String,
    ticket: String,
    strategy: RestoreStrategy,
    archive_sha256: String,
    size_bytes: u64,
    status: ReservationStatus,
    requested_at_ms: i64,
    started_at_ms: Option<i64>,
    completed_at_ms: Option<i64>,
    restart_request_id: Option<String>,
    error: Option<String>,
    cleanup_warning: Option<String>,
}

pub struct DaemonBackupRestoreOwner {
    control_root: PathBuf,
    staging_root: PathBuf,
    service_lifecycle: Arc<DaemonServiceLifecycleOwner>,
    operation_lock: tokio::sync::Mutex<()>,
}

impl DaemonBackupRestoreOwner {
    pub fn new(
        control_root: PathBuf,
        staging_root: PathBuf,
        service_lifecycle: Arc<DaemonServiceLifecycleOwner>,
    ) -> Self {
        Self {
            control_root,
            staging_root,
            service_lifecycle,
            operation_lock: tokio::sync::Mutex::new(()),
        }
    }
}

impl BackupRestoreOwner for DaemonBackupRestoreOwner {
    fn schedule(
        &self,
        input: BackupRestoreScheduleInput,
    ) -> BackupRestoreOwnerFuture<'_, BackupRestoreScheduleResult> {
        Box::pin(async move {
            let _guard = self.operation_lock.lock().await;
            if !self.service_lifecycle.managed_by_systemd() {
                return Err(BackupRestoreOwnerError::Conflict(
                    "backup restore requires patinad to run under its systemd user service"
                        .to_string(),
                ));
            }
            if let Some(existing) =
                read_reservation(&self.control_root).map_err(BackupRestoreOwnerError::Internal)?
            {
                if existing.status.is_active() || existing.status == ReservationStatus::Failed {
                    return Err(BackupRestoreOwnerError::Conflict(format!(
                        "backup restore request `{}` must finish or be cancelled first",
                        existing.request_id
                    )));
                }
            }

            let staging_root = self.staging_root.clone();
            let validation_input = input.clone();
            tokio::task::spawn_blocking(move || {
                validate_staged_archive(&staging_root, &validation_input)
            })
            .await
            .map_err(|error| {
                BackupRestoreOwnerError::Internal(format!(
                    "backup restore validation task failed: {error}"
                ))
            })??;

            let requested_at_ms = now_ms();
            let mut reservation = RestoreReservation {
                version: RESERVATION_VERSION,
                request_id: random_id("restore").map_err(BackupRestoreOwnerError::Internal)?,
                ticket: input.ticket,
                strategy: input.strategy,
                archive_sha256: input.expected_sha256,
                size_bytes: input.expected_size_bytes,
                status: ReservationStatus::Prepared,
                requested_at_ms,
                started_at_ms: None,
                completed_at_ms: None,
                restart_request_id: None,
                error: None,
                cleanup_warning: None,
            };
            write_reservation_atomic(&self.control_root, &reservation)
                .map_err(BackupRestoreOwnerError::Internal)?;

            let restart = match self.service_lifecycle.request_restart(requested_at_ms) {
                Ok(restart) => restart,
                Err(error) => {
                    reservation.status = ReservationStatus::Failed;
                    reservation.completed_at_ms = Some(now_ms());
                    reservation.error = Some(runtime_control_error_message(&error));
                    let _ = write_reservation_atomic(&self.control_root, &reservation);
                    return Err(map_runtime_control_error(error));
                }
            };
            reservation.status = ReservationStatus::PendingRestart;
            reservation.restart_request_id = restart
                .service
                .restart
                .as_ref()
                .map(|marker| marker.request_id.clone());
            if let Err(error) = write_reservation_atomic(&self.control_root, &reservation) {
                reservation.status = ReservationStatus::Failed;
                reservation.completed_at_ms = Some(now_ms());
                reservation.error = Some(format!(
                    "failed to finalize backup restore reservation: {error}"
                ));
                let _ = write_reservation_atomic(&self.control_root, &reservation);
                return Err(BackupRestoreOwnerError::Internal(error));
            }

            Ok(BackupRestoreScheduleResult {
                restore: snapshot(&reservation),
                service: restart.service,
                reconnect_required: restart.reconnect_required,
            })
        })
    }

    fn snapshot(
        &self,
        request_id: Option<String>,
    ) -> BackupRestoreOwnerFuture<'_, Option<BackupRestoreSnapshot>> {
        Box::pin(async move {
            let _guard = self.operation_lock.lock().await;
            let reservation =
                read_reservation(&self.control_root).map_err(BackupRestoreOwnerError::Internal)?;
            let Some(reservation) = reservation else {
                return Ok(None);
            };
            if request_id
                .as_deref()
                .is_some_and(|request_id| request_id != reservation.request_id)
            {
                return Err(BackupRestoreOwnerError::NotFound(
                    "backup restore request was not found".to_string(),
                ));
            }
            Ok(Some(snapshot(&reservation)))
        })
    }

    fn cancel(&self, request_id: String) -> BackupRestoreOwnerFuture<'_, BackupRestoreSnapshot> {
        Box::pin(async move {
            let _guard = self.operation_lock.lock().await;
            let mut reservation = read_reservation(&self.control_root)
                .map_err(BackupRestoreOwnerError::Internal)?
                .ok_or_else(|| {
                    BackupRestoreOwnerError::NotFound(
                        "backup restore request was not found".to_string(),
                    )
                })?;
            if reservation.request_id != request_id {
                return Err(BackupRestoreOwnerError::NotFound(
                    "backup restore request was not found".to_string(),
                ));
            }
            if reservation.status != ReservationStatus::Failed {
                return Err(BackupRestoreOwnerError::Conflict(
                    "only a failed backup restore request can be cancelled".to_string(),
                ));
            }

            crate::platform::backup_restore_staging::discard(
                &self.staging_root,
                &reservation.ticket,
            )
            .map_err(BackupRestoreOwnerError::Internal)?;
            reservation.status = ReservationStatus::Cancelled;
            reservation.completed_at_ms = Some(now_ms());
            write_reservation_atomic(&self.control_root, &reservation)
                .map_err(BackupRestoreOwnerError::Internal)?;
            Ok(snapshot(&reservation))
        })
    }

    fn references_staged_ticket(&self, ticket: String) -> BackupRestoreOwnerFuture<'_, bool> {
        Box::pin(async move {
            let _guard = self.operation_lock.lock().await;
            Ok(read_reservation(&self.control_root)
                .map_err(BackupRestoreOwnerError::Internal)?
                .is_some_and(|reservation| {
                    reservation.ticket == ticket
                        && matches!(
                            reservation.status,
                            ReservationStatus::Prepared
                                | ReservationStatus::PendingRestart
                                | ReservationStatus::Running
                                | ReservationStatus::Failed
                        )
                }))
        })
    }
}

pub async fn run_startup_restore(
    control_root: &Path,
    staging_root: &Path,
    pool: &Pool<Sqlite>,
    started_at_ms: i64,
) -> Result<Option<BackupRestoreSnapshot>, String> {
    let Some(mut reservation) = read_reservation(control_root)? else {
        return Ok(None);
    };
    if matches!(
        reservation.status,
        ReservationStatus::Completed | ReservationStatus::Failed | ReservationStatus::Cancelled
    ) {
        return Ok(Some(snapshot(&reservation)));
    }
    if reservation.status == ReservationStatus::Prepared {
        reservation.status = ReservationStatus::Failed;
        reservation.completed_at_ms = Some(started_at_ms);
        reservation.error =
            Some("backup restore reservation was not finalized before daemon restart".to_string());
        write_reservation_atomic(control_root, &reservation)?;
        return Ok(Some(snapshot(&reservation)));
    }

    if crate::data::repositories::backup_restore::receipt_exists(
        pool,
        &reservation.request_id,
        &reservation.archive_sha256,
    )
    .await?
    {
        return complete_from_receipt(control_root, staging_root, reservation).map(Some);
    }

    reservation.status = ReservationStatus::Running;
    reservation.started_at_ms.get_or_insert(started_at_ms);
    reservation.error = None;
    write_reservation_atomic(control_root, &reservation)?;

    let input = BackupRestoreScheduleInput {
        ticket: reservation.ticket.clone(),
        expected_sha256: reservation.archive_sha256.clone(),
        expected_size_bytes: reservation.size_bytes,
        strategy: reservation.strategy,
    };
    let result = match validate_staged_archive(staging_root, &input) {
        Ok(path) => {
            crate::data::backup::restore_backup_from_path_with_receipt(
                pool,
                &path,
                reservation.strategy,
                started_at_ms,
                &reservation.request_id,
                &reservation.archive_sha256,
            )
            .await
        }
        Err(error) => Err(owner_error_message(error)),
    };

    match result {
        Ok(()) => complete_from_receipt(control_root, staging_root, reservation).map(Some),
        Err(error) => {
            reservation.status = ReservationStatus::Failed;
            reservation.completed_at_ms = Some(now_ms());
            reservation.error = Some(error);
            write_reservation_atomic(control_root, &reservation)?;
            Ok(Some(snapshot(&reservation)))
        }
    }
}

fn complete_from_receipt(
    control_root: &Path,
    staging_root: &Path,
    mut reservation: RestoreReservation,
) -> Result<BackupRestoreSnapshot, String> {
    reservation.status = ReservationStatus::Completed;
    reservation.completed_at_ms = Some(now_ms());
    reservation.error = None;
    write_reservation_atomic(control_root, &reservation)?;
    if let Err(error) =
        crate::platform::backup_restore_staging::discard(staging_root, &reservation.ticket)
    {
        reservation.cleanup_warning = Some(error);
        write_reservation_atomic(control_root, &reservation)?;
    }
    Ok(snapshot(&reservation))
}

fn validate_staged_archive(
    staging_root: &Path,
    input: &BackupRestoreScheduleInput,
) -> Result<PathBuf, BackupRestoreOwnerError> {
    let path = crate::platform::backup_restore_staging::validate(
        staging_root,
        &input.ticket,
        &input.expected_sha256,
        input.expected_size_bytes,
    )
    .map_err(map_staging_error)?;
    let (preview, actual_sha256, actual_size_bytes) =
        crate::data::backup::inspect_restore_archive(&path)
            .map_err(BackupRestoreOwnerError::InvalidInput)?;
    if !preview.restore_supported {
        return Err(BackupRestoreOwnerError::InvalidInput(
            preview.restore_message,
        ));
    }
    if actual_sha256 != input.expected_sha256 || actual_size_bytes != input.expected_size_bytes {
        return Err(BackupRestoreOwnerError::Conflict(
            "staged backup archive changed during validation".to_string(),
        ));
    }
    Ok(path)
}

fn map_staging_error(error: String) -> BackupRestoreOwnerError {
    if error.contains("ticket is invalid") || error.contains("fingerprint is invalid") {
        BackupRestoreOwnerError::InvalidInput(error)
    } else if error.contains("is unavailable") {
        BackupRestoreOwnerError::NotFound("staged backup archive no longer exists".to_string())
    } else if error.contains("changed") || error.contains("does not match") {
        BackupRestoreOwnerError::Conflict(error)
    } else {
        BackupRestoreOwnerError::Internal(error)
    }
}

fn snapshot(reservation: &RestoreReservation) -> BackupRestoreSnapshot {
    BackupRestoreSnapshot {
        request_id: reservation.request_id.clone(),
        status: reservation.status.as_str().to_string(),
        strategy: reservation.strategy,
        archive_sha256: reservation.archive_sha256.clone(),
        size_bytes: reservation.size_bytes,
        requested_at_ms: reservation.requested_at_ms,
        started_at_ms: reservation.started_at_ms,
        completed_at_ms: reservation.completed_at_ms,
        restart_request_id: reservation.restart_request_id.clone(),
        error: reservation.error.clone(),
        cleanup_warning: reservation.cleanup_warning.clone(),
    }
}

fn reservation_path(control_root: &Path) -> PathBuf {
    control_root.join(RESERVATION_FILE_NAME)
}

fn read_reservation(control_root: &Path) -> Result<Option<RestoreReservation>, String> {
    let path = reservation_path(control_root);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to inspect backup restore reservation: {error}"
            ))
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("backup restore reservation must be a regular non-symlink file".to_string());
    }
    if metadata.len() > MAX_RESERVATION_BYTES {
        return Err("backup restore reservation exceeds the size limit".to_string());
    }
    require_owner_only_file(&path)?;
    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read backup restore reservation: {error}"))?;
    let reservation: RestoreReservation = serde_json::from_str(&raw)
        .map_err(|error| format!("failed to parse backup restore reservation: {error}"))?;
    if reservation.version != RESERVATION_VERSION {
        return Err(format!(
            "unsupported backup restore reservation version {}",
            reservation.version
        ));
    }
    Ok(Some(reservation))
}

fn write_reservation_atomic(
    control_root: &Path,
    reservation: &RestoreReservation,
) -> Result<(), String> {
    fs::create_dir_all(control_root)
        .map_err(|error| format!("failed to create daemon control directory: {error}"))?;
    let metadata = fs::symlink_metadata(control_root)
        .map_err(|error| format!("failed to inspect daemon control directory: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("daemon control root must be a real directory".to_string());
    }
    let path = reservation_path(control_root);
    let temporary_path = control_root.join(format!(
        ".{RESERVATION_FILE_NAME}.tmp-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<(), String> {
        let mut file = options
            .open(&temporary_path)
            .map_err(|error| format!("failed to create backup restore reservation: {error}"))?;
        let bytes = serde_json::to_vec(reservation)
            .map_err(|error| format!("failed to serialize backup restore reservation: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("failed to write backup restore reservation: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync backup restore reservation: {error}"))?;
        drop(file);
        fs::rename(&temporary_path, &path)
            .map_err(|error| format!("failed to replace backup restore reservation: {error}"))?;
        require_owner_only_file(&path)?;
        File::open(control_root)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| format!("failed to sync daemon control directory: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

#[cfg(unix)]
fn require_owner_only_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = fs::metadata(path)
        .map_err(|error| format!("failed to inspect backup restore reservation: {error}"))?;
    if metadata.nlink() != 1 || metadata.permissions().mode() & 0o777 != 0o600 {
        return Err("backup restore reservation must be an owner-only 0600 file".to_string());
    }
    Ok(())
}

#[cfg(not(unix))]
fn require_owner_only_file(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn random_id(prefix: &str) -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to generate backup restore request id: {error}"))?;
    Ok(format!(
        "{prefix}_{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    ))
}

fn now_ms() -> i64 {
    crate::app::runtime::now_ms().min(i64::MAX as u64) as i64
}

fn runtime_control_error_message(error: &RuntimeControlError) -> String {
    match error {
        RuntimeControlError::InvalidInput(message)
        | RuntimeControlError::Conflict(message)
        | RuntimeControlError::Internal(message) => message.clone(),
    }
}

fn map_runtime_control_error(error: RuntimeControlError) -> BackupRestoreOwnerError {
    match error {
        RuntimeControlError::InvalidInput(message) => {
            BackupRestoreOwnerError::InvalidInput(message)
        }
        RuntimeControlError::Conflict(message) => BackupRestoreOwnerError::Conflict(message),
        RuntimeControlError::Internal(message) => BackupRestoreOwnerError::Internal(message),
    }
}

fn owner_error_message(error: BackupRestoreOwnerError) -> String {
    match error {
        BackupRestoreOwnerError::InvalidInput(message)
        | BackupRestoreOwnerError::NotFound(message)
        | BackupRestoreOwnerError::Conflict(message)
        | BackupRestoreOwnerError::Internal(message) => message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::backup::BackupSession;
    use crate::engine::api::backup_restore_owner::BackupRestoreOwner;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "patina-daemon-backup-restore-{label}-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ))
    }

    async fn prepared_pool(root: &Path) -> Pool<Sqlite> {
        crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(
            &root.join("data/patina.db"),
            true,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn startup_restore_commits_receipt_closes_active_time_and_removes_exact_stage() {
        let root = temp_root("success");
        let control_root = root.join("control");
        let staging_root = control_root.join("staging");
        let source = root.join("source.zip");
        let pool = prepared_pool(&root).await;
        let mut tx = pool.begin().await.unwrap();
        crate::data::repositories::sessions::insert_for_restore(
            &mut tx,
            &[BackupSession {
                id: 1,
                app_name: "Zen".to_string(),
                exe_name: "zen".to_string(),
                window_title: Some("Example".to_string()),
                start_time: 1_000,
                end_time: None,
                duration: None,
                continuity_group_start_time: Some(1_000),
            }],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        crate::data::backup::export_scheduled_backup_create_new(&pool, &source)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        crate::data::repositories::sessions::clear_for_restore(&mut tx)
            .await
            .unwrap();
        crate::data::repositories::sessions::insert_for_restore(
            &mut tx,
            &[BackupSession {
                id: 2,
                app_name: "Old".to_string(),
                exe_name: "old".to_string(),
                window_title: Some("Old".to_string()),
                start_time: 10,
                end_time: Some(20),
                duration: Some(10),
                continuity_group_start_time: Some(10),
            }],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let staged =
            crate::platform::backup_restore_staging::stage_file(&staging_root, &source).unwrap();
        let reservation = RestoreReservation {
            version: RESERVATION_VERSION,
            request_id: "restore_0123456789abcdef0123456789abcdef".to_string(),
            ticket: staged.ticket.clone(),
            strategy: RestoreStrategy::Replace,
            archive_sha256: staged.sha256.clone(),
            size_bytes: staged.size_bytes,
            status: ReservationStatus::PendingRestart,
            requested_at_ms: 4_000,
            started_at_ms: None,
            completed_at_ms: None,
            restart_request_id: Some("restart_test".to_string()),
            error: None,
            cleanup_warning: None,
        };
        write_reservation_atomic(&control_root, &reservation).unwrap();

        let completed = run_startup_restore(&control_root, &staging_root, &pool, 5_000)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(completed.status, "completed");
        assert!(!staging_root.join(format!("{}.zip", staged.ticket)).exists());
        let restored = crate::data::repositories::sessions::fetch_all_for_backup(&pool)
            .await
            .unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].exe_name, "zen");
        assert_eq!(restored[0].end_time, Some(5_000));
        assert_eq!(restored[0].duration, Some(4_000));
        assert!(crate::data::repositories::backup_restore::receipt_exists(
            &pool,
            &reservation.request_id,
            &reservation.archive_sha256,
        )
        .await
        .unwrap());

        pool.close().await;
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn failed_startup_restore_keeps_database_and_stage_until_explicit_cancel() {
        let root = temp_root("failure");
        let control_root = root.join("control");
        let staging_root = control_root.join("staging");
        let source = root.join("invalid.zip");
        fs::create_dir_all(&root).unwrap();
        fs::write(&source, b"not a backup").unwrap();
        let pool = prepared_pool(&root).await;
        let mut tx = pool.begin().await.unwrap();
        crate::data::repositories::sessions::insert_for_restore(
            &mut tx,
            &[BackupSession {
                id: 1,
                app_name: "Keep".to_string(),
                exe_name: "keep".to_string(),
                window_title: Some("Keep".to_string()),
                start_time: 10,
                end_time: Some(20),
                duration: Some(10),
                continuity_group_start_time: Some(10),
            }],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let staged =
            crate::platform::backup_restore_staging::stage_file(&staging_root, &source).unwrap();
        let request_id = "restore_abcdef0123456789abcdef0123456789".to_string();
        write_reservation_atomic(
            &control_root,
            &RestoreReservation {
                version: RESERVATION_VERSION,
                request_id: request_id.clone(),
                ticket: staged.ticket.clone(),
                strategy: RestoreStrategy::Replace,
                archive_sha256: staged.sha256,
                size_bytes: staged.size_bytes,
                status: ReservationStatus::PendingRestart,
                requested_at_ms: 1_000,
                started_at_ms: None,
                completed_at_ms: None,
                restart_request_id: Some("restart_test".to_string()),
                error: None,
                cleanup_warning: None,
            },
        )
        .unwrap();

        let failed = run_startup_restore(&control_root, &staging_root, &pool, 2_000)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(failed.status, "failed");
        assert!(failed.error.is_some());
        assert!(staging_root
            .join(format!("{}.zip", staged.ticket))
            .is_file());
        let sessions = crate::data::repositories::sessions::fetch_all_for_backup(&pool)
            .await
            .unwrap();
        assert_eq!(
            sessions
                .iter()
                .filter(|session| session.exe_name == "keep")
                .count(),
            1
        );

        let lifecycle =
            Arc::new(DaemonServiceLifecycleOwner::new(&control_root, true, 3_000).unwrap());
        let owner =
            DaemonBackupRestoreOwner::new(control_root.clone(), staging_root.clone(), lifecycle);
        let cancelled = owner.cancel(request_id).await.unwrap();
        assert_eq!(cancelled.status, "cancelled");
        assert!(cancelled.error.is_some());
        assert!(!staging_root.join(format!("{}.zip", staged.ticket)).exists());

        pool.close().await;
        fs::remove_dir_all(root).unwrap();
    }
}
