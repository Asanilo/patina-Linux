use crate::data::repositories::app_settings;
use crate::domain::web_activity::WebActivityBridgeSnapshot;
use crate::engine::api::context::ApiRuntimeContext;
use crate::engine::api::types::{
    ApiResponse, DiagnosticsResponse, RouteResponse, TrackerRuntimeDiagnostics,
};
use crate::engine::tracking::runtime_snapshot::{
    TrackingRuntimeProbeDiagnostics, TrackingRuntimeProbeStatus,
};
use crate::platform::tracking_diagnostics::PlatformTrackingDiagnostics;

pub async fn get_diagnostics(context: &ApiRuntimeContext) -> RouteResponse {
    let platform = load_platform_tracking_diagnostics().await;
    let tracker_runtime = context.tracking_snapshot().map(|snapshot| {
        (
            snapshot.probe_status,
            snapshot.degraded_reason,
            snapshot.probe_diagnostics,
        )
    });
    let web_activity_bridge = load_web_activity_bridge_snapshot(context).await;

    RouteResponse {
        status: 200,
        body: serde_json::to_value(ApiResponse {
            data: build_diagnostics_response(platform, tracker_runtime, web_activity_bridge),
        })
        .unwrap_or_default(),
    }
}

pub(crate) fn build_diagnostics_response(
    platform: PlatformTrackingDiagnostics,
    tracker_runtime: Option<(
        TrackingRuntimeProbeStatus,
        Option<String>,
        TrackingRuntimeProbeDiagnostics,
    )>,
    web_activity_bridge: Option<WebActivityBridgeSnapshot>,
) -> DiagnosticsResponse {
    DiagnosticsResponse {
        window_tracking: platform.window_tracking,
        tracker_runtime: tracker_runtime.map(
            |(probe_status, degraded_reason, probe_diagnostics)| TrackerRuntimeDiagnostics {
                probe_status,
                degraded_reason,
                probe_diagnostics,
            },
        ),
        web_activity_bridge,
    }
}

async fn load_platform_tracking_diagnostics() -> PlatformTrackingDiagnostics {
    #[cfg(target_os = "linux")]
    {
        match tauri::async_runtime::spawn_blocking(crate::platform::tracking_diagnostics::current)
            .await
        {
            Ok(diagnostics) => diagnostics,
            Err(error) => {
                eprintln!("[api] failed to collect platform diagnostics: {error}");
                crate::platform::tracking_diagnostics::unavailable(
                    "platform-diagnostics-unavailable",
                )
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        crate::platform::tracking_diagnostics::current()
    }
}

async fn load_web_activity_bridge_snapshot(
    context: &ApiRuntimeContext,
) -> Option<WebActivityBridgeSnapshot> {
    let settings = app_settings::load_web_activity_settings(context.pool())
        .await
        .ok()?;
    context.web_activity_snapshot(&settings)
}
