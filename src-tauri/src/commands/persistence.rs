use crate::data::{maintenance, sqlite_pool};
use crate::domain::data_maintenance::{TrackingDataCleanupResult, WindowTitleCleanupResult};
use crate::engine::tracking::runtime::emit_tracking_data_changed;
use tauri::{AppHandle, Runtime};

#[tauri::command]
pub async fn cmd_reopen_sqlite_pool<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    sqlite_pool::reopen_sqlite_pool(&app).await.map(|_| ())
}

#[tauri::command]
pub async fn cmd_delete_tracking_data_before<R: Runtime>(
    cutoff_time_ms: i64,
    app: AppHandle<R>,
) -> Result<TrackingDataCleanupResult, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .delete_tracking_data_before(cutoff_time_ms)
            .await
            .map_err(|error| error.to_string());
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    let result = maintenance::delete_tracking_data_before(&pool, cutoff_time_ms).await?;
    emit_tracking_data_changed(&app, "tracking-data-cleaned", crate::app::runtime::now_ms())
        .map_err(|error| format!("failed to emit data cleanup event: {error}"))?;
    Ok(result)
}

#[tauri::command]
pub async fn cmd_clear_all_window_titles<R: Runtime>(
    app: AppHandle<R>,
) -> Result<WindowTitleCleanupResult, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .clear_window_titles()
            .await
            .map_err(|error| error.to_string());
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    let result = maintenance::clear_all_window_titles(&pool).await?;
    emit_tracking_data_changed(&app, "window-titles-cleared", crate::app::runtime::now_ms())
        .map_err(|error| format!("failed to emit window title cleanup event: {error}"))?;
    Ok(result)
}
