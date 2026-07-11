mod status;

use std::path::PathBuf;

use sqlx::{Pool, Sqlite};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use crate::engine::api::types::RouteResponse;

pub use status::DaemonStartupStatus;

#[derive(Debug)]
pub struct DaemonSqliteRuntime {
    pub db_path: PathBuf,
    pub pool: Pool<Sqlite>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DaemonRunOptions {
    pub serve_minimal_api: bool,
}

impl DaemonRunOptions {
    pub fn from_args(args: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        Self {
            serve_minimal_api: args.into_iter().any(|arg| arg.as_ref() == "--serve-api"),
        }
    }
}

pub struct MinimalApiServer {
    port: u16,
    listener: TcpListener,
    auth_token: String,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

#[derive(Clone)]
pub struct MinimalApiShutdown {
    shutdown_tx: watch::Sender<bool>,
}

impl MinimalApiShutdown {
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

impl MinimalApiServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn shutdown_handle(&self) -> MinimalApiShutdown {
        MinimalApiShutdown {
            shutdown_tx: self.shutdown_tx.clone(),
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                accept_result = self.listener.accept() => {
                    match accept_result {
                        Ok((stream, _peer_addr)) => {
                            let auth_token = self.auth_token.clone();
                            tokio::spawn(async move {
                                handle_minimal_api_connection(stream, auth_token).await;
                            });
                        }
                        Err(error) => {
                            eprintln!("[patinad] minimal API accept error: {error}");
                        }
                    }
                }
                _ = self.shutdown_rx.changed() => {
                    if *self.shutdown_rx.borrow() {
                        break;
                    }
                }
            }
        }
    }
}

pub fn build_startup_status(version: impl Into<String>) -> DaemonStartupStatus {
    let storage_paths = default_daemon_storage_paths();
    status::build_startup_status(
        version,
        crate::engine::api::server::DEFAULT_PORT,
        crate::engine::api::auth::token_file_path(),
        storage_paths.data_root,
        storage_paths.db_path,
        storage_paths.webview_root,
    )
}

fn default_daemon_storage_paths() -> crate::platform::storage_paths::StoragePaths {
    let roots = crate::platform::app_paths::AppPathRoots {
        config: env_path("XDG_CONFIG_HOME").unwrap_or_else(|| home_path().join(".config")),
        data: env_path("XDG_DATA_HOME").unwrap_or_else(|| home_path().join(".local/share")),
        local_data: env_path("XDG_DATA_HOME").unwrap_or_else(|| home_path().join(".local/share")),
    };
    let profile = crate::platform::app_paths::AppProfile::Production;
    let paths = crate::platform::app_paths::profile_paths(&roots, profile);

    crate::platform::storage_paths::StoragePaths::from_roots(
        paths.control_root,
        paths.data_root.clone(),
        paths.data_root,
        paths.webview_root,
        false,
        false,
    )
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn home_path() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn run(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<(), String> {
    run_with_options(DaemonRunOptions::from_args(args))
}

pub fn run_with_options(options: DaemonRunOptions) -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("failed to create daemon async runtime: {error}"))?;
    let status = build_startup_status(env!("CARGO_PKG_VERSION"));
    crate::engine::api::auth::initialize_api_token(None)?;
    let sqlite_runtime =
        runtime.block_on(prepare_sqlite_runtime_at_path(status.db_path.clone()))?;
    println!(
        "[{}] {} {} ({})",
        status.service_name, status.mode, status.version, status.stage
    );
    for note in &status.notes {
        println!("[{}] {note}", status.service_name);
    }
    println!(
        "[{}] planned local API http://127.0.0.1:{}",
        status.service_name, status.local_api_port
    );
    println!(
        "[{}] API token file {}",
        status.service_name,
        status.api_token_path.display()
    );
    println!(
        "[{}] data root {}",
        status.service_name,
        status.data_root.display()
    );
    println!("[{}] db {}", status.service_name, status.db_path.display());
    println!("[{}] sqlite ready", status.service_name);
    if options.serve_minimal_api {
        let token = crate::engine::api::auth::get_api_token();
        let server = runtime.block_on(prepare_minimal_api_server(status.local_api_port, token))?;
        println!(
            "[{}] minimal API listening on http://127.0.0.1:{}",
            status.service_name,
            server.port()
        );
        runtime.block_on(server.run());
    }
    runtime.block_on(async move {
        sqlite_runtime.pool.close().await;
    });
    Ok(())
}

pub async fn prepare_sqlite_runtime_at_path(
    db_path: impl Into<PathBuf>,
) -> Result<DaemonSqliteRuntime, String> {
    let db_path = db_path.into();
    let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(&db_path, true).await?;
    Ok(DaemonSqliteRuntime { db_path, pool })
}

pub fn route_minimal_api_request(method: &str, path: &str) -> RouteResponse {
    crate::engine::api::router::route_minimal_request(method, path)
}

pub async fn prepare_minimal_api_server(
    port: u16,
    auth_token: impl Into<String>,
) -> Result<MinimalApiServer, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|error| format!("failed to bind patinad minimal API: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("failed to inspect patinad minimal API address: {error}"))?
        .port();
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    Ok(MinimalApiServer {
        port,
        listener,
        auth_token: auth_token.into(),
        shutdown_tx,
        shutdown_rx,
    })
}

async fn handle_minimal_api_connection(stream: TcpStream, auth_token: String) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).await.is_err() {
        return;
    }
    let parts = request_line.split_whitespace().collect::<Vec<_>>();
    let mut authorized = false;
    loop {
        let mut header_line = String::new();
        match reader.read_line(&mut header_line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let trimmed = header_line.trim();
                if trimmed.is_empty() {
                    break;
                }
                if let Some(value) = trimmed.strip_prefix("Authorization:") {
                    let token = value.trim().strip_prefix("Bearer ").unwrap_or(value.trim());
                    authorized = token == auth_token;
                }
            }
        }
    }

    let response = if !authorized {
        RouteResponse {
            status: 401,
            body: serde_json::to_value(crate::engine::api::types::ApiError::unauthorized())
                .unwrap_or_default(),
        }
    } else if parts.len() >= 2 {
        route_minimal_api_request(parts[0], parts[1])
    } else {
        crate::engine::api::router::route_minimal_request("", "")
    };

    let body = serde_json::to_string(&response.body).unwrap_or_else(|_| "{}".to_string());
    let status_text = match response.status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        _ => "Unknown",
    };
    let raw = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        status_text,
        body.len(),
        body
    );
    let _ = writer.write_all(raw.as_bytes()).await;
}
