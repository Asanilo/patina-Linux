use crate::engine::tracking::runtime as tracking_runtime;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, watch};
use zbus::proxy;

const POWER_EVENT_SOURCE: &str = "power_lifecycle_v1";
const POWER_EVENT_BUFFER: usize = 16;

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
    shutdown: watch::Receiver<bool>,
    event_tx: mpsc::Sender<PowerLifecycleEvent>,
) -> Result<(), String> {
    if *shutdown.borrow() {
        return Ok(());
    }
    let conn = zbus::Connection::system()
        .await
        .map_err(|error| format!("failed to connect to system D-Bus: {error}"))?;
    watch_connection(&conn, shutdown, event_tx).await
}

pub(super) async fn watch_connection(
    conn: &zbus::Connection,
    mut shutdown: watch::Receiver<bool>,
    event_tx: mpsc::Sender<PowerLifecycleEvent>,
) -> Result<(), String> {
    let manager = LoginManagerProxy::new(conn)
        .await
        .map_err(|_| "login1 manager unavailable")?;
    let mut sleep = manager
        .receive_prepare_for_sleep()
        .await
        .map_err(|_| "PrepareForSleep subscription failed")?;
    let mut stop = manager
        .receive_prepare_for_shutdown()
        .await
        .map_err(|_| "PrepareForShutdown subscription failed")?;
    send_event(&event_tx, "ready").await?;
    let sessions = watch_graphical_sessions(conn, shutdown.clone(), event_tx.clone());
    tokio::pin!(sessions);
    // Sleep and shutdown remain observable even before login or between sessions.
    loop {
        tokio::select! {
            _ = shutdown.changed() => return Ok(()),
            result = &mut sessions => return result,
            signal = sleep.next() => {
                let signal = signal.ok_or("PrepareForSleep stream ended")?;
                let args = signal.args().map_err(|_| "invalid PrepareForSleep signal")?;
                send_event(&event_tx, sleep_state(*args.start())).await?;
            }
            signal = stop.next() => {
                let signal = signal.ok_or("PrepareForShutdown stream ended")?;
                let args = signal.args().map_err(|_| "invalid PrepareForShutdown signal")?;
                if *args.start() {
                    send_event(&event_tx, "shutdown").await?;
                }
            }
        }
    }
}

async fn watch_graphical_sessions(
    conn: &zbus::Connection,
    mut shutdown: watch::Receiver<bool>,
    event_tx: mpsc::Sender<PowerLifecycleEvent>,
) -> Result<(), String> {
    let mut refresh = tokio::time::interval(std::time::Duration::from_secs(1));
    refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = shutdown.changed() => return Ok(()),
            _ = refresh.tick() => {}
        }
        let Ok(current) = super::session::read_on(conn).await else {
            continue;
        };
        let Ok(session) = LoginSessionProxy::builder(conn)
            .path(current.path.clone())
            .map_err(|_| "invalid login1 session path")?
            .build()
            .await
        else {
            continue;
        };
        let Ok(mut lock) = session.receive_lock().await else {
            continue;
        };
        let Ok(mut unlock) = session.receive_unlock().await else {
            continue;
        };
        let mut hint = session.receive_locked_hint_changed().await;
        let Ok(initial) = session.locked_hint().await else {
            continue;
        };
        if !same_session(conn, &current).await {
            continue;
        }
        send_event(&event_tx, "ready").await?;
        // Explicit unlock also clears a lock retained from the previous login.
        send_event(&event_tx, lock_state(initial)).await?;
        let mut last_locked = initial;
        loop {
            tokio::select! {
                _ = shutdown.changed() => return Ok(()),
                _ = refresh.tick() => {
                    if !same_session(conn, &current).await { break; }
                }
                signal = lock.next() => {
                    if signal.is_none() || !same_session(conn, &current).await { break; }
                    if !last_locked {
                        send_event(&event_tx, "lock").await?;
                        last_locked = true;
                    }
                }
                signal = unlock.next() => {
                    if signal.is_none() || !same_session(conn, &current).await { break; }
                    if last_locked {
                        send_event(&event_tx, "unlock").await?;
                        last_locked = false;
                    }
                }
                changed = hint.next() => {
                    let Some(changed) = changed else { break; };
                    if !same_session(conn, &current).await { break; }
                    let Ok(locked) = changed.get().await else { break; };
                    if locked != last_locked {
                        send_event(&event_tx, lock_state(locked)).await?;
                        last_locked = locked;
                    }
                }
            }
        }
    }
}

async fn same_session(
    conn: &zbus::Connection,
    previous: &super::session::GraphicalSession,
) -> bool {
    super::session::read_on(conn).await.as_ref() == Ok(previous)
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
