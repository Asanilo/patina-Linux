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
    local_api_port: u16,
    storage_paths: &crate::platform::storage_paths::StoragePaths,
) -> DaemonStartupStatus {
    DaemonStartupStatus {
        service_name: "patinad",
        mode: "daemon",
        profile,
        version: version.into(),
        stage: "stage-1-read-only",
        sqlite_enabled: true,
        tracking_enabled: false,
        local_api_enabled,
        local_api_port,
        api_token_path: storage_paths.api_token_path.clone(),
        data_root: storage_paths.data_root.clone(),
        db_path: storage_paths.db_path.clone(),
        webview_root: storage_paths.webview_root.clone(),
        notes: vec![
            "daemon exposes the shared read-only local API when enabled",
            "tracking stays owned by the desktop runtime in this stage",
        ],
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
    fn daemon_status_identifies_stage_one_read_only_runtime() {
        let paths = storage_paths("/tmp/Patina");
        let status = build_startup_status(
            "1.8.3",
            crate::platform::app_paths::AppProfile::Dev,
            false,
            14_840,
            &paths,
        );

        assert_eq!(status.service_name, "patinad");
        assert_eq!(status.mode, "daemon");
        assert_eq!(status.profile, crate::platform::app_paths::AppProfile::Dev);
        assert_eq!(status.version, "1.8.3");
        assert_eq!(status.stage, "stage-1-read-only");
        assert!(status.sqlite_enabled);
        assert!(!status.tracking_enabled);
        assert!(!status.local_api_enabled);
        assert_eq!(status.local_api_port, 14_840);
        assert!(status.api_token_path.ends_with("api_token"));
        assert!(status.data_root.ends_with("Patina"));
        assert!(status.db_path.ends_with("patina.db"));
        assert!(status.webview_root.ends_with("Patina"));
        assert!(status.notes.iter().any(|note| note.contains("read-only")));
    }

    #[test]
    fn daemon_status_reports_confirmed_enabled_api_port() {
        let paths = storage_paths("/tmp/Patina Dev");
        let status = build_startup_status(
            "1.8.3",
            crate::platform::app_paths::AppProfile::Dev,
            true,
            42_321,
            &paths,
        );

        assert!(status.local_api_enabled);
        assert_eq!(status.local_api_port, 42_321);
        assert!(!status.tracking_enabled);
    }
}
