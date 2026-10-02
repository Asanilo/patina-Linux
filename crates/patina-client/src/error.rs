use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClientError {
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

impl ClientError {
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

impl fmt::Display for ClientError {
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

impl std::error::Error for ClientError {}
