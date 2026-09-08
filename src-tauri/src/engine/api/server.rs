use crate::engine::api::{
    auth::ApiCredentialStore,
    context::ApiRuntimeContext,
    router::{self, ApiRequest},
    surface::ApiSurface,
    types::{ApiError, RouteResponse},
};
use crate::engine::runtime_event::{
    RuntimeEventEnvelope, RuntimeEventHub, RuntimeEventSubscription,
};
use axum::{
    body::to_bytes,
    extract::{DefaultBodyLimit, Request, State},
    http::{
        header::{
            ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
            ACCESS_CONTROL_ALLOW_ORIGIN, AUTHORIZATION, HOST, ORIGIN, VARY,
        },
        HeaderMap, HeaderValue, Method, StatusCode,
    },
    response::{sse::Event, IntoResponse, Response, Sse},
    routing::{any, get},
    Json, Router,
};
use futures_util::{FutureExt, Stream};
use std::collections::VecDeque;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::panic::AssertUnwindSafe;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{watch, OwnedSemaphorePermit, Semaphore};
use tower::ServiceBuilder;

pub const DEFAULT_PORT: u16 = 14840;
const BODY_LIMIT: usize = 64 * 1024;
const API_REQUEST_CONCURRENCY_LIMIT: usize = 32;
const SSE_CONNECTION_LIMIT: usize = 8;
const API_HANDLER_TIMEOUT: Duration = Duration::from_secs(15);
const ACTIVITY_IMPORT_HANDLER_TIMEOUT: Duration = Duration::from_secs(120);
const SERVER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const LAST_EVENT_ID: &str = "last-event-id";

pub struct ApiServerState {
    inner: Mutex<ApiServerRuntime>,
}

#[derive(Default)]
struct ApiServerRuntime {
    shutdown_tx: Option<watch::Sender<bool>>,
    port: Option<u16>,
    ready: Option<Arc<AtomicBool>>,
}

pub struct PreparedApiListener {
    port: u16,
    listener: TcpListener,
}

pub struct StandaloneApiServer {
    port: u16,
    listener: TcpListener,
    app: Router,
    #[cfg(test)]
    event_hub: Option<Arc<RuntimeEventHub>>,
    #[cfg_attr(not(test), allow(dead_code))]
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

pub struct ApiServerHandle {
    shutdown_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl ApiServerHandle {
    pub fn readiness(&self) -> watch::Receiver<bool> {
        self.readiness_rx.clone()
    }

    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let mut task = self.task;
        if tokio::time::timeout(SERVER_SHUTDOWN_TIMEOUT, &mut task)
            .await
            .is_err()
        {
            task.abort();
            let _ = task.await;
        }
    }
}

#[cfg(test)]
#[derive(Clone)]
pub struct StandaloneApiShutdown {
    shutdown_tx: watch::Sender<bool>,
    event_hub: Option<Arc<RuntimeEventHub>>,
}

#[cfg(test)]
impl StandaloneApiShutdown {
    pub fn shutdown(&self) {
        if let Some(event_hub) = self.event_hub.as_ref() {
            event_hub.shutdown();
        }
        let _ = self.shutdown_tx.send(true);
    }
}

#[derive(Clone)]
struct ApiTransportState {
    credentials: ApiCredentialStore,
    context: Arc<ApiRuntimeContext>,
    surface: ApiSurface,
    event_hub: Option<Arc<RuntimeEventHub>>,
    api_budget: Arc<Semaphore>,
    sse_budget: Arc<Semaphore>,
}

struct TransportRejection {
    status: StatusCode,
    error: ApiError,
}

impl TransportRejection {
    fn into_response(self) -> Response {
        error_response(self.status, self.error)
    }
}

impl StandaloneApiServer {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn start(self) -> ApiServerHandle {
        let shutdown_tx = self.shutdown_tx.clone();
        let (readiness_tx, readiness_rx) = watch::channel(true);
        let task = tokio::spawn(async move {
            self.run().await;
            let _ = readiness_tx.send(false);
        });
        ApiServerHandle {
            shutdown_tx,
            readiness_rx,
            task,
        }
    }

    #[cfg(test)]
    pub fn shutdown_handle(&self) -> StandaloneApiShutdown {
        StandaloneApiShutdown {
            shutdown_tx: self.shutdown_tx.clone(),
            event_hub: self.event_hub.clone(),
        }
    }

    pub async fn run(self) {
        serve_listener(self.listener, self.app, self.shutdown_rx, "patinad").await;
    }
}

#[cfg(test)]
pub async fn prepare_standalone_server(
    port: u16,
    credentials: ApiCredentialStore,
    context: ApiRuntimeContext,
    surface: ApiSurface,
) -> Result<StandaloneApiServer, String> {
    prepare_standalone_server_internal(port, credentials, context, surface, None).await
}

pub async fn prepare_standalone_server_with_events(
    port: u16,
    credentials: ApiCredentialStore,
    context: ApiRuntimeContext,
    surface: ApiSurface,
    event_hub: Arc<RuntimeEventHub>,
) -> Result<StandaloneApiServer, String> {
    prepare_standalone_server_internal(port, credentials, context, surface, Some(event_hub)).await
}

async fn prepare_standalone_server_internal(
    port: u16,
    credentials: ApiCredentialStore,
    context: ApiRuntimeContext,
    surface: ApiSurface,
    event_hub: Option<Arc<RuntimeEventHub>>,
) -> Result<StandaloneApiServer, String> {
    if surface.has_event_stream() && event_hub.is_none() {
        return Err("API surface requires a runtime event hub".to_string());
    }
    let prepared = prepare_listener(port).await?;
    let app = build_router(credentials, Arc::new(context), surface, event_hub.clone());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    Ok(StandaloneApiServer {
        port: prepared.port,
        listener: prepared.listener,
        app,
        #[cfg(test)]
        event_hub,
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
        let mut guard = self.lock_runtime();
        if let Some(tx) = guard.shutdown_tx.take() {
            let _ = tx.send(true);
        }
        guard.port = None;
        guard.ready = None;
    }

    pub async fn prepare_listener(&self, port: u16) -> Result<PreparedApiListener, String> {
        prepare_listener(port).await
    }

    pub fn install_prepared(
        &self,
        credentials: ApiCredentialStore,
        context: ApiRuntimeContext,
        surface: ApiSurface,
        prepared: PreparedApiListener,
    ) {
        let port = prepared.port();
        let listener = prepared.listener;
        let app = build_router(credentials, Arc::new(context), surface, None);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let ready = Arc::new(AtomicBool::new(true));
        let previous = self.replace_runtime(port, shutdown_tx, ready.clone());

        tokio::spawn(async move {
            println!("[api] listening on http://127.0.0.1:{port}");
            serve_listener(listener, app, shutdown_rx, "api").await;
            ready.store(false, Ordering::Release);
        });
        if let Some(previous) = previous {
            let _ = previous.send(true);
        }
    }

    fn replace_runtime(
        &self,
        port: u16,
        shutdown_tx: watch::Sender<bool>,
        ready: Arc<AtomicBool>,
    ) -> Option<watch::Sender<bool>> {
        let mut guard = self.lock_runtime();
        let previous = guard.shutdown_tx.replace(shutdown_tx);
        guard.port = Some(port);
        guard.ready = Some(ready);
        previous
    }

    pub fn confirmed_port(&self) -> Option<u16> {
        let guard = self.lock_runtime();
        guard
            .ready
            .as_ref()
            .is_some_and(|ready| ready.load(Ordering::Acquire))
            .then_some(guard.port)
            .flatten()
    }

    fn lock_runtime(&self) -> std::sync::MutexGuard<'_, ApiServerRuntime> {
        match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
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

fn build_router(
    credentials: ApiCredentialStore,
    context: Arc<ApiRuntimeContext>,
    surface: ApiSurface,
    event_hub: Option<Arc<RuntimeEventHub>>,
) -> Router {
    let state = ApiTransportState {
        credentials,
        context,
        surface,
        event_hub,
        api_budget: Arc::new(Semaphore::new(API_REQUEST_CONCURRENCY_LIMIT)),
        sse_budget: Arc::new(Semaphore::new(SSE_CONNECTION_LIMIT)),
    };
    Router::new()
        .route(
            "/api/v1/events",
            get(events_handler).options(preflight_handler),
        )
        .fallback(any(api_handler))
        .with_state(state)
        .layer(ServiceBuilder::new().layer(DefaultBodyLimit::disable()))
}

async fn serve_listener(
    listener: TcpListener,
    app: Router,
    mut shutdown: watch::Receiver<bool>,
    log_target: &'static str,
) {
    let shutdown_signal = async move {
        if !*shutdown.borrow() {
            let _ = shutdown.changed().await;
        }
    };
    if let Err(error) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
    {
        eprintln!("[{log_target}] API server stopped: {error}");
    }
}

async fn api_handler(State(state): State<ApiTransportState>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let method = parts.method;
    let uri = parts.uri;
    let headers = parts.headers;
    let cors_origin = match validate_request_boundary(&headers) {
        Ok(origin) => origin,
        Err(rejection) => return rejection.into_response(),
    };
    if method == Method::OPTIONS {
        return preflight_response(cors_origin);
    }
    if !state
        .credentials
        .validate(headers.get(AUTHORIZATION).and_then(header_text))
    {
        return with_cors(
            error_response(StatusCode::UNAUTHORIZED, ApiError::unauthorized()),
            cors_origin,
        );
    }
    let Ok(_permit) = state.api_budget.clone().try_acquire_owned() else {
        return with_cors(
            error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                ApiError::unavailable("API request concurrency limit reached"),
            ),
            cors_origin,
        );
    };
    let body = match to_bytes(body, BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => {
            return with_cors(
                error_response(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    ApiError::bad_request("request body is too large"),
                ),
                cors_origin,
            )
        }
    };
    let request = ApiRequest {
        method: method.as_str().to_string(),
        path: uri.path().to_string(),
        query: uri.query().map(str::to_string),
        body: body.to_vec(),
    };
    let request_label = format!("{} {}", request.method, request.path);
    let handler_timeout = if request.path == "/api/v1/imports/canonical/commit" {
        ACTIVITY_IMPORT_HANDLER_TIMEOUT
    } else {
        API_HANDLER_TIMEOUT
    };
    let routed = AssertUnwindSafe(router::route_request(
        request,
        state.context.as_ref(),
        state.surface,
    ))
    .catch_unwind();
    let response = match tokio::time::timeout(handler_timeout, routed).await {
        Ok(Ok(response)) => route_response(response),
        Ok(Err(_)) => {
            eprintln!("[api] handler panicked while serving {request_label}");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError::internal("handler panicked"),
            )
        }
        Err(_) => {
            eprintln!("[api] handler timed out while serving {request_label}");
            error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                ApiError::unavailable("API request timed out"),
            )
        }
    };
    with_cors(response, cors_origin)
}

async fn events_handler(State(state): State<ApiTransportState>, headers: HeaderMap) -> Response {
    let cors_origin = match validate_request_boundary(&headers) {
        Ok(origin) => origin,
        Err(rejection) => return rejection.into_response(),
    };
    let Some(credential_revision) = state
        .credentials
        .validate_with_revision(headers.get(AUTHORIZATION).and_then(header_text))
    else {
        return with_cors(
            error_response(StatusCode::UNAUTHORIZED, ApiError::unauthorized()),
            cors_origin,
        );
    };
    if !state.surface.allows_request("GET", "/api/v1/events") {
        return with_cors(
            error_response(
                StatusCode::NOT_FOUND,
                ApiError::not_found("endpoint not available"),
            ),
            cors_origin,
        );
    }
    let last_event_id = match parse_last_event_id(&headers) {
        Ok(value) => value,
        Err(rejection) => return with_cors(rejection.into_response(), cors_origin),
    };
    let Some(event_hub) = state.event_hub.as_ref() else {
        return with_cors(
            error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                ApiError::unavailable("runtime event stream is not ready"),
            ),
            cors_origin,
        );
    };
    let Ok(permit) = state.sse_budget.clone().try_acquire_owned() else {
        return with_cors(
            error_response(
                StatusCode::SERVICE_UNAVAILABLE,
                ApiError::unavailable("event stream connection limit reached"),
            ),
            cors_origin,
        );
    };
    let stream = event_stream(
        event_hub.subscribe_after(last_event_id),
        credential_revision,
        permit,
    );
    let response = Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response();
    with_cors(response, cors_origin)
}

async fn preflight_handler(headers: HeaderMap) -> Response {
    match validate_request_boundary(&headers) {
        Ok(origin) => preflight_response(origin),
        Err(rejection) => rejection.into_response(),
    }
}

fn event_stream(
    subscription: RuntimeEventSubscription,
    credential_revision: watch::Receiver<u64>,
    permit: OwnedSemaphorePermit,
) -> impl Stream<Item = Result<Event, Infallible>> {
    struct StreamState {
        subscription: RuntimeEventSubscription,
        credential_revision: watch::Receiver<u64>,
        pending: VecDeque<Event>,
        _permit: OwnedSemaphorePermit,
    }

    let mut pending = VecDeque::new();
    if subscription.resync_required {
        pending.push_back(resync_event("replay-gap", None));
    }
    pending.extend(subscription.replay.iter().map(runtime_event));
    futures_util::stream::unfold(
        StreamState {
            subscription,
            credential_revision,
            pending,
            _permit: permit,
        },
        |mut state| async move {
            if state.credential_revision.has_changed().unwrap_or(true) {
                return None;
            }
            if let Some(event) = state.pending.pop_front() {
                return Some((Ok(event), state));
            }
            if *state.subscription.shutdown.borrow() {
                return None;
            }
            loop {
                tokio::select! {
                    event = state.subscription.receiver.recv() => {
                        match event {
                            Ok(event) => return Some((Ok(runtime_event(&event)), state)),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
                                return Some((Ok(resync_event("receiver-lagged", Some(missed))), state));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                        }
                    }
                    changed = state.subscription.shutdown.changed() => {
                        if changed.is_err() || *state.subscription.shutdown.borrow() {
                            return None;
                        }
                    }
                    _ = state.credential_revision.changed() => return None,
                }
            }
        },
    )
}

fn runtime_event(envelope: &RuntimeEventEnvelope) -> Event {
    let data = serde_json::to_string(envelope).unwrap_or_else(|_| "{}".to_string());
    Event::default()
        .id(envelope.sequence.to_string())
        .event(envelope.event.event_name())
        .data(data)
}

fn resync_event(reason: &str, missed: Option<u64>) -> Event {
    Event::default().event("resync-required").data(
        serde_json::json!({
            "reason": reason,
            "missed": missed,
        })
        .to_string(),
    )
}

fn validate_request_boundary(
    headers: &HeaderMap,
) -> Result<Option<HeaderValue>, TransportRejection> {
    let Some(host) = headers.get(HOST).and_then(header_text) else {
        return Err(TransportRejection {
            status: StatusCode::BAD_REQUEST,
            error: ApiError::bad_request("missing Host header"),
        });
    };
    if !is_loopback_authority(host) {
        return Err(TransportRejection {
            status: StatusCode::BAD_REQUEST,
            error: ApiError::bad_request("Host must resolve to loopback"),
        });
    }
    let Some(origin) = headers.get(ORIGIN) else {
        return Ok(None);
    };
    let Some(origin_text) = header_text(origin) else {
        return Err(TransportRejection {
            status: StatusCode::BAD_REQUEST,
            error: ApiError::bad_request("invalid Origin header"),
        });
    };
    if !is_loopback_origin(origin_text) {
        return Err(TransportRejection {
            status: StatusCode::FORBIDDEN,
            error: ApiError::forbidden("Origin is not allowed"),
        });
    }
    Ok(Some(origin.clone()))
}

fn is_loopback_authority(authority: &str) -> bool {
    axum::http::uri::Authority::from_str(authority)
        .ok()
        .is_some_and(|authority| is_loopback_host(authority.host()))
}

fn is_loopback_origin(origin: &str) -> bool {
    url::Url::parse(origin).ok().is_some_and(|origin| {
        matches!(origin.scheme(), "http" | "https" | "tauri")
            && origin.host_str().is_some_and(is_loopback_host)
    })
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

fn parse_last_event_id(headers: &HeaderMap) -> Result<Option<u64>, TransportRejection> {
    headers
        .get(LAST_EVENT_ID)
        .map(|value| {
            header_text(value)
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or_else(|| TransportRejection {
                    status: StatusCode::BAD_REQUEST,
                    error: ApiError::bad_request("invalid Last-Event-ID"),
                })
        })
        .transpose()
}

fn header_text(value: &HeaderValue) -> Option<&str> {
    value.to_str().ok()
}

fn route_response(response: RouteResponse) -> Response {
    let status = StatusCode::from_u16(response.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (status, Json(response.body)).into_response()
}

fn error_response(status: StatusCode, error: ApiError) -> Response {
    (status, Json(error)).into_response()
}

fn preflight_response(origin: Option<HeaderValue>) -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    let headers = response.headers_mut();
    headers.insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Authorization, Content-Type, Last-Event-ID"),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, DELETE, OPTIONS"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::runtime_event::{RuntimeEvent, RuntimeEventSink};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn test_credentials() -> ApiCredentialStore {
        let path = std::env::temp_dir().join(format!(
            "patina-server-token-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ));
        let credentials = ApiCredentialStore::new();
        credentials
            .initialize_at(&path, Some("test-token"))
            .unwrap();
        credentials
    }

    async fn test_context() -> ApiRuntimeContext {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::Executor::execute(&pool, crate::data::schema::CURRENT_BASELINE_SCHEMA_SQL)
            .await
            .unwrap();
        ApiRuntimeContext::new(crate::engine::runtime_context::RuntimeContext::system(pool))
    }

    async fn request(port: u16, request: &[u8]) -> String {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream.write_all(request).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response
    }

    #[tokio::test]
    async fn occupied_port_preparation_preserves_the_active_runtime() {
        let occupied = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let occupied_port = occupied.local_addr().unwrap().port();
        let state = ApiServerState::new();
        let (old_shutdown_tx, old_shutdown) = watch::channel(false);
        {
            let mut guard = state.inner.lock().unwrap();
            guard.port = Some(14_840);
            guard.shutdown_tx = Some(old_shutdown_tx);
            guard.ready = Some(Arc::new(AtomicBool::new(true)));
        }

        assert!(state.prepare_listener(occupied_port).await.is_err());
        assert_eq!(state.confirmed_port(), Some(14_840));
        assert!(!*old_shutdown.borrow());
    }

    #[tokio::test]
    async fn daemon_event_surface_requires_an_event_hub() {
        let result = prepare_standalone_server(
            0,
            test_credentials(),
            test_context().await,
            ApiSurface::DaemonReadOnly,
        )
        .await;

        let error = match result {
            Ok(_) => panic!("daemon event surface must not advertise an unavailable stream"),
            Err(error) => error,
        };
        assert_eq!(error, "API surface requires a runtime event hub");
    }

    #[test]
    fn confirmed_port_tracks_live_task_readiness() {
        let state = ApiServerState::new();
        let ready = Arc::new(AtomicBool::new(true));
        {
            let mut guard = state.inner.lock().unwrap();
            guard.port = Some(14_840);
            guard.ready = Some(ready.clone());
        }

        assert_eq!(state.confirmed_port(), Some(14_840));
        ready.store(false, Ordering::Release);
        assert_eq!(state.confirmed_port(), None);
    }

    #[tokio::test]
    async fn standalone_server_enforces_auth_host_origin_and_body_limit() {
        let server = prepare_standalone_server(
            0,
            test_credentials(),
            test_context().await,
            ApiSurface::Desktop,
        )
        .await
        .unwrap();
        let port = server.port();
        let handle = server.start();

        let ok = request(
            port,
            b"GET /api/v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer test-token\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(ok.starts_with("HTTP/1.1 200 OK"));
        assert!(!ok.contains("access-control-allow-origin"));

        let allowed_origin = request(
            port,
            b"GET /api/v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://localhost:3000\r\nAuthorization: Bearer test-token\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(allowed_origin.starts_with("HTTP/1.1 200 OK"));
        assert!(allowed_origin.contains("access-control-allow-origin: http://localhost:3000"));

        let preflight = request(
            port,
            b"OPTIONS /api/v1/health HTTP/1.1\r\nHost: localhost\r\nOrigin: http://localhost:3000\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(preflight.starts_with("HTTP/1.1 204 No Content"));
        assert!(preflight.contains("access-control-allow-origin: http://localhost:3000"));

        let hostile_origin = request(
            port,
            b"GET /api/v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: https://example.com\r\nAuthorization: Bearer test-token\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(hostile_origin.starts_with("HTTP/1.1 403 Forbidden"));

        let hostile_host = request(
            port,
            b"GET /api/v1/health HTTP/1.1\r\nHost: example.com\r\nAuthorization: Bearer test-token\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(hostile_host.starts_with("HTTP/1.1 400 Bad Request"));

        let missing_token = request(
            port,
            b"GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(missing_token.starts_with("HTTP/1.1 401 Unauthorized"));

        let unauthorized_write = request(
            port,
            b"POST /api/v1/settings/tracker/pause HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"paused\":true}",
        )
        .await;
        assert!(unauthorized_write.starts_with("HTTP/1.1 401 Unauthorized"));

        let authorized_write = request(
            port,
            b"POST /api/v1/settings/tracker/pause HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer test-token\r\nContent-Type: application/json\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"paused\":true}",
        )
        .await;
        assert!(authorized_write.starts_with("HTTP/1.1 200 OK"));

        let oversized = request(
            port,
            format!(
                "POST /api/v1/settings/tracker/afk-threshold HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer test-token\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                BODY_LIMIT + 1,
                "x".repeat(BODY_LIMIT + 1)
            )
            .as_bytes(),
        )
        .await;
        assert!(oversized.starts_with("HTTP/1.1 413 Payload Too Large"));
        assert!(oversized.contains("\"code\":\"bad_request\""));

        handle.shutdown().await;
    }

    #[tokio::test]
    async fn event_stream_replays_and_stops_during_graceful_shutdown() {
        let hub = Arc::new(RuntimeEventHub::new(8));
        hub.emit(RuntimeEvent::TrackingDataChanged {
            reason: "window-changed".to_string(),
            changed_at_ms: 1_000,
        })
        .unwrap();
        let server = prepare_standalone_server_with_events(
            0,
            test_credentials(),
            test_context().await,
            ApiSurface::DaemonReadOnly,
            hub,
        )
        .await
        .unwrap();
        let port = server.port();
        let shutdown = server.shutdown_handle();
        let server_task = tokio::spawn(server.run());

        let invalid_cursor = request(
            port,
            b"GET /api/v1/events HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer test-token\r\nLast-Event-ID: stale\r\nConnection: close\r\n\r\n",
        )
        .await;
        assert!(invalid_cursor.starts_with("HTTP/1.1 400 Bad Request"));
        assert!(invalid_cursor.contains("invalid Last-Event-ID"));

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        stream
            .write_all(
                b"GET /api/v1/events HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer test-token\r\nLast-Event-ID: 0\r\n\r\n",
            )
            .await
            .unwrap();
        let mut response = vec![0_u8; 2048];
        let read = tokio::time::timeout(Duration::from_secs(1), stream.read(&mut response))
            .await
            .unwrap()
            .unwrap();
        let response = String::from_utf8_lossy(&response[..read]);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("content-type: text/event-stream"));
        assert!(response.contains("event: tracking-data-changed"));
        assert!(response.contains("\"sequence\":1"));

        shutdown.shutdown();
        tokio::time::timeout(Duration::from_secs(2), server_task)
            .await
            .expect("server should stop after event hub shutdown")
            .unwrap();
    }

    #[tokio::test]
    async fn token_rotation_revokes_sse_before_buffered_replay_is_emitted() {
        use futures_util::StreamExt;

        let credentials = test_credentials();
        let revision = credentials
            .validate_with_revision(Some("Bearer test-token"))
            .unwrap();
        let hub = RuntimeEventHub::new(8);
        hub.emit(RuntimeEvent::TrackingDataChanged {
            reason: "before-rotation".to_string(),
            changed_at_ms: 1_000,
        })
        .unwrap();
        let permit = Arc::new(Semaphore::new(1)).try_acquire_owned().unwrap();
        let stream = event_stream(hub.subscribe_after(None), revision, permit);
        futures_util::pin_mut!(stream);

        credentials.rotate().unwrap();

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn saturated_api_budget_fails_fast() {
        let state = ApiTransportState {
            credentials: test_credentials(),
            context: Arc::new(test_context().await),
            surface: ApiSurface::Desktop,
            event_hub: None,
            api_budget: Arc::new(Semaphore::new(API_REQUEST_CONCURRENCY_LIMIT)),
            sse_budget: Arc::new(Semaphore::new(SSE_CONNECTION_LIMIT)),
        };
        let _all_permits = state
            .api_budget
            .clone()
            .acquire_many_owned(API_REQUEST_CONCURRENCY_LIMIT as u32)
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("localhost"));
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer test-token"));

        let mut request = axum::http::Request::builder()
            .method(Method::GET)
            .uri(axum::http::Uri::from_static("/api/v1/health"))
            .body(axum::body::Body::empty())
            .unwrap();
        *request.headers_mut() = headers;
        let response = api_handler(State(state), request).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn saturated_sse_budget_fails_fast() {
        let state = ApiTransportState {
            credentials: test_credentials(),
            context: Arc::new(test_context().await),
            surface: ApiSurface::DaemonReadOnly,
            event_hub: Some(Arc::new(RuntimeEventHub::new(8))),
            api_budget: Arc::new(Semaphore::new(API_REQUEST_CONCURRENCY_LIMIT)),
            sse_budget: Arc::new(Semaphore::new(SSE_CONNECTION_LIMIT)),
        };
        let _all_permits = state
            .sse_budget
            .clone()
            .acquire_many_owned(SSE_CONNECTION_LIMIT as u32)
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("localhost"));
        headers.insert(AUTHORIZATION, HeaderValue::from_static("Bearer test-token"));

        let response = events_handler(State(state), headers).await;

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn loopback_boundary_accepts_only_local_authorities_and_origins() {
        assert!(is_loopback_authority("127.0.0.1:14840"));
        assert!(is_loopback_authority("localhost"));
        assert!(is_loopback_authority("[::1]:14840"));
        assert!(!is_loopback_authority("example.com"));
        assert!(is_loopback_origin("http://127.0.0.1:14840"));
        assert!(is_loopback_origin("https://localhost"));
        assert!(is_loopback_origin("tauri://localhost"));
        assert!(!is_loopback_origin("https://example.com"));
        assert!(!is_loopback_origin("null"));
    }
}
