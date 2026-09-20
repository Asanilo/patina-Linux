use crate::app::runtime::DesktopRuntimeMode;
use crate::data::storage_migration;
use crate::domain::storage::{
    StorageDirectoryKind, StorageMigrationPreview, StoragePendingMigrationSnapshot,
    StorageSnapshot, StorageTargetKind,
};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

fn ensure_storage_maintenance_supported(mode: DesktopRuntimeMode) -> Result<(), String> {
    if mode.owns_embedded_runtime() || mode.is_managed_daemon_client() {
        Ok(())
    } else {
        Err("storage-maintenance-unsupported-in-daemon-preview".to_string())
    }
}

fn ensure_storage_plan_editable(control_root: &Path) -> Result<(), String> {
    if crate::platform::storage_anchor::storage_migration_journal_exists(control_root)? {
        return Err(
            "storage migration recovery is pending; restart Patina before changing the request"
                .to_string(),
        );
    }
    Ok(())
}

#[tauri::command]
pub fn cmd_get_storage_snapshot(app: AppHandle) -> Result<StorageSnapshot, String> {
    storage_migration::storage_snapshot(&app)
}

#[tauri::command]
pub async fn cmd_pick_storage_parent(initial_path: Option<String>) -> Option<String> {
    let mut dialog = rfd::AsyncFileDialog::new();
    if let Some(path) = initial_path.filter(|path| !path.trim().is_empty()) {
        dialog = dialog.set_directory(path);
    }
    dialog
        .pick_folder()
        .await
        .map(|file| file.path().to_string_lossy().into_owned())
}

#[tauri::command]
pub fn cmd_preview_storage_migration(
    app: AppHandle,
    kind: StorageTargetKind,
    selected_parent: String,
) -> Result<StorageMigrationPreview, String> {
    ensure_storage_maintenance_supported(*app.state::<DesktopRuntimeMode>())?;
    storage_migration::preview_storage_migration(&app, kind, PathBuf::from(selected_parent))
}

#[tauri::command]
pub fn cmd_preview_restore_default_storage(
    app: AppHandle,
    kind: StorageTargetKind,
) -> Result<StorageMigrationPreview, String> {
    ensure_storage_maintenance_supported(*app.state::<DesktopRuntimeMode>())?;
    storage_migration::preview_restore_default_storage(&app, kind)
}

#[tauri::command]
pub async fn cmd_schedule_storage_migration(
    app: AppHandle,
    kind: StorageTargetKind,
    selected_parent: String,
) -> Result<StoragePendingMigrationSnapshot, String> {
    let mutation = app.state::<crate::app::daemon_service::DaemonServiceMutationState>();
    let _guard = mutation.lock().await;
    let mode = *app.state::<DesktopRuntimeMode>();
    ensure_storage_maintenance_supported(mode)?;
    ensure_storage_plan_editable(
        &crate::platform::storage_paths::default_storage_paths(&app)?.control_root,
    )?;
    storage_migration::schedule_storage_migration(
        app.clone(),
        kind,
        PathBuf::from(selected_parent),
        mode.owns_embedded_runtime(),
    )
    .await
    .map(storage_migration::pending_snapshot)
}

#[tauri::command]
pub async fn cmd_schedule_restore_default_storage(
    app: AppHandle,
    kind: StorageTargetKind,
) -> Result<StoragePendingMigrationSnapshot, String> {
    let mutation = app.state::<crate::app::daemon_service::DaemonServiceMutationState>();
    let _guard = mutation.lock().await;
    let mode = *app.state::<DesktopRuntimeMode>();
    ensure_storage_maintenance_supported(mode)?;
    ensure_storage_plan_editable(
        &crate::platform::storage_paths::default_storage_paths(&app)?.control_root,
    )?;
    storage_migration::schedule_restore_default_storage(
        app.clone(),
        kind,
        mode.owns_embedded_runtime(),
    )
    .await
    .map(storage_migration::pending_snapshot)
}

#[tauri::command]
pub async fn cmd_cancel_pending_storage_migration(
    app: AppHandle,
) -> Result<StorageSnapshot, String> {
    let mutation = app.state::<crate::app::daemon_service::DaemonServiceMutationState>();
    let _guard = mutation.lock().await;
    ensure_storage_plan_editable(
        &crate::platform::storage_paths::default_storage_paths(&app)?.control_root,
    )?;
    storage_migration::cancel_pending_storage_migration(&app)?;
    storage_migration::storage_snapshot(&app)
}

#[tauri::command]
pub async fn cmd_schedule_webview_cache_clear(
    app: AppHandle,
    pending: bool,
) -> Result<StorageSnapshot, String> {
    let mutation = app.state::<crate::app::daemon_service::DaemonServiceMutationState>();
    let _guard = mutation.lock().await;
    if pending {
        ensure_storage_maintenance_supported(*app.state::<DesktopRuntimeMode>())?;
    }
    storage_migration::schedule_webview_cache_clear(&app, pending)?;
    storage_migration::storage_snapshot(&app)
}

#[tauri::command]
pub fn cmd_open_storage_directory(
    app: AppHandle,
    kind: StorageDirectoryKind,
) -> Result<(), String> {
    let path = storage_migration::storage_directory(&app, kind)?;
    app.opener()
        .open_path(path.to_string_lossy().into_owned(), None::<String>)
        .map_err(|error| format!("failed to open storage directory: {error}"))
}

#[tauri::command]
pub async fn cmd_restart_for_storage_maintenance(app: AppHandle) -> Result<(), String> {
    let mutation = app.state::<crate::app::daemon_service::DaemonServiceMutationState>();
    let _guard = mutation.lock().await;
    ensure_storage_maintenance_supported(*app.state::<DesktopRuntimeMode>())?;
    app.state::<crate::app::state::AppExitState>()
        .request_exit();
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_maintenance_supports_embedded_and_managed_desktop_hosts() {
        assert!(ensure_storage_maintenance_supported(DesktopRuntimeMode::Embedded).is_ok());
        assert!(
            ensure_storage_maintenance_supported(DesktopRuntimeMode::DaemonClientManaged).is_ok()
        );
    }

    #[test]
    fn storage_maintenance_rejects_unmanaged_daemon_preview() {
        assert_eq!(
            ensure_storage_maintenance_supported(DesktopRuntimeMode::DaemonClientPreview)
                .unwrap_err(),
            "storage-maintenance-unsupported-in-daemon-preview"
        );
    }

    #[test]
    fn storage_plan_cannot_be_changed_while_recovery_journal_exists() {
        let root = std::env::temp_dir().join(format!(
            "patina-storage-plan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&root).unwrap();
        assert!(ensure_storage_plan_editable(&root).is_ok());
        let journal = crate::platform::storage_anchor::storage_migration_journal_path(&root);
        std::fs::write(&journal, "interrupted recovery").unwrap();
        assert!(ensure_storage_plan_editable(&root)
            .unwrap_err()
            .contains("recovery is pending"));
        assert_eq!(
            std::fs::read_to_string(journal).unwrap(),
            "interrupted recovery"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
