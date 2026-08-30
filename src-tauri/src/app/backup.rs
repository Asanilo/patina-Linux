use crate::app::desktop_behavior;
use crate::data::backup::{self, RestoreStrategy};
use crate::engine::tracking::runtime as tracking_runtime;
use tauri::{AppHandle, Emitter};

pub(crate) async fn restore_backup_and_refresh(
    app: AppHandle,
    backup_path: String,
    strategy: RestoreStrategy,
) -> Result<(), String> {
    let _scheduled_backup_guard = crate::app::scheduled_backup::lock_for_restore(&app).await;
    backup::restore_backup(backup_path, app.clone(), strategy).await?;
    if strategy == RestoreStrategy::Replace {
        crate::app::scheduled_backup::reset_after_replace_restore_while_locked(&app).await?;
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
