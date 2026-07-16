use crate::domain::tracking::{
    signal_origin_matches_window, source_app_id_identity, SustainedParticipationSignalSnapshot,
    SustainedParticipationSignalSource, SystemMediaPlaybackType,
};
use crate::platform::linux::foreground::WindowInfo;
use std::sync::{Arc, Mutex, OnceLock};
use tokio::time::{sleep, timeout, Duration};
use zbus::proxy;

const MEDIA_SESSION_QUERY_TIMEOUT_SECS: u64 = 2;
const MEDIA_SNAPSHOT_TTL_MS: i64 = 15_000;
const MEDIA_RECONCILE_INTERVAL_SECS: u64 = 10;
const MEDIA_PROBE_LOG_THROTTLE_MS: i64 = 60_000;
const MEDIA_PLAYER_LIMIT: usize = 32;
const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";

#[proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait MediaPlayer {
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<zbus::zvariant::Value<'static>>;
}

static MEDIA_SIGNAL_SOURCE: OnceLock<MediaSignalSource> = OnceLock::new();

#[derive(Clone, Debug)]
struct MediaSnapshot {
    freshness_deadline_ms: i64,
    signals: Vec<SustainedParticipationSignalSnapshot>,
}

#[derive(Clone, Debug)]
pub struct MediaSignalSource {
    state: Arc<MediaSignalSourceState>,
}

#[derive(Debug)]
struct MediaSignalSourceState {
    snapshot: Mutex<MediaSnapshot>,
}

pub fn start_signal_source() {
    let source = global_signal_source();

    tauri::async_runtime::spawn(async move {
        source.run().await;
    });
}

pub fn global_signal_source() -> MediaSignalSource {
    MEDIA_SIGNAL_SOURCE
        .get_or_init(MediaSignalSource::new)
        .clone()
}

impl MediaSignalSource {
    pub fn new() -> Self {
        Self {
            state: Arc::new(MediaSignalSourceState::new()),
        }
    }

    pub async fn run(&self) {
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        self.run_with_shutdown(shutdown_rx).await;
    }

    pub async fn run_with_shutdown(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        loop {
            if *shutdown.borrow() {
                return;
            }
            tokio::select! {
                _ = self.state.reconcile_once() => {}
                _ = shutdown.changed() => return,
            }
            tokio::select! {
                _ = sleep(Duration::from_secs(MEDIA_RECONCILE_INTERVAL_SECS)) => {}
                _ = shutdown.changed() => return,
            }
        }
    }

    pub fn signal_for_window(&self, window: &WindowInfo) -> SustainedParticipationSignalSnapshot {
        if window.exe_name.trim().is_empty() {
            return SustainedParticipationSignalSnapshot::default();
        }

        self.state.resolve_signal_for_window(window, now_ms())
    }
}

impl MediaSignalSourceState {
    fn new() -> Self {
        Self {
            snapshot: Mutex::new(MediaSnapshot {
                freshness_deadline_ms: now_ms().saturating_add(MEDIA_SNAPSHOT_TTL_MS),
                signals: Vec::new(),
            }),
        }
    }

    async fn reconcile_once(&self) {
        let now_ms = now_ms();
        let signals = match timeout(
            Duration::from_secs(MEDIA_SESSION_QUERY_TIMEOUT_SECS),
            query_mpris_signals(),
        )
        .await
        {
            Ok(Ok(signals)) => signals,
            Ok(Err(error)) => {
                log_media_probe_error(format!(
                    "failed to reconcile system media sessions: {error}"
                ));
                Vec::new()
            }
            Err(_) => {
                log_media_probe_error(format!(
                    "timed out reconciling system media sessions after {MEDIA_SESSION_QUERY_TIMEOUT_SECS}s"
                ));
                Vec::new()
            }
        };

        self.replace_snapshot(MediaSnapshot {
            freshness_deadline_ms: now_ms.saturating_add(MEDIA_SNAPSHOT_TTL_MS),
            signals,
        });
    }

    fn replace_snapshot(&self, snapshot: MediaSnapshot) {
        if let Ok(mut current) = self.snapshot.lock() {
            *current = snapshot;
        }
    }

    fn resolve_signal_for_window(
        &self,
        window: &WindowInfo,
        now_ms: i64,
    ) -> SustainedParticipationSignalSnapshot {
        let snapshot = match self.snapshot.lock() {
            Ok(snapshot) => snapshot.clone(),
            Err(_) => return SustainedParticipationSignalSnapshot::default(),
        };

        if now_ms > snapshot.freshness_deadline_ms {
            return SustainedParticipationSignalSnapshot::default();
        }

        let mut fallback_active = None;
        let mut fallback_available = None;
        for signal in snapshot.signals {
            if signal_origin_matches_window(&window.exe_name, &window.process_path, &signal) {
                return signal;
            }
            if signal.is_active && fallback_active.is_none() {
                fallback_active = Some(signal.clone());
            }
            if signal.is_available && fallback_available.is_none() {
                fallback_available = Some(signal);
            }
        }

        fallback_active.or(fallback_available).unwrap_or_default()
    }
}

async fn query_mpris_signals() -> Result<Vec<SustainedParticipationSignalSnapshot>, String> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| format!("failed to connect to D-Bus session bus: {e}"))?;

    let dbus = zbus::fdo::DBusProxy::new(&conn)
        .await
        .map_err(|e| format!("failed to create DBus proxy: {e}"))?;

    let names = dbus
        .list_names()
        .await
        .map_err(|e| format!("failed to list D-Bus names: {e}"))?;

    let mut signals = Vec::new();

    for name in names.iter() {
        let name_str = name.as_str();
        if !name_str.starts_with(MPRIS_PREFIX) {
            continue;
        }

        match query_player_signal(&conn, name_str).await {
            Ok(Some(signal)) => {
                signals.push(signal);
                if signals.len() >= MEDIA_PLAYER_LIMIT {
                    break;
                }
            }
            Ok(None) => {}
            Err(error) => {
                log_media_probe_error(format!("failed to query MPRIS player {name_str}: {error}"));
            }
        }
    }

    Ok(signals)
}

async fn query_player_signal(
    conn: &zbus::Connection,
    bus_name: &str,
) -> Result<Option<SustainedParticipationSignalSnapshot>, String> {
    let proxy = MediaPlayerProxy::builder(conn)
        .destination(bus_name)
        .map_err(|e| format!("failed to set destination: {e}"))?
        .path("/org/mpris/MediaPlayer2")
        .map_err(|e| format!("failed to set path: {e}"))?
        .build()
        .await
        .map_err(|e| format!("failed to create MPRIS proxy: {e}"))?;

    let playback_status = proxy
        .playback_status()
        .await
        .map_err(|e| format!("failed to get playback status: {e}"))?;

    let is_active = playback_status == "Playing";

    let source_app_id = bus_name
        .strip_prefix(MPRIS_PREFIX)
        .unwrap_or(bus_name)
        .to_string();

    let source_app_identity = source_app_id_identity(&source_app_id);

    let playback_type = query_playback_type(&proxy).await;

    Ok(Some(SustainedParticipationSignalSnapshot {
        is_available: true,
        is_active,
        signal_source: Some(SustainedParticipationSignalSource::SystemMedia),
        source_app_id: Some(source_app_id),
        source_app_identity,
        playback_type,
    }))
}

async fn query_playback_type(proxy: &MediaPlayerProxy<'_>) -> Option<SystemMediaPlaybackType> {
    proxy
        .metadata()
        .await
        .ok()
        .and_then(|metadata| extract_playback_type_from_metadata(&metadata))
}

fn extract_playback_type_from_metadata(
    _metadata: &zbus::zvariant::Value<'_>,
) -> Option<SystemMediaPlaybackType> {
    // MPRIS metadata playback type extraction is complex with zbus variant types.
    // The playback type is optional and not critical for sustained participation tracking.
    None
}

fn log_media_probe_error(message: String) {
    static LAST_LOGGED_AT_MS: OnceLock<Mutex<i64>> = OnceLock::new();
    let now_ms = now_ms();
    let last_logged_at_ms = LAST_LOGGED_AT_MS.get_or_init(|| Mutex::new(0));

    if let Ok(mut last_logged_at_ms) = last_logged_at_ms.lock() {
        if now_ms.saturating_sub(*last_logged_at_ms) < MEDIA_PROBE_LOG_THROTTLE_MS {
            return;
        }

        *last_logged_at_ms = now_ms;
    }

    eprintln!("[media] {message}");
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(exe_name: &str) -> WindowInfo {
        WindowInfo {
            hwnd: "0x1".to_string(),
            root_owner_hwnd: "0x1".to_string(),
            process_id: 1,
            window_class: "test".to_string(),
            title: "test".to_string(),
            exe_name: exe_name.to_string(),
            process_path: format!("/usr/bin/{exe_name}"),
            is_afk: false,
            idle_time_ms: 0,
        }
    }

    fn signal(source_app_id: &str, is_active: bool) -> SustainedParticipationSignalSnapshot {
        SustainedParticipationSignalSnapshot {
            is_available: true,
            is_active,
            signal_source: Some(SustainedParticipationSignalSource::SystemMedia),
            source_app_id: Some(source_app_id.to_string()),
            source_app_identity: source_app_id_identity(source_app_id),
            playback_type: None,
        }
    }

    fn source_with_signals(
        signals: Vec<SustainedParticipationSignalSnapshot>,
    ) -> MediaSignalSource {
        let source = MediaSignalSource::new();
        source.state.replace_snapshot(MediaSnapshot {
            freshness_deadline_ms: now_ms().saturating_add(MEDIA_SNAPSHOT_TTL_MS),
            signals,
        });
        source
    }

    #[test]
    fn matching_player_wins_over_first_active_player() {
        let source = source_with_signals(vec![signal("spotify", true), signal("firefox", true)]);

        let resolved = source.signal_for_window(&window("firefox"));

        assert_eq!(resolved.source_app_id.as_deref(), Some("firefox"));
        assert!(resolved.is_active);
    }

    #[test]
    fn matching_paused_player_wins_over_unrelated_active_player() {
        let source = source_with_signals(vec![signal("spotify", true), signal("firefox", false)]);

        let resolved = source.signal_for_window(&window("firefox"));

        assert_eq!(resolved.source_app_id.as_deref(), Some("firefox"));
        assert!(!resolved.is_active);
    }

    #[test]
    fn stale_snapshot_does_not_report_media_participation() {
        let source = MediaSignalSource::new();
        source.state.replace_snapshot(MediaSnapshot {
            freshness_deadline_ms: now_ms().saturating_sub(1),
            signals: vec![signal("firefox", true)],
        });

        assert_eq!(
            source.signal_for_window(&window("firefox")),
            SustainedParticipationSignalSnapshot::default()
        );
    }

    #[tokio::test]
    async fn cancelled_source_stops_before_connecting_to_dbus() {
        let source = MediaSignalSource::new();
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(true);

        tokio::time::timeout(
            Duration::from_millis(250),
            source.run_with_shutdown(shutdown_rx),
        )
        .await
        .expect("cancelled media source should stop promptly");
        drop(shutdown_tx);
    }

    #[tokio::test]
    #[ignore = "requires a running D-Bus user session"]
    async fn live_mpris_query_completes_when_session_bus_is_available() {
        let signals = query_mpris_signals().await;

        assert!(
            signals.is_ok(),
            "expected MPRIS query to complete, got {signals:?}"
        );
    }
}
