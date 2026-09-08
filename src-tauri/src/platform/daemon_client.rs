use eventsource_stream::{Event, Eventsource};
use futures_util::{stream::BoxStream, StreamExt};
use reqwest::{
    header::{ACCEPT, CONTENT_TYPE},
    redirect::Policy,
    StatusCode,
};
use serde::{de::DeserializeOwned, Serialize};
use std::fmt;
use std::time::Duration;

use crate::engine::api::types::{
    ActiveSessionResponse, AfkThresholdRequest, ApiError, ApiResponse, AppSettingMutationRequest,
    AppSettingsMutationsRequest, AudioParticipationRequest, CapabilitiesResponse,
    ClassificationMutationRequest, ClassificationMutationsRequest, CreateReminderRequest,
    CreateSoftwareReminderRuleRequest, CurrentWindowResponse, DiagnosticsResponse,
    StartPomodoroRequest, StartTimerRequest, TrackerSettingsResponse, TrackingPausedRequest,
};
use crate::engine::runtime_event::RuntimeEventEnvelope;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(750);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_EVENT_DATA_BYTES: usize = 64 * 1024;

#[derive(Clone)]
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

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatinadStreamEvent {
    Runtime(RuntimeEventEnvelope),
    ResyncRequired {
        reason: String,
        missed: Option<u64>,
    },
    Ignored {
        event: String,
        sequence: Option<u64>,
    },
}

impl PatinadStreamEvent {
    pub fn sequence(&self) -> Option<u64> {
        match self {
            Self::Runtime(envelope) => Some(envelope.sequence),
            Self::Ignored { sequence, .. } => *sequence,
            Self::ResyncRequired { .. } => None,
        }
    }
}

#[allow(dead_code)]
pub struct PatinadEventStream {
    inner: BoxStream<'static, Result<Event, String>>,
}

#[allow(dead_code)]
impl PatinadEventStream {
    pub async fn next_event(&mut self) -> Result<Option<PatinadStreamEvent>, PatinadClientError> {
        let Some(event) = self.inner.next().await else {
            return Ok(None);
        };
        let event = event.map_err(PatinadClientError::Unreachable)?;
        parse_stream_event(event).map(Some)
    }
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
        self.get_json("/api/v1/capabilities", "capabilities").await
    }

    #[allow(dead_code)]
    pub async fn current_window(&self) -> Result<CurrentWindowResponse, PatinadClientError> {
        self.get_json("/api/v1/current", "current window").await
    }

    #[allow(dead_code)]
    pub async fn active_session(
        &self,
    ) -> Result<Option<ActiveSessionResponse>, PatinadClientError> {
        self.get_json("/api/v1/sessions/active", "active session")
            .await
    }

    pub async fn tracker_settings(&self) -> Result<TrackerSettingsResponse, PatinadClientError> {
        self.get_json("/api/v1/settings/tracker", "tracker settings")
            .await
    }

    pub async fn diagnostics(&self) -> Result<DiagnosticsResponse, PatinadClientError> {
        self.get_json("/api/v1/diagnostics", "diagnostics").await
    }

    pub async fn local_api_configuration(
        &self,
    ) -> Result<crate::engine::api::runtime_control::LocalApiRuntimeSnapshot, PatinadClientError>
    {
        self.get_json("/api/v1/settings/local-api", "local API configuration")
            .await
    }

    pub async fn set_afk_threshold(&self, seconds: u64) -> Result<(), PatinadClientError> {
        self.post_ack(
            "/api/v1/settings/tracker/afk-threshold",
            &AfkThresholdRequest { seconds },
            "AFK threshold update",
        )
        .await
    }

    pub async fn set_tracking_paused(&self, paused: bool) -> Result<(), PatinadClientError> {
        self.post_ack(
            "/api/v1/settings/tracker/pause",
            &TrackingPausedRequest { paused },
            "tracking pause update",
        )
        .await
    }

    pub async fn toggle_tracking_paused(&self) -> Result<(), PatinadClientError> {
        let settings = self.tracker_settings().await?;
        self.set_tracking_paused(!settings.tracking_paused).await
    }

    pub async fn set_audio_participation_enabled(
        &self,
        enabled: bool,
    ) -> Result<(), PatinadClientError> {
        let _: serde_json::Value = self
            .post_json(
                "/api/v1/settings/runtime/audio-participation",
                &AudioParticipationRequest { enabled },
                "audio participation update",
            )
            .await?;
        Ok(())
    }

    pub async fn configure_browser_activity(
        &self,
        configuration: crate::engine::api::runtime_control::BrowserActivityRuntimeConfiguration,
    ) -> Result<crate::engine::api::types::BrowserActivitySettingsResponse, PatinadClientError>
    {
        self.post_json(
            "/api/v1/settings/runtime/browser-activity",
            &configuration,
            "browser activity configuration",
        )
        .await
    }

    pub async fn apply_local_api_port(
        &self,
        port: u16,
    ) -> Result<crate::engine::api::runtime_control::LocalApiPortApplyResult, PatinadClientError>
    {
        self.post_json(
            "/api/v1/settings/local-api/port",
            &crate::engine::api::types::LocalApiPortRequest { port },
            "local API port update",
        )
        .await
    }

    pub async fn rotate_local_api_token(
        &self,
    ) -> Result<crate::engine::api::runtime_control::LocalApiTokenRotationResult, PatinadClientError>
    {
        self.post_empty_json(
            "/api/v1/settings/local-api/token/rotate",
            "local API token rotation",
        )
        .await
    }

    pub async fn commit_classification_settings(
        &self,
        mutations: Vec<ClassificationMutationRequest>,
    ) -> Result<(), PatinadClientError> {
        self.post_ack(
            "/api/v1/settings/classification",
            &ClassificationMutationsRequest { mutations },
            "classification settings update",
        )
        .await
    }

    pub async fn commit_app_settings(
        &self,
        mutations: Vec<AppSettingMutationRequest>,
    ) -> Result<(), PatinadClientError> {
        self.post_ack(
            "/api/v1/settings/app",
            &AppSettingsMutationsRequest { mutations },
            "app settings update",
        )
        .await
    }

    pub async fn tools_snapshot(
        &self,
    ) -> Result<crate::domain::tools::ToolsRuntimeSnapshot, PatinadClientError> {
        self.get_json("/api/v1/tools/snapshot", "Tools snapshot")
            .await
    }

    pub async fn create_reminder(
        &self,
        request: CreateReminderRequest,
    ) -> Result<crate::domain::tools::ToolsRuntimeSnapshot, PatinadClientError> {
        self.post_json("/api/v1/tools/reminders", &request, "reminder creation")
            .await
    }

    pub async fn cancel_reminder(
        &self,
        reminder_id: i64,
    ) -> Result<crate::domain::tools::ToolsRuntimeSnapshot, PatinadClientError> {
        self.post_empty_json(
            &format!("/api/v1/tools/reminders/{reminder_id}/cancel"),
            "reminder cancellation",
        )
        .await
    }

    pub async fn create_software_reminder_rule(
        &self,
        request: CreateSoftwareReminderRuleRequest,
    ) -> Result<crate::domain::tools::ToolsRuntimeSnapshot, PatinadClientError> {
        self.post_json(
            "/api/v1/tools/software-reminder-rules",
            &request,
            "software reminder rule creation",
        )
        .await
    }

    pub async fn disable_software_reminder_rule(
        &self,
        rule_id: i64,
    ) -> Result<crate::domain::tools::ToolsRuntimeSnapshot, PatinadClientError> {
        self.post_empty_json(
            &format!("/api/v1/tools/software-reminder-rules/{rule_id}/disable"),
            "software reminder rule disable",
        )
        .await
    }

    pub async fn start_timer(
        &self,
        request: StartTimerRequest,
    ) -> Result<crate::domain::tools::ToolsRuntimeSnapshot, PatinadClientError> {
        self.post_json("/api/v1/tools/timer/start", &request, "timer start")
            .await
    }

    pub async fn tools_action(
        &self,
        path: &str,
        response_name: &str,
    ) -> Result<crate::domain::tools::ToolsRuntimeSnapshot, PatinadClientError> {
        self.post_empty_json(path, response_name).await
    }

    pub async fn start_pomodoro(
        &self,
        request: StartPomodoroRequest,
    ) -> Result<crate::domain::tools::ToolsRuntimeSnapshot, PatinadClientError> {
        self.post_json("/api/v1/tools/pomodoro/start", &request, "Pomodoro start")
            .await
    }

    #[allow(dead_code)]
    pub async fn open_event_stream(
        &self,
        after_sequence: Option<u64>,
    ) -> Result<PatinadEventStream, PatinadClientError> {
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
                PatinadClientError::Unreachable(
                    "timed out while opening patinad event stream".to_string(),
                )
            })?
            .map_err(map_transport_error)?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(PatinadClientError::Unauthorized);
        }
        if !status.is_success() {
            let body = tokio::time::timeout(REQUEST_TIMEOUT, read_limited_body(response))
                .await
                .map_err(|_| {
                    PatinadClientError::Unreachable(
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
            .to_ascii_lowercase()
            .starts_with("text/event-stream")
        {
            return Err(PatinadClientError::InvalidResponse(
                "patinad event stream returned an unexpected content type".to_string(),
            ));
        }
        let inner = response
            .bytes_stream()
            .eventsource()
            .map(|event| event.map_err(|error| format!("patinad event stream failed: {error}")))
            .boxed();
        Ok(PatinadEventStream { inner })
    }

    pub async fn negotiate_tracking_owner(&self) -> Result<PatinadNegotiation, PatinadClientError> {
        let capabilities = self.capabilities().await?;
        negotiate_tracking_capabilities(capabilities)
    }

    async fn get_json<T>(&self, path: &str, response_name: &str) -> Result<T, PatinadClientError>
    where
        T: DeserializeOwned,
    {
        let response = self
            .client
            .get(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .timeout(REQUEST_TIMEOUT)
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

        serde_json::from_slice::<ApiResponse<T>>(&body)
            .map(|response| response.data)
            .map_err(|error| {
                PatinadClientError::InvalidResponse(format!(
                    "failed to decode patinad {response_name}: {error}"
                ))
            })
    }

    async fn post_ack<B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
        response_name: &str,
    ) -> Result<(), PatinadClientError> {
        let _: serde_json::Value = self.post_json(path, body, response_name).await?;
        Ok(())
    }

    async fn post_empty_json<T>(
        &self,
        path: &str,
        response_name: &str,
    ) -> Result<T, PatinadClientError>
    where
        T: DeserializeOwned,
    {
        self.post_json(path, &serde_json::json!({}), response_name)
            .await
    }

    async fn post_json<T, B>(
        &self,
        path: &str,
        body: &B,
        response_name: &str,
    ) -> Result<T, PatinadClientError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let request_body = serde_json::to_vec(body).map_err(|error| {
            PatinadClientError::InvalidConfiguration(format!(
                "failed to encode patinad {response_name} request: {error}"
            ))
        })?;
        let response = self
            .client
            .post(format!("{}{path}", self.base_url))
            .bearer_auth(&self.token)
            .header(CONTENT_TYPE, "application/json")
            .body(request_body)
            .timeout(REQUEST_TIMEOUT)
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

        serde_json::from_slice::<ApiResponse<T>>(&body)
            .map(|response| response.data)
            .map_err(|error| {
                PatinadClientError::InvalidResponse(format!(
                    "failed to decode patinad {response_name}: {error}"
                ))
            })
    }
}

#[allow(dead_code)]
fn parse_stream_event(event: Event) -> Result<PatinadStreamEvent, PatinadClientError> {
    if event.data.len() > MAX_EVENT_DATA_BYTES {
        return Err(PatinadClientError::ResponseTooLarge);
    }
    if event.event == "resync-required" {
        #[derive(serde::Deserialize)]
        struct ResyncPayload {
            reason: String,
            missed: Option<u64>,
        }
        let payload = serde_json::from_str::<ResyncPayload>(&event.data).map_err(|error| {
            PatinadClientError::InvalidResponse(format!(
                "failed to decode patinad resync event: {error}"
            ))
        })?;
        return Ok(PatinadStreamEvent::ResyncRequired {
            reason: payload.reason,
            missed: payload.missed,
        });
    }

    let sequence = parse_optional_event_sequence(&event.id)?;
    let is_known_runtime_event = matches!(
        event.event.as_str(),
        "tracking-data-changed" | "tools-runtime-changed" | "tool-alert"
    );
    if !is_known_runtime_event {
        return Ok(PatinadStreamEvent::Ignored {
            event: event.event,
            sequence,
        });
    }

    let envelope = serde_json::from_str::<RuntimeEventEnvelope>(&event.data).map_err(|error| {
        PatinadClientError::InvalidResponse(format!(
            "failed to decode patinad runtime event: {error}"
        ))
    })?;
    let Some(sequence) = sequence else {
        return Err(PatinadClientError::InvalidResponse(
            "patinad runtime event is missing its sequence ID".to_string(),
        ));
    };
    if envelope.sequence != sequence || envelope.event.event_name() != event.event {
        return Err(PatinadClientError::InvalidResponse(
            "patinad runtime event ID or type does not match its envelope".to_string(),
        ));
    }
    Ok(PatinadStreamEvent::Runtime(envelope))
}

#[allow(dead_code)]
fn parse_optional_event_sequence(value: &str) -> Result<Option<u64>, PatinadClientError> {
    if value.is_empty() {
        return Ok(None);
    }
    value.parse::<u64>().map(Some).map_err(|_| {
        PatinadClientError::InvalidResponse(
            "patinad event stream returned an invalid sequence ID".to_string(),
        )
    })
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
            negotiate_tracking_capabilities(capabilities("daemon", 1, true, true)).unwrap_err();

        assert_eq!(error.code(), "incompatible-protocol");
    }

    #[test]
    fn negotiation_accepts_a_newer_server_that_explicitly_supports_this_client() {
        let mut response = capabilities("daemon", 3, true, true);
        response.protocol.min_supported_client = 2;

        let negotiated = negotiate_tracking_capabilities(response).unwrap();

        assert_eq!(negotiated.protocol_version, 3);
    }

    #[test]
    fn negotiation_accepts_a_starting_tracking_owner() {
        let negotiated =
            negotiate_tracking_capabilities(capabilities("daemon", 2, false, true)).unwrap();

        assert!(!negotiated.tracking_ready);
        assert!(negotiated.event_stream_available);
    }

    #[test]
    fn stream_event_requires_matching_sse_and_envelope_sequences() {
        let envelope = RuntimeEventEnvelope {
            sequence: 7,
            event: crate::engine::runtime_event::RuntimeEvent::TrackingDataChanged {
                reason: "session-transition".to_string(),
                changed_at_ms: 2_000,
            },
        };
        let parsed = parse_stream_event(Event {
            event: "tracking-data-changed".to_string(),
            data: serde_json::to_string(&envelope).unwrap(),
            id: "7".to_string(),
            retry: None,
        })
        .unwrap();
        assert_eq!(parsed, PatinadStreamEvent::Runtime(envelope.clone()));

        let error = parse_stream_event(Event {
            event: "tracking-data-changed".to_string(),
            data: serde_json::to_string(&envelope).unwrap(),
            id: "8".to_string(),
            retry: None,
        })
        .unwrap_err();
        assert_eq!(error.code(), "invalid-response");
    }

    #[test]
    fn stream_event_ignores_forward_compatible_event_names_but_keeps_cursor() {
        let parsed = parse_stream_event(Event {
            event: "future-runtime-event".to_string(),
            data: "{}".to_string(),
            id: "9".to_string(),
            retry: None,
        })
        .unwrap();

        assert_eq!(
            parsed,
            PatinadStreamEvent::Ignored {
                event: "future-runtime-event".to_string(),
                sequence: Some(9),
            }
        );
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
