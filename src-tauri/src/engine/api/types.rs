use serde::{Deserialize, Serialize};

use crate::domain::web_activity::WebActivityBridgeSnapshot;
use crate::engine::tracking::runtime_snapshot::{
    TrackingRuntimeProbeDiagnostics, TrackingRuntimeProbeStatus,
};
use crate::platform::tracking_diagnostics::WindowTrackingDiagnostics;

pub use patina_protocol::{ApiError, ApiResponse};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub platform: String,
}

pub use patina_protocol::{
    AvailabilityCapability, CapabilitiesResponse, OwnedRuntimeCapability, ProtocolCapability,
    WriteApiCapability,
};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CurrentWindowResponse {
    pub exe_name: String,
    pub title: String,
    pub process_id: u32,
    pub is_afk: bool,
    pub idle_time_ms: u32,
    pub process_path: String,
    pub sampled_at_ms: i64,
    pub runtime_snapshot: crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshot,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
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

#[derive(Debug, Deserialize, Serialize)]
pub struct ClassifyRequest {
    pub category: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RenameRequest {
    pub display_name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ExcludeRequest {
    pub excluded: bool,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct TitleRecordingRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AfkThresholdRequest {
    pub seconds: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TrackingPausedRequest {
    pub paused: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AudioParticipationRequest {
    pub enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LocalApiPortRequest {
    pub port: u16,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ConfirmedActionRequest {
    pub confirmed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateReminderRequest {
    pub label: String,
    pub scheduled_at: i64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateSoftwareReminderRuleRequest {
    pub app_name: String,
    pub exe_name: Option<String>,
    pub limit_ms: i64,
    pub message: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StartTimerRequest {
    pub mode: crate::domain::tools::TimerMode,
    pub duration_ms: Option<i64>,
    pub label: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StartPomodoroRequest {
    pub focus_ms: i64,
    pub short_break_ms: i64,
    pub long_break_ms: i64,
    pub long_break_every: i64,
}

pub use patina_protocol::configuration::{
    ClassificationMutationRequest, ClassificationMutationsRequest,
};

#[derive(Debug, Deserialize, Serialize)]
pub struct AppSettingMutationRequest {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AppSettingsMutationsRequest {
    pub mutations: Vec<AppSettingMutationRequest>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TrackingDataCleanupRequest {
    pub cutoff_time_ms: i64,
    pub confirmed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AppTrackingDataCleanupRequest {
    pub exe_names: Vec<String>,
    pub start_time_ms: Option<i64>,
    pub end_time_ms: Option<i64>,
    pub confirmed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WebDomainCleanupRequest {
    pub domain: String,
    pub confirmed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StagedActivityImportCommitRequest {
    pub ticket: String,
    pub source_name: String,
    pub expected_fingerprint: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ScheduledBackupConfigRequest {
    pub config: crate::domain::backup_schedule::ScheduledBackupConfigInput,
    pub confirmed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoteBackupUploadRequest {
    pub config: crate::domain::remote_backup::WebDavBackupConfig,
    pub confirmed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoteBackupListRequest {
    pub config: crate::domain::remote_backup::WebDavBackupConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RemoteBackupRestoreRequest {
    pub config: crate::domain::remote_backup::WebDavBackupConfig,
    pub id: String,
    pub strategy: crate::domain::backup::RestoreStrategy,
    pub confirmed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct StagedBackupRestoreRequest {
    pub ticket: String,
    pub expected_sha256: String,
    pub expected_size_bytes: u64,
    pub strategy: crate::domain::backup::RestoreStrategy,
    pub confirmed: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CancelBackupRestoreRequest {
    pub request_id: String,
    pub confirmed: bool,
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

#[derive(Debug, Deserialize, Serialize)]
pub struct TrackerSettingsResponse {
    pub idle_timeout_secs: u64,
    pub timeline_merge_gap_secs: u64,
    pub tracking_paused: bool,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RuntimeSettingsResponse {
    pub audio_participation_enabled: bool,
    pub browser_activity: BrowserActivitySettingsResponse,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BrowserActivitySettingsResponse {
    pub enabled: bool,
    pub port: u16,
    pub token_present: bool,
    pub url_privacy: crate::domain::settings::WebActivityUrlPrivacyMode,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DiagnosticsResponse {
    pub window_tracking: WindowTrackingDiagnostics,
    pub tracker_runtime: Option<TrackerRuntimeDiagnostics>,
    pub web_activity_bridge: Option<WebActivityBridgeSnapshot>,
}

#[derive(Debug, Deserialize, Serialize)]
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
