use std::sync::Arc;

use crate::app::{
    runtime,
    state::{
        AppExitState, DesktopBehaviorState, MainWindowLifecycleState, WidgetWindowLifecycleState,
    },
    tray,
};
use crate::engine::{
    tools::ToolsRuntimeState,
    tracking::{runtime_snapshot::TrackingRuntimeSnapshotState, watchdog::RuntimeHealthState},
    updater::UpdaterRuntimeState,
    web_activity::WebActivityRuntimeState,
};
use crate::{commands, data};
use tauri::Manager;

pub struct BootstrapInput {
    pub runtime_health: Arc<RuntimeHealthState>,
    pub launched_by_autostart: bool,
    pub runtime_mode: runtime::DesktopRuntimeMode,
    pub app_version: String,
}

pub fn build(input: BootstrapInput) -> tauri::Builder<tauri::Wry> {
    let builder = register_single_instance_plugin(tauri::Builder::<tauri::Wry>::default());
    let builder = register_managed_state_and_plugins(
        builder,
        &input.app_version,
        input.runtime_health.clone(),
        input.runtime_mode,
    );
    let builder = register_invoke_handlers(builder);
    register_runtime_hooks(
        builder,
        input.runtime_health,
        input.launched_by_autostart,
        input.runtime_mode,
    )
}

fn register_single_instance_plugin(
    builder: tauri::Builder<tauri::Wry>,
) -> tauri::Builder<tauri::Wry> {
    #[cfg(all(desktop, not(debug_assertions)))]
    {
        return builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }));
    }

    #[cfg(any(not(desktop), debug_assertions))]
    builder
}

fn register_managed_state_and_plugins(
    builder: tauri::Builder<tauri::Wry>,
    app_version: &str,
    runtime_health: Arc<RuntimeHealthState>,
    runtime_mode: runtime::DesktopRuntimeMode,
) -> tauri::Builder<tauri::Wry> {
    builder
        .manage(DesktopBehaviorState::default())
        .manage(AppExitState::default())
        .manage(MainWindowLifecycleState::default())
        .manage(WidgetWindowLifecycleState::default())
        .manage(TrackingRuntimeSnapshotState::default())
        .manage(runtime_health)
        .manage(runtime_mode)
        .manage(crate::app::daemon_client::PatinadClientState::default())
        .manage(crate::app::daemon_client::runtime::PatinadRuntimeState::default())
        .manage(crate::engine::api::auth::ApiCredentialStore::new())
        .manage(crate::engine::api::server::ApiServerState::new())
        .manage(ToolsRuntimeState::default())
        .manage(crate::app::scheduled_backup::ScheduledBackupRuntimeState::default())
        .manage(crate::platform::web_activity_bridge::WebActivityBridgeRuntimeState::default())
        .manage(crate::engine::remote_status_bridge::RemoteStatusBridgeRuntimeState::default())
        .manage(WebActivityRuntimeState::default())
        .manage(UpdaterRuntimeState::new(app_version.to_string()))
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(vec![runtime::AUTOSTART_ARG.to_string()])
                .build(),
        )
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
}

fn register_invoke_handlers(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(tauri::generate_handler![
        commands::activity_import::cmd_pick_activity_import_file,
        commands::activity_import::cmd_preview_activity_import,
        commands::activity_import::cmd_commit_activity_import,
        commands::activity_import::cmd_list_activity_import_batches,
        commands::activity_import::cmd_delete_activity_import_batch,
        commands::apps::get_icon,
        commands::tracking::get_current_active_window,
        commands::tracking::get_current_tracking_snapshot,
        commands::tracking::cmd_get_tracker_health_snapshot,
        commands::tracking::cmd_set_afk_threshold,
        commands::settings::cmd_set_desktop_behavior,
        commands::settings::cmd_set_launch_behavior,
        commands::settings::cmd_set_background_optimization,
        commands::settings::cmd_set_audio_participation_enabled,
        commands::settings::cmd_apply_local_api_port,
        commands::settings::cmd_rotate_local_api_token,
        commands::settings::cmd_commit_app_settings,
        commands::settings::cmd_commit_classification_settings,
        commands::storage::cmd_get_storage_snapshot,
        commands::storage::cmd_pick_storage_parent,
        commands::storage::cmd_preview_storage_migration,
        commands::storage::cmd_preview_restore_default_storage,
        commands::storage::cmd_schedule_storage_migration,
        commands::storage::cmd_schedule_restore_default_storage,
        commands::storage::cmd_cancel_pending_storage_migration,
        commands::storage::cmd_schedule_webview_cache_clear,
        commands::storage::cmd_open_storage_directory,
        commands::storage::cmd_restart_for_storage_maintenance,
        commands::tools::cmd_get_tools_snapshot,
        commands::tools::cmd_get_tool_alerts,
        commands::tools::cmd_dismiss_tool_alert,
        commands::tools::cmd_create_reminder,
        commands::tools::cmd_cancel_reminder,
        commands::tools::cmd_create_software_reminder_rule,
        commands::tools::cmd_disable_software_reminder_rule,
        commands::tools::cmd_start_timer,
        commands::tools::cmd_pause_timer,
        commands::tools::cmd_resume_timer,
        commands::tools::cmd_reset_timer,
        commands::tools::cmd_add_timer_lap,
        commands::tools::cmd_start_pomodoro,
        commands::tools::cmd_pause_pomodoro,
        commands::tools::cmd_resume_pomodoro,
        commands::tools::cmd_skip_pomodoro_phase,
        commands::tools::cmd_reset_pomodoro,
        commands::widget::cmd_get_widget_icon_map,
        commands::widget::cmd_get_widget_icon,
        commands::widget::cmd_get_widget_placement,
        commands::widget::cmd_set_widget_placement,
        commands::widget::cmd_apply_widget_layout,
        commands::widget::cmd_set_widget_expanded,
        commands::widget::cmd_show_main_window,
        commands::widget::cmd_hide_widget_window,
        commands::widget::cmd_toggle_tracking_paused,
        commands::widget::cmd_show_widget_window,
        commands::widget::cmd_is_primary_mouse_button_down,
        commands::window::cmd_minimize_main_window,
        commands::update::cmd_get_update_snapshot,
        commands::update::cmd_check_for_updates,
        commands::update::cmd_download_update,
        commands::update::cmd_install_update,
        commands::web_activity::cmd_get_web_activity_bridge_snapshot,
        commands::backup::cmd_pick_backup_save_file,
        commands::backup::cmd_pick_backup_file,
        commands::backup::cmd_preview_backup,
        commands::backup::cmd_export_backup,
        commands::backup::cmd_restore_backup,
        commands::backup::cmd_save_webdav_backup_secret,
        commands::backup::cmd_delete_webdav_backup_secret,
        commands::backup::cmd_has_webdav_backup_secret,
        commands::backup::cmd_reveal_webdav_backup_secret,
        commands::backup::cmd_test_webdav_backup_target,
        commands::backup::cmd_upload_webdav_backup,
        commands::backup::cmd_list_webdav_backups,
        commands::backup::cmd_restore_webdav_backup,
        commands::backup::cmd_get_scheduled_backup_snapshot,
        commands::backup::cmd_pick_scheduled_backup_directory,
        commands::backup::cmd_save_scheduled_backup_config,
        commands::persistence::cmd_reopen_sqlite_pool,
        commands::persistence::cmd_delete_tracking_data_before,
        commands::persistence::cmd_clear_all_window_titles,
        commands::persistence::cmd_delete_app_tracking_data,
        commands::diagnostics::cmd_get_local_api_diagnostics,
        commands::diagnostics::cmd_get_local_api_settings,
        commands::diagnostics::cmd_get_desktop_integration_diagnostics,
        commands::diagnostics::cmd_get_daemon_service_diagnostics,
        commands::diagnostics::cmd_get_daemon_client_diagnostics,
        commands::diagnostics::cmd_repair_autostart_desktop_file,
        commands::diagnostics::cmd_get_resource_diagnostics
    ])
}

fn register_runtime_hooks(
    builder: tauri::Builder<tauri::Wry>,
    runtime_health: Arc<RuntimeHealthState>,
    launched_by_autostart: bool,
    runtime_mode: runtime::DesktopRuntimeMode,
) -> tauri::Builder<tauri::Wry> {
    builder
        .on_menu_event(tray::handle_menu_event)
        .on_tray_icon_event(tray::handle_tray_icon_event)
        .on_window_event(tray::handle_window_event)
        .setup(move |app| {
            if runtime_mode.owns_embedded_runtime() {
                let profile = crate::platform::app_paths::app_profile(app.handle());
                #[cfg(target_os = "linux")]
                tauri::async_runtime::block_on(
                    crate::app::daemon_service::stop_conflicting_service_before_embedded_startup(
                        profile,
                    ),
                )
                .map_err(std::io::Error::other)?;
                let control_root =
                    crate::platform::storage_paths::default_storage_paths(app.handle())?
                        .control_root;
                let runtime_lease = crate::app::runtime_lease::acquire_runtime_lease(
                    &control_root,
                    profile,
                    crate::app::runtime_lease::RuntimeRole::Desktop,
                )
                .map_err(|error| std::io::Error::other(error.to_string()))?;
                app.manage(runtime_lease);
                if let Err(error) = tauri::async_runtime::block_on(
                    data::storage_migration::run_startup_storage_maintenance(app.handle()),
                ) {
                    eprintln!("[storage] startup storage maintenance failed: {error}");
                    rfd::MessageDialog::new()
                        .set_level(rfd::MessageLevel::Error)
                        .set_title("Patina storage unavailable")
                        .set_description(format!(
                            "Patina could not open its configured storage. Restore the configured mount or directory, then start Patina again.\n\n{error}"
                        ))
                        .set_buttons(rfd::MessageButtons::Ok)
                        .show();
                    return Err(std::io::Error::other(error).into());
                }
                tauri::async_runtime::block_on(data::sqlite_pool::initialize_app_sqlite(
                    app.handle(),
                ))
                .map_err(std::io::Error::other)?;
            } else {
                tauri::async_runtime::block_on(
                    data::sqlite_pool::initialize_existing_app_sqlite(app.handle()),
                )
                .map_err(std::io::Error::other)?;
            }
            Ok(runtime::setup(
                app,
                runtime_health.clone(),
                launched_by_autostart,
                runtime_mode,
            )?)
        })
}

pub(crate) fn handle_run_event(app: &tauri::AppHandle, event: tauri::RunEvent) {
    if let tauri::RunEvent::ExitRequested { api, .. } = event {
        let exit_requested = app.state::<AppExitState>().is_exit_requested();
        let keep_tray_visible = app
            .state::<DesktopBehaviorState>()
            .snapshot()
            .should_keep_tray_visible();

        if keep_tray_visible && !exit_requested {
            api.prevent_exit();
        } else {
            if let Some(runtime) =
                app.try_state::<crate::app::daemon_client::runtime::PatinadDesktopRuntimeHandle>()
            {
                tauri::async_runtime::block_on(runtime.shutdown());
            }
            tauri::async_runtime::block_on(
                app.state::<crate::platform::web_activity_bridge::WebActivityBridgeRuntimeState>()
                    .shutdown(),
            );
            app.state::<crate::engine::api::server::ApiServerState>()
                .shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn startup_storage_maintenance_precedes_sqlite_initialization() {
        let source = include_str!("bootstrap.rs");
        let setup = source
            .split(".setup(move |app|")
            .nth(1)
            .expect("runtime setup hook");
        let lease = setup
            .find("acquire_runtime_lease")
            .expect("runtime lease acquisition");
        let storage = setup
            .find("run_startup_storage_maintenance")
            .expect("startup storage maintenance call");
        let sqlite = setup
            .find("initialize_app_sqlite")
            .expect("sqlite initialization call");

        assert!(lease < storage);
        assert!(storage < sqlite);
    }

    #[test]
    fn daemon_client_startup_does_not_enter_the_embedded_owner_branch() {
        let source = include_str!("bootstrap.rs");
        let setup = source
            .split(".setup(move |app|")
            .nth(1)
            .expect("runtime setup hook");
        let owner_branch = setup
            .find("if runtime_mode.owns_embedded_runtime()")
            .expect("explicit embedded owner branch");
        let existing_database = setup
            .find("initialize_existing_app_sqlite")
            .expect("daemon client database initialization");

        assert!(owner_branch < existing_database);
        assert!(setup.contains("initialize_app_sqlite"));
    }
}
