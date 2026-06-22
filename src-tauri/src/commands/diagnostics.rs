#[cfg(target_os = "linux")]
use crate::platform::linux::{foreground, icon, resource};
#[cfg(target_os = "windows")]
use crate::platform::windows::{foreground, icon, resource};
use serde::Serialize;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
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
pub fn cmd_get_local_api_diagnostics() -> LocalApiDiagnosticsSnapshot {
    let port = crate::engine::api::server::DEFAULT_PORT;
    let token_path = crate::engine::api::auth::token_file_path();
    let token_present = crate::engine::api::auth::get_api_token().trim().len() > 0;
    let listening = is_local_api_listening(port);

    build_local_api_diagnostics(port, token_path, token_present, listening)
}

#[tauri::command]
pub fn cmd_get_desktop_integration_diagnostics(
    desktop_behavior_state: State<crate::app::state::DesktopBehaviorState>,
) -> DesktopIntegrationDiagnosticsSnapshot {
    let settings = desktop_behavior_state.snapshot();
    let autostart = inspect_autostart_desktop_file();

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
    let executable_path = std::env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
    repair_autostart_desktop_file(&autostart_desktop_file_path(), &executable_path)
        .map_err(|error| format!("failed to repair autostart desktop file: {error}"))?;

    Ok(DesktopIntegrationDiagnosticsSnapshot {
        launch_at_login: desktop_behavior_state.snapshot().launch_at_login,
        start_minimized: desktop_behavior_state.snapshot().start_minimized,
        autostart: inspect_autostart_desktop_file(),
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

fn inspect_autostart_desktop_file() -> AutostartDiagnosticsSnapshot {
    let path = autostart_desktop_file_path();
    let content = std::fs::read_to_string(&path).ok();
    let exec = content.as_deref().and_then(extract_desktop_exec);
    let exists = path.exists();
    let reason = resolve_autostart_reason(exists, exec.as_deref());

    AutostartDiagnosticsSnapshot {
        path: path.display().to_string(),
        exists,
        exec: exec.map(str::to_string),
        valid: reason.is_none(),
        reason,
    }
}

fn repair_autostart_desktop_file(
    desktop_file_path: &Path,
    executable_path: &Path,
) -> std::io::Result<()> {
    if let Some(parent) = desktop_file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(
        desktop_file_path,
        build_autostart_desktop_file(executable_path),
    )
}

fn build_autostart_desktop_file(executable_path: &Path) -> String {
    let executable = quote_desktop_exec_argument(&executable_path.display().to_string());
    format!(
        "[Desktop Entry]\n\
Type=Application\n\
Version=1.0\n\
Name=Patina\n\
Comment=Start Patina in the background\n\
Exec={executable} {}\n\
StartupNotify=false\n\
Terminal=false\n\
X-GNOME-Autostart-enabled=true\n",
        crate::app::runtime::AUTOSTART_ARG
    )
}

fn quote_desktop_exec_argument(argument: &str) -> String {
    if !argument
        .chars()
        .any(|character| character.is_whitespace() || matches!(character, '"' | '\\' | '$' | '`'))
    {
        return argument.to_string();
    }

    let escaped = argument
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`");
    format!("\"{escaped}\"")
}

fn autostart_desktop_file_path() -> PathBuf {
    let config_home = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            Path::new(&home).join(".config")
        });
    config_home.join("autostart").join("Patina.desktop")
}

fn extract_desktop_exec(content: &str) -> Option<&str> {
    content.lines().find_map(|line| {
        let trimmed = line.trim();
        trimmed.strip_prefix("Exec=").map(str::trim)
    })
}

fn resolve_autostart_reason(exists: bool, exec: Option<&str>) -> Option<String> {
    if !exists {
        return Some("desktop-file-missing".to_string());
    }

    let Some(exec) = exec.filter(|value| !value.trim().is_empty()) else {
        return Some("exec-missing".to_string());
    };

    let normalized = exec.to_ascii_lowercase();
    if !normalized.contains("patina") {
        return Some("exec-not-patina".to_string());
    }
    if !normalized.contains("--autostart") {
        return Some("autostart-arg-missing".to_string());
    }

    None
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

    #[test]
    fn autostart_exec_validation_detects_wrong_or_incomplete_commands() {
        assert_eq!(
            super::extract_desktop_exec("Name=Patina\nExec=/usr/bin/patina --autostart\n"),
            Some("/usr/bin/patina --autostart")
        );
        assert_eq!(
            super::resolve_autostart_reason(true, Some("/usr/local/bin/ghostty --autostart"))
                .as_deref(),
            Some("exec-not-patina")
        );
        assert_eq!(
            super::resolve_autostart_reason(true, Some("/usr/bin/patina")).as_deref(),
            Some("autostart-arg-missing")
        );
        assert_eq!(
            super::resolve_autostart_reason(true, Some("/usr/bin/patina --autostart")),
            None
        );
    }

    #[test]
    fn autostart_repair_file_points_to_patina_with_autostart_arg() {
        let content =
            super::build_autostart_desktop_file(std::path::Path::new("/opt/Patina/patina"));

        assert!(content.contains("Name=Patina\n"));
        assert!(content.contains("Exec=/opt/Patina/patina --autostart\n"));
        assert!(content.contains("X-GNOME-Autostart-enabled=true\n"));
    }

    #[test]
    fn autostart_repair_quotes_executable_paths_with_spaces() {
        let content = super::build_autostart_desktop_file(std::path::Path::new(
            "/home/user/My Apps/Patina/patina",
        ));

        assert!(content.contains("Exec=\"/home/user/My Apps/Patina/patina\" --autostart\n"));
    }
}
