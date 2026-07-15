use crate::domain::{settings::WebActivitySettings, web_activity::WebActivityBridgeSnapshot};
use crate::engine::api::context::{ApiRuntimeContext, ApiRuntimeStateProvider};
use crate::engine::runtime_context::RuntimeContext;
use crate::engine::tracking::runtime_snapshot::{
    TrackingRuntimeSnapshot, TrackingRuntimeSnapshotState,
};
use crate::engine::web_activity::WebActivityRuntimeState;
use sqlx::{Pool, Sqlite};
use std::sync::Arc;
use tauri::Manager;

struct DesktopApiRuntimeState {
    app: tauri::AppHandle,
}

impl ApiRuntimeStateProvider for DesktopApiRuntimeState {
    fn tracking_snapshot(&self) -> Option<TrackingRuntimeSnapshot> {
        self.app
            .try_state::<TrackingRuntimeSnapshotState>()
            .and_then(|state| state.snapshot())
    }

    fn web_activity_snapshot(
        &self,
        settings: &WebActivitySettings,
        now_ms: i64,
    ) -> Option<WebActivityBridgeSnapshot> {
        self.app
            .try_state::<WebActivityRuntimeState>()
            .map(|state| state.snapshot(settings, now_ms))
    }
}

pub fn build_context(app: &tauri::AppHandle, pool: Pool<Sqlite>) -> ApiRuntimeContext {
    ApiRuntimeContext::with_state(
        RuntimeContext::system(pool),
        app.package_info().version.to_string(),
        std::env::consts::OS,
        Arc::new(DesktopApiRuntimeState { app: app.clone() }),
    )
}
