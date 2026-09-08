mod app;
mod commands;
mod data;
mod domain;
mod engine;
mod platform;

use std::sync::Arc;

pub fn run_daemon(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<(), String> {
    app::daemon::run(args)
}

pub fn is_controlled_daemon_restart(error: &str) -> bool {
    app::daemon::is_controlled_restart_error(error)
}

pub const CONTROLLED_DAEMON_RESTART_EXIT_CODE: i32 = app::daemon::CONTROLLED_RESTART_EXIT_CODE;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(debug_assertions)]
    let context = tauri::generate_context!("tauri.dev.conf.json");
    #[cfg(all(not(debug_assertions), patina_local_build))]
    let context = tauri::generate_context!("tauri.local.conf.json");
    #[cfg(all(not(debug_assertions), not(patina_local_build)))]
    let context = if app::runtime::should_use_local_build_context() {
        tauri::generate_context!("tauri.local.conf.json")
    } else {
        tauri::generate_context!()
    };
    let runtime_health = Arc::new(engine::tracking::watchdog::RuntimeHealthState::default());
    let launched_by_autostart = app::runtime::was_launched_by_autostart();
    let profile = platform::app_paths::AppProfile::from_identifier(&context.config().identifier);
    let runtime_mode = app::runtime::desktop_runtime_mode(profile);
    let app_version = context.package_info().version.to_string();

    app::bootstrap::build(app::bootstrap::BootstrapInput {
        runtime_health,
        launched_by_autostart,
        runtime_mode,
        app_version,
    })
    .build(context)
    .expect("error while building tauri application")
    .run(app::bootstrap::handle_run_event);
}
