#[cfg(target_os = "linux")]
use crate::platform::linux::{foreground, icon, resource};
#[cfg(target_os = "windows")]
use crate::platform::windows::{foreground, icon, resource};
use serde::Serialize;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;
use tauri::Manager;

#[derive(Clone, Debug, Serialize)]
pub struct ResourceDiagnosticsSnapshot {
    pub webview_window_count: usize,
    pub webview_window_labels: Vec<String>,
    pub process_resources: resource::WindowsProcessResourceSnapshot,
    pub process_details_cache: foreground::ProcessDetailsCacheStats,
    pub icon_result_cache: icon::IconResultCacheStats,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalApiDiagnosticsSnapshot {
    pub base_url: String,
    pub token_path: String,
    pub token_present: bool,
    pub listening: bool,
}

#[tauri::command]
pub fn cmd_get_resource_diagnostics(app: tauri::AppHandle) -> ResourceDiagnosticsSnapshot {
    let webview_window_labels = app
        .webview_windows()
        .keys()
        .cloned()
        .collect::<Vec<String>>();

    ResourceDiagnosticsSnapshot {
        webview_window_count: webview_window_labels.len(),
        webview_window_labels,
        process_resources: resource::current_process_resource_snapshot(),
        process_details_cache: foreground::process_details_cache_stats(),
        icon_result_cache: icon::icon_result_cache_stats(),
    }
}

#[tauri::command]
pub fn cmd_get_local_api_diagnostics() -> LocalApiDiagnosticsSnapshot {
    let port = crate::engine::api::server::DEFAULT_PORT;
    let token_path = crate::engine::api::auth::token_file_path();
    let token_present = crate::engine::api::auth::get_api_token().trim().len() > 0;
    let listening = is_local_api_listening(port);

    build_local_api_diagnostics(port, token_path, token_present, listening)
}

fn build_local_api_diagnostics(
    port: u16,
    token_path: PathBuf,
    token_present: bool,
    listening: bool,
) -> LocalApiDiagnosticsSnapshot {
    LocalApiDiagnosticsSnapshot {
        base_url: format!("http://127.0.0.1:{port}"),
        token_path: token_path.display().to_string(),
        token_present,
        listening,
    }
}

fn is_local_api_listening(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_api_diagnostics_exposes_url_token_path_and_listening_state_without_token_value() {
        let snapshot = super::build_local_api_diagnostics(
            14840,
            std::path::PathBuf::from("/tmp/patina/api_token"),
            true,
            false,
        );

        assert_eq!(snapshot.base_url, "http://127.0.0.1:14840");
        assert_eq!(snapshot.token_path, "/tmp/patina/api_token");
        assert!(snapshot.token_present);
        assert!(!snapshot.listening);
    }
}
