use serde::Serialize;

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
}

pub async fn inspect(
    desktop_launch_at_login: bool,
    desktop_autostart_valid: bool,
) -> DaemonServiceDiagnosticsSnapshot {
    #[cfg(target_os = "linux")]
    {
        let service = crate::platform::linux::systemd_user_service::inspect_patinad_service().await;
        build_diagnostics(service, desktop_launch_at_login, desktop_autostart_valid)
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
    }
}

#[cfg(target_os = "linux")]
pub async fn stop_conflicting_service_before_embedded_startup(
    profile: crate::platform::app_paths::AppProfile,
) -> Result<(), String> {
    let snapshot = crate::platform::linux::systemd_user_service::inspect_patinad_service().await;
    if !should_stop_conflicting_service(profile, &snapshot) {
        return Ok(());
    }
    eprintln!(
        "[patinad] stopping an active packaged daemon before the embedded desktop owner starts"
    );
    crate::platform::linux::systemd_user_service::control_patinad_service(
        crate::platform::linux::systemd_user_service::PatinadServiceControlAction::Stop,
    )
    .await?;
    Ok(())
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
    desktop_launch_at_login: bool,
    desktop_autostart_valid: bool,
) -> DaemonServiceDiagnosticsSnapshot {
    let (migration_state, migration_reason) = if !service.manager_available {
        (
            "blocked",
            "systemd user manager is unavailable; service migration cannot be evaluated",
        )
    } else if !service.unit_installed {
        (
            "not-installed",
            "patinad.service is not installed; install the daemon-backed DEB before migration",
        )
    } else if service.enabled || service.active {
        (
            "owner-conflict",
            "patinad.service is enabled or active while Patina Desktop still owns tracking",
        )
    } else if desktop_launch_at_login && desktop_autostart_valid {
        (
            "ready",
            "desktop autostart can be migrated after the desktop becomes a daemon client",
        )
    } else if desktop_launch_at_login {
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
        control_available: false,
        error: service.error,
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{build_diagnostics, should_stop_conflicting_service};
    use crate::platform::app_paths::AppProfile;
    use crate::platform::linux::systemd_user_service::SystemdUserServiceSnapshot;

    #[test]
    fn disabled_installed_service_is_ready_for_a_future_autostart_migration() {
        let snapshot = build_diagnostics(service_snapshot("disabled", false), true, true);

        assert_eq!(snapshot.migration_state, "ready");
        assert!(!snapshot.control_available);
    }

    #[test]
    fn enabled_service_is_a_conflict_before_desktop_owner_cutover() {
        let snapshot = build_diagnostics(service_snapshot("enabled", true), true, true);

        assert_eq!(snapshot.migration_state, "owner-conflict");
        assert!(!snapshot.control_available);
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
