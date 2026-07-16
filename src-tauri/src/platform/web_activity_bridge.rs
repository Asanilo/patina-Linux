use crate::domain::settings::WebActivityBridgeSettings;
use crate::engine::web_activity::{WebActivityBridgeHttpRequest, WebActivityBridgeHttpResponse};
use serde_json::json;
use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener as StdTcpListener};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Mutex};
use tokio::task::{JoinHandle, JoinSet};

const WEB_ACTIVITY_BRIDGE_HTTP_BODY_MAX_BYTES: usize = 64 * 1024;
const WEB_ACTIVITY_BRIDGE_HTTP_HEADER_MAX_BYTES: usize = 16 * 1024;
const WEB_ACTIVITY_BRIDGE_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const WEB_ACTIVITY_BRIDGE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
pub const WEB_ACTIVITY_BRIDGE_SETTINGS_CHANGED_EVENT: &str = "app-settings-changed";
pub const WEB_ACTIVITY_BRIDGE_ACTIVE_WINDOW_EVENT: &str = "active-window-changed";
pub const WEB_ACTIVITY_BRIDGE_TRACKING_DATA_EVENT: &str = "tracking-data-changed";

pub type WebActivityBridgeHttpFuture =
    Pin<Box<dyn Future<Output = WebActivityBridgeHttpResponse> + Send>>;
pub type WebActivityBridgeHttpHandler = Arc<
    dyn Fn(WebActivityBridgeHttpRequest) -> WebActivityBridgeHttpFuture + Send + Sync + 'static,
>;

pub struct PreparedWebActivityBridgeServer {
    address: SocketAddr,
    listener: StdTcpListener,
    handler: WebActivityBridgeHttpHandler,
}

impl PreparedWebActivityBridgeServer {
    pub fn port(&self) -> u16 {
        self.address.port()
    }

    pub fn start(self) -> WebActivityBridgeServerHandle {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(run_server(
            self.address,
            self.listener,
            self.handler,
            shutdown_rx,
        ));
        WebActivityBridgeServerHandle { shutdown_tx, task }
    }
}

pub struct WebActivityBridgeServerHandle {
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl WebActivityBridgeServerHandle {
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let mut task = self.task;
        if tokio::time::timeout(WEB_ACTIVITY_BRIDGE_SHUTDOWN_TIMEOUT, &mut task)
            .await
            .is_err()
        {
            task.abort();
            let _ = task.await;
        }
    }
}

#[derive(Default)]
pub struct WebActivityBridgeRuntimeState {
    inner: Mutex<WebActivityBridgeRuntimeInner>,
}

#[derive(Default)]
struct WebActivityBridgeRuntimeInner {
    settings: WebActivityBridgeSettings,
    server: Option<WebActivityBridgeServerHandle>,
}

impl WebActivityBridgeRuntimeState {
    pub async fn update(
        &self,
        settings: WebActivityBridgeSettings,
        handler: WebActivityBridgeHttpHandler,
    ) -> bool {
        let mut inner = self.inner.lock().await;
        let should_restart =
            should_restart_server(&inner.settings, &settings, inner.server.is_some());

        if should_restart {
            if let Some(server) = inner.server.take() {
                server.shutdown().await;
            }
        }

        if settings.enabled && (should_restart || inner.server.is_none()) {
            match prepare_web_activity_bridge_server(settings.port, handler) {
                Ok(server) => inner.server = Some(server.start()),
                Err(error) => eprintln!(
                    "[web-activity-bridge] failed to bind 127.0.0.1:{}: {error}",
                    settings.port
                ),
            }
        }

        inner.settings = settings;
        inner.server.is_some()
    }

    pub async fn shutdown(&self) {
        let server = self.inner.lock().await.server.take();
        if let Some(server) = server {
            server.shutdown().await;
        }
    }
}

pub fn prepare_web_activity_bridge_server(
    port: u16,
    handler: WebActivityBridgeHttpHandler,
) -> io::Result<PreparedWebActivityBridgeServer> {
    let (address, listener) = open_web_activity_bridge_listener(port)?;
    Ok(PreparedWebActivityBridgeServer {
        address,
        listener,
        handler,
    })
}

fn should_restart_server(
    previous_settings: &WebActivityBridgeSettings,
    settings: &WebActivityBridgeSettings,
    has_server: bool,
) -> bool {
    previous_settings.enabled != settings.enabled
        || previous_settings.port != settings.port
        || previous_settings.token != settings.token
        || (!settings.enabled && has_server)
}

async fn run_server(
    address: SocketAddr,
    std_listener: StdTcpListener,
    handler: WebActivityBridgeHttpHandler,
    mut shutdown: watch::Receiver<bool>,
) {
    let listener = match TcpListener::from_std(std_listener) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("[web-activity-bridge] failed to attach listener {address}: {error}");
            return;
        }
    };
    let mut clients = JoinSet::new();

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                let _ = changed;
                break;
            }
            completed = clients.join_next(), if !clients.is_empty() => {
                if let Some(Err(error)) = completed {
                    eprintln!("[web-activity-bridge] client task failed: {error}");
                }
            }
            accepted = listener.accept() => {
                match accepted {
                    Ok((stream, remote_addr)) => {
                        let handler = handler.clone();
                        let client_shutdown = shutdown.clone();
                        clients.spawn(async move {
                            if let Err(error) = handle_client(stream, handler, client_shutdown).await {
                                eprintln!("[web-activity-bridge] client {remote_addr} closed: {error}");
                            }
                        });
                    }
                    Err(error) => eprintln!("[web-activity-bridge] accept failed: {error}"),
                }
            }
        }
    }

    clients.abort_all();
    while clients.join_next().await.is_some() {}
}

fn open_web_activity_bridge_listener(port: u16) -> io::Result<(SocketAddr, StdTcpListener)> {
    let requested = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let listener = StdTcpListener::bind(requested)?;
    listener.set_nonblocking(true)?;
    Ok((listener.local_addr()?, listener))
}

async fn handle_client(
    stream: TcpStream,
    handler: WebActivityBridgeHttpHandler,
    mut shutdown: watch::Receiver<bool>,
) -> Result<(), String> {
    tokio::select! {
        changed = shutdown.changed() => changed.map_err(|error| format!("shutdown channel closed: {error}")),
        result = tokio::time::timeout(WEB_ACTIVITY_BRIDGE_REQUEST_TIMEOUT, handle_http_client(stream, handler)) => {
            result.map_err(|_| "http request timed out".to_string())?
        },
    }
}

async fn handle_http_client(
    mut stream: TcpStream,
    handler: WebActivityBridgeHttpHandler,
) -> Result<(), String> {
    let response = match read_http_request(&mut stream).await {
        Ok(request) if request.method.eq_ignore_ascii_case("OPTIONS") => {
            WebActivityBridgeHttpResponse::json(204, json!({}))
        }
        Ok(request) => handler(request).await,
        Err(error) => {
            WebActivityBridgeHttpResponse::json(400, json!({ "ok": false, "message": error }))
        }
    };
    write_http_response(&mut stream, response).await
}

async fn read_http_request(stream: &mut TcpStream) -> Result<WebActivityBridgeHttpRequest, String> {
    let mut buffer = Vec::with_capacity(2048);
    let header_end = loop {
        if let Some(index) = find_http_header_end(&buffer) {
            break index;
        }
        if buffer.len() > WEB_ACTIVITY_BRIDGE_HTTP_HEADER_MAX_BYTES {
            return Err("http headers are too large".to_string());
        }

        let mut chunk = [0_u8; 1024];
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| format!("failed to read http request: {error}"))?;
        if read == 0 {
            return Err("client closed before http headers completed".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
    };

    let header_text = std::str::from_utf8(&buffer[..header_end])
        .map_err(|error| format!("invalid http headers: {error}"))?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "missing http request line".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| "missing http method".to_string())?
        .to_string();
    let path = request_parts
        .next()
        .ok_or_else(|| "missing http path".to_string())?
        .to_string();
    let mut authorization = None;
    let mut content_length = 0_usize;

    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.trim().to_ascii_lowercase().as_str() {
            "authorization" => authorization = Some(value.trim().to_string()),
            "content-length" => {
                content_length = value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "invalid content-length header".to_string())?;
            }
            _ => {}
        }
    }

    if content_length > WEB_ACTIVITY_BRIDGE_HTTP_BODY_MAX_BYTES {
        return Err("http body is too large".to_string());
    }

    let body_start = header_end + 4;
    while buffer.len().saturating_sub(body_start) < content_length {
        let mut chunk = [0_u8; 1024];
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| format!("failed to read http body: {error}"))?;
        if read == 0 {
            return Err("client closed before http body completed".to_string());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len().saturating_sub(body_start) > WEB_ACTIVITY_BRIDGE_HTTP_BODY_MAX_BYTES {
            return Err("http body is too large".to_string());
        }
    }

    Ok(WebActivityBridgeHttpRequest {
        method,
        path,
        authorization,
        body: buffer[body_start..body_start + content_length].to_vec(),
    })
}

fn find_http_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn write_http_response(
    stream: &mut TcpStream,
    response: WebActivityBridgeHttpResponse,
) -> Result<(), String> {
    let status_text = match response.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let body = if response.status == 204 {
        Vec::new()
    } else {
        response.body.into_bytes()
    };
    let headers = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Headers: Authorization, Content-Type\r\n\
         Access-Control-Allow-Methods: POST, OPTIONS\r\n\r\n",
        response.status,
        status_text,
        body.len(),
    );
    stream
        .write_all(headers.as_bytes())
        .await
        .map_err(|error| format!("failed to write http response headers: {error}"))?;
    if !body.is_empty() {
        stream
            .write_all(&body)
            .await
            .map_err(|error| format!("failed to write http response body: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok_handler() -> WebActivityBridgeHttpHandler {
        Arc::new(|_| {
            Box::pin(async { WebActivityBridgeHttpResponse::json(200, json!({"ok": true})) })
        })
    }

    #[test]
    fn listener_bind_can_recover_after_occupied_port_is_released() {
        let (_address, occupied_listener) = open_web_activity_bridge_listener(0).unwrap();
        let port = occupied_listener.local_addr().unwrap().port();
        assert!(open_web_activity_bridge_listener(port).is_err());
        drop(occupied_listener);
        let (address, recovered_listener) = open_web_activity_bridge_listener(port).unwrap();
        assert_eq!(address.port(), port);
        drop(recovered_listener);
    }

    #[test]
    fn token_rotation_requires_server_restart() {
        let previous = WebActivityBridgeSettings {
            enabled: true,
            port: 12_345,
            token: "old-token".to_string(),
        };
        let next = WebActivityBridgeSettings {
            token: "new-token".to_string(),
            ..previous.clone()
        };
        assert!(should_restart_server(&previous, &next, true));
    }

    #[tokio::test]
    async fn shutdown_releases_listener_with_stalled_client() {
        let server = prepare_web_activity_bridge_server(0, ok_handler()).unwrap();
        let port = server.port();
        let handle = server.start();
        let mut stalled = TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        stalled
            .write_all(b"POST /web-activity HTTP/1.1\r\nAuthorization:")
            .await
            .unwrap();
        handle.shutdown().await;
        let rebound = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        drop(rebound);
    }
}
