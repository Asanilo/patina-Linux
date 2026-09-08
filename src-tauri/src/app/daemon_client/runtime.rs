use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::watch;

use crate::engine::api::types::{ActiveSessionResponse, CurrentWindowResponse};
use crate::engine::runtime_event::{RuntimeEvent, RuntimeEventEnvelope, RuntimeEventSink};
use crate::engine::tracking::watchdog::RuntimeHealthState;
use crate::platform::daemon_client::{PatinadClient, PatinadClientError, PatinadStreamEvent};

const INITIAL_REPLAY_CURSOR: u64 = 0;
const INITIAL_RETRY_DELAY: Duration = Duration::from_millis(500);
const MAX_RETRY_DELAY: Duration = Duration::from_secs(10);
const INCOHERENT_SNAPSHOT_RETRY_DELAY: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PatinadRuntimeConnectionStatus {
    Connecting,
    Ready,
    Reconnecting,
    Stopped,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PatinadRuntimeReadSnapshot {
    pub current_window: CurrentWindowResponse,
    pub active_session: Option<ActiveSessionResponse>,
    pub last_event_sequence: Option<u64>,
    pub coherent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PatinadRuntimeAdapterSnapshot {
    pub connection_status: PatinadRuntimeConnectionStatus,
    pub runtime: Option<PatinadRuntimeReadSnapshot>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

impl Default for PatinadRuntimeAdapterSnapshot {
    fn default() -> Self {
        Self {
            connection_status: PatinadRuntimeConnectionStatus::Stopped,
            runtime: None,
            error_code: None,
            error_message: None,
        }
    }
}

pub trait PatinadRuntimeOutput: Send + Sync {
    fn connection_changed(
        &self,
        status: PatinadRuntimeConnectionStatus,
        error: Option<&PatinadClientError>,
    );

    fn snapshot_changed(&self, snapshot: PatinadRuntimeReadSnapshot);

    fn tracking_data_changed(&self, event: &RuntimeEventEnvelope);

    fn resync_required(&self, reason: &str, missed: Option<u64>);
}

#[derive(Debug, Default)]
pub struct PatinadRuntimeState {
    inner: Mutex<PatinadRuntimeAdapterSnapshot>,
}

impl PatinadRuntimeState {
    pub fn snapshot(&self) -> PatinadRuntimeAdapterSnapshot {
        match self.inner.lock() {
            Ok(snapshot) => snapshot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn update(&self, update: impl FnOnce(&mut PatinadRuntimeAdapterSnapshot)) {
        match self.inner.lock() {
            Ok(mut snapshot) => update(&mut snapshot),
            Err(poisoned) => update(&mut poisoned.into_inner()),
        }
    }

    pub fn report_connection_error(&self, error: &PatinadClientError) {
        self.connection_changed(PatinadRuntimeConnectionStatus::Reconnecting, Some(error));
    }
}

impl PatinadRuntimeOutput for PatinadRuntimeState {
    fn connection_changed(
        &self,
        status: PatinadRuntimeConnectionStatus,
        error: Option<&PatinadClientError>,
    ) {
        self.update(|snapshot| {
            snapshot.connection_status = status;
            snapshot.error_code = error.map(|error| error.code().to_string());
            snapshot.error_message = error.map(ToString::to_string);
            if error.is_some() || status == PatinadRuntimeConnectionStatus::Stopped {
                snapshot.runtime = None;
            }
        });
    }

    fn snapshot_changed(&self, runtime: PatinadRuntimeReadSnapshot) {
        self.update(|snapshot| {
            snapshot.runtime = Some(runtime);
            snapshot.error_code = None;
            snapshot.error_message = None;
        });
    }

    fn tracking_data_changed(&self, _event: &RuntimeEventEnvelope) {}

    fn resync_required(&self, _reason: &str, _missed: Option<u64>) {}
}

pub struct PatinadRuntimeAdapter {
    client_state: crate::app::daemon_client::PatinadClientState,
    output: Arc<dyn PatinadRuntimeOutput>,
}

pub struct PatinadDesktopRuntimeHandle {
    shutdown_tx: watch::Sender<bool>,
    task: std::sync::Mutex<Option<JoinHandle<()>>>,
}

impl PatinadDesktopRuntimeHandle {
    pub fn start<R: Runtime + 'static>(
        app: AppHandle<R>,
        client_state: crate::app::daemon_client::PatinadClientState,
        runtime_health: Arc<RuntimeHealthState>,
    ) -> Self {
        let output = Arc::new(TauriPatinadRuntimeOutput {
            app,
            runtime_health,
            client_state: client_state.clone(),
        });
        let adapter = PatinadRuntimeAdapter::new_with_client_state(client_state, output);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tauri::async_runtime::spawn(async move {
            adapter.run(shutdown_rx).await;
        });
        Self {
            shutdown_tx,
            task: std::sync::Mutex::new(Some(task)),
        }
    }

    pub async fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
        let task = match self.task.lock() {
            Ok(mut task) => task.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        let Some(mut task) = task else {
            return;
        };
        if tokio::time::timeout(Duration::from_secs(2), &mut task)
            .await
            .is_err()
        {
            task.abort();
            let _ = task.await;
        }
    }
}

struct TauriPatinadRuntimeOutput<R: Runtime> {
    app: AppHandle<R>,
    runtime_health: Arc<RuntimeHealthState>,
    client_state: crate::app::daemon_client::PatinadClientState,
}

impl<R: Runtime> PatinadRuntimeOutput for TauriPatinadRuntimeOutput<R> {
    fn connection_changed(
        &self,
        status: PatinadRuntimeConnectionStatus,
        error: Option<&PatinadClientError>,
    ) {
        if let Some(state) = self.app.try_state::<PatinadRuntimeState>() {
            state.connection_changed(status, error);
        }
        if error.is_some() || status == PatinadRuntimeConnectionStatus::Stopped {
            if let Some(state) = self.app.try_state::<
                crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState,
            >() {
                state.clear();
            }
            if error.is_some() {
                let sink =
                    crate::engine::tracking::runtime::TauriRuntimeEventSink::new(self.app.clone());
                let _ = sink.emit(RuntimeEvent::TrackingDataChanged {
                    reason: "daemon-client-disconnected".to_string(),
                    changed_at_ms: crate::app::runtime::now_ms(),
                });
            }
        }
    }

    fn snapshot_changed(&self, snapshot: PatinadRuntimeReadSnapshot) {
        let tracking_snapshot = snapshot.current_window.runtime_snapshot.clone();
        if let Some(state) = self.app.try_state::<PatinadRuntimeState>() {
            state.snapshot_changed(snapshot);
        }
        let mut window_changed = true;
        if let Some(state) = self
            .app
            .try_state::<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>(
        ) {
            window_changed = state
                .snapshot()
                .is_none_or(|previous| previous.window != tracking_snapshot.window);
            state.replace(tracking_snapshot.clone());
        }
        self.runtime_health
            .note_heartbeat(tracking_snapshot.sampled_at_ms);
        if let Some(last_successful_sample_at_ms) = tracking_snapshot
            .probe_diagnostics
            .last_successful_sample_at_ms
            .or_else(|| {
                (tracking_snapshot.probe_status
                    == crate::engine::tracking::runtime_snapshot::TrackingRuntimeProbeStatus::Ok)
                    .then_some(tracking_snapshot.sampled_at_ms)
            })
        {
            self.runtime_health
                .note_successful_sample(last_successful_sample_at_ms);
        }
        if window_changed {
            let _ = self
                .app
                .emit("active-window-changed", &tracking_snapshot.window);
        }
    }

    fn tracking_data_changed(&self, event: &RuntimeEventEnvelope) {
        match &event.event {
            RuntimeEvent::ToolsRuntimeChanged { .. } => {
                let app = self.app.clone();
                let client_state = self.client_state.clone();
                tauri::async_runtime::spawn(async move {
                    let client = match client_state.require() {
                        Ok(client) => client,
                        Err(error) => {
                            eprintln!("[patinad-client] failed to refresh Tools snapshot: {error}");
                            return;
                        }
                    };
                    match client.tools_snapshot().await {
                        Ok(snapshot) => {
                            if let Some(state) =
                                app.try_state::<crate::engine::tools::ToolsRuntimeState>()
                            {
                                state.replace(snapshot.clone());
                            }
                            if let Err(error) = app
                                .emit(crate::engine::tools::TOOLS_RUNTIME_CHANGED_EVENT, snapshot)
                            {
                                eprintln!(
                                    "[patinad-client] failed to emit Tools snapshot: {error}"
                                );
                            }
                        }
                        Err(error) => {
                            eprintln!("[patinad-client] failed to refresh Tools snapshot: {error}");
                        }
                    }
                });
                return;
            }
            RuntimeEvent::ToolAlert { alert } => {
                crate::engine::tools::deliver_alert_to_desktop(&self.app, alert);
                return;
            }
            RuntimeEvent::TrackingDataChanged { .. }
            | RuntimeEvent::ScheduledBackupChanged { .. } => {}
        }
        let sink = crate::engine::tracking::runtime::TauriRuntimeEventSink::new(self.app.clone());
        if let Err(error) = sink.emit(event.event.clone()) {
            eprintln!("[patinad-client] failed to forward runtime event: {error}");
        }
    }

    fn resync_required(&self, reason: &str, missed: Option<u64>) {
        eprintln!(
            "[patinad-client] event stream resync required: reason={reason}, missed={missed:?}"
        );
    }
}

impl PatinadRuntimeAdapter {
    pub fn new(client: PatinadClient, output: Arc<dyn PatinadRuntimeOutput>) -> Self {
        let client_state = crate::app::daemon_client::PatinadClientState::default();
        client_state.install(client);
        Self::new_with_client_state(client_state, output)
    }

    pub fn new_with_client_state(
        client_state: crate::app::daemon_client::PatinadClientState,
        output: Arc<dyn PatinadRuntimeOutput>,
    ) -> Self {
        Self {
            client_state,
            output,
        }
    }

    pub async fn synchronize_once(
        &self,
        last_event_sequence: Option<u64>,
    ) -> Result<PatinadRuntimeReadSnapshot, PatinadClientError> {
        let client = self.client()?;
        client.negotiate_tracking_owner().await?;
        self.read_snapshot(&client, last_event_sequence).await
    }

    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) {
        let mut cursor = Some(INITIAL_REPLAY_CURSOR);
        let mut backoff = RetryBackoff::default();
        let mut first_attempt = true;

        loop {
            if *shutdown.borrow() {
                self.output
                    .connection_changed(PatinadRuntimeConnectionStatus::Stopped, None);
                return;
            }
            self.output.connection_changed(
                if first_attempt {
                    PatinadRuntimeConnectionStatus::Connecting
                } else {
                    PatinadRuntimeConnectionStatus::Reconnecting
                },
                None,
            );
            first_attempt = false;

            match self.run_connection(&mut cursor, &mut shutdown).await {
                Ok(ConnectionExit::Shutdown) => {
                    self.output
                        .connection_changed(PatinadRuntimeConnectionStatus::Stopped, None);
                    return;
                }
                Ok(ConnectionExit::Reconfigure) => {
                    cursor = None;
                    backoff.reset();
                    continue;
                }
                Err(error) => {
                    self.output.connection_changed(
                        PatinadRuntimeConnectionStatus::Reconnecting,
                        Some(&error),
                    );
                }
            }

            let delay = backoff.next_delay();
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        self.output.connection_changed(
                            PatinadRuntimeConnectionStatus::Stopped,
                            None,
                        );
                        return;
                    }
                }
            }
        }
    }

    async fn run_connection(
        &self,
        cursor: &mut Option<u64>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<ConnectionExit, PatinadClientError> {
        let mut client_revision = self.client_state.subscribe();
        let client = self.client()?;
        client.negotiate_tracking_owner().await?;
        let mut events = client.open_event_stream(*cursor).await?;
        let mut snapshot = self.read_snapshot(&client, *cursor).await?;
        self.output.snapshot_changed(snapshot.clone());
        self.output
            .connection_changed(PatinadRuntimeConnectionStatus::Ready, None);

        loop {
            let event = tokio::select! {
                event = events.next_event() => event?,
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(ConnectionExit::Shutdown);
                    }
                    continue;
                }
                changed = client_revision.changed() => {
                    if changed.is_err() {
                        return Err(PatinadClientError::InvalidConfiguration(
                            "patinad client configuration channel closed".to_string(),
                        ));
                    }
                    return Ok(ConnectionExit::Reconfigure);
                }
            };
            let Some(event) = event else {
                return Err(PatinadClientError::Unreachable(
                    "patinad event stream closed".to_string(),
                ));
            };

            if let Some(sequence) = event.sequence() {
                *cursor = Some(sequence);
            }
            match event {
                PatinadStreamEvent::Runtime(envelope) => {
                    let RuntimeEvent::TrackingDataChanged { changed_at_ms, .. } = &envelope.event
                    else {
                        continue;
                    };
                    let changed_at_ms = i64::try_from(*changed_at_ms).unwrap_or(i64::MAX);
                    if changed_at_ms < snapshot.current_window.sampled_at_ms {
                        continue;
                    }
                    snapshot = self.read_snapshot(&client, *cursor).await?;
                    self.output.snapshot_changed(snapshot.clone());
                    self.output.tracking_data_changed(&envelope);
                }
                PatinadStreamEvent::ResyncRequired { reason, missed } => {
                    *cursor = None;
                    snapshot = self.read_snapshot(&client, *cursor).await?;
                    self.output.snapshot_changed(snapshot.clone());
                    self.output.resync_required(&reason, missed);
                }
                PatinadStreamEvent::Ignored { .. } => {}
            }
        }
    }

    async fn read_snapshot(
        &self,
        client: &PatinadClient,
        last_event_sequence: Option<u64>,
    ) -> Result<PatinadRuntimeReadSnapshot, PatinadClientError> {
        let first = self.read_snapshot_once(client, last_event_sequence).await?;
        if first.coherent {
            return Ok(first);
        }
        tokio::time::sleep(INCOHERENT_SNAPSHOT_RETRY_DELAY).await;
        self.read_snapshot_once(client, last_event_sequence).await
    }

    async fn read_snapshot_once(
        &self,
        client: &PatinadClient,
        last_event_sequence: Option<u64>,
    ) -> Result<PatinadRuntimeReadSnapshot, PatinadClientError> {
        let (current_window, active_session) =
            tokio::try_join!(client.current_window(), client.active_session())?;
        let coherent = active_session.as_ref().is_none_or(|active| {
            current_window.is_afk
                || active
                    .exe_name
                    .eq_ignore_ascii_case(&current_window.exe_name)
        });
        Ok(PatinadRuntimeReadSnapshot {
            current_window,
            active_session,
            last_event_sequence,
            coherent,
        })
    }

    fn client(&self) -> Result<PatinadClient, PatinadClientError> {
        self.client_state.require().map_err(|_| {
            PatinadClientError::InvalidConfiguration(
                "patinad client is not configured for this profile".to_string(),
            )
        })
    }
}

enum ConnectionExit {
    Shutdown,
    Reconfigure,
}

struct RetryBackoff {
    current: Duration,
}

impl Default for RetryBackoff {
    fn default() -> Self {
        Self {
            current: INITIAL_RETRY_DELAY,
        }
    }
}

impl RetryBackoff {
    fn next_delay(&mut self) -> Duration {
        let delay = self.current;
        self.current = self.current.saturating_mul(2).min(MAX_RETRY_DELAY);
        delay
    }

    fn reset(&mut self) {
        self.current = INITIAL_RETRY_DELAY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_state_never_keeps_a_stale_error_after_a_snapshot() {
        let state = PatinadRuntimeState::default();
        state.connection_changed(
            PatinadRuntimeConnectionStatus::Reconnecting,
            Some(&PatinadClientError::Unauthorized),
        );
        state.snapshot_changed(snapshot("ghostty", Some("ghostty"), 1_000, Some(4)));

        let current = state.snapshot();
        assert!(current.error_code.is_none());
        assert!(current.error_message.is_none());
        assert_eq!(current.runtime.unwrap().last_event_sequence, Some(4));
    }

    #[test]
    fn runtime_state_drops_stale_live_data_after_disconnect() {
        let state = PatinadRuntimeState::default();
        state.snapshot_changed(snapshot("ghostty", Some("ghostty"), 1_000, Some(4)));

        state.connection_changed(
            PatinadRuntimeConnectionStatus::Reconnecting,
            Some(&PatinadClientError::Unreachable("offline".to_string())),
        );

        let current = state.snapshot();
        assert!(current.runtime.is_none());
        assert_eq!(current.error_code.as_deref(), Some("unreachable"));
    }

    #[test]
    fn snapshot_marks_cross_request_window_transition_as_incoherent() {
        let snapshot = snapshot("ghostty", Some("obsidian"), 1_000, None);

        assert!(!snapshot.coherent);
    }

    #[test]
    fn retry_backoff_is_bounded() {
        let mut backoff = RetryBackoff::default();

        assert_eq!(backoff.next_delay(), INITIAL_RETRY_DELAY);
        for _ in 0..10 {
            backoff.next_delay();
        }
        assert_eq!(backoff.next_delay(), MAX_RETRY_DELAY);
    }

    fn snapshot(
        current_exe: &str,
        active_exe: Option<&str>,
        sampled_at_ms: i64,
        last_event_sequence: Option<u64>,
    ) -> PatinadRuntimeReadSnapshot {
        let runtime_snapshot = crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshot {
            generation: 0,
            window: crate::platform::linux::foreground::WindowInfo {
                hwnd: "0x100".to_string(),
                root_owner_hwnd: "0x100".to_string(),
                process_id: 42,
                window_class: current_exe.to_string(),
                title: "Window".to_string(),
                exe_name: current_exe.to_string(),
                process_path: format!("/usr/bin/{current_exe}"),
                is_afk: false,
                idle_time_ms: 0,
            },
            status: crate::domain::tracking::TrackingStatusSnapshot::default(),
            sampled_at_ms,
            probe_status: crate::engine::tracking::runtime_snapshot::TrackingRuntimeProbeStatus::Ok,
            degraded_reason: None,
            probe_diagnostics:
                crate::engine::tracking::runtime_snapshot::TrackingRuntimeProbeDiagnostics::default(
                ),
        };
        PatinadRuntimeReadSnapshot {
            current_window: CurrentWindowResponse {
                exe_name: current_exe.to_string(),
                title: "Window".to_string(),
                process_id: 42,
                is_afk: false,
                idle_time_ms: 0,
                process_path: format!("/usr/bin/{current_exe}"),
                sampled_at_ms,
                runtime_snapshot,
            },
            active_session: active_exe.map(|exe_name| ActiveSessionResponse {
                id: 1,
                app_name: exe_name.to_string(),
                exe_name: exe_name.to_string(),
                window_title: Some("Window".to_string()),
                start_time: 900,
                end_time: None,
                duration: 100,
                continuity_group_start_time: 900,
                sampled_at_ms: 1_000,
            }),
            last_event_sequence,
            coherent: active_exe.is_none_or(|active| active.eq_ignore_ascii_case(current_exe)),
        }
    }
}
