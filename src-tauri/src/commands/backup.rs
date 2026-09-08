use crate::app;
use crate::data::backup::{self, RestoreStrategy};
use crate::data::remote_backup::{
    self, RemoteBackupDownloadResult, RemoteBackupEntry, RemoteBackupUploadResult,
    WebDavBackupConfigDto, WebDavTestResult,
};
use crate::domain::backup::BackupPreview;
use crate::domain::backup_schedule::{ScheduledBackupConfigInput, ScheduledBackupSnapshot};
use tauri::AppHandle;

#[tauri::command]
pub fn cmd_pick_backup_save_file(initial_path: Option<String>) -> Option<String> {
    backup::pick_backup_save_file(initial_path)
}

#[tauri::command]
pub fn cmd_pick_backup_file(initial_path: Option<String>) -> Option<String> {
    backup::pick_backup_file(initial_path)
}

#[tauri::command]
pub async fn cmd_export_backup(
    backup_path: Option<String>,
    app: AppHandle,
) -> Result<String, String> {
    backup::export_backup(backup_path, app).await
}

#[tauri::command]
pub async fn cmd_restore_backup(
    backup_path: String,
    restore_strategy: Option<RestoreStrategy>,
    app: AppHandle,
) -> Result<(), String> {
    if crate::app::daemon_client::command_client(&app)?.is_some() {
        return Err("backup restore requires the pending daemon maintenance flow".to_string());
    }
    app::backup::restore_backup_and_refresh(app, backup_path, restore_strategy.unwrap_or_default())
        .await
}

#[tauri::command]
pub async fn cmd_preview_backup(backup_path: String) -> Result<BackupPreview, String> {
    backup::preview_backup(backup_path).await
}

#[tauri::command]
pub fn cmd_save_webdav_backup_secret(username: String, password: String) -> Result<(), String> {
    remote_backup::save_webdav_backup_secret(username, password)
}

#[tauri::command]
pub fn cmd_delete_webdav_backup_secret() -> Result<(), String> {
    remote_backup::delete_webdav_backup_secret()
}

#[tauri::command]
pub fn cmd_has_webdav_backup_secret() -> Result<bool, String> {
    remote_backup::has_webdav_backup_secret()
}

#[tauri::command]
pub fn cmd_reveal_webdav_backup_secret() -> Result<Option<String>, String> {
    remote_backup::reveal_webdav_backup_secret()
}

#[tauri::command]
pub async fn cmd_test_webdav_backup_target(
    config: WebDavBackupConfigDto,
    password: Option<String>,
) -> Result<WebDavTestResult, String> {
    remote_backup::test_webdav_backup_target(config, password).await
}

#[tauri::command]
pub async fn cmd_upload_webdav_backup(
    config: WebDavBackupConfigDto,
    app: AppHandle,
) -> Result<RemoteBackupUploadResult, String> {
    remote_backup::upload_webdav_backup(app, config).await
}

#[tauri::command]
pub async fn cmd_list_webdav_backups(
    config: WebDavBackupConfigDto,
) -> Result<Vec<RemoteBackupEntry>, String> {
    remote_backup::list_webdav_backups(config).await
}

#[tauri::command]
pub async fn cmd_download_webdav_backup(
    config: WebDavBackupConfigDto,
    id: String,
    app: AppHandle,
) -> Result<RemoteBackupDownloadResult, String> {
    remote_backup::download_webdav_backup(app, config, id).await
}

#[tauri::command]
pub async fn cmd_get_scheduled_backup_snapshot(
    app: AppHandle,
) -> Result<ScheduledBackupSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .scheduled_backup_snapshot()
            .await
            .map_err(|error| error.to_string());
    }
    app::scheduled_backup::get_snapshot(&app).await
}

#[tauri::command]
pub fn cmd_pick_scheduled_backup_directory(initial_path: Option<String>) -> Option<String> {
    app::scheduled_backup::pick_directory(initial_path)
}

#[tauri::command]
pub async fn cmd_save_scheduled_backup_config(
    input: ScheduledBackupConfigInput,
    app: AppHandle,
) -> Result<ScheduledBackupSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .save_scheduled_backup_config(input)
            .await
            .map_err(|error| error.to_string());
    }
    app::scheduled_backup::save_config(&app, input).await
}
