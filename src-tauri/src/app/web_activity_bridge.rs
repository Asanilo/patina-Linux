use crate::data::repositories::app_settings;
use crate::data::sqlite_pool::wait_for_sqlite_pool;
use crate::domain::settings::WebActivityBridgeSettings;
use crate::platform::web_activity_bridge::{
    WebActivityBridgeHttpHandler, WebActivityBridgeReadinessHandler, WebActivityBridgeRuntimeState,
    WEB_ACTIVITY_BRIDGE_ACTIVE_WINDOW_EVENT, WEB_ACTIVITY_BRIDGE_SETTINGS_CHANGED_EVENT,
    WEB_ACTIVITY_BRIDGE_TRACKING_DATA_EVENT,
};
use std::sync::Arc;
use tauri::{AppHandle, Listener, Manager, Runtime};

pub fn start<R: Runtime + 'static>(app: AppHandle<R>) {
    if app.try_state::<WebActivityBridgeRuntimeState>().is_none() {
        eprintln!("[web-activity-bridge] runtime state is not available");
        return;
    }

    spawn_settings_bootstrap(app.clone());
    register_event_handlers(app);
}

fn spawn_settings_bootstrap<R: Runtime + 'static>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        match load_web_activity_bridge_settings(&app).await {
            Ok(settings) => update_runtime_state(app, settings).await,
            Err(error) => eprintln!("[web-activity-bridge] failed to load settings: {error}"),
        }
    });
}

fn register_event_handlers<R: Runtime + 'static>(app: AppHandle<R>) {
    let settings_app = app.clone();
    app.listen_any(WEB_ACTIVITY_BRIDGE_SETTINGS_CHANGED_EVENT, move |_| {
        let settings_app = settings_app.clone();
        tauri::async_runtime::spawn(async move {
            match load_web_activity_bridge_settings(&settings_app).await {
                Ok(settings) => update_runtime_state(settings_app, settings).await,
                Err(error) => {
                    eprintln!("[web-activity-bridge] failed to reload settings: {error}")
                }
            }
        });
    });

    let active_window_app = app.clone();
    app.listen_any(WEB_ACTIVITY_BRIDGE_ACTIVE_WINDOW_EVENT, move |_| {
        crate::app::web_activity::spawn_foreground_sync(active_window_app.clone());
    });

    let tracking_data_app = app.clone();
    app.listen_any(WEB_ACTIVITY_BRIDGE_TRACKING_DATA_EVENT, move |_| {
        crate::app::web_activity::spawn_foreground_sync(tracking_data_app.clone());
    });
}

async fn update_runtime_state<R: Runtime + 'static>(
    app: AppHandle<R>,
    settings: WebActivityBridgeSettings,
) {
    if let Some(state) = app.try_state::<WebActivityBridgeRuntimeState>() {
        let handler_app = app.clone();
        let handler: WebActivityBridgeHttpHandler = Arc::new(move |request| {
            Box::pin(crate::app::web_activity::handle_http_request(
                handler_app.clone(),
                request,
            ))
        });
        let readiness_app = app.clone();
        let readiness: WebActivityBridgeReadinessHandler = Arc::new(move |listening| {
            if let Some(web_state) =
                readiness_app.try_state::<crate::engine::web_activity::WebActivityRuntimeState>()
            {
                web_state.set_listening(listening);
            }
        });
        if let Err(error) = state.update_with_retry(settings, handler, readiness).await {
            eprintln!("[web-activity-bridge] failed to apply settings: {error}");
        }
    }
}

async fn load_web_activity_bridge_settings<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<WebActivityBridgeSettings, String> {
    let pool = wait_for_sqlite_pool(app).await?;
    app_settings::load_web_activity_bridge_settings(&pool)
        .await
        .map_err(|error| format!("failed to load web activity bridge settings: {error}"))
}
