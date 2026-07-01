#[cfg(target_os = "linux")]
use crate::platform::linux::{foreground, icon, resource};
#[cfg(target_os = "windows")]
use crate::platform::windows::{foreground, icon, resource};
use serde::Serialize;
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;
use tauri::{Manager, State};

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

#[derive(Clone, Debug, Serialize)]
pub struct LocalApiSettingsSnapshot {
    pub port: u16,
    pub token: String,
    pub token_path: String,
    pub base_url: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AutostartDiagnosticsSnapshot {
    pub path: String,
    pub exists: bool,
    pub exec: Option<String>,
    pub valid: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DesktopIntegrationDiagnosticsSnapshot {
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub autostart: AutostartDiagnosticsSnapshot,
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
pub fn cmd_get_local_api_diagnostics(
    api_server_state: State<crate::engine::api::server::ApiServerState>,
) -> LocalApiDiagnosticsSnapshot {
    let confirmed_port = api_server_state.confirmed_port();
    let port = confirmed_port.unwrap_or(crate::engine::api::server::DEFAULT_PORT);
    let token_path = crate::engine::api::auth::token_file_path();
    let token_present = !crate::engine::api::auth::get_api_token().trim().is_empty();
    let listening = confirmed_port.map(is_local_api_listening).unwrap_or(false);

    build_local_api_diagnostics(port, token_path, token_present, listening)
}

#[tauri::command]
pub fn cmd_get_local_api_settings(
    api_server_state: State<crate::engine::api::server::ApiServerState>,
) -> LocalApiSettingsSnapshot {
    let port = api_server_state.port();
    let token_path = crate::engine::api::auth::token_file_path();
    LocalApiSettingsSnapshot {
        port,
        token: crate::engine::api::auth::get_api_token(),
        token_path: token_path.display().to_string(),
        base_url: format!("http://127.0.0.1:{port}"),
    }
}

#[tauri::command]
pub fn cmd_get_desktop_integration_diagnostics(
    desktop_behavior_state: State<crate::app::state::DesktopBehaviorState>,
) -> DesktopIntegrationDiagnosticsSnapshot {
    let settings = desktop_behavior_state.snapshot();
    let autostart = build_autostart_diagnostics_snapshot(
        crate::app::autostart::inspect_autostart_desktop_file(),
    );

    DesktopIntegrationDiagnosticsSnapshot {
        launch_at_login: settings.launch_at_login,
        start_minimized: settings.start_minimized,
        autostart,
    }
}

#[tauri::command]
pub fn cmd_repair_autostart_desktop_file(
    desktop_behavior_state: State<crate::app::state::DesktopBehaviorState>,
) -> Result<DesktopIntegrationDiagnosticsSnapshot, String> {
    crate::app::autostart::repair_current_exe_autostart_desktop_file()?;

    Ok(DesktopIntegrationDiagnosticsSnapshot {
        launch_at_login: desktop_behavior_state.snapshot().launch_at_login,
        start_minimized: desktop_behavior_state.snapshot().start_minimized,
        autostart: build_autostart_diagnostics_snapshot(
            crate::app::autostart::inspect_autostart_desktop_file(),
        ),
    })
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

fn build_autostart_diagnostics_snapshot(
    inspection: crate::app::autostart::AutostartDesktopFileInspection,
) -> AutostartDiagnosticsSnapshot {
    let valid = inspection.valid();
    AutostartDiagnosticsSnapshot {
        path: inspection.path.display().to_string(),
        exists: inspection.exists,
        exec: inspection.exec,
        valid,
        reason: inspection.reason,
    }
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
