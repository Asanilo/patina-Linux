use crate::engine::api::{
    auth::ApiCredentialStore,
    context::ApiRuntimeContext,
    server::{prepare_standalone_server_with_events, ApiServerHandle},
    surface::ApiSurface,
};
use crate::engine::runtime_event::RuntimeEventHub;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{watch, Mutex};

const RETIRED_LISTENER_GRACE_MS: u64 = 100;

struct ActiveApiListener {
    port: u16,
    handle: ApiServerHandle,
}

pub struct LocalApiListenerOwner {
    credentials: ApiCredentialStore,
    surface: ApiSurface,
    event_hub: Arc<RuntimeEventHub>,
    active: Mutex<Option<ActiveApiListener>>,
    reconfiguration: Mutex<()>,
    current_generation: Arc<AtomicU64>,
    failure_tx: watch::Sender<u64>,
}

impl LocalApiListenerOwner {
    pub fn new(
        credentials: ApiCredentialStore,
        surface: ApiSurface,
        event_hub: Arc<RuntimeEventHub>,
    ) -> Self {
        let (failure_tx, _failure_rx) = watch::channel(0);
        Self {
            credentials,
            surface,
            event_hub,
            active: Mutex::new(None),
            reconfiguration: Mutex::new(()),
            current_generation: Arc::new(AtomicU64::new(0)),
            failure_tx,
        }
    }

    pub async fn start(
        self: &Arc<Self>,
        port: u16,
        context: ApiRuntimeContext,
    ) -> Result<u16, String> {
        let _guard = self.reconfiguration.lock().await;
        let server = self.prepare(port, context).await?;
        let confirmed_port = server.port();
        self.install(server).await;
        Ok(confirmed_port)
    }

    pub async fn apply_port_with_commit<F, Fut>(
        self: &Arc<Self>,
        requested_port: u16,
        context: ApiRuntimeContext,
        commit: F,
    ) -> Result<u16, String>
    where
        F: FnOnce(u16) -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        let _guard = self.reconfiguration.lock().await;
        if self.confirmed_port().await == Some(requested_port) {
            commit(requested_port).await?;
            return Ok(requested_port);
        }

        let server = self.prepare(requested_port, context).await?;
        let confirmed_port = server.port();
        commit(confirmed_port).await?;
        self.install(server).await;
        Ok(confirmed_port)
    }

    pub async fn confirmed_port(&self) -> Option<u16> {
        self.active.lock().await.as_ref().map(|active| active.port)
    }

    pub fn failure_receiver(&self) -> watch::Receiver<u64> {
        self.failure_tx.subscribe()
    }

    pub async fn wait_until_failed(&self) {
        let mut failures = self.failure_receiver();
        loop {
            let generation = self.current_generation.load(Ordering::Acquire);
            if generation != 0 && *failures.borrow() == generation {
                return;
            }
            if failures.changed().await.is_err() {
                return;
            }
        }
    }

    pub async fn shutdown(&self) {
        self.current_generation.store(0, Ordering::Release);
        if let Some(active) = self.active.lock().await.take() {
            active.handle.shutdown().await;
        }
    }

    async fn prepare(
        &self,
        port: u16,
        context: ApiRuntimeContext,
    ) -> Result<crate::engine::api::server::StandaloneApiServer, String> {
        prepare_standalone_server_with_events(
            port,
            self.credentials.clone(),
            context,
            self.surface,
            self.event_hub.clone(),
        )
        .await
    }

    async fn install(self: &Arc<Self>, server: crate::engine::api::server::StandaloneApiServer) {
        let port = server.port();
        let handle = server.start();
        let mut readiness = handle.readiness();
        let generation = self.current_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let previous = self
            .active
            .lock()
            .await
            .replace(ActiveApiListener { port, handle });

        let current_generation = self.current_generation.clone();
        let failure_tx = self.failure_tx.clone();
        tokio::spawn(async move {
            if *readiness.borrow() {
                let _ = readiness.changed().await;
            }
            if current_generation.load(Ordering::Acquire) == generation {
                failure_tx.send_replace(generation);
            }
        });

        if let Some(previous) = previous {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(RETIRED_LISTENER_GRACE_MS))
                    .await;
                previous.handle.shutdown().await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn token_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "patina-api-listener-owner-{label}-{}-{}",
            std::process::id(),
            TEST_PATH_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn credentials(path: &std::path::Path) -> ApiCredentialStore {
        let credentials = ApiCredentialStore::new();
        credentials.initialize_at(path, Some("test-token")).unwrap();
        credentials
    }

    async fn context() -> ApiRuntimeContext {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        ApiRuntimeContext::new(crate::engine::runtime_context::RuntimeContext::system(pool))
    }

    fn available_port() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.local_addr().unwrap().port()
    }

    #[tokio::test]
    async fn port_conflict_preserves_the_active_listener_and_skips_commit() {
        let path = token_path("conflict");
        let owner = Arc::new(LocalApiListenerOwner::new(
            credentials(&path),
            ApiSurface::DaemonReadOnly,
            Arc::new(RuntimeEventHub::new(8)),
        ));
        let original_port = owner.start(0, context().await).await.unwrap();
        let occupied = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let occupied_port = occupied.local_addr().unwrap().port();
        let committed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let commit_flag = committed.clone();

        let result = owner
            .apply_port_with_commit(occupied_port, context().await, move |_| async move {
                commit_flag.store(true, Ordering::Release);
                Ok(())
            })
            .await;

        assert!(result.is_err());
        assert!(!committed.load(Ordering::Acquire));
        assert_eq!(owner.confirmed_port().await, Some(original_port));
        owner.shutdown().await;
        drop(occupied);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn successful_port_change_commits_before_retiring_the_old_listener() {
        let path = token_path("replace");
        let owner = Arc::new(LocalApiListenerOwner::new(
            credentials(&path),
            ApiSurface::DaemonReadOnly,
            Arc::new(RuntimeEventHub::new(8)),
        ));
        let old_port = owner.start(0, context().await).await.unwrap();
        let new_port = available_port();

        let confirmed = owner
            .apply_port_with_commit(new_port, context().await, |port| async move {
                assert_eq!(port, new_port);
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(confirmed, new_port);
        assert_eq!(owner.confirmed_port().await, Some(new_port));
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let rebound = std::net::TcpListener::bind(("127.0.0.1", old_port)).unwrap();
        drop(rebound);
        owner.shutdown().await;
        let _ = std::fs::remove_file(path);
    }
}
