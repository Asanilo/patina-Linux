use sqlx::{Pool, Sqlite};
use std::sync::Arc;

pub trait RuntimeClock: Send + Sync {
    fn now_ms(&self) -> i64;
}

#[derive(Debug, Default)]
pub struct SystemRuntimeClock;

impl RuntimeClock for SystemRuntimeClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct RuntimeContext {
    pool: Pool<Sqlite>,
    clock: Arc<dyn RuntimeClock>,
}

impl RuntimeContext {
    pub fn new(pool: Pool<Sqlite>, clock: Arc<dyn RuntimeClock>) -> Self {
        Self { pool, clock }
    }

    pub fn system(pool: Pool<Sqlite>) -> Self {
        Self::new(pool, Arc::new(SystemRuntimeClock))
    }

    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    pub fn now_ms(&self) -> i64 {
        self.clock.now_ms()
    }
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn test_block_on<F: std::future::Future>(future: F) -> F::Output {
    #[cfg(feature = "desktop")]
    {
        tauri::async_runtime::block_on(future)
    }
    #[cfg(not(feature = "desktop"))]
    {
        static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
        RUNTIME
            .get_or_init(|| tokio::runtime::Runtime::new().expect("test runtime"))
            .block_on(future)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    struct FixedClock(i64);

    impl RuntimeClock for FixedClock {
        fn now_ms(&self) -> i64 {
            self.0
        }
    }

    #[tokio::test]
    async fn exposes_pool_and_injected_time() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let context = RuntimeContext::new(pool.clone(), Arc::new(FixedClock(42_000)));

        assert_eq!(context.now_ms(), 42_000);
        assert_eq!(context.pool().size(), pool.size());

        context.pool().close().await;
    }
}
