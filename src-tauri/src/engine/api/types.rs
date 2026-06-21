use serde::{Deserialize, Serialize};

use crate::domain::web_activity::WebActivityBridgeSnapshot;
use crate::engine::tracking::runtime_snapshot::{
    TrackingRuntimeProbeDiagnostics, TrackingRuntimeProbeStatus,
};
use crate::platform::tracking_diagnostics::WindowTrackingDiagnostics;

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Serialize)]
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

    pub fn internal(message: &str) -> Self {
        Self {
            error: ApiErrorDetail {
                code: "internal_error".to_string(),
                message: message.to_string(),
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub platform: String,
}

#[derive(Debug, Serialize)]
pub struct CurrentWindowResponse {
    pub exe_name: String,
    pub title: String,
    pub process_id: u32,
    pub is_afk: bool,
    pub idle_time_ms: u32,
    pub process_path: String,
}

#[derive(Debug, Serialize)]
pub struct SessionEntry {
    pub id: i64,
    pub app_name: String,
    pub exe_name: String,
    pub window_title: Option<String>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub duration: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct SessionsResponse {
    pub sessions: Vec<SessionEntry>,
}

#[derive(Debug, Serialize)]
pub struct ActiveSessionResponse {
    pub id: i64,
    pub app_name: String,
    pub exe_name: String,
    pub window_title: Option<String>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub duration: i64,
    pub continuity_group_start_time: i64,
    pub sampled_at_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct AppEntry {
    pub exe_name: String,
    pub display_name: String,
    pub category: Option<String>,
    pub excluded: bool,
}

#[derive(Debug, Serialize)]
pub struct AppsResponse {
    pub apps: Vec<AppEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ClassifyRequest {
    pub category: String,
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ExcludeRequest {
    pub excluded: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TitleRecordingRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
pub struct AfkThresholdRequest {
    pub seconds: u64,
}

#[derive(Debug, Serialize)]
pub struct SummaryResponse {
    pub date: String,
    pub total_active_ms: i64,
    pub apps: Vec<AppSummaryEntry>,
    pub categories: Vec<CategorySummaryEntry>,
}

#[derive(Debug, Serialize)]
pub struct TrendResponse {
    pub period: String,
    pub granularity: String,
    pub from_ms: i64,
    pub to_ms: i64,
    pub data_points: Vec<TrendDataPoint>,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct TrendDataPoint {
    pub date: String,
    pub active_ms: i64,
    pub top_app: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WebActivityResponse {
    pub items: Vec<WebActivityEntry>,
}

#[derive(Debug, Serialize)]
pub struct WebActivityEntry {
    pub id: i64,
    pub browser_client_id: String,
    pub browser_kind: String,
    pub browser_exe_name: String,
    pub domain: String,
    pub normalized_domain: String,
    pub url: Option<String>,
    pub title: Option<String>,
    pub favicon_url: Option<String>,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub duration: i64,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct AppSummaryEntry {
    pub exe_name: String,
    pub total_ms: i64,
    pub percentage: f64,
}

#[derive(Debug, Serialize)]
pub struct CategorySummaryEntry {
    pub name: String,
    pub total_ms: i64,
}

#[derive(Debug, Serialize)]
pub struct TrackerSettingsResponse {
    pub idle_timeout_secs: u64,
    pub timeline_merge_gap_secs: u64,
    pub tracking_paused: bool,
}

#[derive(Debug, Serialize)]
pub struct DiagnosticsResponse {
    pub window_tracking: WindowTrackingDiagnostics,
    pub tracker_runtime: Option<TrackerRuntimeDiagnostics>,
    pub web_activity_bridge: Option<WebActivityBridgeSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct TrackerRuntimeDiagnostics {
    pub probe_status: TrackingRuntimeProbeStatus,
    pub degraded_reason: Option<String>,
    pub probe_diagnostics: TrackingRuntimeProbeDiagnostics,
}

#[derive(Debug, Deserialize)]
pub struct SessionQueryParams {
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub app: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct SummaryQueryParams {
    pub from: Option<i64>,
    pub to: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TrendQueryParams {
    pub period: Option<String>,
    pub granularity: Option<String>,
}

pub struct RouteResponse {
    pub status: u16,
    pub body: serde_json::Value,
}
