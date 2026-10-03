use crate::data::{maintenance, sqlite_pool};
use crate::domain::data_maintenance::{TrackingDataCleanupResult, WindowTitleCleanupResult};
use crate::engine::tracking::runtime::emit_tracking_data_changed;
use tauri::{AppHandle, Manager, Runtime};

#[tauri::command]
pub async fn cmd_get_exact_history<R: Runtime>(
    from_ms: i64,
    to_ms: i64,
    language: String,
    app: AppHandle<R>,
) -> Result<patina_protocol::history::ExactHistorySnapshot, String> {
    if !patina_protocol::history::valid_range(from_ms, to_ms) {
        return Err("invalid exact history range".into());
    }
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .exact_history(from_ms, to_ms, &language)
            .await
            .map_err(|error| error.to_string());
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    crate::data::repositories::exact_history::load_exact_history(
        &pool,
        from_ms,
        to_ms,
        crate::engine::runtime_context::now_ms() as i64,
        &language,
    )
    .await
}

#[tauri::command]
pub async fn cmd_get_dashboard_product<R: Runtime>(
    date: String,
    language: String,
    app: AppHandle<R>,
) -> Result<patina_protocol::dashboard::DashboardProductSnapshot, String> {
    let boundaries = crate::domain::activity_calendar::dashboard_boundaries(&date)?;
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .dashboard(&date, &language)
            .await
            .map_err(|error| error.to_string());
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    crate::data::repositories::daily_activity::load_dashboard_product(
        &pool,
        &boundaries,
        crate::engine::runtime_context::now_ms() as i64,
        &language,
    )
    .await
}

#[tauri::command]
pub async fn cmd_get_migration_observed_apps<R: Runtime>(
    to_ms: i64,
    app: AppHandle<R>,
) -> Result<Vec<crate::domain::observed_apps::ObservedAppStat>, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .migration_observed_apps(to_ms)
            .await
            .map_err(|error| error.to_string());
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    crate::data::repositories::observed_apps::load_migration_observed_apps(
        &pool,
        to_ms,
        crate::engine::runtime_context::now_ms() as i64,
    )
    .await
}

#[tauri::command]
pub async fn cmd_get_observed_apps<R: Runtime>(
    from_ms: i64,
    to_ms: i64,
    app: AppHandle<R>,
) -> Result<Vec<crate::domain::observed_apps::ObservedAppStat>, String> {
    crate::domain::observed_apps::validate_range(from_ms, to_ms)?;
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .observed_apps(from_ms, to_ms)
            .await
            .map_err(|error| error.to_string());
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    crate::data::repositories::observed_apps::load_observed_apps(
        &pool,
        from_ms,
        to_ms,
        crate::engine::runtime_context::now_ms() as i64,
    )
    .await
}

#[tauri::command]
pub async fn cmd_get_daily_apps<R: Runtime>(
    from: String,
    to: String,
    language: String,
    app: AppHandle<R>,
) -> Result<patina_protocol::activity::DailyProductSnapshot, String> {
    let boundaries = crate::domain::daily_activity::local_day_boundaries(&from, &to)?;
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .daily_product(&from, &to, &language)
            .await
            .map_err(|error| match error {
                crate::platform::daemon_client::PatinadClientError::Http {
                    status: 404, ..
                } => "daily-apps-unsupported".to_string(),
                error => error.to_string(),
            });
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    crate::data::repositories::daily_activity::load_daily_product(
        &pool,
        &boundaries,
        crate::engine::runtime_context::now_ms() as i64,
        &language,
    )
    .await
}

#[tauri::command]
pub async fn cmd_get_daily_activity<R: Runtime>(
    from: String,
    to: String,
    app: AppHandle<R>,
) -> Result<crate::domain::daily_activity::DailyActivitySnapshot, String> {
    let boundaries = crate::domain::daily_activity::local_day_boundaries(&from, &to)?;
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .daily_activity(&from, &to)
            .await
            .map_err(|error| match error {
                crate::platform::daemon_client::PatinadClientError::Http {
                    status: 404, ..
                } => "heatmap-unsupported".to_string(),
                error => error.to_string(),
            });
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    crate::data::repositories::daily_activity::load_daily_activity(
        &pool,
        &boundaries,
        crate::engine::runtime_context::now_ms() as i64,
    )
    .await
}

#[tauri::command]
pub async fn cmd_reopen_sqlite_pool<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    if app
        .state::<crate::app::runtime::DesktopRuntimeMode>()
        .owns_embedded_runtime()
    {
        sqlite_pool::reopen_sqlite_pool(&app).await.map(|_| ())
    } else {
        sqlite_pool::reopen_existing_sqlite_pool(&app)
            .await
            .map(|_| ())
    }
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
    emit_tracking_data_changed(
        &app,
        "tracking-data-cleaned",
        crate::engine::runtime_context::now_ms(),
    )
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
    emit_tracking_data_changed(
        &app,
        "window-titles-cleared",
        crate::engine::runtime_context::now_ms(),
    )
    .map_err(|error| format!("failed to emit window title cleanup event: {error}"))?;
    Ok(result)
}

#[tauri::command]
pub async fn cmd_delete_canonical_app_history<R: Runtime>(
    request: patina_protocol::maintenance::CanonicalAppCleanupRequest,
    app: AppHandle<R>,
) -> Result<patina_protocol::maintenance::CanonicalAppCleanupResult, String> {
    crate::domain::data_maintenance::canonical_cleanup_key(&request)?;
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .delete_canonical_app_history(&request)
            .await
            .map_err(|e| e.to_string());
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    let result = maintenance::canonical::delete_canonical_app(
        &pool,
        &request,
        crate::engine::runtime_context::now_ms() as i64,
    )
    .await?;
    if result.matched_executables > 0 {
        emit_tracking_data_changed(
            &app,
            "application-tracking-data-deleted",
            crate::engine::runtime_context::now_ms(),
        )
        .map_err(|e| format!("application cleanup committed but event delivery failed: {e}"))?;
    }
    Ok(result)
}

#[tauri::command]
pub async fn cmd_delete_app_tracking_data<R: Runtime>(
    exe_names: Vec<String>,
    start_time_ms: Option<i64>,
    end_time_ms: Option<i64>,
    app: AppHandle<R>,
) -> Result<(), String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        client
            .delete_app_tracking_data(exe_names, start_time_ms, end_time_ms)
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let pool = sqlite_pool::wait_for_sqlite_pool(&app).await?;
    maintenance::delete_app_tracking_data(&pool, &exe_names, start_time_ms, end_time_ms).await?;
    emit_tracking_data_changed(
        &app,
        "application-tracking-data-deleted",
        crate::engine::runtime_context::now_ms(),
    )
    .map_err(|error| format!("failed to emit app data cleanup event: {error}"))
}
