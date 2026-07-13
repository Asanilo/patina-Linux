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
    api_token_path: PathBuf,
    data_root: PathBuf,
    db_path: PathBuf,
    webview_root: PathBuf,
) -> DaemonStartupStatus {
    DaemonStartupStatus {
        service_name: "patinad",
        mode: "daemon",
        profile,
        version: version.into(),
        stage: "stage-1",
        sqlite_enabled: true,
        tracking_enabled: false,
        local_api_enabled,
        local_api_port,
        api_token_path,
        data_root,
        db_path,
        webview_root,
        notes: vec![
            "daemon skeleton only",
            "tracking and local API stay owned by the desktop runtime in this stage",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_status_identifies_stage_one_runtime() {
        let status = build_startup_status(
            "1.8.3",
            crate::platform::app_paths::AppProfile::Dev,
            false,
            14_840,
            PathBuf::from("/tmp/api_token"),
            PathBuf::from("/tmp/Patina"),
            PathBuf::from("/tmp/Patina/patina.db"),
            PathBuf::from("/tmp/Patina"),
        );

        assert_eq!(status.service_name, "patinad");
        assert_eq!(status.mode, "daemon");
        assert_eq!(status.profile, crate::platform::app_paths::AppProfile::Dev);
        assert_eq!(status.version, "1.8.3");
        assert_eq!(status.stage, "stage-1");
        assert!(status.sqlite_enabled);
        assert!(!status.tracking_enabled);
        assert!(!status.local_api_enabled);
        assert_eq!(status.local_api_port, 14_840);
        assert!(status.api_token_path.ends_with("api_token"));
        assert!(status.data_root.ends_with("Patina"));
        assert!(status.db_path.ends_with("patina.db"));
        assert!(status.webview_root.ends_with("Patina"));
        assert!(status.notes.iter().any(|note| note.contains("skeleton")));
    }
}
