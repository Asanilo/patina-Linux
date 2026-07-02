mod plan;

use crate::data::{backup, sqlite_pool};
use crate::domain::storage::{StorageMigrationPreview, StorageTargetKind};
use crate::platform::{app_paths, storage_anchor, storage_paths, storage_usage, webview_cache};
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
    use std::sync::{Arc, Mutex};

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
}
