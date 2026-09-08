use crate::domain::backup_schedule::{ScheduledBackupConfigInput, ScheduledBackupSnapshot};
use crate::engine::api::scheduled_backup_owner::{
    ScheduledBackupOwner, ScheduledBackupOwnerError, ScheduledBackupOwnerFuture,
};
use crate::engine::runtime_context::RuntimeContext;
use crate::engine::runtime_event::{RuntimeEvent, RuntimeEventSink};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{watch, Mutex, Notify};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

const SCHEDULER_POLL_SECONDS: u64 = 30;

pub struct DaemonScheduledBackupOwner {
    runtime: RuntimeContext,
    default_backup_dir: PathBuf,
    run_lock: Mutex<()>,
    wake: Notify,
    event_sink: Arc<dyn RuntimeEventSink>,
}

impl DaemonScheduledBackupOwner {
    pub fn new(
        runtime: RuntimeContext,
        default_backup_dir: PathBuf,
        event_sink: Arc<dyn RuntimeEventSink>,
    ) -> Self {
        Self {
            runtime,
            default_backup_dir,
            run_lock: Mutex::new(()),
            wake: Notify::new(),
            event_sink,
        }
    }

    async fn snapshot_inner(&self) -> Result<ScheduledBackupSnapshot, String> {
        let _guard = self.run_lock.lock().await;
        crate::engine::scheduled_backup::get_snapshot(self.runtime.pool(), &self.default_backup_dir)
            .await
    }

    async fn save_config_inner(
        &self,
        input: ScheduledBackupConfigInput,
    ) -> Result<ScheduledBackupSnapshot, ScheduledBackupOwnerError> {
        input
            .validate()
            .map_err(ScheduledBackupOwnerError::InvalidInput)?;
        let guard = self.run_lock.lock().await;
        let snapshot = crate::engine::scheduled_backup::save_config(
            self.runtime.pool(),
            &self.default_backup_dir,
            input,
        )
        .await
        .map_err(ScheduledBackupOwnerError::Internal)?;
        drop(guard);
        self.wake.notify_one();
        self.emit_changed();
        Ok(snapshot)
    }

    async fn tick(&self) -> Result<bool, String> {
        let _guard = self.run_lock.lock().await;
        crate::engine::scheduled_backup::tick(self.runtime.pool(), &self.default_backup_dir).await
    }

    fn emit_changed(&self) {
        let changed_at_ms = self.runtime.now_ms().max(0) as u64;
        if let Err(error) = self
            .event_sink
            .emit(RuntimeEvent::ScheduledBackupChanged { changed_at_ms })
        {
            eprintln!("[scheduled-backup] failed to emit daemon state change: {error}");
        }
    }
}

impl ScheduledBackupOwner for DaemonScheduledBackupOwner {
    fn snapshot(&self) -> ScheduledBackupOwnerFuture<'_, ScheduledBackupSnapshot> {
        Box::pin(async move {
            self.snapshot_inner()
                .await
                .map_err(ScheduledBackupOwnerError::Internal)
        })
    }

    fn save_config(
        &self,
        input: ScheduledBackupConfigInput,
    ) -> ScheduledBackupOwnerFuture<'_, ScheduledBackupSnapshot> {
        Box::pin(async move { self.save_config_inner(input).await })
    }
}

pub struct DaemonScheduledBackupTask {
    shutdown_tx: watch::Sender<bool>,
    join: JoinHandle<()>,
}

impl DaemonScheduledBackupTask {
    pub fn start(owner: Arc<DaemonScheduledBackupOwner>) -> Self {
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let join = tokio::spawn(async move {
            loop {
                if *shutdown_rx.borrow() {
                    break;
                }
                match owner.tick().await {
                    Ok(true) => owner.emit_changed(),
                    Ok(false) => {}
                    Err(error) => eprintln!("[scheduled-backup] daemon tick failed: {error}"),
                }
                tokio::select! {
                    _ = owner.wake.notified() => {}
                    _ = sleep(Duration::from_secs(SCHEDULER_POLL_SECONDS)) => {}
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        });
        Self { shutdown_tx, join }
    }

    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.join.await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, Timelike};

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "patina-daemon-scheduled-backup-{label}-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ))
    }

    #[tokio::test]
    async fn daemon_owner_executes_and_records_a_due_backup() {
        let root = temp_root("due");
        let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(
            &root.join("patina.db"),
            true,
        )
        .await
        .unwrap();
        let target_dir = root.join("backups");
        std::fs::create_dir_all(&target_dir).unwrap();
        let now = Local::now();
        let now_ms = now.timestamp_millis();
        let config = crate::domain::backup_schedule::ScheduledBackupConfig {
            enabled: true,
            cadence: crate::domain::backup_schedule::ScheduledBackupCadence::Daily,
            weekday: None,
            local_time_minutes: (now.hour() * 60 + now.minute()) as u16,
            target_dir: target_dir.to_string_lossy().to_string(),
            target_generation: "daemon-test-generation".to_string(),
            schedule_anchor_at_ms: now_ms - 86_400_000,
            updated_at_ms: now_ms - 86_400_000,
        };
        crate::data::repositories::scheduled_backup::save_config(&pool, &config)
            .await
            .unwrap();
        let events = Arc::new(crate::engine::runtime_event::MemoryRuntimeEventSink::default());
        let owner = DaemonScheduledBackupOwner::new(
            RuntimeContext::system(pool.clone()),
            target_dir.clone(),
            events,
        );

        assert!(owner.tick().await.unwrap());
        let snapshot = owner.snapshot_inner().await.unwrap();
        let success = snapshot.recent_success.unwrap();
        let backup_path = PathBuf::from(success.target_path);
        assert!(backup_path.is_file());
        crate::data::backup::validate_scheduled_snapshot(&backup_path).unwrap();

        pool.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
}
