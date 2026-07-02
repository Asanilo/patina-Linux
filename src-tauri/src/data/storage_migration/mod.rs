mod executor;
mod plan;

use crate::data::{backup, sqlite_pool};
use crate::domain::storage::{StorageMigrationPreview, StorageTargetKind};
use crate::platform::{app_paths, storage_anchor, storage_paths, storage_usage, webview_cache};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};

pub fn preview_storage_migration<R: Runtime>(
    app: &AppHandle<R>,
    kind: StorageTargetKind,
    selected_parent: PathBuf,
) -> Result<StorageMigrationPreview, String> {
    let current = storage_paths::resolve_storage_paths(app)?;
    let target = target_root(app_paths::app_profile(app), kind, &selected_parent);
    let plan_kind = target_kind(kind);
    plan::preview_with_deps(
        &current,
        plan_kind,
        target,
        || payload_size(&current, kind),
        storage_usage::available_space_for,
    )
}

pub async fn schedule_storage_migration(
    app: AppHandle,
    kind: StorageTargetKind,
    selected_parent: PathBuf,
) -> Result<storage_anchor::PendingStorageMigration, String> {
    let preview = preview_storage_migration(&app, kind, selected_parent)?;
    let current = storage_paths::resolve_storage_paths(&app)?;
    let existing = storage_anchor::read_pending_migration(&app)?;
    let id = existing
        .as_ref()
        .map(|pending| pending.id.clone())
        .unwrap_or_else(new_migration_id);
    let profile = app_paths::app_profile(&app).key();
    let (requested_data_root, requested_webview_root) = match kind {
        StorageTargetKind::Data => (Some(preview.target_data_root), None),
        StorageTargetKind::Webview => (None, Some(preview.target_webview_root)),
    };
    let pending = plan::plan_pending(
        &current,
        existing.as_ref(),
        requested_data_root,
        requested_webview_root,
        &id,
        profile,
        storage_anchor::now_ms(),
    )?;

    let backup_path = current
        .backup_dir
        .join(format!("Patina-pre-migration-{}.patina-backup", pending.id));
    let backup_app = app.clone();
    let checkpoint_app = app.clone();
    let persist_app = app.clone();
    let persisted_pending = pending.clone();
    schedule_preparation_with(
        move || async move {
            backup::export_backup(
                Some(backup_path.to_string_lossy().into_owned()),
                backup_app,
            )
            .await
            .map(|_| ())
        },
        move || async move { sqlite_pool::checkpoint_current_database(&checkpoint_app).await },
        move || async move {
            storage_anchor::write_pending_migration(&persist_app, &persisted_pending)
        },
    )
    .await?;

    Ok(pending)
}

pub fn cancel_pending_storage_migration<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    storage_anchor::remove_pending_migration(app)
}

pub async fn run_pending_storage_migration<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let Some(pending) = storage_anchor::read_pending_migration(app)? else {
        return Ok(());
    };
    let current = storage_paths::resolve_storage_paths(app)?;
    let execution = executor::execute_pending_with_deps(
        &pending,
        &current,
        |next| match next {
            Some(root) => storage_anchor::write_data_anchor(app, root),
            None => storage_anchor::remove_data_anchor(app),
        },
        |next| match next {
            Some(root) => storage_anchor::write_webview_anchor(app, root),
            None => storage_anchor::remove_webview_anchor(app),
        },
    )
    .await;

    let maintenance = maintenance_state_after_execution(&pending, execution.clone());
    storage_anchor::remove_pending_migration(app)
        .map_err(|error| format!("failed to clear completed storage migration request: {error}"))?;
    if let Err(error) = storage_anchor::write_maintenance_state(app, &maintenance) {
        eprintln!("[storage] failed to persist migration maintenance state: {error}");
    }

    match execution {
        Ok(()) => Ok(()),
        Err(error) => {
            eprintln!("[storage] pending migration failed: {error}");
            ensure_source_can_continue(&pending)?;
            Ok(())
        }
    }
}

fn maintenance_state_after_execution(
    pending: &storage_anchor::PendingStorageMigration,
    execution: Result<(), String>,
) -> storage_anchor::StorageMaintenanceState {
    let mut state = storage_anchor::StorageMaintenanceState::new(&pending.profile);
    match execution {
        Ok(()) => {
            state.last_migration_status = Some("succeeded".to_string());
            if pending.source_data_root != pending.target_data_root {
                state.retained_previous_data_root = Some(pending.source_data_root.clone());
            }
            if pending.source_webview_root != pending.target_webview_root {
                state.retained_previous_webview_root = Some(pending.source_webview_root.clone());
            }
        }
        Err(error) => {
            state.last_migration_status = Some("failed".to_string());
            state.last_maintenance_error = Some(error);
        }
    }
    state
}

fn ensure_source_can_continue(
    pending: &storage_anchor::PendingStorageMigration,
) -> Result<(), String> {
    let database = pending.source_data_root.join("patina.db");
    let metadata = fs::symlink_metadata(&database).map_err(|error| {
        format!(
            "storage migration failed and active source database `{}` is unavailable: {error}",
            database.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "storage migration failed and active source database `{}` is not a regular file",
            database.display()
        ));
    }
    Ok(())
}

fn target_root(
    profile: app_paths::AppProfile,
    kind: StorageTargetKind,
    selected_parent: &Path,
) -> PathBuf {
    let product_root = app_paths::derive_product_root(selected_parent, profile);
    match kind {
        StorageTargetKind::Data => product_root,
        StorageTargetKind::Webview => product_root.join("webview"),
    }
}

fn target_kind(kind: StorageTargetKind) -> plan::TargetKind {
    match kind {
        StorageTargetKind::Data => plan::TargetKind::Data,
        StorageTargetKind::Webview => plan::TargetKind::Webview,
    }
}

fn payload_size(
    current: &storage_paths::StoragePaths,
    kind: StorageTargetKind,
) -> Result<u64, String> {
    match kind {
        StorageTargetKind::Data => {
            let mut total = storage_usage::path_size(&current.db_path)?;
            total = total.saturating_add(storage_usage::path_size(
                &current.data_root.join("patina.db-wal"),
            )?);
            total = total.saturating_add(storage_usage::path_size(
                &current.data_root.join("patina.db-shm"),
            )?);
            total = total.saturating_add(storage_usage::path_size(&current.backup_dir)?);
            Ok(total)
        }
        StorageTargetKind::Webview => webview_cache::persistent_profile_size(&current.webview_root),
    }
}

fn new_migration_id() -> String {
    let mut bytes = [0_u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        return format!("migration-{}", storage_anchor::now_ms());
    }
    let encoded = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("migration-{encoded}")
}

async fn schedule_preparation_with<
    Backup,
    BackupFuture,
    Checkpoint,
    CheckpointFuture,
    Persist,
    PersistFuture,
>(
    backup: Backup,
    checkpoint: Checkpoint,
    persist: Persist,
) -> Result<(), String>
where
    Backup: FnOnce() -> BackupFuture,
    BackupFuture: Future<Output = Result<(), String>>,
    Checkpoint: FnOnce() -> CheckpointFuture,
    CheckpointFuture: Future<Output = Result<(), String>>,
    Persist: FnOnce() -> PersistFuture,
    PersistFuture: Future<Output = Result<(), String>>,
{
    backup().await?;
    checkpoint().await?;
    persist().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::storage_anchor::{
        PendingStorageMigration, STORAGE_MIGRATION_PENDING_FORMAT,
    };
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    fn pending() -> PendingStorageMigration {
        PendingStorageMigration {
            format: STORAGE_MIGRATION_PENDING_FORMAT.to_string(),
            id: "migration-test".to_string(),
            profile: "production".to_string(),
            source_data_root: PathBuf::from("/source/data"),
            target_data_root: PathBuf::from("/target/data"),
            source_webview_root: PathBuf::from("/source/webview"),
            target_webview_root: PathBuf::from("/target/webview"),
            created_at_ms: 1,
            state: "pending-restart".to_string(),
        }
    }

    #[test]
    fn schedule_preparation_orders_backup_checkpoint_then_pending_write() {
        tauri::async_runtime::block_on(async {
            let events = Arc::new(Mutex::new(Vec::new()));
            let backup_events = events.clone();
            let checkpoint_events = events.clone();
            let persist_events = events.clone();

            schedule_preparation_with(
                move || async move {
                    backup_events.lock().unwrap().push("backup");
                    Ok(())
                },
                move || async move {
                    checkpoint_events.lock().unwrap().push("checkpoint");
                    Ok(())
                },
                move || async move {
                    persist_events.lock().unwrap().push("pending");
                    Ok(())
                },
            )
            .await
            .unwrap();

            assert_eq!(
                events.lock().unwrap().as_slice(),
                ["backup", "checkpoint", "pending"]
            );
        });
    }

    #[test]
    fn successful_execution_state_records_retained_sources() {
        let state = maintenance_state_after_execution(&pending(), Ok(()));

        assert_eq!(state.last_migration_status.as_deref(), Some("succeeded"));
        assert_eq!(
            state.retained_previous_data_root,
            Some(PathBuf::from("/source/data"))
        );
        assert_eq!(
            state.retained_previous_webview_root,
            Some(PathBuf::from("/source/webview"))
        );
        assert!(state.last_maintenance_error.is_none());
    }

    #[test]
    fn failed_execution_state_keeps_the_concrete_error() {
        let state = maintenance_state_after_execution(&pending(), Err("copy failed".to_string()));

        assert_eq!(state.last_migration_status.as_deref(), Some("failed"));
        assert_eq!(state.last_maintenance_error.as_deref(), Some("copy failed"));
        assert!(state.retained_previous_data_root.is_none());
    }
}
