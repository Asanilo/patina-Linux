use crate::engine::tracking::runtime as tracking_runtime;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();
const POWER_EVENT_SOURCE: &str = "power_lifecycle_v1";

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PowerLifecycleEvent {
    pub state: String,
    pub timestamp_ms: u64,
    pub source: String,
}

pub fn start(app_handle: AppHandle) {
    if APP_HANDLE.set(app_handle.clone()).is_err() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        if let Err(error) = watch_systemd_logind(app_handle.clone()).await {
            eprintln!("[power] failed to watch systemd-logind: {error}");
        }
    });
}

async fn watch_systemd_logind(app_handle: AppHandle) -> Result<(), String> {
    use zbus::proxy;

    #[proxy(
        interface = "org.freedesktop.login1.Manager",
        default_service = "org.freedesktop.login1",
        default_path = "/org/freedesktop/login1"
    )]
    trait LoginManager {
        #[zbus(signal)]
        fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;

        #[zbus(signal)]
        fn prepare_for_shutdown(&self, start: bool) -> zbus::Result<()>;
    }

    let conn = zbus::Connection::system()
        .await
        .map_err(|e| format!("failed to connect to system D-Bus: {e}"))?;

    let proxy = LoginManagerProxy::new(&conn)
        .await
        .map_err(|e| format!("failed to create login1 proxy: {e}"))?;

    let mut prepare_for_sleep_stream = proxy
        .receive_prepare_for_sleep()
        .await
        .map_err(|e| format!("failed to subscribe to PrepareForSleep: {e}"))?;

    let mut prepare_for_shutdown_stream = proxy
        .receive_prepare_for_shutdown()
        .await
        .map_err(|e| format!("failed to subscribe to PrepareForShutdown: {e}"))?;

    let _ = app_handle.emit(
        "power-watcher-ready",
        PowerLifecycleEvent {
            state: "ready".into(),
            timestamp_ms: now_ms(),
            source: POWER_EVENT_SOURCE.into(),
        },
    );

    loop {
        tokio::select! {
            Some(signal) = prepare_for_sleep_stream.next() => {
                match signal.args() {
                    Ok(args) => {
                        if *args.start() {
                            emit_power_event("suspend");
                        } else {
                            emit_power_event("resume");
                        }
                    }
                    Err(e) => {
                        eprintln!("[power] failed to parse PrepareForSleep signal: {e}");
                    }
                }
            }
            Some(signal) = prepare_for_shutdown_stream.next() => {
                match signal.args() {
                    Ok(args) => {
                        if *args.start() {
                            emit_power_event("shutdown");
                        }
                    }
                    Err(e) => {
                        eprintln!("[power] failed to parse PrepareForShutdown signal: {e}");
                    }
                }
            }
        }
    }
}

fn emit_power_event(state: &str) {
    if let Some(app_handle) = APP_HANDLE.get() {
        let event = PowerLifecycleEvent {
            state: state.to_string(),
            timestamp_ms: now_ms(),
            source: POWER_EVENT_SOURCE.to_string(),
        };
        let _ = app_handle.emit("power-lifecycle-changed", &event);
        let app_handle = app_handle.clone();
        let event_state = event.state.clone();
        let timestamp_ms = event.timestamp_ms as i64;
        tauri::async_runtime::spawn(async move {
            if let Err(error) = tracking_runtime::handle_power_lifecycle_event(
                app_handle,
                &event_state,
                timestamp_ms,
            )
            .await
            {
                eprintln!("[tracker] power lifecycle handling failed: {error}");
            }
        });
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
