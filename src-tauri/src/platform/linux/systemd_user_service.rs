use zbus::{proxy, zvariant::OwnedObjectPath};

pub const PATINAD_SERVICE_NAME: &str = "patinad.service";
const INSPECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemdUserServiceSnapshot {
    pub manager_available: bool,
    pub unit_installed: bool,
    pub unit_file_state: Option<String>,
    pub enabled: bool,
    pub active_state: Option<String>,
    pub sub_state: Option<String>,
    pub active: bool,
    pub error: Option<String>,
}

impl SystemdUserServiceSnapshot {
    fn manager_unavailable(error: impl Into<String>) -> Self {
        Self {
            manager_available: false,
            unit_installed: false,
            unit_file_state: None,
            enabled: false,
            active_state: None,
            sub_state: None,
            active: false,
            error: Some(error.into()),
        }
    }

    fn unit_missing() -> Self {
        Self {
            manager_available: true,
            unit_installed: false,
            unit_file_state: None,
            enabled: false,
            active_state: None,
            sub_state: None,
            active: false,
            error: None,
        }
    }
}

#[proxy(
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1",
    interface = "org.freedesktop.systemd1.Manager"
)]
trait SystemdUserManager {
    #[zbus(name = "GetUnitFileState")]
    fn get_unit_file_state(&self, name: &str) -> zbus::Result<String>;

    #[zbus(name = "GetUnit")]
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    default_service = "org.freedesktop.systemd1",
    interface = "org.freedesktop.systemd1.Unit"
)]
trait SystemdUserUnit {
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn sub_state(&self) -> zbus::Result<String>;
}

pub async fn inspect_patinad_service() -> SystemdUserServiceSnapshot {
    match tokio::time::timeout(INSPECTION_TIMEOUT, inspect_patinad_service_inner()).await {
        Ok(snapshot) => snapshot,
        Err(_) => SystemdUserServiceSnapshot::manager_unavailable(
            "timed out while inspecting the systemd user service",
        ),
    }
}

async fn inspect_patinad_service_inner() -> SystemdUserServiceSnapshot {
    let connection = match zbus::Connection::session().await {
        Ok(connection) => connection,
        Err(error) => {
            return SystemdUserServiceSnapshot::manager_unavailable(format!(
                "failed to connect to the user session bus: {error}"
            ));
        }
    };
    let manager = match SystemdUserManagerProxy::new(&connection).await {
        Ok(manager) => manager,
        Err(error) => {
            return SystemdUserServiceSnapshot::manager_unavailable(format!(
                "failed to connect to the systemd user manager: {error}"
            ));
        }
    };

    let unit_file_state = match manager.get_unit_file_state(PATINAD_SERVICE_NAME).await {
        Ok(state) => state,
        Err(error) if is_missing_unit_error(&error) => {
            return SystemdUserServiceSnapshot::unit_missing();
        }
        Err(error) => {
            return SystemdUserServiceSnapshot::manager_unavailable(format!(
                "failed to inspect {PATINAD_SERVICE_NAME}: {error}"
            ));
        }
    };

    let enabled = unit_file_state_enables_login_start(&unit_file_state);
    let unit_path = match manager.get_unit(PATINAD_SERVICE_NAME).await {
        Ok(path) => Some(path),
        Err(error) if is_missing_unit_error(&error) => None,
        Err(error) => {
            return SystemdUserServiceSnapshot {
                manager_available: true,
                unit_installed: true,
                unit_file_state: Some(unit_file_state),
                enabled,
                active_state: None,
                sub_state: None,
                active: false,
                error: Some(format!(
                    "failed to inspect the loaded {PATINAD_SERVICE_NAME} unit: {error}"
                )),
            };
        }
    };

    let Some(unit_path) = unit_path else {
        return SystemdUserServiceSnapshot {
            manager_available: true,
            unit_installed: true,
            unit_file_state: Some(unit_file_state),
            enabled,
            active_state: Some("inactive".to_string()),
            sub_state: Some("dead".to_string()),
            active: false,
            error: None,
        };
    };
    let unit_builder = match SystemdUserUnitProxy::builder(&connection).path(unit_path) {
        Ok(builder) => builder,
        Err(error) => {
            return SystemdUserServiceSnapshot {
                manager_available: true,
                unit_installed: true,
                unit_file_state: Some(unit_file_state),
                enabled,
                active_state: None,
                sub_state: None,
                active: false,
                error: Some(format!(
                    "failed to address the loaded {PATINAD_SERVICE_NAME} unit: {error}"
                )),
            };
        }
    };
    let unit = match unit_builder.build().await {
        Ok(unit) => unit,
        Err(error) => {
            return SystemdUserServiceSnapshot {
                manager_available: true,
                unit_installed: true,
                unit_file_state: Some(unit_file_state),
                enabled,
                active_state: None,
                sub_state: None,
                active: false,
                error: Some(format!(
                    "failed to connect to the loaded {PATINAD_SERVICE_NAME} unit: {error}"
                )),
            };
        }
    };
    let active_state = match unit.active_state().await {
        Ok(state) => state,
        Err(error) => {
            return SystemdUserServiceSnapshot {
                manager_available: true,
                unit_installed: true,
                unit_file_state: Some(unit_file_state),
                enabled,
                active_state: None,
                sub_state: None,
                active: false,
                error: Some(format!(
                    "failed to read {PATINAD_SERVICE_NAME} ActiveState: {error}"
                )),
            };
        }
    };
    let sub_state = unit.sub_state().await.ok();

    SystemdUserServiceSnapshot {
        manager_available: true,
        unit_installed: true,
        unit_file_state: Some(unit_file_state),
        enabled,
        active: active_state_is_running(&active_state),
        active_state: Some(active_state),
        sub_state,
        error: None,
    }
}

fn unit_file_state_enables_login_start(state: &str) -> bool {
    matches!(state, "enabled" | "enabled-runtime")
}

fn active_state_is_running(state: &str) -> bool {
    matches!(state, "active" | "activating" | "reloading")
}

fn is_missing_unit_error(error: &zbus::Error) -> bool {
    matches!(
        error,
        zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.systemd1.NoSuchUnit"
                || name.as_str() == "org.freedesktop.systemd1.NoSuchFile"
                || name.as_str() == "org.freedesktop.DBus.Error.FileNotFound"
    )
}

#[cfg(test)]
mod tests {
    use super::{active_state_is_running, unit_file_state_enables_login_start};

    #[test]
    fn only_enabled_unit_file_states_start_at_login() {
        assert!(unit_file_state_enables_login_start("enabled"));
        assert!(unit_file_state_enables_login_start("enabled-runtime"));
        assert!(!unit_file_state_enables_login_start("disabled"));
        assert!(!unit_file_state_enables_login_start("static"));
        assert!(!unit_file_state_enables_login_start("linked"));
        assert!(!unit_file_state_enables_login_start("masked"));
    }

    #[test]
    fn transitional_active_states_are_reported_as_running() {
        assert!(active_state_is_running("active"));
        assert!(active_state_is_running("activating"));
        assert!(active_state_is_running("reloading"));
        assert!(!active_state_is_running("inactive"));
        assert!(!active_state_is_running("failed"));
        assert!(!active_state_is_running("deactivating"));
    }

    #[tokio::test]
    #[ignore = "requires a live systemd user manager"]
    async fn live_user_manager_snapshot_is_classified_without_mutation() {
        let snapshot = super::inspect_patinad_service().await;
        eprintln!("{snapshot:?}");
        assert!(snapshot.manager_available, "{:?}", snapshot.error);
    }
}
