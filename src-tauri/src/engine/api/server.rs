use crate::engine::api::router;
use std::net::SocketAddr;
use std::sync::Mutex;
use tokio::net::TcpListener;
use tokio::sync::watch;

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

    pub fn install_prepared(&self, app_handle: tauri::AppHandle, prepared: PreparedApiListener) {
        let port = prepared.port();
        let listener = prepared.listener;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let previous = self.replace_runtime(port, shutdown_tx);

        tauri::async_runtime::spawn(async move {
            run_server(app_handle, port, listener, shutdown_rx).await;
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
    port: u16,
    listener: TcpListener,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("[api] listening on http://{addr}");

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _peer_addr)) => {
                        let app = app_handle.clone();
                        tauri::async_runtime::spawn(async move {
                            router::handle_connection(stream, app).await;
                        });
                    }
                    Err(error) => {
                        eprintln!("[api] accept error: {error}");
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    println!("[api] shutting down");
                    break;
                }
            }
        }
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
}
