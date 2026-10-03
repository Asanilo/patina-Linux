//! Loopback-only client transport shared by native Patina clients.
//! This crate does not discover credentials, touch storage, or own tracking.
//! Hosts negotiate capabilities before using runtime data. Writes are never retried.
mod activity;
mod configuration;
mod product_settings;
mod dashboard;
mod error;
pub mod events;
mod history;
mod icons;
mod maintenance;
mod web_history;
pub mod state;
pub mod sync;
pub use patina_protocol as protocol;
mod sse_budget;
pub use error::ClientError;
pub use eventsource_stream::Event;
use eventsource_stream::Eventsource;
use futures_util::{stream::BoxStream, StreamExt};
use protocol::{ApiError, ApiResponse, CapabilitiesResponse};
use reqwest::{
    header::{ACCEPT, CONTENT_TYPE},
    redirect::Policy,
    StatusCode,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{fmt, time::Duration};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(750);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024;

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
    base_url: String,
    token: String,
}

impl fmt::Debug for Client {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Client")
            .field("base_url", &self.base_url)
            .field("token", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Negotiation {
    pub server_version: String,
    pub protocol_version: u32,
    pub tracking_ready: bool,
    pub event_stream_available: bool,
}

pub struct EventStream {
    inner: BoxStream<'static, Result<Event, String>>,
    instance_id: Option<String>,
}
impl EventStream {
    pub fn instance_id(&self) -> Option<&str> {
        self.instance_id.as_deref()
    }
    pub async fn next_event(&mut self) -> Result<Option<Event>, ClientError> {
        self.inner
            .next()
            .await
            .transpose()
            .map_err(ClientError::Unreachable)
    }
}

impl Client {
    pub fn new(port: u16, token: impl Into<String>) -> Result<Self, ClientError> {
        if port == 0 {
            return Err(ClientError::InvalidConfiguration(
                "patinad client port must not be zero".to_string(),
            ));
        }
        let token = token.into();
        if token.trim().is_empty() {
            return Err(ClientError::InvalidConfiguration(
                "patinad API credential is missing".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .no_proxy()
            .redirect(Policy::none())
            .user_agent(format!("Patina-Client/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| {
                ClientError::InvalidConfiguration(format!(
                    "failed to build patinad client: {error}"
                ))
            })?;

        Ok(Self {
            client,
            base_url: format!("http://127.0.0.1:{port}"),
            token,
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn capabilities(&self) -> Result<CapabilitiesResponse, ClientError> {
        self.get_json("/api/v1/capabilities", "capabilities").await
    }

    pub async fn open_event_stream(
        &self,
        after_sequence: Option<u64>,
    ) -> Result<EventStream, ClientError> {
        let mut request = self
            .client
            .get(format!("{}/api/v1/events", self.base_url))
            .bearer_auth(&self.token)
            .header(ACCEPT, "text/event-stream");
        if let Some(sequence) = after_sequence {
            request = request.header("Last-Event-ID", sequence.to_string());
        }
        let response = tokio::time::timeout(REQUEST_TIMEOUT, request.send())
            .await
            .map_err(|_| {
                ClientError::Unreachable("timed out while opening patinad event stream".to_string())
            })?
            .map_err(map_transport_error)?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if !status.is_success() {
            let body = tokio::time::timeout(REQUEST_TIMEOUT, read_limited_body(response))
                .await
                .map_err(|_| {
                    ClientError::Unreachable(
                        "timed out while reading patinad event stream error".to_string(),
                    )
                })??;
            return Err(map_http_error(status, &body));
        }
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !content_type
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .eq_ignore_ascii_case("text/event-stream")
        {
            return Err(ClientError::InvalidResponse(
                "patinad event stream returned an unexpected content type".to_string(),
            ));
        }
        let instance_id = response
            .headers()
            .get(protocol::EVENT_INSTANCE_HEADER)
            .map(|value| {
                value
                    .to_str()
                    .ok()
                    .filter(|id| {
                        !id.is_empty()
                            && id.len() <= 128
                            && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                    })
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        ClientError::InvalidResponse(
                            "invalid event stream instance identity".into(),
                        )
                    })
            })
            .transpose()?;
        let mut budget = sse_budget::FrameBudget::default();
        let inner = response
            .bytes_stream()
            .map(move |chunk| {
                let bytes = chunk.map_err(|_| "patinad event transport failed".to_string())?;
                budget.feed(&bytes).map_err(str::to_string)?;
                Ok::<_, String>(bytes)
            })
            .eventsource()
            .map(|event| event.map_err(|error| format!("patinad event stream failed: {error}")))
            .boxed();
        Ok(EventStream { inner, instance_id })
    }

    pub async fn open_runtime_event_stream(
        &self,
        after_sequence: Option<u64>,
    ) -> Result<events::RuntimeEventStream, ClientError> {
        Ok(events::RuntimeEventStream {
            inner: self.open_event_stream(after_sequence).await?,
        })
    }

    pub async fn negotiate_tracking_owner(&self) -> Result<Negotiation, ClientError> {
        let capabilities = self.capabilities().await?;
        negotiate_tracking_capabilities(capabilities)
    }

    pub async fn get_json<T>(&self, path: &str, response_name: &str) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
    {
        self.get_json_with_timeout(path, response_name, REQUEST_TIMEOUT)
            .await
    }

    pub async fn get_json_with_timeout<T: DeserializeOwned>(
        &self,
        path: &str,
        response_name: &str,
        timeout: Duration,
    ) -> Result<T, ClientError> {
        self.get_json_with_limits(path, response_name, timeout, MAX_RESPONSE_BYTES)
            .await
    }

    pub async fn get_json_with_limits<T: DeserializeOwned>(
        &self,
        path: &str,
        response_name: &str,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<T, ClientError> {
        let response = self
            .client
            .get(self.endpoint(path)?)
            .bearer_auth(&self.token)
            .timeout(timeout)
            .send()
            .await
            .map_err(map_transport_error)?;
        let status = response.status();
        let body = read_body_with_limit(response, max_bytes).await?;

        if status == StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if !status.is_success() {
            return Err(map_http_error(status, &body));
        }

        serde_json::from_slice::<ApiResponse<T>>(&body)
            .map(|response| response.data)
            .map_err(|error| {
                ClientError::InvalidResponse(format!(
                    "failed to decode patinad {response_name}: {error}"
                ))
            })
    }

    pub async fn post_ack<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
        response_name: &str,
    ) -> Result<(), ClientError> {
        let _: serde_json::Value = self.post_json(path, body, response_name).await?;
        Ok(())
    }

    pub async fn post_empty_json<T>(
        &self,
        path: &str,
        response_name: &str,
    ) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
    {
        self.post_json(path, &serde_json::json!({}), response_name)
            .await
    }

    pub async fn post_json<T, B>(
        &self,
        path: &str,
        body: &B,
        response_name: &str,
    ) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.post_json_with_timeout(path, body, response_name, REQUEST_TIMEOUT)
            .await
    }

    pub async fn post_json_with_timeout<T, B>(
        &self,
        path: &str,
        body: &B,
        response_name: &str,
        timeout: Duration,
    ) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let request_body = serde_json::to_vec(body).map_err(|error| {
            ClientError::InvalidConfiguration(format!(
                "failed to encode patinad {response_name} request: {error}"
            ))
        })?;
        let response = self
            .client
            .post(self.endpoint(path)?)
            .bearer_auth(&self.token)
            .header(CONTENT_TYPE, "application/json")
            .body(request_body)
            .timeout(timeout)
            .send()
            .await
            .map_err(map_transport_error)?;
        let status = response.status();
        let body = read_limited_body(response).await?;

        if status == StatusCode::UNAUTHORIZED {
            return Err(ClientError::Unauthorized);
        }
        if !status.is_success() {
            return Err(map_http_error(status, &body));
        }

        serde_json::from_slice::<ApiResponse<T>>(&body)
            .map(|response| response.data)
            .map_err(|error| {
                ClientError::InvalidResponse(format!(
                    "failed to decode patinad {response_name}: {error}"
                ))
            })
    }
    fn endpoint(&self, path: &str) -> Result<String, ClientError> {
        if !path.starts_with("/api/v1/")
            || path.contains(['#', '\\'])
            || path.bytes().any(|b| b.is_ascii_control())
        {
            return Err(ClientError::InvalidConfiguration(
                "expected an /api/v1/ endpoint".into(),
            ));
        }
        let value = format!("{}{path}", self.base_url);
        let url = reqwest::Url::parse(&value)
            .map_err(|_| ClientError::InvalidConfiguration("invalid API endpoint".into()))?;
        if !url.path().starts_with("/api/v1/") {
            return Err(ClientError::InvalidConfiguration(
                "API endpoint escapes its prefix".into(),
            ));
        }
        Ok(value)
    }
}
pub fn negotiate_tracking_capabilities(
    capabilities: CapabilitiesResponse,
) -> Result<Negotiation, ClientError> {
    if capabilities.runtime_host != "daemon" {
        return Err(ClientError::WrongRuntimeHost(capabilities.runtime_host));
    }

    let client_protocol = protocol::CURRENT_PROTOCOL_VERSION;
    let protocol_compatible = capabilities.protocol.current == capabilities.protocol_version
        && (capabilities.protocol.min_supported_client
            ..=capabilities.protocol.max_supported_client)
            .contains(&client_protocol);
    if !protocol_compatible {
        return Err(ClientError::IncompatibleProtocol {
            client: client_protocol,
            server: capabilities.protocol_version,
            min_supported_client: capabilities.protocol.min_supported_client,
            max_supported_client: capabilities.protocol.max_supported_client,
        });
    }
    if !capabilities.tracking.owned {
        return Err(ClientError::TrackingNotOwned);
    }
    if !capabilities.event_stream.available {
        return Err(ClientError::EventStreamUnavailable);
    }

    Ok(Negotiation {
        server_version: capabilities.server_version,
        protocol_version: capabilities.protocol_version,
        tracking_ready: capabilities.tracking.ready,
        event_stream_available: capabilities.event_stream.available,
    })
}

async fn read_limited_body(response: reqwest::Response) -> Result<Vec<u8>, ClientError> {
    read_body_with_limit(response, MAX_RESPONSE_BYTES).await
}

async fn read_body_with_limit(
    response: reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>, ClientError> {
    if response
        .content_length()
        .is_some_and(|length| length > max_bytes as u64)
    {
        return Err(ClientError::ResponseTooLarge);
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_transport_error)?;
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err(ClientError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn map_transport_error(error: reqwest::Error) -> ClientError {
    let message = if error.is_timeout() {
        "timed out while connecting to patinad"
    } else if error.is_connect() {
        "could not connect to patinad"
    } else {
        "patinad transport failed"
    };
    ClientError::Unreachable(message.to_string())
}

fn map_http_error(status: StatusCode, body: &[u8]) -> ClientError {
    let parsed = serde_json::from_slice::<ApiError>(body).ok();
    ClientError::Http {
        status: status.as_u16(),
        code: parsed.as_ref().map(|error| error.error.code.clone()),
        message: parsed
            .map(|error| error.error.message)
            .unwrap_or_else(|| "patinad request failed".to_string()),
    }
}
