use crate::engine::tracking::runtime as tracking_runtime;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, watch};
use zbus::proxy;
use zbus::zvariant::OwnedObjectPath;

const POWER_EVENT_SOURCE: &str = "power_lifecycle_v1";
const POWER_EVENT_BUFFER: usize = 16;

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait LoginManager {
    fn get_session(&self, session_id: &str) -> zbus::Result<OwnedObjectPath>;
    #[zbus(name = "GetSessionByPID")]
    fn get_session_by_pid(&self, pid: u32) -> zbus::Result<OwnedObjectPath>;
    #[zbus(name = "GetUserByPID")]
    fn get_user_by_pid(&self, pid: u32) -> zbus::Result<OwnedObjectPath>;

    #[zbus(signal)]
    fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;

    #[zbus(signal)]
    fn prepare_for_shutdown(&self, start: bool) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1"
)]
trait LoginSession {
    #[zbus(property)]
    fn locked_hint(&self) -> zbus::Result<bool>;

    #[zbus(signal)]
    fn lock(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn unlock(&self) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.login1.User",
    default_service = "org.freedesktop.login1"
)]
trait LoginUser {
    #[zbus(property)]
    fn display(&self) -> zbus::Result<(String, OwnedObjectPath)>;
}

#[derive(Clone, Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct PowerLifecycleEvent {
    pub state: String,
    pub timestamp_ms: u64,
    pub source: String,
}

impl PowerLifecycleEvent {
    fn new(state: &str) -> Self {
        Self {
            state: state.to_string(),
            timestamp_ms: now_ms(),
            source: POWER_EVENT_SOURCE.to_string(),
        }
    }
}

pub fn start(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let (_shutdown_tx, shutdown_rx) = watch::channel(false);
        let (event_tx, mut event_rx) = mpsc::channel(POWER_EVENT_BUFFER);
        let watcher = tauri::async_runtime::spawn(watch_systemd_logind(shutdown_rx, event_tx));

        while let Some(event) = event_rx.recv().await {
            if event.state == "ready" {
                let _ = app_handle.emit("power-watcher-ready", &event);
                continue;
            }

            let _ = app_handle.emit("power-lifecycle-changed", &event);
            if let Err(error) = tracking_runtime::handle_power_lifecycle_event(
                app_handle.clone(),
                &event.state,
                event.timestamp_ms as i64,
            )
            .await
            {
                eprintln!("[tracker] power lifecycle handling failed: {error}");
            }
        }

        match watcher.await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("[power] failed to watch systemd-logind: {error}"),
            Err(error) => eprintln!("[power] watcher task failed: {error}"),
        }
    });
}

pub async fn watch_systemd_logind(
    mut shutdown: watch::Receiver<bool>,
    event_tx: mpsc::Sender<PowerLifecycleEvent>,
) -> Result<(), String> {
    if *shutdown.borrow() {
        return Ok(());
    }
    let conn = zbus::Connection::system()
        .await
        .map_err(|error| format!("failed to connect to system D-Bus: {error}"))?;
    let manager = LoginManagerProxy::new(&conn)
        .await
        .map_err(|error| format!("failed to create login1 manager proxy: {error}"))?;
    let session_path = resolve_current_session_path(&conn, &manager).await?;
    let session = LoginSessionProxy::builder(&conn)
        .path(session_path)
        .map_err(|error| format!("failed to set login1 session path: {error}"))?
        .build()
        .await
        .map_err(|error| format!("failed to create login1 session proxy: {error}"))?;

    let mut prepare_for_sleep = manager
        .receive_prepare_for_sleep()
        .await
        .map_err(|error| format!("failed to subscribe to PrepareForSleep: {error}"))?;
    let mut prepare_for_shutdown = manager
        .receive_prepare_for_shutdown()
        .await
        .map_err(|error| format!("failed to subscribe to PrepareForShutdown: {error}"))?;
    let mut lock = session
        .receive_lock()
        .await
        .map_err(|error| format!("failed to subscribe to session Lock: {error}"))?;
    let mut unlock = session
        .receive_unlock()
        .await
        .map_err(|error| format!("failed to subscribe to session Unlock: {error}"))?;
    let mut locked_hint = session.receive_locked_hint_changed().await;

    send_event(&event_tx, "ready").await?;
    if session
        .locked_hint()
        .await
        .map_err(|error| format!("failed to read session LockedHint: {error}"))?
    {
        send_event(&event_tx, "lock").await?;
    }

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                let _ = changed;
                return Ok(());
            }
            signal = prepare_for_sleep.next() => {
                let signal = signal.ok_or_else(|| "PrepareForSleep stream ended".to_string())?;
                let args = signal.args().map_err(|error| format!("failed to parse PrepareForSleep signal: {error}"))?;
                send_event(&event_tx, sleep_state(*args.start())).await?;
            }
            signal = prepare_for_shutdown.next() => {
                let signal = signal.ok_or_else(|| "PrepareForShutdown stream ended".to_string())?;
                let args = signal.args().map_err(|error| format!("failed to parse PrepareForShutdown signal: {error}"))?;
                if *args.start() {
                    send_event(&event_tx, "shutdown").await?;
                }
            }
            signal = lock.next() => {
                signal.ok_or_else(|| "session Lock stream ended".to_string())?;
                send_event(&event_tx, "lock").await?;
            }
            signal = unlock.next() => {
                signal.ok_or_else(|| "session Unlock stream ended".to_string())?;
                send_event(&event_tx, "unlock").await?;
            }
            changed = locked_hint.next() => {
                let changed = changed.ok_or_else(|| "session LockedHint stream ended".to_string())?;
                let is_locked = changed.get().await
                    .map_err(|error| format!("failed to read changed session LockedHint: {error}"))?;
                send_event(&event_tx, lock_state(is_locked)).await?;
            }
        }
    }
}

async fn resolve_current_session_path(
    conn: &zbus::Connection,
    manager: &LoginManagerProxy<'_>,
) -> Result<OwnedObjectPath, String> {
    if let Some(session_id) = std::env::var("XDG_SESSION_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        if let Ok(path) = manager.get_session(session_id.trim()).await {
            return Ok(path);
        }
    }

    let pid = std::process::id();
    if let Ok(path) = manager.get_session_by_pid(pid).await {
        return Ok(path);
    }

    let user_path = manager
        .get_user_by_pid(pid)
        .await
        .map_err(|error| format!("failed to resolve current login1 user: {error}"))?;
    let user = LoginUserProxy::builder(conn)
        .path(user_path)
        .map_err(|error| format!("failed to set login1 user path: {error}"))?
        .build()
        .await
        .map_err(|error| format!("failed to create login1 user proxy: {error}"))?;
    let (session_id, session_path) = user
        .display()
        .await
        .map_err(|error| format!("failed to read login1 user Display session: {error}"))?;
    if session_id.trim().is_empty() || session_path.as_str() == "/" {
        return Err("login1 user has no graphical Display session".to_string());
    }
    Ok(session_path)
}

async fn send_event(
    event_tx: &mpsc::Sender<PowerLifecycleEvent>,
    state: &str,
) -> Result<(), String> {
    event_tx
        .send(PowerLifecycleEvent::new(state))
        .await
        .map_err(|_| "power lifecycle consumer stopped".to_string())
}

fn sleep_state(start: bool) -> &'static str {
    if start {
        "suspend"
    } else {
        "resume"
    }
}

fn lock_state(is_locked: bool) -> &'static str {
    if is_locked {
        "lock"
    } else {
        "unlock"
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_signal_maps_both_lifecycle_edges() {
        assert_eq!(sleep_state(true), "suspend");
        assert_eq!(sleep_state(false), "resume");
    }

    #[test]
    fn locked_hint_maps_both_lifecycle_edges() {
        assert_eq!(lock_state(true), "lock");
        assert_eq!(lock_state(false), "unlock");
    }

    #[tokio::test]
    async fn cancelled_watcher_stops_before_connecting_to_dbus() {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        shutdown_tx.send(true).unwrap();
        let (event_tx, _event_rx) = mpsc::channel(1);

        let result = watch_systemd_logind(shutdown_rx, event_tx).await;

        assert!(result.is_ok());
    }
}
