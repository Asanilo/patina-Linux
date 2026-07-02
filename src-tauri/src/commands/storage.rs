use crate::data::storage_migration;
use crate::domain::storage::{
    StorageDirectoryKind, StorageMigrationPreview, StoragePendingMigrationSnapshot,
    StorageSnapshot, StorageTargetKind,
};
use std::path::PathBuf;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub fn cmd_get_storage_snapshot(app: AppHandle) -> Result<StorageSnapshot, String> {
    storage_migration::storage_snapshot(&app)
}

#[tauri::command]
pub fn cmd_pick_storage_parent(initial_path: Option<String>) -> Option<String> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(path) = initial_path.filter(|path| !path.trim().is_empty()) {
        dialog = dialog.set_directory(path);
    }
    dialog
        .pick_folder()
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn cmd_preview_storage_migration(
    app: AppHandle,
    kind: StorageTargetKind,
    selected_parent: String,
) -> Result<StorageMigrationPreview, String> {
    storage_migration::preview_storage_migration(&app, kind, PathBuf::from(selected_parent))
}

#[tauri::command]
pub fn cmd_preview_restore_default_storage(
    app: AppHandle,
    kind: StorageTargetKind,
) -> Result<StorageMigrationPreview, String> {
    storage_migration::preview_restore_default_storage(&app, kind)
}

#[tauri::command]
pub async fn cmd_schedule_storage_migration(
    app: AppHandle,
    kind: StorageTargetKind,
    selected_parent: String,
) -> Result<StoragePendingMigrationSnapshot, String> {
    storage_migration::schedule_storage_migration(app, kind, PathBuf::from(selected_parent))
        .await
        .map(storage_migration::pending_snapshot)
}

#[tauri::command]
pub async fn cmd_schedule_restore_default_storage(
    app: AppHandle,
    kind: StorageTargetKind,
) -> Result<StoragePendingMigrationSnapshot, String> {
    storage_migration::schedule_restore_default_storage(app, kind)
        .await
        .map(storage_migration::pending_snapshot)
}

#[tauri::command]
pub fn cmd_cancel_pending_storage_migration(app: AppHandle) -> Result<StorageSnapshot, String> {
    storage_migration::cancel_pending_storage_migration(&app)?;
    storage_migration::storage_snapshot(&app)
}

#[tauri::command]
pub fn cmd_schedule_webview_cache_clear(
    app: AppHandle,
    pending: bool,
) -> Result<StorageSnapshot, String> {
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
pub fn cmd_restart_for_storage_maintenance(app: AppHandle) {
    app.restart();
}
