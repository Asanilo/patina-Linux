use crate::domain::{settings::WebActivitySettings, web_activity::WebActivityBridgeSnapshot};
use crate::engine::api::context::{ApiRuntimeContext, ApiRuntimeStateProvider};
use crate::engine::runtime_context::RuntimeContext;
use crate::engine::tracking::runtime_snapshot::{
    TrackingRuntimeSnapshot, TrackingRuntimeSnapshotState,
};
use std::sync::Arc;

struct DaemonApiRuntimeState {
    tracking: Option<Arc<TrackingRuntimeSnapshotState>>,
}

impl ApiRuntimeStateProvider for DaemonApiRuntimeState {
    fn tracking_snapshot(&self) -> Option<TrackingRuntimeSnapshot> {
        self.tracking.as_ref().and_then(|state| state.snapshot())
    }

    fn web_activity_snapshot(
        &self,
        _settings: &WebActivitySettings,
        _now_ms: i64,
    ) -> Option<WebActivityBridgeSnapshot> {
        None
    }
}

pub fn build_context(
    runtime: RuntimeContext,
    tracking: Option<Arc<TrackingRuntimeSnapshotState>>,
) -> ApiRuntimeContext {
    ApiRuntimeContext::with_state(
        runtime,
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        Arc::new(DaemonApiRuntimeState { tracking }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::tracking::TrackingStatusSnapshot;
    use crate::engine::tracking::runtime_snapshot::{
        TrackingRuntimeProbeDiagnostics, TrackingRuntimeProbeStatus, TrackingRuntimeSnapshot,
    };
    #[cfg(target_os = "linux")]
    use crate::platform::linux::foreground::WindowInfo;
    #[cfg(target_os = "windows")]
    use crate::platform::windows::foreground::WindowInfo;

    fn snapshot() -> TrackingRuntimeSnapshot {
        TrackingRuntimeSnapshot {
            window: WindowInfo {
                hwnd: "0x100".into(),
                root_owner_hwnd: "0x100".into(),
                process_id: 42,
                window_class: "com.mitchellh.ghostty".into(),
                title: "patinad".into(),
                exe_name: "ghostty".into(),
                process_path: "/usr/bin/ghostty".into(),
                is_afk: false,
                idle_time_ms: 10,
            },
            status: TrackingStatusSnapshot::default(),
            sampled_at_ms: 1_000,
            probe_status: TrackingRuntimeProbeStatus::Ok,
            degraded_reason: None,
            probe_diagnostics: TrackingRuntimeProbeDiagnostics::default(),
        }
    }

    #[tokio::test]
    async fn daemon_context_exposes_owned_tracking_snapshot() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let state = Arc::new(TrackingRuntimeSnapshotState::default());
        state.replace(snapshot());
        let context = build_context(RuntimeContext::system(pool.clone()), Some(state));

        let current = crate::engine::api::handlers::health::get_current(&context);

        assert_eq!(current.status, 200);
        assert_eq!(current.body["data"]["exe_name"], "ghostty");
        assert_eq!(current.body["data"]["title"], "patinad");
        pool.close().await;
    }

    #[tokio::test]
    async fn daemon_context_without_tracking_keeps_live_state_unavailable() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        let context = build_context(RuntimeContext::system(pool.clone()), None);

        assert_eq!(
            crate::engine::api::handlers::health::get_current(&context).status,
            503
        );
        pool.close().await;
    }
}
