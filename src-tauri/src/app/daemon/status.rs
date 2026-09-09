use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonStartupStatus {
    pub service_name: &'static str,
    pub mode: &'static str,
    pub profile: crate::platform::app_paths::AppProfile,
    pub version: String,
    pub stage: &'static str,
    pub sqlite_enabled: bool,
    pub tracking_enabled: bool,
    pub local_api_enabled: bool,
    pub event_stream_enabled: bool,
    pub local_api_port: u16,
    pub api_token_path: PathBuf,
    pub data_root: PathBuf,
    pub db_path: PathBuf,
    pub webview_root: PathBuf,
    pub notes: Vec<&'static str>,
}

pub fn build_startup_status(
    version: impl Into<String>,
    profile: crate::platform::app_paths::AppProfile,
    local_api_enabled: bool,
    tracking_enabled: bool,
    managed_by_systemd: bool,
    local_api_port: u16,
    storage_paths: &crate::platform::storage_paths::StoragePaths,
) -> DaemonStartupStatus {
    let mut notes = if tracking_enabled {
        vec![
            "daemon owns tracking and watchdog for this profile",
            "daemon observes lock, suspend, resume, and shutdown through systemd-logind",
            "daemon owns the Linux audio participation source and follows its persisted setting",
            "daemon owns the Linux MPRIS participation source",
            "daemon owns the configured loopback browser activity bridge",
            "daemon owns Tools reminders, timers, pomodoro, and Linux notifications",
            "daemon owns the local API listener and owner-only credential lifecycle",
        ]
    } else {
        vec![
            "daemon exposes the shared read-only local API and authenticated event stream when enabled",
            "tracking stays owned by the desktop runtime unless --track is supplied",
        ]
    };
    if tracking_enabled {
        notes.push(if managed_by_systemd {
            "systemd user service is the active tracking owner; the desktop must remain a client for this profile"
        } else {
            "manual tracking preview is active; the desktop must not use the same profile"
        });
    }

    DaemonStartupStatus {
        service_name: "patinad",
        mode: "daemon",
        profile,
        version: version.into(),
        stage: if tracking_enabled && managed_by_systemd {
            "managed-service"
        } else if tracking_enabled {
            "tracking-preview"
        } else {
            "read-only"
        },
        sqlite_enabled: true,
        tracking_enabled,
        local_api_enabled,
        event_stream_enabled: local_api_enabled,
        local_api_port,
        api_token_path: storage_paths.api_token_path.clone(),
        data_root: storage_paths.data_root.clone(),
        db_path: storage_paths.db_path.clone(),
        webview_root: storage_paths.webview_root.clone(),
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage_paths(root: &str) -> crate::platform::storage_paths::StoragePaths {
        crate::platform::storage_paths::StoragePaths::from_roots(
            PathBuf::from(format!("{root}/config")),
            PathBuf::from(root),
            PathBuf::from(root),
            PathBuf::from(root),
            false,
            false,
        )
    }

    #[test]
    fn daemon_status_identifies_stage_two_a_event_stream_runtime() {
        let paths = storage_paths("/tmp/Patina");
        let status = build_startup_status(
            "1.8.3",
            crate::platform::app_paths::AppProfile::Dev,
            false,
            false,
            false,
            14_840,
            &paths,
        );

        assert_eq!(status.service_name, "patinad");
        assert_eq!(status.mode, "daemon");
        assert_eq!(status.profile, crate::platform::app_paths::AppProfile::Dev);
        assert_eq!(status.version, "1.8.3");
        assert_eq!(status.stage, "read-only");
        assert!(status.sqlite_enabled);
        assert!(!status.tracking_enabled);
        assert!(!status.local_api_enabled);
        assert!(!status.event_stream_enabled);
        assert_eq!(status.local_api_port, 14_840);
        assert!(status.api_token_path.ends_with("api_token"));
        assert!(status.data_root.ends_with("Patina"));
        assert!(status.db_path.ends_with("patina.db"));
        assert!(status.webview_root.ends_with("Patina"));
        assert!(status
            .notes
            .iter()
            .any(|note| note.contains("event stream")));
    }

    #[test]
    fn daemon_status_reports_confirmed_enabled_api_port() {
        let paths = storage_paths("/tmp/Patina Dev");
        let status = build_startup_status(
            "1.8.3",
            crate::platform::app_paths::AppProfile::Dev,
            true,
            false,
            false,
            42_321,
            &paths,
        );

        assert!(status.local_api_enabled);
        assert!(status.event_stream_enabled);
        assert_eq!(status.local_api_port, 42_321);
        assert!(!status.tracking_enabled);
    }

    #[test]
    fn daemon_status_reports_explicit_tracking_preview() {
        let paths = storage_paths("/tmp/Patina Dev");
        let status = build_startup_status(
            "1.8.3",
            crate::platform::app_paths::AppProfile::Dev,
            true,
            true,
            false,
            42_321,
            &paths,
        );

        assert_eq!(status.stage, "tracking-preview");
        assert!(status.tracking_enabled);
        assert!(status
            .notes
            .iter()
            .any(|note| note.contains("owns tracking")));
        assert!(status
            .notes
            .iter()
            .any(|note| note.contains("audio participation")));
        assert!(status.notes.iter().any(|note| note.contains("MPRIS")));
        assert!(status
            .notes
            .iter()
            .any(|note| note.contains("browser activity bridge")));
        assert!(status.notes.iter().any(|note| note.contains("Tools")));
        assert!(status
            .notes
            .iter()
            .any(|note| note.contains("credential lifecycle")));
        assert!(status
            .notes
            .iter()
            .any(|note| note.contains("manual tracking preview")));
    }

    #[test]
    fn daemon_status_identifies_managed_tracking_service() {
        let paths = storage_paths("/tmp/Patina");
        let status = build_startup_status(
            "1.9.0-beta.3",
            crate::platform::app_paths::AppProfile::Production,
            true,
            true,
            true,
            14_840,
            &paths,
        );

        assert_eq!(status.stage, "managed-service");
        assert!(status
            .notes
            .iter()
            .any(|note| note.contains("systemd user service is the active tracking owner")));
        assert!(!status
            .notes
            .iter()
            .any(|note| note.contains("tracking preview")));
    }
}
