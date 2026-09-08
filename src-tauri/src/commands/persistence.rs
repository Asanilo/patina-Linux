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

#[tauri::command]
pub async fn cmd_delete_app_tracking_data<R: Runtime>(
    exe_names: Vec<String>,
    start_time_ms: Option<i64>,
    end_time_ms: Option<i64>,
    app: AppHandle<R>,
) -> Result<(), String> {
    if crate::app::daemon_client::command_client(&app)?.is_some() {
        return Err(
            "application activity deletion is not available in daemon client preview yet"
                .to_string(),
        );
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    maintenance::delete_app_tracking_data(&pool, &exe_names, start_time_ms, end_time_ms).await?;
    emit_tracking_data_changed(
        &app,
        "application-tracking-data-deleted",
        crate::app::runtime::now_ms(),
    )
    .map_err(|error| format!("failed to emit app data cleanup event: {error}"))
}
