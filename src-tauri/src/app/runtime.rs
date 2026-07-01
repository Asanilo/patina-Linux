use crate::app::desktop_behavior;
use crate::app::main_window;
use crate::app::runtime_tasks;
use crate::app::state::DesktopBehaviorState;
use crate::app::tray::{apply_tray_visibility, setup_tray, MAIN_WINDOW_LABEL};
use crate::data::sqlite_pool::wait_for_sqlite_pool;
use crate::engine::tracking::watchdog::RuntimeHealthState;
#[cfg(target_os = "linux")]
use crate::platform::linux::{audio, media, power};
#[cfg(target_os = "windows")]
use crate::platform::windows::{audio, media, power};
#[cfg(any(test, all(not(debug_assertions), not(patina_local_build))))]
use std::path::Path;
use std::sync::Arc;
use tauri::Manager;

pub const AUTOSTART_ARG: &str = "--autostart";

pub fn was_launched_by_autostart() -> bool {
    std::env::args().any(|arg| arg == AUTOSTART_ARG)
}

#[cfg(any(test, all(not(debug_assertions), not(patina_local_build))))]
#[cfg_attr(debug_assertions, allow(dead_code))]
pub fn should_use_local_build_context() -> bool {
    match std::env::current_exe() {
        Ok(path) => is_workspace_target_binary(&path),
        Err(_) => false,
    }
}

#[cfg(any(test, all(not(debug_assertions), not(patina_local_build))))]
fn is_workspace_target_binary(path: &Path) -> bool {
    let components = path
        .to_string_lossy()
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .map(|component| component.to_ascii_lowercase())
        .collect::<Vec<_>>();

    components.windows(3).any(|window| {
        window == ["src-tauri", "target", "release"] || window == ["src-tauri", "target", "debug"]
    })
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub fn setup(
    app: &mut tauri::App,
    runtime_health: Arc<RuntimeHealthState>,
    launched_by_autostart: bool,
) -> tauri::Result<()> {
    tauri::async_runtime::block_on(crate::engine::remote_status_bridge::ensure_machine_id(
        &app.handle().clone(),
    ))
    .map_err(std::io::Error::other)?;
    power::start(app.handle().clone());
    audio::start_signal_source(load_audio_participation_enabled(app.handle().clone()));
    media::start_signal_source();
    crate::app::web_activity_bridge::start(app.handle().clone());
    crate::engine::remote_status_bridge::start(app.handle().clone());
    crate::app::web_activity::spawn_startup_repair(app.handle().clone());

    // Start HTTP API server for AI agent integration
    let api_server = crate::engine::api::server::ApiServerState::new();
    let local_api_settings = load_local_api_settings(app.handle().clone());
    if let Err(error) =
        tauri::async_runtime::block_on(crate::engine::api::configuration::initialize(
            &app.handle().clone(),
            &api_server,
            local_api_settings,
        ))
    {
        eprintln!("[api] failed to initialize local API: {error}");
    }
    app.manage(api_server);

    let app_handle = app.handle().clone();
    main_window::ensure_main_window_with_initial_visibility(&app_handle, !launched_by_autostart)
        .map_err(std::io::Error::other)?;
    setup_tray(&app_handle)?;
    let desktop_behavior = app_handle.state::<DesktopBehaviorState>().snapshot();
    apply_tray_visibility(&app_handle, desktop_behavior);

    if launched_by_autostart {
        if let Some(window) = app_handle.get_webview_window(MAIN_WINDOW_LABEL) {
            let _ = window.hide();
        }
    }

    desktop_behavior::spawn_sync_from_storage(app.handle().clone(), launched_by_autostart);
    runtime_tasks::spawn_updater_startup_auto_check(app.handle().clone());
    runtime_tasks::spawn_tracking_runtime_restart_loop(
        app.handle().clone(),
        runtime_health.clone(),
    );
    runtime_tasks::spawn_tracking_watchdog_restart_loop(app.handle().clone(), runtime_health);
    runtime_tasks::spawn_tools_runtime_restart_loop(app.handle().clone());

    Ok(())
}

fn load_local_api_settings(app: tauri::AppHandle) -> crate::domain::settings::LocalApiSettings {
    tauri::async_runtime::block_on(async move {
        let pool = wait_for_sqlite_pool(&app).await?;
        crate::data::repositories::app_settings::load_local_api_settings(&pool)
            .await
            .map_err(|error| format!("failed to load local API setting: {error}"))
    })
    .unwrap_or_else(|error| {
        eprintln!("[api] failed to load local API setting: {error}");
        crate::domain::settings::LocalApiSettings::default()
    })
}

fn load_audio_participation_enabled(app: tauri::AppHandle) -> bool {
    tauri::async_runtime::block_on(async move {
        let pool = wait_for_sqlite_pool(&app).await?;
        crate::data::repositories::app_settings::load_audio_participation_enabled(&pool)
            .await
            .map_err(|error| format!("failed to load audio participation setting: {error}"))
    })
    .unwrap_or_else(|error| {
        eprintln!("[audio] failed to load audio participation setting: {error}");
        crate::domain::settings::DEFAULT_AUDIO_PARTICIPATION_ENABLED
    })
}

#[cfg(test)]
mod tests {
    use super::is_workspace_target_binary;
    use std::path::Path;

    #[test]
    fn detects_workspace_target_binary_on_windows_path() {
        assert!(is_workspace_target_binary(Path::new(
            r"C:\Users\SYBao\Documents\Code\Patina\src-tauri\target\release\patina.exe"
        )));
    }

    #[test]
    fn detects_workspace_target_binary_on_unix_path() {
        assert!(is_workspace_target_binary(Path::new(
            "/home/user/project/src-tauri/target/debug/patina"
        )));
    }

    #[test]
    fn ignores_installed_binary_path() {
        assert!(!is_workspace_target_binary(Path::new(
            r"C:\Users\SYBao\AppData\Local\Patina\patina.exe"
        )));
    }
}
