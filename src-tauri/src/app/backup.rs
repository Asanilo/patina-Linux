use crate::app::desktop_behavior;
use crate::data::backup;
use crate::domain::backup::RestoreStrategy;
use crate::engine::tracking::runtime as tracking_runtime;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

pub(crate) struct PreparedDaemonBackupRestore {
    pub request: crate::engine::api::types::StagedBackupRestoreRequest,
    staging_root: PathBuf,
}

impl PreparedDaemonBackupRestore {
    pub fn discard(&self) -> Result<(), String> {
        crate::platform::backup_restore_staging::discard(&self.staging_root, &self.request.ticket)
    }
}

pub(crate) async fn stage_backup_for_daemon(
    app: &AppHandle,
    backup_path: &str,
    strategy: RestoreStrategy,
) -> Result<PreparedDaemonBackupRestore, String> {
    let source = Path::new(backup_path.trim()).to_path_buf();
    if source.as_os_str().is_empty() {
        return Err("backup path cannot be empty".to_string());
    }
    let source_for_inspection = source.clone();
    let (_, expected_sha256, expected_size_bytes) = tokio::task::spawn_blocking(move || {
        backup::inspect_restore_archive(&source_for_inspection)
    })
    .await
    .map_err(|error| format!("backup inspection task failed: {error}"))??;
    let storage_paths = crate::platform::storage_paths::resolve_storage_paths(app)?;
    let staging_root = storage_paths.backup_restore_staging_dir;
    let staging_root_for_copy = staging_root.clone();
    let staged = tokio::task::spawn_blocking(move || {
        crate::platform::backup_restore_staging::stage_file(&staging_root_for_copy, &source)
    })
    .await
    .map_err(|error| format!("backup staging task failed: {error}"))??;
    if staged.sha256 != expected_sha256 || staged.size_bytes != expected_size_bytes {
        let _ = crate::platform::backup_restore_staging::discard(&staging_root, &staged.ticket);
        return Err("backup archive changed after preview; preview it again".to_string());
    }

    Ok(PreparedDaemonBackupRestore {
        request: crate::engine::api::types::StagedBackupRestoreRequest {
            ticket: staged.ticket,
            expected_sha256,
            expected_size_bytes,
            strategy,
            confirmed: true,
        },
        staging_root,
    })
}

pub(crate) async fn restore_backup_and_refresh(
    app: AppHandle,
    backup_path: String,
    strategy: RestoreStrategy,
) -> Result<(), String> {
    let _scheduled_backup_guard = crate::app::scheduled_backup::lock_for_restore(&app).await;
    backup::restore_backup(backup_path, app.clone(), strategy).await?;
    if strategy == RestoreStrategy::Replace {
        crate::app::scheduled_backup::notify_after_replace_restore(&app);
    }
    desktop_behavior::sync_desktop_behavior_from_storage(app.clone(), false).await?;
    app.emit("app-settings-changed", serde_json::json!({}))
        .map_err(|error| format!("failed to emit settings refresh event: {error}"))?;
    tracking_runtime::emit_tracking_data_changed(&app, "backup-restored", now_ms())
        .map_err(|error| format!("failed to emit restore refresh event: {error}"))?;
    Ok(())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
