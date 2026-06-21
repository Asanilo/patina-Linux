use crate::domain::tracking::{
    evaluate_sustained_participation_signal, sustained_participation_app_identity,
    AudioProbeStatus, AudioSessionFact, AudioSignalState, AudioSnapshot,
    SustainedParticipationSignalMatchResult, SustainedParticipationSignalSnapshot,
    SustainedParticipationSignalSource,
};
use crate::platform::linux::foreground::{self, WindowInfo};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock,
};
use tokio::{
    task::spawn_blocking,
    time::{sleep, timeout, Duration},
};

const AUDIO_SESSION_QUERY_TIMEOUT_SECS: u64 = 2;
const AUDIO_SNAPSHOT_TTL_MS: i64 = 15_000;
const AUDIO_RECONCILE_INTERVAL_SECS: u64 = 10;
const AUDIO_SESSION_LIMIT: usize = 64;
const AUDIO_PROBE_LOG_THROTTLE_MS: i64 = 60_000;

static AUDIO_SIGNAL_SOURCE: OnceLock<Arc<AudioSignalSourceState>> = OnceLock::new();

#[derive(Debug)]
struct AudioSignalSourceState {
    snapshot: Mutex<AudioSnapshot>,
    probe_in_flight: Arc<AtomicBool>,
}

struct AudioProbeInFlightGuard {
    probe_in_flight: Arc<AtomicBool>,
}

pub fn start_signal_source() {
    let state = AUDIO_SIGNAL_SOURCE
        .get_or_init(|| Arc::new(AudioSignalSourceState::new()))
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

    let now_ms = now_ms();
    let Some(state) = AUDIO_SIGNAL_SOURCE.get() else {
        return SustainedParticipationSignalSnapshot::default();
    };

    state.resolve_signal_for_window(window, now_ms)
}

impl AudioSignalSourceState {
    fn new() -> Self {
        Self {
            snapshot: Mutex::new(AudioSnapshot::unknown(now_ms(), AUDIO_SNAPSHOT_TTL_MS)),
            probe_in_flight: Arc::new(AtomicBool::new(false)),
        }
    }

    async fn run(&self) {
        loop {
            self.reconcile_once().await;
            sleep(Duration::from_secs(AUDIO_RECONCILE_INTERVAL_SECS)).await;
        }
    }

    async fn reconcile_once(&self) {
        let started_at_ms = now_ms();
        if self.probe_in_flight.swap(true, Ordering::AcqRel) {
            self.replace_snapshot(self.unavailable_snapshot(AudioProbeStatus::BackingOff));
            return;
        }

        let probe_in_flight = self.probe_in_flight.clone();
        let query = spawn_blocking(move || {
            let _guard = AudioProbeInFlightGuard { probe_in_flight };
            query_audio_snapshot(started_at_ms)
        });

        let snapshot =
            match timeout(Duration::from_secs(AUDIO_SESSION_QUERY_TIMEOUT_SECS), query).await {
                Ok(Ok(Ok(snapshot))) => snapshot,
                Ok(Ok(Err(error))) => {
                    log_audio_probe_error(format!("failed to reconcile audio sessions: {error}"));
                    self.unavailable_snapshot(AudioProbeStatus::LinuxApiFailed)
                }
                Ok(Err(error)) => {
                    log_audio_probe_error(format!("audio session query task failed: {error}"));
                    self.unavailable_snapshot(AudioProbeStatus::LinuxApiFailed)
                }
                Err(_) => {
                    log_audio_probe_error(format!(
                        "timed out reconciling audio sessions after {}s",
                        AUDIO_SESSION_QUERY_TIMEOUT_SECS
                    ));
                    self.unavailable_snapshot(AudioProbeStatus::Timeout)
                }
            };

        self.replace_snapshot(snapshot);
    }

    fn unavailable_snapshot(&self, status: AudioProbeStatus) -> AudioSnapshot {
        let last_success_at_ms = self
            .snapshot
            .lock()
            .ok()
            .and_then(|snapshot| snapshot.last_success_at_ms);
        AudioSnapshot::probe_unavailable(
            now_ms(),
            AUDIO_SNAPSHOT_TTL_MS,
            status,
            last_success_at_ms,
        )
    }

    fn replace_snapshot(&self, snapshot: AudioSnapshot) {
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

        if snapshot.signal_state(now_ms) != AudioSignalState::Active {
            return SustainedParticipationSignalSnapshot::default();
        }

        let mut fallback_active: Option<SustainedParticipationSignalSnapshot> = None;
        for session in snapshot.sessions {
            let signal = session_fact_to_signal(&session);
            match evaluate_sustained_participation_signal(
                &window.exe_name,
                &window.process_path,
                &signal,
            )
            .match_result
            {
                SustainedParticipationSignalMatchResult::Matched => return signal,
                SustainedParticipationSignalMatchResult::IdentityMismatch
                | SustainedParticipationSignalMatchResult::Inactive => {
                    if fallback_active.is_none() {
                        fallback_active = Some(signal);
                    }
                }
                SustainedParticipationSignalMatchResult::Unavailable => {}
            }
        }

        fallback_active.unwrap_or_default()
    }
}

impl Drop for AudioProbeInFlightGuard {
    fn drop(&mut self) {
        self.probe_in_flight.store(false, Ordering::Release);
    }
}

fn query_audio_snapshot(now_ms: i64) -> Result<AudioSnapshot, String> {
    let sessions = query_pulseaudio_sessions()?;
    let sessions: Vec<AudioSessionFact> = sessions.into_iter().take(AUDIO_SESSION_LIMIT).collect();

    if sessions.is_empty() {
        return Ok(AudioSnapshot::empty_success(now_ms, AUDIO_SNAPSHOT_TTL_MS));
    }

    Ok(AudioSnapshot {
        generated_at_ms: now_ms,
        last_success_at_ms: Some(now_ms),
        last_error_at_ms: None,
        freshness_deadline_ms: now_ms.saturating_add(AUDIO_SNAPSHOT_TTL_MS),
        probe_status: AudioProbeStatus::Ok,
        sessions,
    })
}

fn query_pulseaudio_sessions() -> Result<Vec<AudioSessionFact>, String> {
    use libpulse_binding::context::{Context, FlagSet as ContextFlagSet};
    use libpulse_binding::mainloop::threaded::Mainloop;
    use libpulse_binding::operation::State as OpState;
    use std::sync::{Arc as StdArc, Mutex as StdMutex};
    use std::time::Instant;

    let result: StdArc<StdMutex<Vec<AudioSessionFact>>> = StdArc::new(StdMutex::new(Vec::new()));
    let done: StdArc<StdMutex<bool>> = StdArc::new(StdMutex::new(false));

    let mut mainloop = Mainloop::new().ok_or("failed to create PulseAudio mainloop")?;
    let result_clone = result.clone();
    let done_clone = done.clone();

    let mut context = Context::new(&mainloop, "patina-audio-probe")
        .ok_or("failed to create PulseAudio context")?;

    context
        .connect(None, ContextFlagSet::NOFLAGS, None)
        .map_err(|e| format!("failed to connect to PulseAudio: {e:?}"))?;

    mainloop
        .start()
        .map_err(|e| format!("failed to start PulseAudio mainloop: {e:?}"))?;

    let query_result = (|| {
        // Wait for context to be ready
        let start = Instant::now();
        loop {
            match context.get_state() {
                libpulse_binding::context::State::Ready => break,
                libpulse_binding::context::State::Failed => {
                    return Err("PulseAudio connection failed".into());
                }
                libpulse_binding::context::State::Terminated => {
                    return Err("PulseAudio connection terminated".into());
                }
                _ => {}
            }
            if start.elapsed().as_secs() > 2 {
                return Err("PulseAudio connection timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let result_for_cb = result_clone;
        let done_for_cb = done_clone;

        let op = context.introspect().get_sink_input_info_list(move |list| {
            use libpulse_binding::callbacks::ListResult;

            if let ListResult::Item(item) = list {
                if let Some(pid_str) = item.proplist.get_str("application.process.id") {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        let exe_name = foreground::get_process_exe_name(pid);
                        let process_path = foreground::get_process_path(pid);
                        let source_identity =
                            sustained_participation_app_identity(&exe_name, &process_path);

                        if let Ok(mut sessions) = result_for_cb.lock() {
                            sessions.push(AudioSessionFact {
                                session_id: format!("{}:{}", pid, exe_name.to_ascii_lowercase()),
                                process_id: pid,
                                exe_name,
                                process_path: if process_path.trim().is_empty() {
                                    None
                                } else {
                                    Some(process_path)
                                },
                                source_identity,
                                state: AudioSignalState::Active,
                                first_observed_at_ms: now_ms(),
                                last_observed_at_ms: now_ms(),
                            });
                        }
                    }
                }
            } else if let ListResult::End = list {
                if let Ok(mut d) = done_for_cb.lock() {
                    *d = true;
                }
            }
        });

        let start = Instant::now();
        loop {
            match op.get_state() {
                OpState::Done => break,
                OpState::Cancelled => return Err("PulseAudio query cancelled".into()),
                _ => {}
            }
            if start.elapsed().as_secs() > 2 {
                return Err("PulseAudio query timed out".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let sessions = result.lock().map(|s| s.clone()).unwrap_or_default();
        Ok(sessions)
    })();

    context.disconnect();
    mainloop.stop();

    query_result
}

fn session_fact_to_signal(session: &AudioSessionFact) -> SustainedParticipationSignalSnapshot {
    SustainedParticipationSignalSnapshot {
        is_available: true,
        is_active: session.state == AudioSignalState::Active,
        signal_source: Some(SustainedParticipationSignalSource::AudioSession),
        source_app_id: Some(session.exe_name.clone()),
        source_app_identity: session.source_identity,
        playback_type: None,
    }
}

fn log_audio_probe_error(message: String) {
    static LAST_LOGGED_AT_MS: OnceLock<Mutex<i64>> = OnceLock::new();
    let now_ms = now_ms();
    let last_logged_at_ms = LAST_LOGGED_AT_MS.get_or_init(|| Mutex::new(0));

    if let Ok(mut last_logged_at_ms) = last_logged_at_ms.lock() {
        if now_ms.saturating_sub(*last_logged_at_ms) < AUDIO_PROBE_LOG_THROTTLE_MS {
            return;
        }

        *last_logged_at_ms = now_ms;
    }

    eprintln!("[audio] {message}");
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

    #[test]
    #[ignore = "requires a running PulseAudio or pipewire-pulse user service"]
    fn live_pulseaudio_query_completes_when_compat_server_is_available() {
        let sessions = query_pulseaudio_sessions();

        assert!(
            sessions.is_ok(),
            "expected PulseAudio compatibility query to complete, got {sessions:?}"
        );
    }
}
