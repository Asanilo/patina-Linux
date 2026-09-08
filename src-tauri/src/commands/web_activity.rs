use crate::app::web_activity;
use crate::domain::web_activity::WebActivityBridgeSnapshot;
use crate::engine::web_activity::WebActivityRuntimeState;
use tauri::{AppHandle, Runtime, State};

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
