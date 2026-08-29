use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::watch;

use crate::engine::api::types::{ActiveSessionResponse, CurrentWindowResponse};
use crate::engine::runtime_event::{RuntimeEvent, RuntimeEventEnvelope};
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
    client: PatinadClient,
    output: Arc<dyn PatinadRuntimeOutput>,
}

impl PatinadRuntimeAdapter {
    pub fn new(client: PatinadClient, output: Arc<dyn PatinadRuntimeOutput>) -> Self {
        Self { client, output }
    }

    pub async fn synchronize_once(
        &self,
        last_event_sequence: Option<u64>,
    ) -> Result<PatinadRuntimeReadSnapshot, PatinadClientError> {
        self.client.negotiate_tracking_owner().await?;
        self.read_snapshot(last_event_sequence).await
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
        self.client.negotiate_tracking_owner().await?;
        let mut events = self.client.open_event_stream(*cursor).await?;
        let mut snapshot = self.read_snapshot(*cursor).await?;
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
                    snapshot = self.read_snapshot(*cursor).await?;
                    self.output.snapshot_changed(snapshot.clone());
                    self.output.tracking_data_changed(&envelope);
                }
                PatinadStreamEvent::ResyncRequired { reason, missed } => {
                    *cursor = None;
                    snapshot = self.read_snapshot(*cursor).await?;
                    self.output.snapshot_changed(snapshot.clone());
                    self.output.resync_required(&reason, missed);
                }
                PatinadStreamEvent::Ignored { .. } => {}
            }
        }
    }

    async fn read_snapshot(
        &self,
        last_event_sequence: Option<u64>,
    ) -> Result<PatinadRuntimeReadSnapshot, PatinadClientError> {
        let first = self.read_snapshot_once(last_event_sequence).await?;
        if first.coherent {
            return Ok(first);
        }
        tokio::time::sleep(INCOHERENT_SNAPSHOT_RETRY_DELAY).await;
        self.read_snapshot_once(last_event_sequence).await
    }

    async fn read_snapshot_once(
        &self,
        last_event_sequence: Option<u64>,
    ) -> Result<PatinadRuntimeReadSnapshot, PatinadClientError> {
        let (current_window, active_session) =
            tokio::try_join!(self.client.current_window(), self.client.active_session())?;
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
}

enum ConnectionExit {
    Shutdown,
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
        PatinadRuntimeReadSnapshot {
            current_window: CurrentWindowResponse {
                exe_name: current_exe.to_string(),
                title: "Window".to_string(),
                process_id: 42,
                is_afk: false,
                idle_time_ms: 0,
                process_path: format!("/usr/bin/{current_exe}"),
                sampled_at_ms,
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
