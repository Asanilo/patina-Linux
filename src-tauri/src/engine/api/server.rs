use crate::engine::api::router;
use std::net::SocketAddr;
use std::sync::Mutex;
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinSet;

pub const DEFAULT_PORT: u16 = 14840;

pub struct ApiServerState {
    inner: Mutex<ApiServerRuntime>,
}

#[derive(Default)]
struct ApiServerRuntime {
    shutdown_tx: Option<watch::Sender<bool>>,
    port: Option<u16>,
}

pub struct PreparedApiListener {
    port: u16,
    listener: TcpListener,
}

pub struct StandaloneApiServer {
    port: u16,
    listener: TcpListener,
    credentials: crate::engine::api::auth::ApiCredentialStore,
    #[cfg_attr(not(test), allow(dead_code))]
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

pub struct ApiServerHandle {
    shutdown_tx: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl ApiServerHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.task.await;
    }
}

#[cfg(test)]
#[derive(Clone)]
pub struct StandaloneApiShutdown {
    shutdown_tx: watch::Sender<bool>,
}

#[cfg(test)]
impl StandaloneApiShutdown {
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

impl StandaloneApiServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn start(self) -> ApiServerHandle {
        let shutdown_tx = self.shutdown_tx.clone();
        let task = tokio::spawn(self.run());
        ApiServerHandle { shutdown_tx, task }
    }

    #[cfg(test)]
    pub fn shutdown_handle(&self) -> StandaloneApiShutdown {
        StandaloneApiShutdown {
            shutdown_tx: self.shutdown_tx.clone(),
        }
    }

    pub async fn run(mut self) {
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                accept_result = self.listener.accept() => {
                    match accept_result {
                        Ok((stream, _peer_addr)) => {
                            let credentials = self.credentials.clone();
                            connections.spawn(async move {
                                router::handle_minimal_connection(stream, credentials).await;
                            });
                        }
                        Err(error) => eprintln!("[patinad] API accept error: {error}"),
                    }
                }
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
        drain_connections(&mut connections).await;
    }
}

pub async fn prepare_standalone_minimal_server(
    port: u16,
    credentials: crate::engine::api::auth::ApiCredentialStore,
) -> Result<StandaloneApiServer, String> {
    let prepared = prepare_listener(port).await?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    Ok(StandaloneApiServer {
        port: prepared.port,
        listener: prepared.listener,
        credentials,
        shutdown_tx,
        shutdown_rx,
    })
}

impl PreparedApiListener {
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl ApiServerState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ApiServerRuntime::default()),
        }
    }

    pub fn shutdown(&self) {
        match self.inner.lock() {
            Ok(mut guard) => {
                if let Some(tx) = guard.shutdown_tx.take() {
                    let _ = tx.send(true);
                }
                guard.port = None;
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                if let Some(tx) = guard.shutdown_tx.take() {
                    let _ = tx.send(true);
                }
                guard.port = None;
            }
        }
    }

    pub async fn prepare_listener(&self, port: u16) -> Result<PreparedApiListener, String> {
        prepare_listener(port).await
    }
}

async fn prepare_listener(port: u16) -> Result<PreparedApiListener, String> {
    let requested_addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(requested_addr)
        .await
        .map_err(|error| format!("failed to bind API server on {requested_addr}: {error}"))?;
    let confirmed_port = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect API listener address: {error}"))?
        .port();
    Ok(PreparedApiListener {
        port: confirmed_port,
        listener,
    })
}

impl ApiServerState {
    pub fn install_prepared(
        &self,
        app_handle: tauri::AppHandle,
        credentials: crate::engine::api::auth::ApiCredentialStore,
        prepared: PreparedApiListener,
    ) {
        let port = prepared.port();
        let listener = prepared.listener;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let previous = self.replace_runtime(port, shutdown_tx);

        tauri::async_runtime::spawn(async move {
            run_server(app_handle, credentials, port, listener, shutdown_rx).await;
        });
        if let Some(previous) = previous {
            let _ = previous.send(true);
        }
    }

    fn replace_runtime(
        &self,
        port: u16,
        shutdown_tx: watch::Sender<bool>,
    ) -> Option<watch::Sender<bool>> {
        match self.inner.lock() {
            Ok(mut guard) => {
                let previous = guard.shutdown_tx.replace(shutdown_tx);
                guard.port = Some(port);
                previous
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                let previous = guard.shutdown_tx.replace(shutdown_tx);
                guard.port = Some(port);
                previous
            }
        }
    }

    pub fn confirmed_port(&self) -> Option<u16> {
        match self.inner.lock() {
            Ok(guard) => guard.port,
            Err(poisoned) => poisoned.into_inner().port,
        }
    }
}

async fn run_server(
    app_handle: tauri::AppHandle,
    credentials: crate::engine::api::auth::ApiCredentialStore,
    port: u16,
    listener: TcpListener,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("[api] listening on http://{addr}");

    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _peer_addr)) => {
                        let app = app_handle.clone();
                        let credentials = credentials.clone();
                        connections.spawn(async move {
                            router::handle_connection(stream, app, credentials).await;
                        });
                    }
                    Err(error) => {
                        eprintln!("[api] accept error: {error}");
                    }
                }
            }
            Some(_) = connections.join_next(), if !connections.is_empty() => {}
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    println!("[api] shutting down");
                    break;
                }
            }
        }
    }
    drain_connections(&mut connections).await;
}

async fn drain_connections(connections: &mut JoinSet<()>) {
    if tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while connections.join_next().await.is_some() {}
    })
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install_test_runtime(state: &ApiServerState, port: u16) -> watch::Receiver<bool> {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let mut guard = state.inner.lock().unwrap();
        guard.port = Some(port);
        guard.shutdown_tx = Some(shutdown_tx);
        shutdown_rx
    }

    #[tokio::test]
    async fn occupied_port_preparation_preserves_the_active_runtime() {
        let occupied = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let occupied_port = occupied.local_addr().unwrap().port();
        let state = ApiServerState::new();
        let old_shutdown = install_test_runtime(&state, 14_840);

        assert!(state.prepare_listener(occupied_port).await.is_err());
        assert_eq!(state.confirmed_port(), Some(14_840));
        assert!(!*old_shutdown.borrow());
    }

    #[tokio::test]
    async fn installing_prepared_runtime_replaces_port_and_returns_previous_shutdown() {
        let state = ApiServerState::new();
        let mut old_shutdown = install_test_runtime(&state, 14_840);
        let prepared = state.prepare_listener(0).await.unwrap();
        let prepared_port = prepared.port();
        let (next_shutdown_tx, _next_shutdown_rx) = watch::channel(false);

        let previous = state.replace_runtime(prepared_port, next_shutdown_tx);
        previous.unwrap().send(true).unwrap();
        old_shutdown.changed().await.unwrap();

        assert_eq!(state.confirmed_port(), Some(prepared_port));
        assert!(*old_shutdown.borrow());
    }

    fn test_credentials() -> crate::engine::api::auth::ApiCredentialStore {
        let path = std::env::temp_dir().join(format!(
            "patina-server-token-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ));
        let credentials = crate::engine::api::auth::ApiCredentialStore::new();
        credentials
            .initialize_at(&path, Some("test-token"))
            .unwrap();
        credentials
    }

    #[tokio::test]
    async fn standalone_shutdown_aborts_stalled_connection_before_returning() {
        let server = prepare_standalone_minimal_server(0, test_credentials())
            .await
            .unwrap();
        let port = server.port();
        let shutdown = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());
        let mut stalled = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::write_all(
            &mut stalled,
            b"GET /api/v1/health HTTP/1.1\r\nAuthorization:",
        )
        .await
        .unwrap();

        shutdown.shutdown();
        tokio::time::timeout(std::time::Duration::from_secs(2), server_task)
            .await
            .expect("server shutdown should be bounded")
            .unwrap();

        let mut remaining = Vec::new();
        if let Err(error) = tokio::io::AsyncReadExt::read_to_end(&mut stalled, &mut remaining).await
        {
            assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
        }
        assert!(remaining.is_empty());
    }
}
