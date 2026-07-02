mod executor;
mod plan;

use crate::data::{backup, sqlite_pool};
use crate::domain::storage::{
    StorageDirectoryKind, StorageMaintenanceSnapshot, StorageMigrationPreview, StoragePathSnapshot,
    StoragePendingMigrationSnapshot, StorageSizeSnapshot, StorageSnapshot, StorageTargetKind,
    WebviewCacheSnapshot,
};
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
    let target = target_root(app_paths::app_profile(app), kind, &selected_parent);
    preview_storage_migration_to_root(app, kind, target, false)
}

pub fn preview_restore_default_storage<R: Runtime>(
    app: &AppHandle<R>,
    kind: StorageTargetKind,
) -> Result<StorageMigrationPreview, String> {
    let defaults = storage_paths::default_storage_paths(app)?;
    let target = match kind {
        StorageTargetKind::Data => defaults.data_root,
        StorageTargetKind::Webview => defaults.webview_root,
    };
    preview_storage_migration_to_root(app, kind, target, true)
}

pub async fn schedule_storage_migration(
    app: AppHandle,
    kind: StorageTargetKind,
    selected_parent: PathBuf,
) -> Result<storage_anchor::PendingStorageMigration, String> {
    let target = target_root(app_paths::app_profile(&app), kind, &selected_parent);
    schedule_storage_migration_to_root(app, kind, target, false).await
}

pub async fn schedule_restore_default_storage(
    app: AppHandle,
    kind: StorageTargetKind,
) -> Result<storage_anchor::PendingStorageMigration, String> {
    let defaults = storage_paths::default_storage_paths(&app)?;
    let target = match kind {
        StorageTargetKind::Data => defaults.data_root,
        StorageTargetKind::Webview => defaults.webview_root,
    };
    schedule_storage_migration_to_root(app, kind, target, true).await
}

async fn schedule_storage_migration_to_root(
    app: AppHandle,
    kind: StorageTargetKind,
    target: PathBuf,
    restore_default: bool,
) -> Result<storage_anchor::PendingStorageMigration, String> {
    let preview = preview_storage_migration_to_root(&app, kind, target, restore_default)?;
    let current = storage_paths::resolve_storage_paths(&app)?;
    let existing = storage_anchor::read_pending_migration(&app)?;
    let id = match existing.as_ref() {
        Some(pending) => pending.id.clone(),
        None => new_migration_id()?,
    };
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

fn preview_storage_migration_to_root<R: Runtime>(
    app: &AppHandle<R>,
    kind: StorageTargetKind,
    target: PathBuf,
    restore_default: bool,
) -> Result<StorageMigrationPreview, String> {
    let current = storage_paths::resolve_storage_paths(app)?;
    let payload = || payload_size(&current, kind);
    if restore_default {
        plan::preview_restore_with_deps(
            &current,
            target_kind(kind),
            target,
            payload,
            storage_usage::available_space_for,
        )
    } else {
        plan::preview_with_deps(
            &current,
            target_kind(kind),
            target,
            payload,
            storage_usage::available_space_for,
        )
    }
}

pub fn storage_snapshot<R: Runtime>(app: &AppHandle<R>) -> Result<StorageSnapshot, String> {
    let current = storage_paths::resolve_storage_paths(app)?;
    let defaults = storage_paths::default_storage_paths(app)?;
    let maintenance = storage_anchor::read_maintenance_state(app)?;
    let pending = storage_anchor::read_pending_migration(app)?;
    let data_bytes = payload_size(&current, StorageTargetKind::Data)?;
    let persistent_webview_bytes = webview_cache::persistent_profile_size(&current.webview_root)?;
    let cache_size_bytes = webview_cache::webkit_cache_size(&current.webview_root)?;

    Ok(StorageSnapshot {
        paths: StoragePathSnapshot {
            data_root: current.data_root,
            default_data_root: defaults.data_root,
            database_path: current.db_path,
            backup_dir: current.backup_dir,
            webview_root: current.webview_root.clone(),
            default_webview_root: defaults.webview_root,
            is_custom_data_root: current.is_custom_data_root,
            is_custom_webview_root: current.is_custom_webview_root,
        },
        sizes: StorageSizeSnapshot {
            data_bytes,
            webview_profile_bytes: persistent_webview_bytes.saturating_add(cache_size_bytes),
        },
        webview_cache: WebviewCacheSnapshot {
            path: webview_cache::webkit_cache_path(&current.webview_root),
            size_bytes: cache_size_bytes,
            clear_on_restart: maintenance.pending_webview_cache_clear,
        },
        maintenance: StorageMaintenanceSnapshot {
            last_webview_cache_clear_at_ms: maintenance.last_webview_cache_clear_at_ms,
            last_error: maintenance.last_maintenance_error,
            last_migration_status: maintenance.last_migration_status,
            retained_previous_data_root: maintenance.retained_previous_data_root,
            retained_previous_webview_root: maintenance.retained_previous_webview_root,
        },
        pending_migration: pending.map(pending_snapshot),
    })
}

pub fn storage_directory<R: Runtime>(
    app: &AppHandle<R>,
    kind: StorageDirectoryKind,
) -> Result<PathBuf, String> {
    let snapshot = storage_snapshot(app)?;
    match kind {
        StorageDirectoryKind::Data => Ok(snapshot.paths.data_root),
        StorageDirectoryKind::Backups => Ok(snapshot.paths.backup_dir),
        StorageDirectoryKind::Webview => Ok(snapshot.paths.webview_root),
        StorageDirectoryKind::RetainedData => snapshot
            .maintenance
            .retained_previous_data_root
            .ok_or_else(|| "no retained previous data directory is recorded".to_string()),
        StorageDirectoryKind::RetainedWebview => snapshot
            .maintenance
            .retained_previous_webview_root
            .ok_or_else(|| "no retained previous WebView directory is recorded".to_string()),
    }
}

pub fn pending_snapshot(
    pending: storage_anchor::PendingStorageMigration,
) -> StoragePendingMigrationSnapshot {
    StoragePendingMigrationSnapshot {
        id: pending.id,
        source_data_root: pending.source_data_root,
        target_data_root: pending.target_data_root,
        source_webview_root: pending.source_webview_root,
        target_webview_root: pending.target_webview_root,
        created_at_ms: pending.created_at_ms,
    }
}

pub fn cancel_pending_storage_migration<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    storage_anchor::remove_pending_migration(app)
}

pub fn schedule_webview_cache_clear<R: Runtime>(
    app: &AppHandle<R>,
    pending: bool,
) -> Result<storage_anchor::StorageMaintenanceState, String> {
    let mut state = storage_anchor::read_maintenance_state(app)?;
    state.pending_webview_cache_clear = pending;
    storage_anchor::write_maintenance_state(app, &state)?;
    Ok(state)
}

pub async fn run_startup_storage_maintenance<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    run_pending_storage_migration(app).await?;

    let mut state = storage_anchor::read_maintenance_state(app)?;
    if !state.pending_webview_cache_clear {
        return Ok(());
    }

    let paths = storage_paths::resolve_storage_paths(app)?;
    state.pending_webview_cache_clear = false;
    match webview_cache::clear_linux_webkit_cache(&paths.webview_root) {
        Ok(()) => {
            state.last_webview_cache_clear_at_ms = Some(storage_anchor::now_ms());
            state.last_maintenance_error = None;
        }
        Err(error) => {
            state.last_maintenance_error = Some(format!("WebKit cache clear failed: {error}"));
            eprintln!("[storage] failed to clear WebKit cache: {error}");
        }
    }
    storage_anchor::write_maintenance_state(app, &state)
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

    let previous_maintenance = storage_anchor::read_maintenance_state(app)?;
    let mut maintenance = maintenance_state_after_execution(&pending, execution.clone());
    maintenance.pending_webview_cache_clear = previous_maintenance.pending_webview_cache_clear;
    maintenance.last_webview_cache_clear_at_ms =
        previous_maintenance.last_webview_cache_clear_at_ms;
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

fn new_migration_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to generate storage migration id: {error}"))?;
    let encoded = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("migration-{encoded}"))
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
