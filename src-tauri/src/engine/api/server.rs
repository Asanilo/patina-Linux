use crate::engine::api::router;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::watch;

pub const DEFAULT_PORT: u16 = 14840;

pub struct ApiServerState {
    shutdown_tx: watch::Sender<bool>,
}

impl ApiServerState {
    pub fn new() -> Self {
        let (shutdown_tx, _) = watch::channel(false);
        Self { shutdown_tx }
    }

    pub fn start(&self, app_handle: tauri::AppHandle, port: Option<u16>) {
        let port = port.unwrap_or(DEFAULT_PORT);
        let token = super::auth::get_api_token().to_string();
        let shutdown_rx = self.shutdown_tx.subscribe();

        println!("[api] token: {token}");

        tauri::async_runtime::spawn(async move {
            if let Err(error) = run_server(app_handle, port, shutdown_rx).await {
                eprintln!("[api] server error: {error}");
            }
        });
    }

    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
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
