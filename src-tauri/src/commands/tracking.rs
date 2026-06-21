use crate::domain::tracking::TrackingStatusSnapshot;
use crate::engine::tracking::runtime_snapshot::{
    TrackingRuntimeProbeDiagnostics, TrackingRuntimeProbeStatus, TrackingRuntimeSnapshotState,
};
use crate::engine::tracking::watchdog::{RuntimeHealthSnapshot, RuntimeHealthState};
#[cfg(target_os = "linux")]
use crate::platform::linux::foreground::WindowInfo;
use crate::platform::tracking_diagnostics::PlatformTrackingDiagnostics;
#[cfg(target_os = "windows")]
use crate::platform::windows::foreground::WindowInfo;
use serde::Serialize;
use std::sync::Arc;
use tauri::Manager;

#[derive(Clone, Debug, Serialize)]
pub struct CurrentTrackingSnapshot {
    pub window: WindowInfo,
    pub status: TrackingStatusSnapshot,
    pub sampled_at_ms: i64,
    pub probe_status: TrackingRuntimeProbeStatus,
    pub degraded_reason: Option<String>,
    pub probe_diagnostics: TrackingRuntimeProbeDiagnostics,
}

#[derive(Clone, Debug, Serialize)]
pub struct TrackerHealthRuntimeSnapshot {
    pub last_heartbeat_ms: Option<i64>,
    pub last_successful_sample_ms: Option<i64>,
    pub last_watchdog_seal_sample_ms: Option<i64>,
    pub platform_diagnostics: PlatformTrackingDiagnostics,
}

impl From<RuntimeHealthSnapshot> for TrackerHealthRuntimeSnapshot {
    fn from(snapshot: RuntimeHealthSnapshot) -> Self {
        Self {
            last_heartbeat_ms: snapshot.last_heartbeat_ms,
            last_successful_sample_ms: snapshot.last_successful_sample_ms,
            last_watchdog_seal_sample_ms: snapshot.last_watchdog_seal_sample_ms,
            platform_diagnostics: crate::platform::tracking_diagnostics::current(),
        }
    }
}

#[tauri::command]
pub fn get_current_active_window(app: tauri::AppHandle) -> Result<WindowInfo, String> {
    app.state::<TrackingRuntimeSnapshotState>()
        .snapshot()
        .map(|snapshot| snapshot.window)
        .ok_or_else(|| "tracking runtime snapshot is not ready".to_string())
}

#[tauri::command]
pub fn get_current_tracking_snapshot(
    app: tauri::AppHandle,
) -> Result<CurrentTrackingSnapshot, String> {
    let snapshot = app
        .state::<TrackingRuntimeSnapshotState>()
        .snapshot()
        .ok_or_else(|| "tracking runtime snapshot is not ready".to_string())?;

    Ok(CurrentTrackingSnapshot {
        window: snapshot.window,
        status: snapshot.status,
        sampled_at_ms: snapshot.sampled_at_ms,
        probe_status: snapshot.probe_status,
        degraded_reason: snapshot.degraded_reason,
        probe_diagnostics: snapshot.probe_diagnostics,
    })
}

#[tauri::command]
pub fn cmd_get_tracker_health_snapshot(
    app: tauri::AppHandle,
) -> Result<TrackerHealthRuntimeSnapshot, String> {
    let snapshot = app.state::<Arc<RuntimeHealthState>>().inner().snapshot();
    Ok(snapshot.into())
}

#[tauri::command]
pub fn cmd_set_afk_threshold(threshold_secs: u64) {
    #[cfg(target_os = "windows")]
    crate::platform::windows::foreground::cmd_set_afk_threshold(threshold_secs);
    #[cfg(target_os = "linux")]
    crate::platform::linux::foreground::cmd_set_afk_threshold(threshold_secs);
}
