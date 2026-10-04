//! Wire types shared by the server and independent clients. No runtime dependencies.
use serde::{Deserialize, Serialize};

pub mod configuration;
pub mod product_settings;
pub mod resource_settings;
pub mod read_budget;
pub mod activity;
pub mod dashboard;
pub mod history;
pub mod history_product;
pub mod icons;
pub mod maintenance;
pub mod web_history;
pub mod events;
pub const EVENT_INSTANCE_HEADER: &str = "x-patina-event-instance";

pub const CURRENT_PROTOCOL_VERSION: u32 = 2;

#[derive(Debug, Deserialize, Serialize)]
pub struct ApiResponse<T> {
    pub data: T,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ApiError {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn not_found(message: &str) -> Self {
        Self {
            error: ApiErrorDetail {
                code: "not_found".to_string(),
                message: message.to_string(),
            },
        }
    }

    pub fn bad_request(message: &str) -> Self {
        Self {
            error: ApiErrorDetail {
                code: "bad_request".to_string(),
                message: message.to_string(),
            },
        }
    }

    pub fn unauthorized() -> Self {
        Self {
            error: ApiErrorDetail {
                code: "unauthorized".to_string(),
                message: "Invalid or missing API token".to_string(),
            },
        }
    }

    pub fn forbidden(message: &str) -> Self {
        Self {
            error: ApiErrorDetail {
                code: "forbidden".to_string(),
                message: message.to_string(),
            },
        }
    }

    pub fn conflict(message: &str) -> Self {
        Self {
            error: ApiErrorDetail {
                code: "conflict".to_string(),
                message: message.to_string(),
            },
        }
    }

    pub fn internal(message: &str) -> Self {
        Self {
            error: ApiErrorDetail {
                code: "internal_error".to_string(),
                message: message.to_string(),
            },
        }
    }

    pub fn unavailable(message: &str) -> Self {
        Self {
            error: ApiErrorDetail {
                code: "service_unavailable".to_string(),
                message: message.to_string(),
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AvailabilityCapability {
    pub available: bool,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProtocolCapability {
    pub current: u32,
    pub min_supported_client: u32,
    pub max_supported_client: u32,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct WriteApiCapability {
    pub available: bool,
    pub operations: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct OwnedRuntimeCapability {
    pub owned: bool,
    pub ready: bool,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CapabilitiesResponse {
    pub server_version: String,
    pub protocol_version: u32,
    pub protocol: ProtocolCapability,
    pub runtime_host: String,
    pub event_stream: AvailabilityCapability,
    pub tracking: OwnedRuntimeCapability,
    pub browser_activity_bridge: OwnedRuntimeCapability,
    pub tools: OwnedRuntimeCapability,
    pub daemon_service: OwnedRuntimeCapability,
    pub write_api: WriteApiCapability,
}
