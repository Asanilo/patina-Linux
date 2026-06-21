use crate::domain::tracking::{
    evaluate_sustained_participation_signal, source_app_id_identity,
    SustainedParticipationSignalMatchResult, SustainedParticipationSignalSnapshot,
    SustainedParticipationSignalSource, SystemMediaPlaybackType,
};
use crate::platform::linux::foreground::WindowInfo;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};
use tokio::time::{sleep, timeout, Duration};
use zbus::proxy;

const MEDIA_SESSION_QUERY_TIMEOUT_SECS: u64 = 2;
const MEDIA_SNAPSHOT_TTL_MS: i64 = 15_000;
const MEDIA_RECONCILE_INTERVAL_SECS: u64 = 10;
const MEDIA_PROBE_LOG_THROTTLE_MS: i64 = 60_000;
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

static MEDIA_SIGNAL_SOURCE: OnceLock<Arc<MediaSignalSourceState>> = OnceLock::new();

#[derive(Debug)]
struct MediaSnapshot {
    generated_at_ms: i64,
    freshness_deadline_ms: i64,
    signal: SustainedParticipationSignalSnapshot,
}

#[derive(Debug)]
struct MediaSignalSourceState {
    snapshot: Mutex<MediaSnapshot>,
    probe_in_flight: Arc<AtomicBool>,
}

struct MediaProbeInFlightGuard {
    probe_in_flight: Arc<AtomicBool>,
}

pub fn start_signal_source() {
    let state = MEDIA_SIGNAL_SOURCE
        .get_or_init(|| Arc::new(MediaSignalSourceState::new()))
        .clone();

    tauri::async_runtime::spawn(async move {
        state.run().await;
    });
}

pub async fn get_sustained_participation_signal(
    window: &WindowInfo,
) -> SustainedParticipationSignalSnapshot {
    if window.exe_name.trim().is_empty() {
        return SustainedParticipationSignalSnapshot::default();
    }

    let Some(state) = MEDIA_SIGNAL_SOURCE.get() else {
        return SustainedParticipationSignalSnapshot::default();
    };

    state.resolve_signal_for_window(window, now_ms())
}

impl MediaSignalSourceState {
    fn new() -> Self {
        Self {
            snapshot: Mutex::new(MediaSnapshot {
                generated_at_ms: now_ms(),
                freshness_deadline_ms: now_ms().saturating_add(MEDIA_SNAPSHOT_TTL_MS),
                signal: SustainedParticipationSignalSnapshot::default(),
            }),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn run(&self) {
        loop {
            self.reconcile_once().await;
            sleep(Duration::from_secs(MEDIA_RECONCILE_INTERVAL_SECS)).await;
        }
    }

    async fn reconcile_once(&self) {
        let now_ms = now_ms();
        if self.probe_in_flight.swap(true, Ordering::AcqRel) {
            self.replace_snapshot(MediaSnapshot {
                generated_at_ms: now_ms,
                freshness_deadline_ms: now_ms.saturating_add(MEDIA_SNAPSHOT_TTL_MS),
                signal: SustainedParticipationSignalSnapshot::default(),
            });
            return;
        }

        let probe_in_flight = self.probe_in_flight.clone();
        let query = tauri::async_runtime::spawn(async move {
            let _guard = MediaProbeInFlightGuard { probe_in_flight };
            query_mpris_signal().await
        });

        let signal = match timeout(Duration::from_secs(MEDIA_SESSION_QUERY_TIMEOUT_SECS), query)
            .await
        {
            Ok(Ok(Ok(Some(signal)))) => signal,
            Ok(Ok(Ok(None))) => SustainedParticipationSignalSnapshot::default(),
            Ok(Ok(Err(error))) => {
                log_media_probe_error(format!(
                    "failed to reconcile system media sessions: {error}"
                ));
                SustainedParticipationSignalSnapshot::default()
            }
            Ok(Err(error)) => {
                log_media_probe_error(format!("system media query task failed: {error}"));
                SustainedParticipationSignalSnapshot::default()
            }
            Err(_) => {
                log_media_probe_error(format!(
                    "timed out reconciling system media sessions after {MEDIA_SESSION_QUERY_TIMEOUT_SECS}s"
                ));
                SustainedParticipationSignalSnapshot::default()
            }
        };

        self.replace_snapshot(MediaSnapshot {
            generated_at_ms: now_ms,
            freshness_deadline_ms: now_ms.saturating_add(MEDIA_SNAPSHOT_TTL_MS),
            signal,
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
            Ok(snapshot) => MediaSnapshot {
                generated_at_ms: snapshot.generated_at_ms,
                freshness_deadline_ms: snapshot.freshness_deadline_ms,
                signal: snapshot.signal.clone(),
            },
            Err(_) => return SustainedParticipationSignalSnapshot::default(),
        };

        if now_ms > snapshot.freshness_deadline_ms {
            return SustainedParticipationSignalSnapshot::default();
        }

        if evaluate_sustained_participation_signal(
            &window.exe_name,
            &window.process_path,
            &snapshot.signal,
        )
        .match_result
            == SustainedParticipationSignalMatchResult::Unavailable
        {
            return SustainedParticipationSignalSnapshot::default();
        }

        snapshot.signal
    }
}

impl Drop for MediaProbeInFlightGuard {
    fn drop(&mut self) {
        self.probe_in_flight.store(false, Ordering::Release);
    }
}

async fn query_mpris_signal() -> Result<Option<SustainedParticipationSignalSnapshot>, String> {
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

    let mut fallback_active: Option<SustainedParticipationSignalSnapshot> = None;
    let mut fallback_available: Option<SustainedParticipationSignalSnapshot> = None;

    for name in names.iter() {
        let name_str = name.as_str();
        if !name_str.starts_with(MPRIS_PREFIX) {
            continue;
        }

        match query_player_signal(&conn, name_str).await {
            Ok(Some(signal)) => {
                if signal.is_active {
                    if fallback_active.is_none() {
                        fallback_active = Some(signal);
                    }
                } else if signal.is_available && fallback_available.is_none() {
                    fallback_available = Some(signal);
                }
            }
            Ok(None) => {}
            Err(error) => {
                log_media_probe_error(format!("failed to query MPRIS player {name_str}: {error}"));
            }
        }
    }

    Ok(fallback_active.or(fallback_available))
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
