pub mod ai;
pub mod apps;
pub mod diagnostics;
pub mod health;
pub mod openapi;
pub mod sessions;
pub mod settings;
pub mod tools;
pub mod trend;
pub mod web_activity;

#[cfg(test)]
mod diagnostics_api_contract_tests {
    use crate::domain::web_activity::WebActivityBridgeSnapshot;
    use crate::engine::tracking::runtime_snapshot::{
        TrackingRuntimeProbeDiagnostics, TrackingRuntimeProbeStatus,
    };
    use crate::platform::tracking_diagnostics::{
        PlatformTrackingDiagnostics, WindowTrackingDiagnostics,
    };

    #[test]
    fn diagnostics_response_exposes_platform_runtime_and_browser_bridge_state() {
        let response = super::diagnostics::build_diagnostics_response(
            PlatformTrackingDiagnostics {
                window_tracking: WindowTrackingDiagnostics {
                    status: "unavailable".into(),
                    reason: Some("gnome-extension-dbus-unavailable".into()),
                    provider: "gnome-shell-extension".into(),
                    session_type: Some("wayland".into()),
                    desktop: Some("GNOME".into()),
                },
            },
            Some((
                TrackingRuntimeProbeStatus::TimeoutFallback,
                Some("probe-timeout".into()),
                TrackingRuntimeProbeDiagnostics {
                    last_successful_sample_at_ms: Some(1000),
                    fallback_started_at_ms: Some(2000),
                    fallback_count: 3,
                    consecutive_fallback_count: 2,
                    recovery_attempt_count: 1,
                    last_recovery_attempt_at_ms: Some(2500),
                },
            )),
            Some(WebActivityBridgeSnapshot {
                enabled: true,
                connected: false,
                browser_client_id: Some("zen-profile".into()),
                browser_kind: Some("firefox".into()),
                extension_version: Some("0.1.0".into()),
                last_activity_at_ms: Some(3000),
            }),
        );

        assert_eq!(response.window_tracking.status, "unavailable");
        assert_eq!(
            response.window_tracking.reason.as_deref(),
            Some("gnome-extension-dbus-unavailable")
        );
        assert_eq!(
            response
                .tracker_runtime
                .as_ref()
                .map(|runtime| runtime.probe_status),
            Some(TrackingRuntimeProbeStatus::TimeoutFallback)
        );
        assert_eq!(
            response
                .web_activity_bridge
                .as_ref()
                .and_then(|bridge| bridge.browser_kind.as_deref()),
            Some("firefox")
        );
    }
}
