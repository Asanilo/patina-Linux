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
pub const DAEMON_CLIENT_PREVIEW_ARG: &str = "--daemon-client-preview";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DesktopRuntimeMode {
    #[default]
    Embedded,
    DaemonClientPreview,
    DaemonClientManaged,
}

impl DesktopRuntimeMode {
    pub fn from_args(args: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        if args
            .into_iter()
            .any(|arg| arg.as_ref() == DAEMON_CLIENT_PREVIEW_ARG)
        {
            Self::DaemonClientPreview
        } else {
            Self::Embedded
        }
    }

    pub const fn owns_embedded_runtime(self) -> bool {
        matches!(self, Self::Embedded)
    }

    pub const fn is_managed_daemon_client(self) -> bool {
        matches!(self, Self::DaemonClientManaged)
    }
}

pub fn was_launched_by_autostart() -> bool {
    std::env::args().any(|arg| arg == AUTOSTART_ARG)
}

pub fn desktop_runtime_mode(profile: crate::platform::app_paths::AppProfile) -> DesktopRuntimeMode {
    let explicit = DesktopRuntimeMode::from_args(std::env::args());
    if explicit == DesktopRuntimeMode::DaemonClientPreview {
        return explicit;
    }

    #[cfg(target_os = "linux")]
    {
        let roots = crate::platform::app_paths::environment_roots();
        let control_root =
            crate::platform::storage_paths::default_storage_paths_for_profile(&roots, profile)
                .control_root;
        let decision =
            crate::app::runtime_owner_cutover::decide_desktop_startup(&control_root, profile);
        select_desktop_runtime_mode(explicit, profile, !decision.owns_embedded_runtime())
    }

    #[cfg(not(target_os = "linux"))]
    explicit
}

fn select_desktop_runtime_mode(
    explicit: DesktopRuntimeMode,
    profile: crate::platform::app_paths::AppProfile,
    cutover_requires_client: bool,
) -> DesktopRuntimeMode {
    if explicit == DesktopRuntimeMode::DaemonClientPreview {
        explicit
    } else if profile == crate::platform::app_paths::AppProfile::Production
        && cutover_requires_client
    {
        DesktopRuntimeMode::DaemonClientManaged
    } else {
        explicit
    }
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
    runtime_mode: DesktopRuntimeMode,
) -> tauri::Result<()> {
    match runtime_mode {
        DesktopRuntimeMode::Embedded => setup_embedded_runtime(app, runtime_health.clone())?,
        DesktopRuntimeMode::DaemonClientPreview | DesktopRuntimeMode::DaemonClientManaged => {
            setup_daemon_client_runtime(app, runtime_health.clone())?
        }
    }

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

    Ok(())
}

fn setup_embedded_runtime(
    app: &mut tauri::App,
    runtime_health: Arc<RuntimeHealthState>,
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
    crate::app::web_activity::spawn_stale_watchdog(app.handle().clone());

    let api_token_path = crate::platform::storage_paths::resolve_storage_paths(app.handle())
        .map_err(std::io::Error::other)?
        .api_token_path;
    let local_api_settings = load_local_api_settings(app.handle().clone());
    let api_server = app.state::<crate::engine::api::server::ApiServerState>();
    let api_credentials = app.state::<crate::engine::api::auth::ApiCredentialStore>();
    if let Err(error) =
        tauri::async_runtime::block_on(crate::engine::api::configuration::initialize(
            &app.handle().clone(),
            &api_server,
            &api_credentials,
            &api_token_path,
            local_api_settings,
        ))
    {
        eprintln!("[api] failed to initialize local API: {error}");
    }

    runtime_tasks::spawn_tracking_runtime_restart_loop(
        app.handle().clone(),
        runtime_health.clone(),
    );
    runtime_tasks::spawn_tracking_watchdog_restart_loop(app.handle().clone(), runtime_health);
    runtime_tasks::spawn_tools_runtime_restart_loop(app.handle().clone());
    runtime_tasks::spawn_scheduled_backup_runtime_restart_loop(app.handle().clone());
    Ok(())
}

fn setup_daemon_client_runtime(
    app: &mut tauri::App,
    runtime_health: Arc<RuntimeHealthState>,
) -> tauri::Result<()> {
    let api_token_path = crate::platform::storage_paths::resolve_storage_paths(app.handle())
        .map_err(std::io::Error::other)?
        .api_token_path;
    let local_api_settings = load_local_api_settings(app.handle().clone());
    let api_credentials = app.state::<crate::engine::api::auth::ApiCredentialStore>();
    let token = match api_credentials.load_existing_at(&api_token_path) {
        Ok(token) => token,
        Err(error) => {
            report_daemon_client_configuration_error(app, error);
            return Ok(());
        }
    };
    let client =
        match crate::platform::daemon_client::PatinadClient::new(local_api_settings.port, token) {
            Ok(client) => client,
            Err(error) => {
                report_daemon_client_error(app, error);
                return Ok(());
            }
        };
    let client_state = app
        .state::<crate::app::daemon_client::PatinadClientState>()
        .inner()
        .clone();
    client_state.install(client);
    let handle = crate::app::daemon_client::runtime::PatinadDesktopRuntimeHandle::start(
        app.handle().clone(),
        client_state,
        runtime_health,
    );
    app.manage(handle);
    Ok(())
}

fn report_daemon_client_configuration_error(app: &tauri::App, message: String) {
    report_daemon_client_error(
        app,
        crate::platform::daemon_client::PatinadClientError::InvalidConfiguration(message),
    );
}

fn report_daemon_client_error(
    app: &tauri::App,
    error: crate::platform::daemon_client::PatinadClientError,
) {
    eprintln!("[patinad-client] preview unavailable: {error}");
    app.state::<crate::app::daemon_client::runtime::PatinadRuntimeState>()
        .report_connection_error(&error);
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
    use super::{
        is_workspace_target_binary, select_desktop_runtime_mode, DesktopRuntimeMode,
        DAEMON_CLIENT_PREVIEW_ARG,
    };
    use crate::platform::app_paths::AppProfile;
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

    #[test]
    fn daemon_client_preview_requires_an_explicit_argument() {
        assert_eq!(
            DesktopRuntimeMode::from_args(["patina"]),
            DesktopRuntimeMode::Embedded
        );
        assert_eq!(
            DesktopRuntimeMode::from_args(["patina", DAEMON_CLIENT_PREVIEW_ARG]),
            DesktopRuntimeMode::DaemonClientPreview
        );
        assert!(!DesktopRuntimeMode::DaemonClientPreview.owns_embedded_runtime());
        assert!(!DesktopRuntimeMode::DaemonClientManaged.owns_embedded_runtime());
        assert!(DesktopRuntimeMode::DaemonClientManaged.is_managed_daemon_client());
    }

    #[test]
    fn only_production_uses_a_durable_cutover_to_select_the_managed_client() {
        assert_eq!(
            select_desktop_runtime_mode(DesktopRuntimeMode::Embedded, AppProfile::Production, true,),
            DesktopRuntimeMode::DaemonClientManaged
        );
        assert_eq!(
            select_desktop_runtime_mode(DesktopRuntimeMode::Embedded, AppProfile::Local, true),
            DesktopRuntimeMode::Embedded
        );
        assert_eq!(
            select_desktop_runtime_mode(DesktopRuntimeMode::Embedded, AppProfile::Dev, true),
            DesktopRuntimeMode::Embedded
        );
        assert_eq!(
            select_desktop_runtime_mode(
                DesktopRuntimeMode::DaemonClientPreview,
                AppProfile::Production,
                false,
            ),
            DesktopRuntimeMode::DaemonClientPreview
        );
    }
}
