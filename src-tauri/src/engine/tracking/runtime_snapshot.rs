use crate::domain::tracking::TrackingStatusSnapshot;
#[cfg(target_os = "linux")]
use crate::platform::linux::foreground::WindowInfo;
#[cfg(target_os = "windows")]
use crate::platform::windows::foreground::WindowInfo;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TrackingRuntimeProbeStatus {
    Ok,
    TimeoutFallback,
    TimeoutInactive,
    BackingOffFallback,
    BackingOffInactive,
    RecoveryAttemptedFallback,
    RecoveryAttemptedInactive,
    HardDegradedFallback,
    HardDegradedInactive,
    TaskFailedFallback,
    TaskFailedInactive,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct TrackingRuntimeProbeDiagnostics {
    pub last_successful_sample_at_ms: Option<i64>,
    pub fallback_started_at_ms: Option<i64>,
    pub fallback_count: u64,
    pub consecutive_fallback_count: u64,
    pub recovery_attempt_count: u64,
    pub last_recovery_attempt_at_ms: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TrackingRuntimeSnapshot {
    #[serde(skip)]
    pub(crate) generation: u64,
    pub window: WindowInfo,
    pub status: TrackingStatusSnapshot,
    pub sampled_at_ms: i64,
    pub probe_status: TrackingRuntimeProbeStatus,
    pub degraded_reason: Option<String>,
    pub probe_diagnostics: TrackingRuntimeProbeDiagnostics,
}

#[derive(Debug, Default)]
struct TrackingLifecycle {
    generation: u64,
    locked: bool,
    suspended: bool,
    shutting_down: bool,
    pending_stop: Option<(u64, i64, &'static str)>,
}

#[derive(Clone, Debug)]
pub struct TrackingRuntimeSnapshotState {
    inner: Arc<Mutex<Option<TrackingRuntimeSnapshot>>>,
    transition: Arc<tokio::sync::Mutex<()>>,
    lifecycle: Arc<Mutex<TrackingLifecycle>>,
}

impl Default for TrackingRuntimeSnapshotState {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            transition: Arc::new(tokio::sync::Mutex::new(())),
            lifecycle: Arc::new(Mutex::new(TrackingLifecycle::default())),
        }
    }
}

impl TrackingRuntimeSnapshotState {
    pub(crate) async fn lock_transition(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.transition.lock().await
    }

    pub(crate) fn lifecycle_generation(&self) -> u64 {
        self.lifecycle
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .generation
    }

    pub(crate) fn accepts_sample(&self, generation: u64) -> bool {
        let lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        lifecycle.generation == generation
            && !lifecycle.locked
            && !lifecycle.suspended
            && !lifecycle.shutting_down
            && lifecycle.pending_stop.is_none()
    }

    pub(crate) fn note_power_event(&self, event: &str, timestamp_ms: i64) {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match event {
            "lock" => lifecycle.locked = true,
            "unlock" => lifecycle.locked = false,
            "suspend" => lifecycle.suspended = true,
            "resume" => lifecycle.suspended = false,
            "shutdown" => lifecycle.shutting_down = true,
            _ => return,
        }
        lifecycle.generation = lifecycle.generation.wrapping_add(1);
        if matches!(event, "lock" | "suspend" | "shutdown") {
            let default_reason = match event {
                "lock" => "lock",
                "suspend" => "suspend",
                _ => "shutdown",
            };
            let (_, boundary, reason) = lifecycle
                .pending_stop
                .filter(|(_, boundary, _)| *boundary <= timestamp_ms)
                .unwrap_or((lifecycle.generation, timestamp_ms, default_reason));
            lifecycle.pending_stop = Some((lifecycle.generation, boundary, reason));
        }
        drop(lifecycle);
        self.invalidate_activity();
    }

    pub(crate) fn pending_stop(&self) -> Option<(u64, i64, &'static str)> {
        self.lifecycle
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pending_stop
    }

    pub(crate) fn acknowledge_stop(&self, stop: (u64, i64, &'static str)) {
        let mut lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if lifecycle.pending_stop == Some(stop) {
            lifecycle.pending_stop = None;
        }
    }

    pub(crate) fn invalidate_activity(&self) {
        let mut guard = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(snapshot) = guard.as_mut() {
            snapshot.status.is_tracking_active = false;
        }
    }

    pub fn replace(&self, snapshot: TrackingRuntimeSnapshot) {
        match self.inner.lock() {
            Ok(mut guard) => {
                *guard = Some(snapshot);
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                *guard = Some(snapshot);
            }
        }
    }

    pub fn snapshot(&self) -> Option<TrackingRuntimeSnapshot> {
        let mut snapshot = match self.inner.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }?;
        if !self.accepts_sample(snapshot.generation) {
            snapshot.status.is_tracking_active = false;
        }
        Some(snapshot)
    }

    pub fn clear(&self) {
        match self.inner.lock() {
            Ok(mut guard) => {
                *guard = None;
            }
            Err(poisoned) => {
                *poisoned.into_inner() = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_window() -> WindowInfo {
        WindowInfo {
            hwnd: "0x100".into(),
            root_owner_hwnd: "0x100".into(),
            process_id: 123,
            window_class: "Chrome_WidgetWin_1".into(),
            title: "Window".into(),
            exe_name: "QQ.exe".into(),
            process_path: r"C:\Program Files\QQ\QQ.exe".into(),
            is_afk: false,
            idle_time_ms: 0,
        }
    }

    #[test]
    fn snapshot_state_returns_latest_runtime_snapshot() {
        let state = TrackingRuntimeSnapshotState::default();
        let snapshot = TrackingRuntimeSnapshot {
            generation: 0,
            window: make_window(),
            status: TrackingStatusSnapshot::default(),
            sampled_at_ms: 123,
            probe_status: TrackingRuntimeProbeStatus::Ok,
            degraded_reason: None,
            probe_diagnostics: TrackingRuntimeProbeDiagnostics::default(),
        };

        state.replace(snapshot.clone());

        let loaded = state.snapshot().unwrap();
        assert_eq!(loaded.sampled_at_ms, 123);
        assert_eq!(loaded.probe_status, TrackingRuntimeProbeStatus::Ok);
        assert_eq!(loaded.window.exe_name, snapshot.window.exe_name);
    }

    #[test]
    fn clearing_snapshot_removes_stale_live_state() {
        let state = TrackingRuntimeSnapshotState::default();
        state.replace(TrackingRuntimeSnapshot {
            generation: 0,
            window: make_window(),
            status: TrackingStatusSnapshot::default(),
            sampled_at_ms: 123,
            probe_status: TrackingRuntimeProbeStatus::Ok,
            degraded_reason: None,
            probe_diagnostics: TrackingRuntimeProbeDiagnostics::default(),
        });

        state.clear();

        assert!(state.snapshot().is_none());
    }

    #[test]
    fn lifecycle_events_invalidate_old_snapshots_until_the_session_is_active_again() {
        let state = TrackingRuntimeSnapshotState::default();
        let mut snapshot = TrackingRuntimeSnapshot {
            generation: state.lifecycle_generation(),
            window: make_window(),
            status: TrackingStatusSnapshot {
                is_tracking_active: true,
                ..Default::default()
            },
            sampled_at_ms: 1_000,
            probe_status: TrackingRuntimeProbeStatus::Ok,
            degraded_reason: None,
            probe_diagnostics: TrackingRuntimeProbeDiagnostics::default(),
        };
        state.replace(snapshot.clone());

        state.note_power_event("lock", 5_000);
        assert!(!state.snapshot().unwrap().status.is_tracking_active);
        let stop = state.pending_stop().unwrap();
        state.note_power_event("suspend", 6_000);
        state.acknowledge_stop(stop);
        assert!(
            state.pending_stop().is_some(),
            "stale handlers cannot acknowledge a newer lifecycle generation"
        );

        let stop = state.pending_stop().unwrap();
        state.acknowledge_stop(stop);
        state.note_power_event("unlock", 7_000);
        assert!(!state.accepts_sample(state.lifecycle_generation()));
        state.note_power_event("resume", 8_000);
        assert!(state.accepts_sample(state.lifecycle_generation()));
        snapshot.generation = state.lifecycle_generation();
        state.replace(snapshot);
        assert!(state.snapshot().unwrap().status.is_tracking_active);
    }
}
