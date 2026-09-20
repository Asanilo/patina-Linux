use crate::app::web_activity;
use crate::domain::web_activity::WebActivityBridgeSnapshot;
use crate::engine::web_activity::WebActivityRuntimeState;
use tauri::{AppHandle, Runtime, State};

#[tauri::command]
pub async fn cmd_delete_web_activity_segments_by_domain<R: Runtime>(
    domain: String,
    app: AppHandle<R>,
) -> Result<(), String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .delete_web_domain_history(domain)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string());
    }
    let pool = crate::data::sqlite_pool::wait_for_sqlite_pool(&app).await?;
    crate::data::maintenance::delete_web_activity_segments_by_domain(&pool, &domain).await?;
    crate::engine::tracking::runtime::emit_tracking_data_changed(
        &app,
        crate::domain::web_activity::WEB_ACTIVITY_CHANGED_REASON,
        crate::app::runtime::now_ms(),
    )
    .map_err(|error| format!("failed to emit web history cleanup event: {error}"))
}

#[tauri::command]
pub async fn cmd_get_web_activity_bridge_snapshot<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, WebActivityRuntimeState>,
) -> Result<WebActivityBridgeSnapshot, String> {
    if let Some(client) = crate::app::daemon_client::command_client(&app)? {
        return client
            .diagnostics()
            .await
            .map_err(|error| error.to_string())?
            .web_activity_bridge
            .ok_or_else(|| "patinad browser activity bridge is not ready".to_string());
    }
    web_activity::get_bridge_snapshot(app, state).await
}
