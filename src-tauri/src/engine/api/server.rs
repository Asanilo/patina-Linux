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

impl ApiServerState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ApiServerRuntime::default()),
        }
    }

    pub fn start(&self, app_handle: tauri::AppHandle, port: Option<u16>) {
        let port = port.unwrap_or(DEFAULT_PORT);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        match self.inner.lock() {
            Ok(mut guard) => {
                if let Some(previous) = guard.shutdown_tx.take() {
                    let _ = previous.send(true);
                }
                guard.shutdown_tx = Some(shutdown_tx);
                guard.port = Some(port);
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                if let Some(previous) = guard.shutdown_tx.take() {
                    let _ = previous.send(true);
                }
                guard.shutdown_tx = Some(shutdown_tx);
                guard.port = Some(port);
            }
        }

        tauri::async_runtime::spawn(async move {
            if let Err(error) = run_server(app_handle, port, shutdown_rx).await {
                eprintln!("[api] server error: {error}");
            }
        });
    }

    pub fn restart(&self, app_handle: tauri::AppHandle, port: u16) {
        self.start(app_handle, Some(port));
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

    pub fn port(&self) -> u16 {
        match self.inner.lock() {
            Ok(guard) => guard.port.unwrap_or(DEFAULT_PORT),
            Err(poisoned) => poisoned.into_inner().port.unwrap_or(DEFAULT_PORT),
        }
    }
}

async fn run_server(
    app_handle: tauri::AppHandle,
    port: u16,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("failed to bind API server on {addr}: {e}"))?;

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

    Ok(())
}
