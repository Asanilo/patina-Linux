use crate::engine::runtime_context::RuntimeContext;

#[derive(Clone)]
pub struct ApiRuntimeContext {
    runtime: RuntimeContext,
}

impl ApiRuntimeContext {
    pub fn new(runtime: RuntimeContext) -> Self {
        Self { runtime }
    }

    pub fn pool(&self) -> &sqlx::Pool<sqlx::Sqlite> {
        self.runtime.pool()
    }

    pub fn now_ms(&self) -> i64 {
        self.runtime.now_ms()
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
        ];

        assert!(responses.iter().all(|response| response.status == 200));
        pool.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}
