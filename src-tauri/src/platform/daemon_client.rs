#[cfg(test)]
use patina_client::negotiate_tracking_capabilities;
#[cfg(test)]
use patina_client::{events::parse_stream_event, Event};
pub use patina_client::{ClientError as PatinadClientError, Negotiation as PatinadNegotiation};
use patina_client::{MAX_RESPONSE_BYTES, REQUEST_TIMEOUT};
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;

use crate::engine::api::types::{
    ActiveSessionResponse, AfkThresholdRequest, AppSettingMutationRequest,
    AppSettingsMutationsRequest, AudioParticipationRequest, CapabilitiesResponse,
    ClassificationMutationRequest, ClassificationMutationsRequest, CreateReminderRequest,
    CreateSoftwareReminderRuleRequest, CurrentWindowResponse, DiagnosticsResponse,
    StartPomodoroRequest, StartTimerRequest, TrackerSettingsResponse, TrackingDataCleanupRequest,
    TrackingPausedRequest,
};
#[cfg(test)]
use crate::engine::runtime_event::RuntimeEventEnvelope;

const IMPORT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const RESTORE_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const RESTORE_COMPLETION_TIMEOUT: Duration = Duration::from_secs(300);

/// Domain-specific Desktop facade. HTTP/SSE and negotiation live in patina-client.
#[derive(Clone, Debug)]
pub struct PatinadClient {
    transport: patina_client::Client,
}

#[cfg(test)]
pub use patina_client::events::{
    RuntimeEventStream as PatinadEventStream, StreamEvent as PatinadStreamEvent,
};

impl PatinadClient {
    pub(crate) fn from_transport(transport: patina_client::Client) -> Self {
        Self { transport }
    }
    pub(crate) fn transport(&self) -> &patina_client::Client {
        &self.transport
    }
    pub fn new(port: u16, token: impl Into<String>) -> Result<Self, PatinadClientError> {
        Ok(Self {
            transport: patina_client::Client::new(port, token)?,
        })
    }
    pub fn base_url(&self) -> &str {
        self.transport.base_url()
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
            &ClassificationMutationsRequest {
                mutations,
                expected_revision: None,
            },
            "classification settings update",
        )
        .await
    }

    pub async fn classification_snapshot(
        &self,
    ) -> Result<patina_protocol::configuration::ClassificationSnapshot, PatinadClientError> {
        self.transport.classification_snapshot().await
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

    pub async fn delete_tracking_data_before(
        &self,
        cutoff_time_ms: i64,
    ) -> Result<crate::domain::data_maintenance::TrackingDataCleanupResult, PatinadClientError>
    {
        self.post_json(
            "/api/v1/data/cleanup",
            &TrackingDataCleanupRequest {
                cutoff_time_ms,
                confirmed: true,
            },
            "tracking data cleanup",
        )
        .await
    }

    pub async fn clear_window_titles(
        &self,
    ) -> Result<crate::domain::data_maintenance::WindowTitleCleanupResult, PatinadClientError> {
        self.post_json(
            "/api/v1/data/window-titles/clear",
            &crate::engine::api::types::ConfirmedActionRequest { confirmed: true },
            "window title cleanup",
        )
        .await
    }

    pub async fn delete_app_tracking_data(
        &self,
        exe_names: Vec<String>,
        start_time_ms: Option<i64>,
        end_time_ms: Option<i64>,
    ) -> Result<crate::domain::data_maintenance::AppTrackingDataCleanupResult, PatinadClientError>
    {
        self.post_json(
            "/api/v1/data/apps/delete",
            &crate::engine::api::types::AppTrackingDataCleanupRequest {
                exe_names,
                start_time_ms,
                end_time_ms,
                confirmed: true,
            },
            "application data cleanup",
        )
        .await
    }

    pub async fn activity_import_batches(
        &self,
    ) -> Result<Vec<crate::domain::activity_import::ImportBatchDto>, PatinadClientError> {
        self.get_json("/api/v1/imports", "activity import batches")
            .await
    }

    pub async fn delete_web_domain_history(
        &self,
        domain: String,
    ) -> Result<crate::domain::data_maintenance::WebDomainCleanupResult, PatinadClientError> {
        self.post_json(
            "/api/v1/data/web-domains/delete",
            &crate::engine::api::types::WebDomainCleanupRequest {
                domain,
                confirmed: true,
            },
            "web history cleanup",
        )
        .await
    }

    pub async fn commit_staged_activity_import(
        &self,
        request: &crate::engine::api::types::StagedActivityImportCommitRequest,
    ) -> Result<crate::domain::activity_import::ImportCommitReportDto, PatinadClientError> {
        self.post_json_with_timeout(
            "/api/v1/imports/canonical/commit",
            request,
            "activity import commit",
            IMPORT_REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn delete_activity_import_batch(
        &self,
        batch_id: &str,
    ) -> Result<crate::domain::activity_import::ImportDeleteReportDto, PatinadClientError> {
        if batch_id.is_empty()
            || !batch_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(PatinadClientError::InvalidConfiguration(
                "activity import batch ID is invalid".to_string(),
            ));
        }
        self.post_json(
            &format!("/api/v1/imports/{batch_id}/delete"),
            &crate::engine::api::types::ConfirmedActionRequest { confirmed: true },
            "activity import deletion",
        )
        .await
    }

    pub async fn scheduled_backup_snapshot(
        &self,
    ) -> Result<crate::domain::backup_schedule::ScheduledBackupSnapshot, PatinadClientError> {
        self.get_json("/api/v1/backups/schedule", "scheduled backup snapshot")
            .await
    }

    pub async fn save_scheduled_backup_config(
        &self,
        config: crate::domain::backup_schedule::ScheduledBackupConfigInput,
    ) -> Result<crate::domain::backup_schedule::ScheduledBackupSnapshot, PatinadClientError> {
        self.post_json(
            "/api/v1/backups/schedule",
            &crate::engine::api::types::ScheduledBackupConfigRequest {
                config,
                confirmed: true,
            },
            "scheduled backup configuration",
        )
        .await
    }

    pub async fn upload_remote_backup(
        &self,
        config: crate::domain::remote_backup::WebDavBackupConfig,
    ) -> Result<crate::domain::remote_backup::RemoteBackupUploadResult, PatinadClientError> {
        self.post_json_with_timeout(
            "/api/v1/backups/remote/upload",
            &crate::engine::api::types::RemoteBackupUploadRequest {
                config,
                confirmed: true,
            },
            "remote backup upload",
            RESTORE_REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn list_remote_backups(
        &self,
        config: crate::domain::remote_backup::WebDavBackupConfig,
    ) -> Result<Vec<crate::domain::remote_backup::RemoteBackupEntry>, PatinadClientError> {
        self.post_json_with_timeout(
            "/api/v1/backups/remote/list",
            &crate::engine::api::types::RemoteBackupListRequest { config },
            "remote backup list",
            RESTORE_REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn schedule_remote_backup_restore(
        &self,
        config: crate::domain::remote_backup::WebDavBackupConfig,
        id: String,
        strategy: crate::domain::backup::RestoreStrategy,
    ) -> Result<
        crate::engine::api::backup_restore_owner::BackupRestoreScheduleResult,
        PatinadClientError,
    > {
        self.post_json_with_timeout(
            "/api/v1/backups/remote/restore",
            &crate::engine::api::types::RemoteBackupRestoreRequest {
                config,
                id,
                strategy,
                confirmed: true,
            },
            "remote backup restore scheduling",
            RESTORE_REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn schedule_backup_restore(
        &self,
        request: &crate::engine::api::types::StagedBackupRestoreRequest,
    ) -> Result<
        crate::engine::api::backup_restore_owner::BackupRestoreScheduleResult,
        PatinadClientError,
    > {
        self.post_json_with_timeout(
            "/api/v1/backups/restore",
            request,
            "backup restore scheduling",
            RESTORE_REQUEST_TIMEOUT,
        )
        .await
    }

    pub async fn backup_restore_status(
        &self,
        request_id: &str,
    ) -> Result<
        Option<crate::engine::api::backup_restore_owner::BackupRestoreSnapshot>,
        PatinadClientError,
    > {
        if !valid_restore_request_id(request_id) {
            return Err(PatinadClientError::InvalidConfiguration(
                "backup restore request ID is invalid".to_string(),
            ));
        }
        self.get_json(
            &format!("/api/v1/backups/restore?request_id={request_id}"),
            "backup restore status",
        )
        .await
    }

    pub async fn wait_for_backup_restore(
        &self,
        request_id: &str,
    ) -> Result<crate::engine::api::backup_restore_owner::BackupRestoreSnapshot, PatinadClientError>
    {
        let deadline = tokio::time::Instant::now() + RESTORE_COMPLETION_TIMEOUT;
        loop {
            match self.backup_restore_status(request_id).await {
                Ok(Some(snapshot)) if snapshot.status == "completed" => return Ok(snapshot),
                Ok(Some(snapshot))
                    if matches!(snapshot.status.as_str(), "failed" | "cancelled") =>
                {
                    return Err(PatinadClientError::InvalidResponse(
                        snapshot.error.unwrap_or_else(|| {
                            format!("backup restore ended with status {}", snapshot.status)
                        }),
                    ));
                }
                Ok(_) | Err(PatinadClientError::Unreachable(_)) => {}
                Err(error) => return Err(error),
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(PatinadClientError::Unreachable(
                    "timed out waiting for patinad to complete backup restore".to_string(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
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

    #[cfg(test)]
    pub async fn open_event_stream(
        &self,
        after_sequence: Option<u64>,
    ) -> Result<PatinadEventStream, PatinadClientError> {
        self.transport
            .open_runtime_event_stream(after_sequence)
            .await
    }
    pub async fn negotiate_tracking_owner(&self) -> Result<PatinadNegotiation, PatinadClientError> {
        self.transport.negotiate_tracking_owner().await
    }
    pub async fn service_snapshot(
        &self,
    ) -> Result<crate::engine::api::runtime_control::DaemonServiceRuntimeSnapshot, PatinadClientError>
    {
        self.get_json("/api/v1/system/service", "daemon service")
            .await
    }

    pub async fn restart_service(
        &self,
    ) -> Result<crate::engine::api::runtime_control::DaemonServiceRestartResult, PatinadClientError>
    {
        self.post_json(
            "/api/v1/system/service/restart",
            &serde_json::json!({"confirmed": true}),
            "daemon restart",
        )
        .await
    }

    async fn get_json<T>(&self, path: &str, response_name: &str) -> Result<T, PatinadClientError>
    where
        T: DeserializeOwned,
    {
        self.get_json_with_timeout(path, response_name, REQUEST_TIMEOUT)
            .await
    }

    pub async fn observed_apps(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<crate::domain::observed_apps::ObservedAppStat>, PatinadClientError> {
        crate::domain::observed_apps::validate_range(from_ms, to_ms)
            .map_err(PatinadClientError::InvalidConfiguration)?;
        self.get_json_with_limits(
            &format!("/api/v1/classification/observed-apps?from_ms={from_ms}&to_ms={to_ms}"),
            "observed apps",
            Duration::from_secs(18),
            crate::domain::observed_apps::MAX_OBSERVED_APPS_RESPONSE_BYTES,
        )
        .await
    }

    pub async fn migration_observed_apps(
        &self,
        to_ms: i64,
    ) -> Result<Vec<crate::domain::observed_apps::ObservedAppStat>, PatinadClientError> {
        if to_ms <= 0 {
            return Err(PatinadClientError::InvalidConfiguration(
                "invalid migration cutoff".into(),
            ));
        }
        self.get_json_with_limits(
            &format!("/api/v1/classification/observed-apps?from_ms=0&to_ms={to_ms}&scope=legacy-migration"),
            "legacy classification evidence",
            Duration::from_secs(35),
            crate::domain::observed_apps::MAX_OBSERVED_APPS_RESPONSE_BYTES,
        ).await
    }

    pub async fn daily_product(
        &self,
        from: &str,
        to: &str,
        language: &str,
    ) -> Result<patina_protocol::activity::DailyProductSnapshot, PatinadClientError> {
        self.transport.daily_product(from, to, language).await
    }

    pub async fn dashboard(
        &self,
        date: &str,
        language: &str,
    ) -> Result<patina_protocol::dashboard::DashboardProductSnapshot, PatinadClientError> {
        self.transport.dashboard(date, language).await
    }

    pub async fn exact_history(
        &self,
        from_ms: i64,
        to_ms: i64,
        language: &str,
    ) -> Result<patina_protocol::history::ExactHistorySnapshot, PatinadClientError> {
        self.transport.exact_history(from_ms, to_ms, language).await
    }

    pub async fn daily_activity(
        &self,
        from: &str,
        to: &str,
    ) -> Result<crate::domain::daily_activity::DailyActivitySnapshot, PatinadClientError> {
        crate::domain::daily_activity::local_day_boundaries(from, to)
            .map_err(PatinadClientError::InvalidConfiguration)?;
        self.get_json_with_timeout(
            &format!("/api/v1/heatmap?from={from}&to={to}"),
            "daily activity",
            Duration::from_secs(18),
        )
        .await
    }

    async fn get_json_with_timeout<T: DeserializeOwned>(
        &self,
        path: &str,
        response_name: &str,
        timeout: Duration,
    ) -> Result<T, PatinadClientError> {
        self.get_json_with_limits(path, response_name, timeout, MAX_RESPONSE_BYTES)
            .await
    }

    async fn get_json_with_limits<T: DeserializeOwned>(
        &self,
        path: &str,
        response_name: &str,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<T, PatinadClientError> {
        self.transport
            .get_json_with_limits(path, response_name, timeout, max_bytes)
            .await
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
        self.post_json_with_timeout(path, body, response_name, REQUEST_TIMEOUT)
            .await
    }

    async fn post_json_with_timeout<T: DeserializeOwned, B: Serialize + ?Sized>(
        &self,
        path: &str,
        body: &B,
        response_name: &str,
        timeout: Duration,
    ) -> Result<T, PatinadClientError> {
        self.transport
            .post_json_with_timeout(path, body, response_name, timeout)
            .await
    }
}

fn valid_restore_request_id(value: &str) -> bool {
    value.strip_prefix("restore_").is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::api::types::{
        ApiResponse, AvailabilityCapability, OwnedRuntimeCapability, ProtocolCapability,
        WriteApiCapability,
    };

    #[tokio::test]
    async fn daily_product_transport_has_scoped_budget_and_no_old_daemon_fallback() {
        use crate::domain::daily_activity::MAX_DAILY_APPS_RESPONSE_BYTES;
        use patina_protocol::activity::{
            DailyProductAppTotal, DailyProductDay, DailyProductSnapshot, ProductAppIdentity,
        };
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let boundaries =
            crate::domain::daily_activity::local_day_boundaries("2026-01-01", "2026-01-02")
                .unwrap();
        let data = DailyProductSnapshot {
            tracking_health: patina_protocol::activity::ActivityReadHealth {
                status: patina_protocol::activity::ActivityReadStatus::Unavailable,
                last_heartbeat_ms: None,
                live_cutoff_ms: 0,
                stale_after_ms: 8000,
            },
            configuration_revision: "0".repeat(64),
            applications: (0..2000)
                .map(|index| ProductAppIdentity {
                    app_key: format!("fixture-app-{index:04}"),
                    app_name: "Fixture".into(),
                    exe_name: format!("fixture-app-{index:04}"),
                    category: "other".into(),
                    display_name_override: None,
                })
                .collect(),
            sampled_at_ms: boundaries[1],
            days: vec![DailyProductDay {
                start_ms: boundaries[0],
                end_ms: boundaries[1],
                active_ms: 2000,
                apps: (0..2000)
                    .map(|index| DailyProductAppTotal {
                        app_key: format!("fixture-app-{index:04}"),
                        active_ms: 1,
                    })
                    .collect(),
            }],
        };
        let encoded = serde_json::to_string(&ApiResponse { data: data.clone() }).unwrap();
        assert!(encoded.len() > MAX_RESPONSE_BYTES);
        for (status, body, error) in [
            (200, encoded, None),
            (404, "{}".into(), Some("http-error")),
            (401, "{}".into(), Some("unauthorized")),
            (200, "{}".into(), Some("invalid-response")),
            (
                200,
                "x".repeat(MAX_DAILY_APPS_RESPONSE_BYTES + 1),
                Some("response-too-large"),
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let client =
                PatinadClient::new(listener.local_addr().unwrap().port(), "fixture-token").unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 1024];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0 && request.len() < 8192);
                    request.extend_from_slice(&chunk[..count]);
                }
                let request = String::from_utf8(request).unwrap().to_lowercase();
                assert!(request
                    .starts_with("get /api/v1/activity/daily-product?from=2026-01-01&to=2026-01-02&language=en-us "));
                assert!(request.contains("authorization: bearer fixture-token\r\n"));
                let response = format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = socket.write_all(response.as_bytes()).await;
            });
            let result = client
                .daily_product("2026-01-01", "2026-01-02", "en-US")
                .await;
            if let Some(error) = error {
                assert_eq!(result.unwrap_err().code(), error);
            } else {
                assert_eq!(result.unwrap(), data);
            }
            server.await.unwrap();
        }
    }

    #[tokio::test]
    async fn observed_apps_transport_has_its_own_budget_and_propagates_old_daemon_errors() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let rows = (0..800)
            .map(|index| crate::domain::observed_apps::ObservedAppStat {
                exe_name: format!("app-{index}"),
                app_name: "Example app".into(),
                total_duration_ms: 1000,
                last_seen_ms: 1000,
            })
            .collect::<Vec<_>>();
        let encoded = serde_json::to_string(&ApiResponse { data: rows.clone() }).unwrap();
        assert!(encoded.len() > MAX_RESPONSE_BYTES);
        for legacy in [false, true] {
            for (status, body, error) in [
                (200, encoded.clone(), None),
                (404, "{}".into(), Some("http-error")),
                (401, "{}".into(), Some("unauthorized")),
                (200, "{}".into(), Some("invalid-response")),
                (
                    200,
                    "x".repeat(crate::domain::observed_apps::MAX_OBSERVED_APPS_RESPONSE_BYTES + 1),
                    Some("response-too-large"),
                ),
            ] {
                let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                let client =
                    PatinadClient::new(listener.local_addr().unwrap().port(), "fixture-token")
                        .unwrap();
                let server = tokio::spawn(async move {
                    let (mut socket, _) = listener.accept().await.unwrap();
                    let mut request = Vec::new();
                    let mut chunk = [0; 1024];
                    while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                        let count = socket.read(&mut chunk).await.unwrap();
                        assert!(count > 0 && request.len() < 8192);
                        request.extend_from_slice(&chunk[..count]);
                    }
                    let request = String::from_utf8(request).unwrap().to_lowercase();
                    let scope = if legacy {
                        "&scope=legacy-migration"
                    } else {
                        ""
                    };
                    assert!(request.starts_with(&format!(
                        "get /api/v1/classification/observed-apps?from_ms=0&to_ms=3000{scope} "
                    )));
                    assert!(request.contains("authorization: bearer fixture-token\r\n"));
                    let response = format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
                    let _ = socket.write_all(response.as_bytes()).await;
                });
                let result = if legacy {
                    client.migration_observed_apps(3000).await
                } else {
                    client.observed_apps(0, 3000).await
                };
                if let Some(error) = error {
                    assert_eq!(result.unwrap_err().code(), error);
                } else {
                    assert_eq!(result.unwrap(), rows);
                }
                server.await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn daily_activity_transport_is_authenticated_bounded_and_never_falls_back() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let boundaries =
            crate::domain::daily_activity::local_day_boundaries("2026-01-01", "2026-01-03")
                .unwrap();
        let snapshot = crate::domain::daily_activity::DailyActivitySnapshot {
            sampled_at_ms: boundaries[2],
            earliest_start_ms: Some(boundaries[0]),
            days: boundaries
                .windows(2)
                .map(|day| crate::domain::daily_activity::DailyActivityTotal {
                    start_ms: day[0],
                    end_ms: day[1],
                    active_ms: 123,
                })
                .collect(),
        };
        let encoded = serde_json::to_string(&ApiResponse {
            data: snapshot.clone(),
        })
        .unwrap();
        for (status, body, expected_error) in [
            (200, encoded, None),
            (404, "{}".to_string(), Some("http-error")),
            (401, "{}".to_string(), Some("unauthorized")),
            (200, "{}".to_string(), Some("invalid-response")),
            (
                200,
                "x".repeat(MAX_RESPONSE_BYTES + 1),
                Some("response-too-large"),
            ),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let client =
                PatinadClient::new(listener.local_addr().unwrap().port(), "test-heatmap-token")
                    .unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut chunk = [0; 1024];
                while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                    let count = socket.read(&mut chunk).await.unwrap();
                    assert!(count > 0 && request.len() < 8192);
                    request.extend_from_slice(&chunk[..count]);
                }
                let request = String::from_utf8(request).unwrap().to_lowercase();
                assert!(request.starts_with("get /api/v1/heatmap?from=2026-01-01&to=2026-01-03 "));
                assert!(request.contains("authorization: bearer test-heatmap-token\r\n"));
                let response = format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                let _ = socket.write_all(response.as_bytes()).await;
            });
            let result = client.daily_activity("2026-01-01", "2026-01-03").await;
            if let Some(code) = expected_error {
                assert_eq!(result.unwrap_err().code(), code);
            } else {
                assert_eq!(result.unwrap(), snapshot);
            }
            server.await.unwrap();
        }
        let client = PatinadClient::new(1, "test-token").unwrap();
        assert_eq!(
            client
                .daily_activity("2026-01-01&extra=1", "2026-01-03")
                .await
                .unwrap_err()
                .code(),
            "invalid-configuration"
        );
    }

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
    fn restore_request_ids_are_strictly_bounded_before_transport() {
        assert!(valid_restore_request_id(
            "restore_0123456789abcdef0123456789abcdef"
        ));
        assert!(!valid_restore_request_id("../restore_0123456789abcdef"));
        assert!(!valid_restore_request_id(
            "restore_0123456789ABCDEF0123456789ABCDEF"
        ));
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
