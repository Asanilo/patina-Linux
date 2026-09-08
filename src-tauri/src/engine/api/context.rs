use crate::domain::{settings::WebActivitySettings, web_activity::WebActivityBridgeSnapshot};
use crate::engine::runtime_context::RuntimeContext;
use crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshot;
use std::sync::Arc;

pub trait ApiRuntimeStateProvider: Send + Sync {
    fn tracking_snapshot(&self) -> Option<TrackingRuntimeSnapshot>;

    fn tracking_runtime_state(
        &self,
    ) -> Option<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState> {
        None
    }

    fn web_activity_snapshot(
        &self,
        settings: &WebActivitySettings,
        now_ms: i64,
    ) -> Option<WebActivityBridgeSnapshot>;

    fn tools_runtime_ready(&self) -> bool;
}

#[cfg(test)]
#[derive(Debug, Default)]
pub struct UnavailableApiRuntimeState;

#[cfg(test)]
impl ApiRuntimeStateProvider for UnavailableApiRuntimeState {
    fn tracking_snapshot(&self) -> Option<TrackingRuntimeSnapshot> {
        None
    }

    fn web_activity_snapshot(
        &self,
        _settings: &WebActivitySettings,
        _now_ms: i64,
    ) -> Option<WebActivityBridgeSnapshot> {
        None
    }

    fn tools_runtime_ready(&self) -> bool {
        false
    }
}

#[derive(Clone)]
pub struct ApiRuntimeContext {
    runtime: RuntimeContext,
    version: String,
    platform: String,
    state: Arc<dyn ApiRuntimeStateProvider>,
    event_sink: Option<Arc<dyn crate::engine::runtime_event::RuntimeEventSink>>,
    runtime_control: Option<Arc<dyn crate::engine::api::runtime_control::ApiRuntimeControl>>,
    activity_import_owner:
        Option<Arc<dyn crate::engine::api::activity_import_owner::ActivityImportOwner>>,
    tools_owner: Option<Arc<crate::engine::tools::ToolsRuntimeOwner>>,
}

impl ApiRuntimeContext {
    #[cfg(test)]
    pub fn new(runtime: RuntimeContext) -> Self {
        Self::with_state(
            runtime,
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            Arc::new(UnavailableApiRuntimeState),
        )
    }

    #[cfg(test)]
    pub fn with_state(
        runtime: RuntimeContext,
        version: impl Into<String>,
        platform: impl Into<String>,
        state: Arc<dyn ApiRuntimeStateProvider>,
    ) -> Self {
        Self::with_state_and_events(runtime, version, platform, state, None)
    }

    pub fn with_state_and_events(
        runtime: RuntimeContext,
        version: impl Into<String>,
        platform: impl Into<String>,
        state: Arc<dyn ApiRuntimeStateProvider>,
        event_sink: Option<Arc<dyn crate::engine::runtime_event::RuntimeEventSink>>,
    ) -> Self {
        Self {
            runtime,
            version: version.into(),
            platform: platform.into(),
            state,
            event_sink,
            runtime_control: None,
            activity_import_owner: None,
            tools_owner: None,
        }
    }

    pub fn with_runtime_control(
        mut self,
        runtime_control: Arc<dyn crate::engine::api::runtime_control::ApiRuntimeControl>,
    ) -> Self {
        self.runtime_control = Some(runtime_control);
        self
    }

    pub fn with_tools_owner(
        mut self,
        tools_owner: Arc<crate::engine::tools::ToolsRuntimeOwner>,
    ) -> Self {
        self.tools_owner = Some(tools_owner);
        self
    }

    pub fn with_activity_import_owner(
        mut self,
        owner: Arc<dyn crate::engine::api::activity_import_owner::ActivityImportOwner>,
    ) -> Self {
        self.activity_import_owner = Some(owner);
        self
    }

    pub fn pool(&self) -> &sqlx::Pool<sqlx::Sqlite> {
        self.runtime.pool()
    }

    pub fn now_ms(&self) -> i64 {
        self.runtime.now_ms()
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn platform(&self) -> &str {
        &self.platform
    }

    pub fn tracking_snapshot(&self) -> Option<TrackingRuntimeSnapshot> {
        self.state.tracking_snapshot()
    }

    pub fn tracking_runtime_state(
        &self,
    ) -> Option<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState> {
        self.state.tracking_runtime_state()
    }

    pub fn web_activity_snapshot(
        &self,
        settings: &WebActivitySettings,
    ) -> Option<WebActivityBridgeSnapshot> {
        self.state.web_activity_snapshot(settings, self.now_ms())
    }

    pub fn tools_runtime_ready(&self) -> bool {
        self.state.tools_runtime_ready()
    }

    pub fn emit_tracking_data_changed(&self, reason: &str) {
        let Some(event_sink) = self.event_sink.as_ref() else {
            return;
        };
        let changed_at_ms = self.now_ms().max(0) as u64;
        if let Err(error) = event_sink.emit(
            crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged {
                reason: reason.to_string(),
                changed_at_ms,
            },
        ) {
            eprintln!("[api] failed to emit data change event: {error}");
        }
    }

    pub fn runtime_control(
        &self,
    ) -> Option<&Arc<dyn crate::engine::api::runtime_control::ApiRuntimeControl>> {
        self.runtime_control.as_ref()
    }

    pub fn activity_import_owner(
        &self,
    ) -> Option<&Arc<dyn crate::engine::api::activity_import_owner::ActivityImportOwner>> {
        self.activity_import_owner.as_ref()
    }

    pub fn tools_owner(&self) -> Option<&Arc<crate::engine::tools::ToolsRuntimeOwner>> {
        self.tools_owner.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct FixedClock(i64);

    impl crate::engine::runtime_context::RuntimeClock for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    async fn test_context(
        label: &str,
    ) -> (std::path::PathBuf, sqlx::SqlitePool, ApiRuntimeContext) {
        let root = std::env::temp_dir().join(format!(
            "patina-api-context-{label}-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ));
        let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(
            &root.join("patina.db"),
            true,
        )
        .await
        .unwrap();
        let runtime = crate::engine::runtime_context::RuntimeContext::new(
            pool.clone(),
            Arc::new(FixedClock(1_782_000_000_000)),
        );
        (root, pool, ApiRuntimeContext::new(runtime))
    }

    #[tokio::test]
    async fn tracker_settings_handler_runs_without_tauri_app() {
        let (root, pool, context) = test_context("settings").await;

        let response = crate::engine::api::handlers::settings::get_tracker_settings(&context).await;

        assert_eq!(response.status, 200);
        assert_eq!(response.body["data"]["idle_timeout_secs"], 180);
        pool.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn database_read_handlers_share_one_host_neutral_context() {
        let (root, pool, context) = test_context("read-handlers").await;

        let responses = [
            crate::engine::api::handlers::sessions::get_sessions(&context, None).await,
            crate::engine::api::handlers::sessions::get_active_session(&context).await,
            crate::engine::api::handlers::sessions::get_summary_today(&context).await,
            crate::engine::api::handlers::sessions::get_summary_week(&context).await,
            crate::engine::api::handlers::trend::get_trend(&context, None).await,
            crate::engine::api::handlers::web_activity::get_web_activity(&context, None).await,
            crate::engine::api::handlers::apps::get_apps(&context).await,
            crate::engine::api::handlers::settings::get_tracker_settings(&context).await,
            crate::engine::api::handlers::tools::get_tools_snapshot(&context).await,
        ];

        assert!(responses.iter().all(|response| response.status == 200));
        pool.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn unavailable_live_state_is_reported_without_false_readiness() {
        let (root, pool, context) = test_context("unavailable-live-state").await;

        let current = crate::engine::api::handlers::health::get_current(&context);
        let diagnostics =
            crate::engine::api::handlers::diagnostics::get_diagnostics(&context).await;

        assert_eq!(current.status, 503);
        assert_eq!(diagnostics.status, 200);
        assert!(diagnostics.body["data"]["tracker_runtime"].is_null());
        assert!(diagnostics.body["data"]["web_activity_bridge"].is_null());
        pool.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}
