//! Daemon-owned analytical connections. No migrations, writes or unbounded queue.
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqliteConnection, SqlitePool,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub const MAX_ANALYTICAL_READS: u32 = 2;

#[derive(Clone, Debug)]
pub struct AnalyticalReads {
    pool: SqlitePool,
    admission: Arc<Semaphore>,
    stopping: Arc<AtomicBool>,
}

pub struct AnalyticalRead {
    pool: SqlitePool,
    _permit: Option<OwnedSemaphorePermit>,
}

impl AnalyticalRead {
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Explicit compatibility for embedded hosts and in-memory test contexts.
    pub(crate) fn shared_pool(pool: SqlitePool) -> Self {
        Self {
            pool,
            _permit: None,
        }
    }
}

impl AnalyticalReads {
    pub async fn open(writer: &SqlitePool) -> Result<Self, String> {
        Self::open_with_deadline(writer, patina_protocol::read_budget::ANALYTICS.query).await
    }

    async fn open_with_deadline(writer: &SqlitePool, deadline: Duration) -> Result<Self, String> {
        let path = writer.connect_options().get_filename().to_path_buf();
        if path.as_os_str().is_empty()
            || path == std::path::Path::new(":memory:")
            || !path.is_file()
        {
            return Err("analytical reads require the existing owner database file".into());
        }
        // Only the writer owner may change journaling, before exposing the API.
        let mode: String = sqlx::query_scalar("PRAGMA journal_mode=WAL")
            .fetch_one(writer)
            .await
            .map_err(|e| format!("cannot enable analytical WAL reads: {e}"))?;
        if !mode.eq_ignore_ascii_case("wal") {
            return Err("analytical reads require WAL journaling".into());
        }
        let stopping = Arc::new(AtomicBool::new(false));
        let on_connect = stopping.clone();
        let on_acquire = stopping.clone();
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .read_only(true)
            .pragma("query_only", "ON")
            .pragma("foreign_keys", "ON")
            .busy_timeout(Duration::from_secs(1));
        let pool = SqlitePoolOptions::new()
            .min_connections(MAX_ANALYTICAL_READS)
            .max_connections(MAX_ANALYTICAL_READS)
            .acquire_timeout(Duration::from_secs(3))
            .after_connect(move |connection, _| {
                let stopping = on_connect.clone();
                Box::pin(
                    async move { install_progress_budget(connection, stopping, deadline).await },
                )
            })
            .before_acquire(move |connection, _| {
                let stopping = on_acquire.clone();
                Box::pin(async move {
                    install_progress_budget(connection, stopping, deadline).await?;
                    Ok(true)
                })
            })
            .connect_with(options)
            .await
            .map_err(|e| format!("cannot open analytical read pool: {e}"))?;
        Ok(Self {
            pool,
            admission: Arc::new(Semaphore::new(MAX_ANALYTICAL_READS as usize)),
            stopping,
        })
    }

    pub fn try_read(&self) -> Result<AnalyticalRead, &'static str> {
        if self.pool.is_closed() {
            return Err("analytical reads are stopped");
        }
        let permit = self
            .admission
            .clone()
            .try_acquire_owned()
            .map_err(|_| "analytical reads are busy or stopped")?;
        Ok(AnalyticalRead {
            pool: self.pool.clone(),
            _permit: Some(permit),
        })
    }

    pub async fn close(&self) {
        self.stopping.store(true, Ordering::Release);
        self.admission.close();
        self.pool.close().await;
    }
}

async fn install_progress_budget(
    connection: &mut SqliteConnection,
    stopping: Arc<AtomicBool>,
    deadline: Duration,
) -> Result<(), sqlx::Error> {
    let started = Instant::now();
    connection
        .lock_handle()
        .await?
        .set_progress_handler(1000, move || {
            !stopping.load(Ordering::Acquire) && started.elapsed() < deadline
        });
    Ok(())
}

#[cfg(test)]
mod tests;
