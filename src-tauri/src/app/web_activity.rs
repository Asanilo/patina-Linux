use crate::data::repositories::app_settings;
use crate::data::sqlite_pool::wait_for_sqlite_pool;
use crate::domain::tracking::TrackingDataChangedPayload;
use crate::domain::web_activity::WEB_ACTIVITY_CHANGED_REASON;
use crate::engine::tracking::runtime::TauriRuntimeEventSink;
use crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState;
use crate::engine::web_activity::{
    repair_active_segment_after_restart, seal_if_tracking_inactive, seal_stale_active_segment,
    WebActivityBridgeHttpRequest, WebActivityBridgeHttpResponse, WebActivityRuntimeState,
    WEB_ACTIVITY_STALE_CHECK_INTERVAL,
};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

pub async fn handle_http_request<R: Runtime>(
    app: AppHandle<R>,
    request: WebActivityBridgeHttpRequest,
) -> WebActivityBridgeHttpResponse {
    let pool = match wait_for_sqlite_pool(&app).await {
        Ok(pool) => pool,
        Err(error) => {
            return WebActivityBridgeHttpResponse::json(
                500,
                serde_json::json!({
                    "ok": false,
                    "code": "storage-unavailable",
                    "message": error,
                }),
            );
        }
    };
    let Some(state) = app.try_state::<WebActivityRuntimeState>() else {
        return WebActivityBridgeHttpResponse::json(
            500,
            serde_json::json!({
                "ok": false,
                "code": "runtime-unavailable",
                "message": "web activity runtime is unavailable",
            }),
        );
    };
    let tracking_snapshot = app
        .try_state::<TrackingRuntimeSnapshotState>()
        .and_then(|state| state.snapshot());
    let context = crate::engine::runtime_context::RuntimeContext::system(pool);
    let event_sink = TauriRuntimeEventSink::new(app.clone());
    crate::engine::web_activity::handle_http_request(
        &context,
        &state,
        tracking_snapshot,
        &event_sink,
        request,
    )
    .await
}

pub fn spawn_foreground_sync<R: Runtime + 'static>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        if let Err(error) = sync_foreground_state(app).await {
            eprintln!("[web-activity] failed to sync foreground state: {error}");
        }
    });
}

pub fn spawn_startup_repair<R: Runtime + 'static>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let now_ms = crate::app::runtime::now_ms() as i64;
        let pool = match wait_for_sqlite_pool(&app).await {
            Ok(pool) => pool,
            Err(error) => {
                eprintln!("[web-activity] failed to load sqlite pool for startup repair: {error}");
                return;
            }
        };
        match repair_active_segment_after_restart(&pool, now_ms).await {
            Ok(Some(repaired_at_ms)) => emit_web_activity_changed(&app, repaired_at_ms),
            Ok(None) => {}
            Err(error) => eprintln!("[web-activity] failed to repair active segment: {error}"),
        }
    });
}

pub fn spawn_stale_watchdog<R: Runtime + 'static>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let start_at = tokio::time::Instant::now() + WEB_ACTIVITY_STALE_CHECK_INTERVAL;
        let mut interval = tokio::time::interval_at(start_at, WEB_ACTIVITY_STALE_CHECK_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if let Err(error) = sync_stale_state(&app).await {
                eprintln!("[web-activity] stale watchdog failed: {error}");
            }
        }
    });
}

async fn sync_stale_state<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let pool = wait_for_sqlite_pool(app).await?;
    let Some(state) = app.try_state::<WebActivityRuntimeState>() else {
        return Err("web activity runtime is unavailable".to_string());
    };
    let now_ms = crate::app::runtime::now_ms() as i64;
    if let Some(sealed_at_ms) = seal_stale_active_segment(&pool, &state, now_ms).await? {
        emit_web_activity_changed(app, sealed_at_ms);
    }
    Ok(())
}

pub async fn sync_foreground_state<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let pool = wait_for_sqlite_pool(&app).await?;
    let now_ms = crate::app::runtime::now_ms() as i64;
    let tracking_snapshot = app
        .try_state::<TrackingRuntimeSnapshotState>()
        .and_then(|state| state.snapshot());
    if seal_if_tracking_inactive(&pool, tracking_snapshot, now_ms).await? {
        emit_web_activity_changed(&app, now_ms);
    }
    Ok(())
}

pub async fn get_bridge_snapshot<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, WebActivityRuntimeState>,
) -> Result<crate::domain::web_activity::WebActivityBridgeSnapshot, String> {
    let pool = wait_for_sqlite_pool(&app).await?;
    let settings = app_settings::load_web_activity_settings(&pool)
        .await
        .map_err(|error| format!("failed to load web activity settings: {error}"))?;
    Ok(state.snapshot(&settings, crate::app::runtime::now_ms() as i64))
}

fn emit_web_activity_changed<R: Runtime>(app: &AppHandle<R>, changed_at_ms: i64) {
    let _ = app.emit(
        "tracking-data-changed",
        TrackingDataChangedPayload::new(WEB_ACTIVITY_CHANGED_REASON, changed_at_ms as u64),
    );
}
