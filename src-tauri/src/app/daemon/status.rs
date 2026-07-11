use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaemonStartupStatus {
    pub service_name: &'static str,
    pub mode: &'static str,
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
    local_api_port: u16,
    api_token_path: PathBuf,
    data_root: PathBuf,
    db_path: PathBuf,
    webview_root: PathBuf,
) -> DaemonStartupStatus {
    DaemonStartupStatus {
        service_name: "patinad",
        mode: "daemon",
        version: version.into(),
        stage: "stage-1",
        sqlite_enabled: true,
        tracking_enabled: false,
        local_api_enabled: false,
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
