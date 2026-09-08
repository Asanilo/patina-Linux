use serde::Serialize;

#[derive(Debug, Default)]
pub struct DaemonServiceMutationState {
    gate: tokio::sync::Mutex<()>,
}

impl DaemonServiceMutationState {
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.gate.lock().await
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmbeddedCutoverPreparation {
    ContinueEmbedded,
    RestartAsDaemonClient { request_id: String },
}

#[derive(Clone, Debug, Serialize)]
pub struct DaemonServiceDiagnosticsSnapshot {
    pub service_name: String,
    pub manager_available: bool,
    pub unit_installed: bool,
    pub unit_file_state: Option<String>,
    pub enabled: bool,
    pub active_state: Option<String>,
    pub sub_state: Option<String>,
    pub active: bool,
    pub migration_state: String,
    pub migration_reason: String,
    pub control_available: bool,
    pub error: Option<String>,
    pub cutover: crate::app::runtime_owner_cutover::RuntimeOwnerCutoverDiagnosticsSnapshot,
}

pub async fn inspect(
    profile: crate::platform::app_paths::AppProfile,
    control_root: &std::path::Path,
    background_tracking_at_login: bool,
    desktop_launch_at_login: bool,
    desktop_autostart_valid: bool,
    desktop_owns_embedded_runtime: bool,
) -> DaemonServiceDiagnosticsSnapshot {
    #[cfg(target_os = "linux")]
    {
        let service = crate::platform::linux::systemd_user_service::inspect_patinad_service().await;
        let cutover = crate::app::runtime_owner_cutover::diagnose(control_root, profile);
        build_diagnostics(
            service,
            cutover,
            background_tracking_at_login,
            desktop_launch_at_login,
            desktop_autostart_valid,
            desktop_owns_embedded_runtime,
        )
    }

    #[cfg(not(target_os = "linux"))]
    DaemonServiceDiagnosticsSnapshot {
        service_name: "patinad.service".to_string(),
        manager_available: false,
        unit_installed: false,
        unit_file_state: None,
        enabled: false,
        active_state: None,
        sub_state: None,
        active: false,
        migration_state: "unsupported".to_string(),
        migration_reason: "patinad systemd integration is only available on Linux".to_string(),
        control_available: false,
        error: None,
        cutover: crate::app::runtime_owner_cutover::RuntimeOwnerCutoverDiagnosticsSnapshot {
            state: "unsupported".to_string(),
            request_id: None,
            updated_at_ms: None,
            failure_code: None,
            failure_message: None,
            background_tracking_at_login: None,
        },
    }
}

#[cfg(target_os = "linux")]
pub async fn stop_conflicting_service_before_embedded_startup(
    profile: crate::platform::app_paths::AppProfile,
    control_root: &std::path::Path,
) -> Result<(), String> {
    use crate::platform::linux::systemd_user_service::{
        control_patinad_service, PatinadServiceControlAction,
    };

    let mut snapshot =
        crate::platform::linux::systemd_user_service::inspect_patinad_service().await;
    if should_stop_conflicting_service(profile, &snapshot) {
        eprintln!(
            "[patinad] stopping an active packaged daemon before the embedded desktop owner starts"
        );
        snapshot = control_patinad_service(PatinadServiceControlAction::Stop).await?;
    }
    let rolled_back =
        crate::app::runtime_owner_cutover::diagnose(control_root, profile).state == "rolled-back";
    if rolled_back && snapshot.enabled {
        eprintln!("[patinad] disabling packaged daemon for the explicit embedded fallback");
        control_patinad_service(PatinadServiceControlAction::Disable).await?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub async fn prepare_runtime_owner_cutover(
    profile: crate::platform::app_paths::AppProfile,
    control_root: &std::path::Path,
    settings: crate::domain::settings::DesktopBehaviorSettings,
) -> Result<EmbeddedCutoverPreparation, String> {
    use crate::platform::linux::systemd_user_service::inspect_patinad_service;

    if profile != crate::platform::app_paths::AppProfile::Production {
        return Ok(EmbeddedCutoverPreparation::ContinueEmbedded);
    }
    if crate::app::runtime_owner_cutover::diagnose(control_root, profile).state == "rolled-back" {
        return Ok(EmbeddedCutoverPreparation::ContinueEmbedded);
    }
    let service = inspect_patinad_service().await;
    if !service.manager_available || !service.unit_installed || service.error.is_some() {
        return Ok(EmbeddedCutoverPreparation::ContinueEmbedded);
    }
    if service.active {
        return Err("patinad.service is still active before owner cutover preparation".to_string());
    }

    let reservation = crate::app::runtime_owner_cutover::prepare(
        control_root,
        profile,
        settings.background_tracking_at_login,
        settings.launch_at_login,
        crate::app::runtime::now_ms(),
    )?;
    if reservation.status != crate::app::runtime_owner_cutover::RuntimeOwnerCutoverStatus::Prepared
    {
        return Err(format!(
            "runtime owner cutover `{}` is not in prepared state",
            reservation.request_id
        ));
    }

    let prepare_result = async {
        crate::app::autostart::apply_linux_autostart(settings.launch_at_login)?;
        let service =
            apply_background_tracking_login_preference(settings.background_tracking_at_login)
                .await?;
        if service.active {
            return Err("patinad.service started before the embedded owner exited".to_string());
        }
        Ok::<(), String>(())
    }
    .await;
    if let Err(error) = prepare_result {
        return Err(record_cutover_failure(
            control_root,
            profile,
            &reservation.request_id,
            "prepare-failed",
            &error,
        ));
    }

    Ok(EmbeddedCutoverPreparation::RestartAsDaemonClient {
        request_id: reservation.request_id,
    })
}

#[cfg(target_os = "linux")]
pub async fn activate_runtime_owner_cutover(
    profile: crate::platform::app_paths::AppProfile,
    control_root: &std::path::Path,
) -> Result<(), String> {
    use crate::app::runtime_owner_cutover::{
        RuntimeOwnerCutoverStatus, RuntimeOwnerStartupDecision,
    };
    use crate::platform::linux::systemd_user_service::{
        control_patinad_service, PatinadServiceControlAction,
    };

    let decision = crate::app::runtime_owner_cutover::decide_desktop_startup(control_root, profile);
    let (reservation, should_attempt_service_start) = match decision {
        RuntimeOwnerStartupDecision::Embedded => {
            return Err(
                "managed daemon client has no runtime owner cutover reservation".to_string(),
            )
        }
        RuntimeOwnerStartupDecision::Blocked { reason } => return Err(reason),
        RuntimeOwnerStartupDecision::DaemonClient {
            reservation,
            should_attempt_service_start,
        } => (reservation, should_attempt_service_start),
    };
    if !should_attempt_service_start {
        return Err(reservation
            .failure_message
            .unwrap_or_else(|| "runtime owner cutover requires explicit repair".to_string()));
    }
    if reservation.status == RuntimeOwnerCutoverStatus::Completed {
        apply_background_tracking_login_preference(reservation.background_tracking_at_login)
            .await?;
        crate::app::runtime_lease::wait_for_runtime_lease_release(
            control_root,
            std::time::Duration::from_secs(5),
        )
        .await?;
        control_patinad_service(PatinadServiceControlAction::Start).await?;
        return Ok(());
    }

    let activating = crate::app::runtime_owner_cutover::mark_activating(
        control_root,
        profile,
        &reservation.request_id,
        crate::app::runtime::now_ms(),
    )?;
    let activation_result = async {
        crate::app::autostart::apply_linux_autostart(activating.desktop_launch_at_login)?;
        apply_background_tracking_login_preference(activating.background_tracking_at_login).await?;
        crate::app::runtime_lease::wait_for_runtime_lease_release(
            control_root,
            std::time::Duration::from_secs(5),
        )
        .await?;
        control_patinad_service(PatinadServiceControlAction::Start).await?;
        Ok::<(), String>(())
    }
    .await;
    activation_result.map_err(|error| {
        record_cutover_failure(
            control_root,
            profile,
            &activating.request_id,
            "activation-failed",
            &error,
        )
    })
}

#[cfg(target_os = "linux")]
pub async fn prepare_explicit_runtime_owner_retry(
    profile: crate::platform::app_paths::AppProfile,
    control_root: &std::path::Path,
    settings: crate::domain::settings::DesktopBehaviorSettings,
) -> Result<crate::app::runtime_owner_cutover::RuntimeOwnerCutoverSnapshot, String> {
    use crate::platform::linux::systemd_user_service::{
        control_patinad_service, inspect_patinad_service, PatinadServiceControlAction,
    };

    if profile != crate::platform::app_paths::AppProfile::Production {
        return Err("runtime owner cutover retry is only available for Production".to_string());
    }
    let cutover = crate::app::runtime_owner_cutover::diagnose(control_root, profile);
    if !explicit_retry_allowed(&cutover) {
        return Err("runtime owner cutover is not in a retryable state".to_string());
    }
    let service = inspect_patinad_service().await;
    if !service.manager_available {
        return Err(service
            .error
            .unwrap_or_else(|| "systemd user manager is unavailable".to_string()));
    }
    if !service.unit_installed {
        return Err("patinad.service is not installed".to_string());
    }
    if let Some(error) = service.error {
        return Err(error);
    }

    control_patinad_service(PatinadServiceControlAction::Stop).await?;
    crate::app::runtime_lease::wait_for_runtime_lease_release(
        control_root,
        std::time::Duration::from_secs(5),
    )
    .await?;
    crate::app::runtime_owner_cutover::prepare_explicit_retry(
        control_root,
        profile,
        settings.background_tracking_at_login,
        settings.launch_at_login,
        crate::app::runtime::now_ms(),
    )
}

#[cfg(target_os = "linux")]
pub async fn prepare_explicit_runtime_owner_rollback(
    profile: crate::platform::app_paths::AppProfile,
    control_root: &std::path::Path,
    settings: crate::domain::settings::DesktopBehaviorSettings,
    pool: &sqlx::Pool<sqlx::Sqlite>,
) -> Result<crate::app::runtime_owner_cutover::RuntimeOwnerCutoverSnapshot, String> {
    use crate::platform::linux::systemd_user_service::{
        control_patinad_service, inspect_patinad_service, PatinadServiceControlAction,
    };

    if profile != crate::platform::app_paths::AppProfile::Production {
        return Err("runtime owner rollback is only available for Production".to_string());
    }
    let cutover = crate::app::runtime_owner_cutover::diagnose(control_root, profile);
    if !explicit_rollback_allowed(&cutover) {
        return Err("runtime owner cutover is not in a rollback-eligible state".to_string());
    }
    let service = inspect_patinad_service().await;
    if !service.manager_available {
        return Err(service
            .error
            .unwrap_or_else(|| "systemd user manager is unavailable".to_string()));
    }
    if !service.unit_installed {
        return Err("patinad.service is not installed".to_string());
    }
    if let Some(error) = service.error {
        return Err(error);
    }

    let rolling_back = crate::app::runtime_owner_cutover::prepare_explicit_rollback(
        control_root,
        profile,
        settings.launch_at_login,
        crate::app::runtime::now_ms(),
    )?;
    control_patinad_service(PatinadServiceControlAction::Stop).await?;
    crate::app::runtime_lease::wait_for_runtime_lease_release(
        control_root,
        std::time::Duration::from_secs(5),
    )
    .await?;
    control_patinad_service(PatinadServiceControlAction::Disable).await?;
    crate::app::autostart::apply_linux_autostart(settings.launch_at_login)?;
    crate::data::repositories::app_settings::save_background_tracking_login_preference(pool, false)
        .await?;
    crate::app::runtime_owner_cutover::mark_rolled_back(
        control_root,
        profile,
        &rolling_back.request_id,
        crate::app::runtime::now_ms(),
    )
}

#[cfg(target_os = "linux")]
pub async fn set_background_tracking_login_preference(
    profile: crate::platform::app_paths::AppProfile,
    control_root: &std::path::Path,
    enabled: bool,
) -> Result<crate::app::runtime_owner_cutover::RuntimeOwnerCutoverSnapshot, String> {
    if profile != crate::platform::app_paths::AppProfile::Production {
        return Err(
            "background tracking login preference is only available for Production".to_string(),
        );
    }
    let reservation = crate::app::runtime_owner_cutover::update_completed_background_preference(
        control_root,
        profile,
        enabled,
        crate::app::runtime::now_ms(),
    )?;
    apply_background_tracking_login_preference(enabled).await?;
    Ok(reservation)
}

#[cfg(target_os = "linux")]
pub async fn reconcile_completed_background_login_preference(
    profile: crate::platform::app_paths::AppProfile,
    control_root: &std::path::Path,
    pool: &sqlx::Pool<sqlx::Sqlite>,
) -> Result<Option<bool>, String> {
    let decision = crate::app::runtime_owner_cutover::decide_desktop_startup(control_root, profile);
    let crate::app::runtime_owner_cutover::RuntimeOwnerStartupDecision::DaemonClient {
        reservation,
        ..
    } = decision
    else {
        return Ok(None);
    };
    if reservation.status != crate::app::runtime_owner_cutover::RuntimeOwnerCutoverStatus::Completed
    {
        return Ok(None);
    }
    crate::data::repositories::app_settings::save_background_tracking_login_preference(
        pool,
        reservation.background_tracking_at_login,
    )
    .await?;
    Ok(Some(reservation.background_tracking_at_login))
}

#[cfg(target_os = "linux")]
async fn apply_background_tracking_login_preference(
    enabled: bool,
) -> Result<crate::platform::linux::systemd_user_service::SystemdUserServiceSnapshot, String> {
    crate::platform::linux::systemd_user_service::control_patinad_service(if enabled {
        crate::platform::linux::systemd_user_service::PatinadServiceControlAction::Enable
    } else {
        crate::platform::linux::systemd_user_service::PatinadServiceControlAction::Disable
    })
    .await
}

#[cfg(target_os = "linux")]
fn explicit_retry_allowed(
    cutover: &crate::app::runtime_owner_cutover::RuntimeOwnerCutoverDiagnosticsSnapshot,
) -> bool {
    matches!(cutover.state.as_str(), "failed" | "blocked")
}

#[cfg(target_os = "linux")]
fn explicit_rollback_allowed(
    cutover: &crate::app::runtime_owner_cutover::RuntimeOwnerCutoverDiagnosticsSnapshot,
) -> bool {
    matches!(
        cutover.state.as_str(),
        "completed" | "failed" | "blocked" | "rolling-back"
    )
}

#[cfg(target_os = "linux")]
pub async fn confirm_runtime_owner_cutover(
    profile: crate::platform::app_paths::AppProfile,
    control_root: std::path::PathBuf,
    client_state: crate::app::daemon_client::PatinadClientState,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    use crate::app::runtime_owner_cutover::{
        RuntimeOwnerCutoverStatus, RuntimeOwnerStartupDecision,
    };
    use std::time::{Duration, Instant};

    let decision =
        crate::app::runtime_owner_cutover::decide_desktop_startup(&control_root, profile);
    let reservation = match decision {
        RuntimeOwnerStartupDecision::DaemonClient { reservation, .. }
            if reservation.status == RuntimeOwnerCutoverStatus::Activating =>
        {
            reservation
        }
        _ => return,
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    let failure_message = loop {
        if *shutdown.borrow() {
            return;
        }
        let attempt_error = match client_state.require() {
            Ok(client) => match client.negotiate_tracking_owner().await {
                Ok(negotiation) if negotiation.tracking_ready => {
                    match crate::app::runtime_owner_cutover::mark_completed(
                        &control_root,
                        profile,
                        &reservation.request_id,
                        crate::app::runtime::now_ms(),
                    ) {
                        Ok(_) => println!(
                            "[patinad] runtime owner cutover {} completed",
                            reservation.request_id
                        ),
                        Err(error) => eprintln!(
                            "[patinad] failed to confirm runtime owner cutover {}: {error}",
                            reservation.request_id
                        ),
                    }
                    return;
                }
                Ok(_) => "patinad owns tracking but is not ready".to_string(),
                Err(error) => {
                    if permanent_negotiation_failure(&error) {
                        break error.to_string();
                    }
                    error.to_string()
                }
            },
            Err(error) => error,
        };
        if Instant::now() >= deadline {
            break attempt_error;
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(250)) => {}
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            }
        }
    };
    let error = record_cutover_failure(
        &control_root,
        profile,
        &reservation.request_id,
        "daemon-not-ready",
        &failure_message,
    );
    eprintln!(
        "[patinad] runtime owner cutover {} failed: {error}",
        reservation.request_id
    );
}

#[cfg(target_os = "linux")]
fn permanent_negotiation_failure(
    error: &crate::platform::daemon_client::PatinadClientError,
) -> bool {
    matches!(
        error,
        crate::platform::daemon_client::PatinadClientError::InvalidConfiguration(_)
            | crate::platform::daemon_client::PatinadClientError::Unauthorized
            | crate::platform::daemon_client::PatinadClientError::InvalidResponse(_)
            | crate::platform::daemon_client::PatinadClientError::WrongRuntimeHost(_)
            | crate::platform::daemon_client::PatinadClientError::IncompatibleProtocol { .. }
            | crate::platform::daemon_client::PatinadClientError::TrackingNotOwned
            | crate::platform::daemon_client::PatinadClientError::EventStreamUnavailable
    )
}

#[cfg(target_os = "linux")]
fn record_cutover_failure(
    control_root: &std::path::Path,
    profile: crate::platform::app_paths::AppProfile,
    request_id: &str,
    code: &str,
    message: &str,
) -> String {
    match crate::app::runtime_owner_cutover::mark_failed(
        control_root,
        profile,
        request_id,
        code,
        message,
        crate::app::runtime::now_ms(),
    ) {
        Ok(_) => message.to_string(),
        Err(marker_error) => {
            format!("{message}; failed to persist cutover failure: {marker_error}")
        }
    }
}

#[cfg(target_os = "linux")]
fn should_stop_conflicting_service(
    profile: crate::platform::app_paths::AppProfile,
    service: &crate::platform::linux::systemd_user_service::SystemdUserServiceSnapshot,
) -> bool {
    profile == crate::platform::app_paths::AppProfile::Production && service.active
}

#[cfg(target_os = "linux")]
fn build_diagnostics(
    service: crate::platform::linux::systemd_user_service::SystemdUserServiceSnapshot,
    cutover: crate::app::runtime_owner_cutover::RuntimeOwnerCutoverDiagnosticsSnapshot,
    background_tracking_at_login: bool,
    desktop_launch_at_login: bool,
    desktop_autostart_valid: bool,
    desktop_owns_embedded_runtime: bool,
) -> DaemonServiceDiagnosticsSnapshot {
    let (migration_state, migration_reason) =
        if matches!(cutover.state.as_str(), "failed" | "blocked") {
            (
                "cutover-failed",
                "runtime owner cutover requires explicit repair",
            )
        } else if matches!(cutover.state.as_str(), "prepared" | "activating") {
            (
                "cutover-pending",
                "runtime owner cutover is waiting for restart or daemon readiness",
            )
        } else if cutover.state == "rolling-back" {
            (
                "rollback-pending",
                "runtime owner rollback is waiting for service shutdown or desktop restart",
            )
        } else if !service.manager_available {
            (
                "blocked",
                "systemd user manager is unavailable; service migration cannot be evaluated",
            )
        } else if !service.unit_installed {
            (
                "not-installed",
                "patinad.service is not installed; install the daemon-backed DEB before migration",
            )
        } else if desktop_owns_embedded_runtime && (service.enabled || service.active) {
            (
                "owner-conflict",
                "patinad.service is enabled or active while Patina Desktop still owns tracking",
            )
        } else if desktop_owns_embedded_runtime && cutover.state == "rolled-back" {
            (
                "embedded-rollback",
                "Patina Desktop is using the explicit embedded runtime fallback",
            )
        } else if !desktop_owns_embedded_runtime && !service.active {
            (
                "managed-blocked",
                "Patina Desktop is a daemon client but patinad.service is not active",
            )
        } else if !desktop_owns_embedded_runtime
            && cutover.state == "completed"
            && cutover
                .background_tracking_at_login
                .is_some_and(|enabled| enabled != service.enabled)
        {
            (
            "preference-mismatch",
            "patinad.service login state does not match the saved background tracking preference",
        )
        } else if !desktop_owns_embedded_runtime && service.active {
            (
                "managed",
                "patinad.service is the active tracking owner for Patina Desktop",
            )
        } else if background_tracking_at_login
            && (!desktop_launch_at_login || desktop_autostart_valid)
        {
            (
                "ready",
                if desktop_launch_at_login {
                    "desktop autostart can be migrated after the desktop becomes a daemon client"
                } else {
                    "background tracking can be enabled without desktop autostart"
                },
            )
        } else if background_tracking_at_login {
            (
                "blocked",
                "desktop launch-at-login is enabled but its autostart entry is not valid",
            )
        } else {
            (
                "not-requested",
                "background login startup has not been requested; the service remains disabled",
            )
        };

    DaemonServiceDiagnosticsSnapshot {
        service_name: crate::platform::linux::systemd_user_service::PATINAD_SERVICE_NAME
            .to_string(),
        manager_available: service.manager_available,
        unit_installed: service.unit_installed,
        unit_file_state: service.unit_file_state,
        enabled: service.enabled,
        active_state: service.active_state,
        sub_state: service.sub_state,
        active: service.active,
        migration_state: migration_state.to_string(),
        migration_reason: migration_reason.to_string(),
        control_available: !desktop_owns_embedded_runtime
            && service.manager_available
            && service.unit_installed
            && service.error.is_none()
            && matches!(
                cutover.state.as_str(),
                "completed" | "failed" | "blocked" | "rolling-back"
            ),
        error: service.error,
        cutover,
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{
        build_diagnostics, explicit_retry_allowed, explicit_rollback_allowed,
        permanent_negotiation_failure, should_stop_conflicting_service,
    };
    use crate::platform::app_paths::AppProfile;
    use crate::platform::linux::systemd_user_service::SystemdUserServiceSnapshot;

    fn cutover(
        state: &str,
    ) -> crate::app::runtime_owner_cutover::RuntimeOwnerCutoverDiagnosticsSnapshot {
        crate::app::runtime_owner_cutover::RuntimeOwnerCutoverDiagnosticsSnapshot {
            state: state.to_string(),
            request_id: None,
            updated_at_ms: None,
            failure_code: None,
            failure_message: None,
            background_tracking_at_login: None,
        }
    }

    #[test]
    fn disabled_installed_service_is_ready_for_a_future_autostart_migration() {
        let snapshot = build_diagnostics(
            service_snapshot("disabled", false),
            cutover("not-requested"),
            true,
            true,
            true,
            true,
        );

        assert_eq!(snapshot.migration_state, "ready");
        assert!(!snapshot.control_available);
    }

    #[test]
    fn background_login_does_not_require_desktop_autostart_when_desktop_login_is_disabled() {
        let snapshot = build_diagnostics(
            service_snapshot("disabled", false),
            cutover("not-requested"),
            true,
            false,
            false,
            true,
        );

        assert_eq!(snapshot.migration_state, "ready");
        assert_eq!(
            snapshot.migration_reason,
            "background tracking can be enabled without desktop autostart"
        );
    }

    #[test]
    fn enabled_service_is_a_conflict_before_desktop_owner_cutover() {
        let snapshot = build_diagnostics(
            service_snapshot("enabled", true),
            cutover("not-requested"),
            true,
            true,
            true,
            true,
        );

        assert_eq!(snapshot.migration_state, "owner-conflict");
        assert!(!snapshot.control_available);
    }

    #[test]
    fn active_service_is_healthy_after_desktop_becomes_a_managed_client() {
        let mut service = service_snapshot("enabled", true);
        service.active = true;
        service.active_state = Some("active".to_string());
        service.sub_state = Some("running".to_string());

        let snapshot = build_diagnostics(service, cutover("completed"), true, true, true, false);

        assert_eq!(snapshot.migration_state, "managed");
        assert!(snapshot.active);
        assert!(snapshot.control_available);
    }

    #[test]
    fn inactive_service_is_blocked_after_desktop_becomes_a_managed_client() {
        let snapshot = build_diagnostics(
            service_snapshot("disabled", false),
            cutover("completed"),
            true,
            true,
            true,
            false,
        );

        assert_eq!(snapshot.migration_state, "managed-blocked");
        assert!(!snapshot.active);
    }

    #[test]
    fn failed_cutover_takes_priority_over_service_state() {
        let mut failed = cutover("failed");
        failed.failure_code = Some("daemon-not-ready".to_string());
        failed.failure_message = Some("timed out".to_string());

        let snapshot = build_diagnostics(
            service_snapshot("enabled", true),
            failed,
            true,
            true,
            true,
            false,
        );

        assert_eq!(snapshot.migration_state, "cutover-failed");
        assert_eq!(
            snapshot.cutover.failure_message.as_deref(),
            Some("timed out")
        );
        assert!(snapshot.control_available);
    }

    #[test]
    fn completed_cutover_reports_a_login_preference_mismatch() {
        let mut service = service_snapshot("disabled", false);
        service.active = true;
        service.active_state = Some("active".to_string());
        service.sub_state = Some("running".to_string());
        let mut completed = cutover("completed");
        completed.background_tracking_at_login = Some(true);

        let snapshot = build_diagnostics(service, completed, true, true, true, false);

        assert_eq!(snapshot.migration_state, "preference-mismatch");
        assert!(snapshot.control_available);
    }

    #[test]
    fn explicit_retry_never_interrupts_a_healthy_or_pending_cutover() {
        assert!(explicit_retry_allowed(&cutover("failed")));
        assert!(explicit_retry_allowed(&cutover("blocked")));
        assert!(!explicit_retry_allowed(&cutover("completed")));
        assert!(!explicit_retry_allowed(&cutover("activating")));
        assert!(!explicit_retry_allowed(&cutover("not-requested")));
    }

    #[test]
    fn explicit_rollback_only_accepts_managed_or_interrupted_states() {
        assert!(explicit_rollback_allowed(&cutover("completed")));
        assert!(explicit_rollback_allowed(&cutover("failed")));
        assert!(explicit_rollback_allowed(&cutover("blocked")));
        assert!(explicit_rollback_allowed(&cutover("rolling-back")));
        assert!(!explicit_rollback_allowed(&cutover("prepared")));
        assert!(!explicit_rollback_allowed(&cutover("rolled-back")));
        assert!(!explicit_rollback_allowed(&cutover("not-requested")));
    }

    #[test]
    fn completed_rollback_is_a_healthy_embedded_fallback_when_service_is_disabled() {
        let snapshot = build_diagnostics(
            service_snapshot("disabled", false),
            cutover("rolled-back"),
            false,
            true,
            true,
            true,
        );

        assert_eq!(snapshot.migration_state, "embedded-rollback");
        assert!(!snapshot.active);
    }

    #[test]
    fn only_production_embedded_startup_stops_a_conflicting_service() {
        let mut service = service_snapshot("enabled", true);
        service.active = true;
        service.active_state = Some("active".to_string());

        assert!(should_stop_conflicting_service(
            AppProfile::Production,
            &service
        ));
        assert!(!should_stop_conflicting_service(
            AppProfile::Local,
            &service
        ));
        assert!(!should_stop_conflicting_service(AppProfile::Dev, &service));

        service.active = false;
        assert!(!should_stop_conflicting_service(
            AppProfile::Production,
            &service
        ));
    }

    #[test]
    fn only_permanent_negotiation_errors_abort_cutover_immediately() {
        use crate::platform::daemon_client::PatinadClientError;

        assert!(permanent_negotiation_failure(
            &PatinadClientError::IncompatibleProtocol {
                client: 2,
                server: 3,
                min_supported_client: 3,
                max_supported_client: 3,
            }
        ));
        assert!(permanent_negotiation_failure(
            &PatinadClientError::WrongRuntimeHost("desktop".to_string())
        ));
        assert!(!permanent_negotiation_failure(
            &PatinadClientError::Unreachable("starting".to_string())
        ));
        assert!(!permanent_negotiation_failure(&PatinadClientError::Http {
            status: 503,
            code: Some("starting".to_string()),
            message: "not ready".to_string(),
        }));
    }

    fn service_snapshot(unit_file_state: &str, enabled: bool) -> SystemdUserServiceSnapshot {
        SystemdUserServiceSnapshot {
            manager_available: true,
            unit_installed: true,
            unit_file_state: Some(unit_file_state.to_string()),
            enabled,
            active_state: Some(if enabled { "failed" } else { "inactive" }.to_string()),
            sub_state: Some(if enabled { "failed" } else { "dead" }.to_string()),
            active: false,
            error: None,
        }
    }
}
