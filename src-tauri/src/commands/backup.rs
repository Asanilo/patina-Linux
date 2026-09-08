use crate::app;
use crate::data::backup;
use crate::data::remote_backup::{
    self, RemoteBackupDownloadResult, RemoteBackupEntry, RemoteBackupUploadResult,
    WebDavBackupConfig, WebDavTestResult,
};
use crate::domain::backup::BackupPreview;
use crate::domain::backup::RestoreStrategy;
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
    let strategy = restore_strategy.unwrap_or_default();
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        let staged = app::backup::stage_backup_for_daemon(&app, &backup_path, strategy).await?;
        let scheduled = match client.schedule_backup_restore(&staged.request).await {
            Ok(scheduled) => scheduled,
            Err(error) => {
                // Keep the owner-only file when delivery is ambiguous; the daemon may have
                // persisted the reservation immediately before the connection closed.
                if matches!(
                    error,
                    crate::platform::daemon_client::PatinadClientError::InvalidConfiguration(_)
                        | crate::platform::daemon_client::PatinadClientError::Unauthorized
                        | crate::platform::daemon_client::PatinadClientError::Http {
                            status: 400..=499,
                            ..
                        }
                ) {
                    let _ = staged.discard();
                }
                return Err(error.to_string());
            }
        };
        client
            .wait_for_backup_restore(&scheduled.restore.request_id)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    app::backup::restore_backup_and_refresh(app, backup_path, strategy).await
}

#[tauri::command]
pub async fn cmd_preview_backup(backup_path: String) -> Result<BackupPreview, String> {
    backup::preview_backup(backup_path).await
}

#[tauri::command]
pub async fn cmd_save_webdav_backup_secret(
    username: String,
    password: String,
    app: AppHandle,
) -> Result<(), String> {
    remote_backup::save_webdav_backup_secret(
        crate::platform::app_paths::app_profile(&app),
        username,
        password,
    )
    .await
}

#[tauri::command]
pub async fn cmd_delete_webdav_backup_secret(app: AppHandle) -> Result<(), String> {
    remote_backup::delete_webdav_backup_secret(crate::platform::app_paths::app_profile(&app)).await
}

#[tauri::command]
pub async fn cmd_has_webdav_backup_secret(app: AppHandle) -> Result<bool, String> {
    remote_backup::has_webdav_backup_secret(crate::platform::app_paths::app_profile(&app)).await
}

#[tauri::command]
pub async fn cmd_reveal_webdav_backup_secret(app: AppHandle) -> Result<Option<String>, String> {
    remote_backup::reveal_webdav_backup_secret(crate::platform::app_paths::app_profile(&app)).await
}

#[tauri::command]
pub async fn cmd_test_webdav_backup_target(
    config: WebDavBackupConfig,
    password: Option<String>,
    app: AppHandle,
) -> Result<WebDavTestResult, String> {
    remote_backup::test_webdav_backup_target(
        crate::platform::app_paths::app_profile(&app),
        config,
        password,
    )
    .await
}

#[tauri::command]
pub async fn cmd_upload_webdav_backup(
    config: WebDavBackupConfig,
    app: AppHandle,
) -> Result<RemoteBackupUploadResult, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .upload_remote_backup(config)
            .await
            .map_err(|error| error.to_string());
    }
    remote_backup::upload_webdav_backup(app, config).await
}

#[tauri::command]
pub async fn cmd_list_webdav_backups(
    config: WebDavBackupConfig,
    app: AppHandle,
) -> Result<Vec<RemoteBackupEntry>, String> {
    remote_backup::list_webdav_backups(crate::platform::app_paths::app_profile(&app), config).await
}

#[tauri::command]
pub async fn cmd_download_webdav_backup(
    config: WebDavBackupConfig,
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
