use futures_util::StreamExt;
use reqwest::{redirect::Policy, StatusCode};
use std::fmt;
use std::time::Duration;

use crate::engine::api::types::{ApiError, ApiResponse, CapabilitiesResponse};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(750);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

pub struct PatinadClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
}

impl fmt::Debug for PatinadClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PatinadClient")
            .field("base_url", &self.base_url)
            .field("token", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatinadNegotiation {
    pub server_version: String,
    pub protocol_version: u32,
    pub tracking_ready: bool,
    pub event_stream_available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatinadClientError {
    InvalidConfiguration(String),
    Unreachable(String),
    Unauthorized,
    Http {
        status: u16,
        code: Option<String>,
        message: String,
    },
    ResponseTooLarge,
    InvalidResponse(String),
    WrongRuntimeHost(String),
    IncompatibleProtocol {
        client: u32,
        server: u32,
        min_supported_client: u32,
        max_supported_client: u32,
    },
    TrackingNotOwned,
    EventStreamUnavailable,
}

impl PatinadClientError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfiguration(_) => "invalid-configuration",
            Self::Unreachable(_) => "unreachable",
            Self::Unauthorized => "unauthorized",
            Self::Http { .. } => "http-error",
            Self::ResponseTooLarge => "response-too-large",
            Self::InvalidResponse(_) => "invalid-response",
            Self::WrongRuntimeHost(_) => "wrong-runtime-host",
            Self::IncompatibleProtocol { .. } => "incompatible-protocol",
            Self::TrackingNotOwned => "tracking-not-owned",
            Self::EventStreamUnavailable => "event-stream-unavailable",
        }
    }
}

impl fmt::Display for PatinadClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message)
            | Self::Unreachable(message)
            | Self::InvalidResponse(message) => formatter.write_str(message),
            Self::Unauthorized => formatter.write_str("patinad rejected the API credential"),
            Self::Http {
                status,
                code,
                message,
            } => write!(
                formatter,
                "patinad returned HTTP {status}{}: {message}",
                code.as_ref()
                    .map(|code| format!(" ({code})"))
                    .unwrap_or_default()
            ),
            Self::ResponseTooLarge => {
                formatter.write_str("patinad response exceeded the local client size limit")
            }
            Self::WrongRuntimeHost(host) => {
                write!(formatter, "expected patinad runtime host, received `{host}`")
            }
            Self::IncompatibleProtocol {
                client,
                server,
                min_supported_client,
                max_supported_client,
            } => write!(
                formatter,
                "client protocol {client} is incompatible with server protocol {server} (server accepts clients {min_supported_client}..={max_supported_client})"
            ),
            Self::TrackingNotOwned => {
                formatter.write_str("patinad does not own tracking for this profile")
            }
            Self::EventStreamUnavailable => {
                formatter.write_str("patinad event stream is unavailable")
            }
        }
    }
}

impl std::error::Error for PatinadClientError {}

impl PatinadClient {
    pub fn new(port: u16, token: impl Into<String>) -> Result<Self, PatinadClientError> {
        if port == 0 {
            return Err(PatinadClientError::InvalidConfiguration(
                "patinad client port must not be zero".to_string(),
            ));
        }
        let token = token.into();
        if token.trim().is_empty() {
            return Err(PatinadClientError::InvalidConfiguration(
                "patinad API credential is missing".to_string(),
            ));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .redirect(Policy::none())
            .user_agent(format!("Patina-Desktop/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| {
                PatinadClientError::InvalidConfiguration(format!(
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

    pub async fn capabilities(&self) -> Result<CapabilitiesResponse, PatinadClientError> {
        let response = self
            .client
            .get(format!("{}/api/v1/capabilities", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(map_transport_error)?;
        let status = response.status();
        let body = read_limited_body(response).await?;

        if status == StatusCode::UNAUTHORIZED {
            return Err(PatinadClientError::Unauthorized);
        }
        if !status.is_success() {
            return Err(map_http_error(status, &body));
        }

        serde_json::from_slice::<ApiResponse<CapabilitiesResponse>>(&body)
            .map(|response| response.data)
            .map_err(|error| {
                PatinadClientError::InvalidResponse(format!(
                    "failed to decode patinad capabilities: {error}"
                ))
            })
    }

    pub async fn negotiate_tracking_owner(&self) -> Result<PatinadNegotiation, PatinadClientError> {
        let capabilities = self.capabilities().await?;
        negotiate_tracking_capabilities(capabilities)
    }
}

fn negotiate_tracking_capabilities(
    capabilities: CapabilitiesResponse,
) -> Result<PatinadNegotiation, PatinadClientError> {
    if capabilities.runtime_host != "daemon" {
        return Err(PatinadClientError::WrongRuntimeHost(
            capabilities.runtime_host,
        ));
    }

    let client_protocol = crate::engine::api::protocol::CURRENT_PROTOCOL_VERSION;
    let protocol_compatible = capabilities.protocol.current == capabilities.protocol_version
        && (capabilities.protocol.min_supported_client
            ..=capabilities.protocol.max_supported_client)
            .contains(&client_protocol);
    if !protocol_compatible {
        return Err(PatinadClientError::IncompatibleProtocol {
            client: client_protocol,
            server: capabilities.protocol_version,
            min_supported_client: capabilities.protocol.min_supported_client,
            max_supported_client: capabilities.protocol.max_supported_client,
        });
    }
    if !capabilities.tracking.owned {
        return Err(PatinadClientError::TrackingNotOwned);
    }
    if !capabilities.event_stream.available {
        return Err(PatinadClientError::EventStreamUnavailable);
    }

    Ok(PatinadNegotiation {
        server_version: capabilities.server_version,
        protocol_version: capabilities.protocol_version,
        tracking_ready: capabilities.tracking.ready,
        event_stream_available: capabilities.event_stream.available,
    })
}

async fn read_limited_body(response: reqwest::Response) -> Result<Vec<u8>, PatinadClientError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return Err(PatinadClientError::ResponseTooLarge);
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(map_transport_error)?;
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(PatinadClientError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn map_transport_error(error: reqwest::Error) -> PatinadClientError {
    let message = if error.is_timeout() {
        "timed out while connecting to patinad"
    } else if error.is_connect() {
        "could not connect to patinad"
    } else {
        "patinad transport failed"
    };
    PatinadClientError::Unreachable(format!("{message}: {error}"))
}

fn map_http_error(status: StatusCode, body: &[u8]) -> PatinadClientError {
    let parsed = serde_json::from_slice::<ApiError>(body).ok();
    PatinadClientError::Http {
        status: status.as_u16(),
        code: parsed.as_ref().map(|error| error.error.code.clone()),
        message: parsed
            .map(|error| error.error.message)
            .unwrap_or_else(|| "patinad request failed".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::api::types::{
        AvailabilityCapability, OwnedRuntimeCapability, ProtocolCapability, WriteApiCapability,
    };

    #[test]
    fn debug_output_never_contains_the_bearer_token() {
        let client = PatinadClient::new(14840, "patina_api_do-not-log").unwrap();
        let debug = format!("{client:?}");

        assert!(debug.contains("[redacted]"));
        assert!(!debug.contains("do-not-log"));
    }

    #[test]
    fn negotiation_rejects_the_desktop_api_even_when_it_is_reachable() {
        let error =
            negotiate_tracking_capabilities(capabilities("desktop", 1, true, true)).unwrap_err();

        assert_eq!(error.code(), "wrong-runtime-host");
    }

    #[test]
    fn negotiation_rejects_incompatible_protocols_before_runtime_use() {
        let error =
            negotiate_tracking_capabilities(capabilities("daemon", 2, true, true)).unwrap_err();

        assert_eq!(error.code(), "incompatible-protocol");
    }

    #[test]
    fn negotiation_accepts_a_newer_server_that_explicitly_supports_this_client() {
        let mut response = capabilities("daemon", 2, true, true);
        response.protocol.min_supported_client = 1;

        let negotiated = negotiate_tracking_capabilities(response).unwrap();

        assert_eq!(negotiated.protocol_version, 2);
    }

    #[test]
    fn negotiation_accepts_a_starting_tracking_owner() {
        let negotiated =
            negotiate_tracking_capabilities(capabilities("daemon", 1, false, true)).unwrap();

        assert!(!negotiated.tracking_ready);
        assert!(negotiated.event_stream_available);
    }

    fn capabilities(
        runtime_host: &str,
        protocol_version: u32,
        tracking_ready: bool,
        event_stream_available: bool,
    ) -> CapabilitiesResponse {
        CapabilitiesResponse {
            server_version: "1.8.3".to_string(),
            protocol_version,
            protocol: ProtocolCapability {
                current: protocol_version,
                min_supported_client: protocol_version,
                max_supported_client: protocol_version,
            },
            runtime_host: runtime_host.to_string(),
            event_stream: AvailabilityCapability {
                available: event_stream_available,
            },
            tracking: OwnedRuntimeCapability {
                owned: true,
                ready: tracking_ready,
            },
            browser_activity_bridge: OwnedRuntimeCapability {
                owned: true,
                ready: true,
            },
            tools: OwnedRuntimeCapability {
                owned: true,
                ready: true,
            },
            daemon_service: OwnedRuntimeCapability {
                owned: true,
                ready: true,
            },
            write_api: WriteApiCapability {
                available: true,
                operations: Vec::new(),
            },
        }
    }
}
