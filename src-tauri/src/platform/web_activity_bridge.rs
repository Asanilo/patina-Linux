use crate::domain::settings::WebActivityBridgeSettings;
use crate::engine::web_activity::{WebActivityBridgeHttpRequest, WebActivityBridgeHttpResponse};
use axum::{
    body::to_bytes,
    extract::{DefaultBodyLimit, Request, State},
    http::{
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN, AUTHORIZATION, CONTENT_TYPE, HOST, ORIGIN, VARY,
        },
        HeaderMap, HeaderValue, Method, StatusCode,
    },
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use futures_util::FutureExt;
use serde_json::json;
use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener as StdTcpListener};
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{watch, Mutex, Semaphore};
use tokio::task::JoinHandle;
use tower::ServiceBuilder;

const WEB_ACTIVITY_BRIDGE_HTTP_BODY_MAX_BYTES: usize = 64 * 1024;
const WEB_ACTIVITY_BRIDGE_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const WEB_ACTIVITY_BRIDGE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const WEB_ACTIVITY_BRIDGE_CONCURRENCY_LIMIT: usize = 8;
pub const WEB_ACTIVITY_BRIDGE_SETTINGS_CHANGED_EVENT: &str = "app-settings-changed";
pub const WEB_ACTIVITY_BRIDGE_ACTIVE_WINDOW_EVENT: &str = "active-window-changed";
pub const WEB_ACTIVITY_BRIDGE_TRACKING_DATA_EVENT: &str = "tracking-data-changed";

pub type WebActivityBridgeHttpFuture =
    Pin<Box<dyn Future<Output = WebActivityBridgeHttpResponse> + Send>>;
pub type WebActivityBridgeHttpHandler = Arc<
    dyn Fn(WebActivityBridgeHttpRequest) -> WebActivityBridgeHttpFuture + Send + Sync + 'static,
>;
pub type WebActivityBridgeReadinessHandler = Arc<dyn Fn(bool) + Send + Sync + 'static>;

pub struct PreparedWebActivityBridgeServer {
    address: SocketAddr,
    listener: StdTcpListener,
    handler: WebActivityBridgeHttpHandler,
}

impl PreparedWebActivityBridgeServer {
    #[cfg(test)]
    pub fn port(&self) -> u16 {
        self.address.port()
    }

    #[cfg(test)]
    pub fn start(self) -> WebActivityBridgeServerHandle {
        self.start_with_readiness(Arc::new(|_| {}))
    }

    pub fn start_with_readiness(
        self,
        readiness: WebActivityBridgeReadinessHandler,
    ) -> WebActivityBridgeServerHandle {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let (readiness_tx, readiness_rx) = watch::channel(true);
        readiness(true);
        let readiness_guard = ReadinessGuard {
            callback: readiness,
            readiness_tx,
        };
        let task = tokio::spawn(async move {
            let _readiness_guard = readiness_guard;
            run_server(self.address, self.listener, self.handler, shutdown_rx).await;
        });
        WebActivityBridgeServerHandle {
            shutdown_tx,
            readiness_rx,
            task,
        }
    }
}

struct ReadinessGuard {
    callback: WebActivityBridgeReadinessHandler,
    readiness_tx: watch::Sender<bool>,
}

impl Drop for ReadinessGuard {
    fn drop(&mut self) {
        (self.callback)(false);
        let _ = self.readiness_tx.send(false);
    }
}

pub struct WebActivityBridgeServerHandle {
    shutdown_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,
    task: JoinHandle<()>,
}

impl WebActivityBridgeServerHandle {
    pub fn is_ready(&self) -> bool {
        *self.readiness_rx.borrow()
    }

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
        readiness: WebActivityBridgeReadinessHandler,
    ) -> Result<bool, String> {
        self.update_with_commit(settings, handler, readiness, || async { Ok(()) })
            .await
    }

    pub async fn update_with_commit<F, Fut>(
        &self,
        settings: WebActivityBridgeSettings,
        handler: WebActivityBridgeHttpHandler,
        readiness: WebActivityBridgeReadinessHandler,
        commit: F,
    ) -> Result<bool, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        let mut inner = self.inner.lock().await;
        let has_ready_server = inner
            .server
            .as_ref()
            .is_some_and(WebActivityBridgeServerHandle::is_ready);
        let same_ready_listener =
            has_ready_server && inner.settings.enabled && inner.settings.port == settings.port;

        if same_ready_listener && settings.enabled {
            commit().await?;
            inner.settings = settings;
            return Ok(true);
        }

        if !settings.enabled {
            commit().await?;
            if let Some(server) = inner.server.take() {
                server.shutdown().await;
            }
            inner.settings = settings;
            readiness(false);
            return Ok(false);
        }

        if inner.server.is_some() && !has_ready_server && inner.settings.port == settings.port {
            if let Some(server) = inner.server.take() {
                server.shutdown().await;
            }
        }

        let prepared =
            prepare_web_activity_bridge_server(settings.port, handler).map_err(|error| {
                format!(
                    "failed to bind browser activity bridge on 127.0.0.1:{}: {error}",
                    settings.port
                )
            })?;
        commit().await?;
        if let Some(server) = inner.server.take() {
            server.shutdown().await;
        }
        inner.server = Some(prepared.start_with_readiness(readiness));
        inner.settings = settings;
        Ok(inner
            .server
            .as_ref()
            .is_some_and(WebActivityBridgeServerHandle::is_ready))
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

#[derive(Clone)]
struct BridgeTransportState {
    handler: WebActivityBridgeHttpHandler,
    budget: Arc<Semaphore>,
}

struct BridgeBoundaryRejection {
    status: u16,
    message: &'static str,
}

impl BridgeBoundaryRejection {
    fn into_response(self) -> Response {
        boundary_error(self.status, self.message)
    }
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
    let state = BridgeTransportState {
        handler,
        budget: Arc::new(Semaphore::new(WEB_ACTIVITY_BRIDGE_CONCURRENCY_LIMIT)),
    };
    let app = Router::new()
        .fallback(any(bridge_handler))
        .with_state(state)
        .layer(ServiceBuilder::new().layer(DefaultBodyLimit::disable()));
    let shutdown_signal = async move {
        if !*shutdown.borrow() {
            let _ = shutdown.changed().await;
        }
    };
    if let Err(error) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
    {
        eprintln!("[web-activity-bridge] listener {address} stopped: {error}");
    }
}

fn open_web_activity_bridge_listener(port: u16) -> io::Result<(SocketAddr, StdTcpListener)> {
    let requested = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let listener = StdTcpListener::bind(requested)?;
    listener.set_nonblocking(true)?;
    Ok((listener.local_addr()?, listener))
}

async fn bridge_handler(State(state): State<BridgeTransportState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let cors_origin = match validate_request_boundary(&parts.headers) {
        Ok(origin) => origin,
        Err(rejection) => return rejection.into_response(),
    };
    if parts.method == Method::OPTIONS {
        return preflight_response(cors_origin);
    }
    let Ok(_permit) = state.budget.clone().try_acquire_owned() else {
        return with_cors(
            bridge_response(WebActivityBridgeHttpResponse::json(
                503,
                json!({
                    "ok": false,
                    "code": "bridge-busy",
                    "message": "browser activity bridge is busy",
                }),
            )),
            cors_origin,
        );
    };
    let body = match to_bytes(body, WEB_ACTIVITY_BRIDGE_HTTP_BODY_MAX_BYTES).await {
        Ok(body) => body,
        Err(_) => {
            return with_cors(
                bridge_response(WebActivityBridgeHttpResponse::json(
                    413,
                    json!({
                        "ok": false,
                        "code": "payload-too-large",
                        "message": "http body is too large",
                    }),
                )),
                cors_origin,
            )
        }
    };
    let request = WebActivityBridgeHttpRequest {
        method: parts.method.as_str().to_string(),
        path: parts.uri.path().to_string(),
        authorization: parts
            .headers
            .get(AUTHORIZATION)
            .and_then(header_text)
            .map(str::to_string),
        body: body.to_vec(),
    };
    let response = match tokio::time::timeout(
        WEB_ACTIVITY_BRIDGE_REQUEST_TIMEOUT,
        AssertUnwindSafe((state.handler)(request)).catch_unwind(),
    )
    .await
    {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => WebActivityBridgeHttpResponse::json(
            500,
            json!({
                "ok": false,
                "code": "handler-panicked",
                "message": "browser activity handler panicked",
            }),
        ),
        Err(_) => WebActivityBridgeHttpResponse::json(
            503,
            json!({
                "ok": false,
                "code": "handler-timeout",
                "message": "browser activity handler timed out",
            }),
        ),
    };
    with_cors(bridge_response(response), cors_origin)
}

fn validate_request_boundary(
    headers: &HeaderMap,
) -> Result<Option<HeaderValue>, BridgeBoundaryRejection> {
    let Some(host) = headers.get(HOST).and_then(header_text) else {
        return Err(BridgeBoundaryRejection {
            status: 400,
            message: "missing Host header",
        });
    };
    if !is_loopback_authority(host) {
        return Err(BridgeBoundaryRejection {
            status: 400,
            message: "Host must resolve to loopback",
        });
    }
    let Some(origin) = headers.get(ORIGIN) else {
        return Ok(None);
    };
    let Some(origin_text) = header_text(origin) else {
        return Err(BridgeBoundaryRejection {
            status: 400,
            message: "invalid Origin header",
        });
    };
    if !is_browser_extension_origin(origin_text) {
        return Err(BridgeBoundaryRejection {
            status: 403,
            message: "Origin is not an allowed browser extension",
        });
    }
    Ok(Some(origin.clone()))
}

fn is_loopback_authority(authority: &str) -> bool {
    axum::http::uri::Authority::from_str(authority)
        .ok()
        .is_some_and(|authority| is_loopback_host(authority.host()))
}

fn is_loopback_host(host: &str) -> bool {
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .ok()
            .is_some_and(|address| address.is_loopback())
}

fn is_browser_extension_origin(origin: &str) -> bool {
    url::Url::parse(origin).ok().is_some_and(|origin| {
        matches!(origin.scheme(), "chrome-extension" | "moz-extension")
            && origin.host_str().is_some_and(|host| !host.is_empty())
    })
}

fn boundary_error(status: u16, message: &str) -> Response {
    bridge_response(WebActivityBridgeHttpResponse::json(
        status,
        json!({
            "ok": false,
            "code": "request-boundary-rejected",
            "message": message,
        }),
    ))
}

fn bridge_response(response: WebActivityBridgeHttpResponse) -> Response {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    if status == StatusCode::NO_CONTENT {
        return status.into_response();
    }
    (
        status,
        [(CONTENT_TYPE, "application/json; charset=utf-8")],
        response.body,
    )
        .into_response()
}

fn preflight_response(origin: Option<HeaderValue>) -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Authorization, Content-Type"),
    );
    response.headers_mut().insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("POST, OPTIONS"),
    );
    with_cors(response, origin)
}

fn with_cors(mut response: Response, origin: Option<HeaderValue>) -> Response {
    if let Some(origin) = origin {
        response
            .headers_mut()
            .insert(ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        response
            .headers_mut()
            .insert(VARY, HeaderValue::from_static("Origin"));
    }
    response
}

fn header_text(value: &HeaderValue) -> Option<&str> {
    value.to_str().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn ok_handler() -> WebActivityBridgeHttpHandler {
        Arc::new(|_| {
            Box::pin(async { WebActivityBridgeHttpResponse::json(200, json!({"ok": true})) })
        })
    }

    async fn exchange(port: u16, request: &[u8]) -> String {
        let mut stream = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        stream.write_all(request).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    fn available_port() -> u16 {
        let listener = StdTcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.local_addr().unwrap().port()
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

    #[tokio::test]
    async fn same_port_update_keeps_the_ready_listener_running() {
        let runtime = WebActivityBridgeRuntimeState::default();
        let port = available_port();
        let readiness_changes = Arc::new(AtomicUsize::new(0));
        let readiness: WebActivityBridgeReadinessHandler = {
            let readiness_changes = readiness_changes.clone();
            Arc::new(move |_| {
                readiness_changes.fetch_add(1, Ordering::SeqCst);
            })
        };
        let previous = WebActivityBridgeSettings {
            enabled: true,
            port,
            token: "old-token".to_string(),
        };
        runtime
            .update(previous.clone(), ok_handler(), readiness.clone())
            .await
            .unwrap();
        assert_eq!(readiness_changes.load(Ordering::SeqCst), 1);

        let next = WebActivityBridgeSettings {
            token: "new-token".to_string(),
            ..previous
        };
        runtime.update(next, ok_handler(), readiness).await.unwrap();

        assert_eq!(readiness_changes.load(Ordering::SeqCst), 1);
        let response = exchange(
            port,
            b"POST /web-activity HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        runtime.shutdown().await;
    }

    #[test]
    fn origin_boundary_accepts_firefox_and_chromium_extensions_only() {
        assert!(is_browser_extension_origin(
            "moz-extension://3f6af12b-1111-2222-3333-123456789abc"
        ));
        assert!(is_browser_extension_origin(
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop"
        ));
        assert!(!is_browser_extension_origin("https://example.com"));
        assert!(!is_browser_extension_origin("null"));
    }

    #[tokio::test]
    async fn bridge_serves_extension_preflight_and_post_without_wildcard_cors() {
        let server = prepare_web_activity_bridge_server(0, ok_handler()).unwrap();
        let port = server.port();
        let handle = server.start();
        let preflight = exchange(
            port,
            b"OPTIONS /web-activity HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: moz-extension://test-extension\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(preflight.starts_with("HTTP/1.1 204 No Content"));
        assert!(preflight.contains("access-control-allow-origin: moz-extension://test-extension"));
        assert!(!preflight.contains("access-control-allow-origin: *"));

        let post = exchange(
            port,
            b"POST /web-activity HTTP/1.1\r\nHost: localhost\r\nOrigin: chrome-extension://test-extension\r\nAuthorization: Bearer token\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        )
        .await;
        assert!(post.starts_with("HTTP/1.1 200 OK"));
        assert!(post.contains("{\"ok\":true}"));

        let hostile = exchange(
            port,
            b"OPTIONS /web-activity HTTP/1.1\r\nHost: localhost\r\nOrigin: https://example.com\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(hostile.starts_with("HTTP/1.1 403 Forbidden"));
        handle.shutdown().await;
    }

    #[tokio::test]
    async fn runtime_update_keeps_old_listener_when_persistence_fails() {
        let runtime = WebActivityBridgeRuntimeState::default();
        let old_port = available_port();
        let old_settings = WebActivityBridgeSettings {
            enabled: true,
            port: old_port,
            token: "old-token".to_string(),
        };
        runtime
            .update(old_settings, ok_handler(), Arc::new(|_| {}))
            .await
            .unwrap();

        let new_port = available_port();
        let error = runtime
            .update_with_commit(
                WebActivityBridgeSettings {
                    enabled: true,
                    port: new_port,
                    token: "new-token".to_string(),
                },
                ok_handler(),
                Arc::new(|_| {}),
                || async { Err("database unavailable".to_string()) },
            )
            .await
            .unwrap_err();

        assert_eq!(error, "database unavailable");
        let response = exchange(
            old_port,
            b"POST /web-activity HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
        )
        .await;
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let rebound = StdTcpListener::bind((Ipv4Addr::LOCALHOST, new_port)).unwrap();
        drop(rebound);
        runtime.shutdown().await;
    }

    #[tokio::test]
    async fn saturated_bridge_budget_fails_fast() {
        let state = BridgeTransportState {
            handler: ok_handler(),
            budget: Arc::new(Semaphore::new(WEB_ACTIVITY_BRIDGE_CONCURRENCY_LIMIT)),
        };
        let _all_permits = state
            .budget
            .clone()
            .acquire_many_owned(WEB_ACTIVITY_BRIDGE_CONCURRENCY_LIMIT as u32)
            .await
            .unwrap();
        let mut request = axum::http::Request::builder()
            .method(Method::POST)
            .uri("/web-activity")
            .body(axum::body::Body::empty())
            .unwrap();
        request
            .headers_mut()
            .insert(HOST, HeaderValue::from_static("localhost"));

        let response = bridge_handler(State(state), request).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn shutdown_releases_listener_and_clears_readiness_with_stalled_client() {
        let server = prepare_web_activity_bridge_server(0, ok_handler()).unwrap();
        let port = server.port();
        let ready = Arc::new(AtomicBool::new(false));
        let callback_ready = ready.clone();
        let handle = server.start_with_readiness(Arc::new(move |value| {
            callback_ready.store(value, Ordering::Release);
        }));
        assert!(handle.is_ready());
        assert!(ready.load(Ordering::Acquire));
        let mut stalled = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        stalled
            .write_all(b"POST /web-activity HTTP/1.1\r\nAuthorization:")
            .await
            .unwrap();
        handle.shutdown().await;
        assert!(!ready.load(Ordering::Acquire));
        let rebound = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .unwrap();
        drop(rebound);
    }

    #[tokio::test]
    async fn unexpected_listener_task_exit_clears_readiness() {
        let server = prepare_web_activity_bridge_server(0, ok_handler()).unwrap();
        let ready = Arc::new(AtomicBool::new(false));
        let callback_ready = ready.clone();
        let handle = server.start_with_readiness(Arc::new(move |value| {
            callback_ready.store(value, Ordering::Release);
        }));
        let mut readiness = handle.readiness_rx.clone();
        assert!(*readiness.borrow());

        handle.task.abort();
        tokio::time::timeout(Duration::from_secs(1), readiness.changed())
            .await
            .expect("aborted listener should publish readiness loss")
            .unwrap();

        assert!(!*readiness.borrow());
        assert!(!ready.load(Ordering::Acquire));
        handle.shutdown().await;
    }
}
