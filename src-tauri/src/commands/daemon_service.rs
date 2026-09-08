use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub async fn cmd_retry_runtime_owner_cutover(
    confirmed: bool,
    app: AppHandle,
) -> Result<(), String> {
    if !confirmed {
        return Err("runtime owner cutover retry requires confirmation".to_string());
    }

    #[cfg(target_os = "linux")]
    {
        let mutation_state = app.state::<crate::app::daemon_service::DaemonServiceMutationState>();
        let _mutation_guard = mutation_state.lock().await;
        let runtime_mode = *app.state::<crate::app::runtime::DesktopRuntimeMode>();
        if !runtime_mode.is_managed_daemon_client() {
            return Err(
                "runtime owner cutover retry requires managed daemon client mode".to_string(),
            );
        }
        let profile = crate::platform::app_paths::app_profile(&app);
        let control_root =
            crate::platform::storage_paths::default_storage_paths(&app)?.control_root;
        let pool = crate::data::sqlite_pool::wait_for_sqlite_pool(&app).await?;
        let settings =
            crate::data::repositories::app_settings::load_desktop_behavior_settings(&pool)
                .await
                .map_err(|error| format!("failed to load desktop behavior settings: {error}"))?;
        crate::app::daemon_service::prepare_explicit_runtime_owner_retry(
            profile,
            &control_root,
            settings,
        )
        .await?;
        app.state::<crate::app::state::AppExitState>()
            .request_exit();
        app.restart();
    }

    #[cfg(not(target_os = "linux"))]
    Err("patinad service recovery is only available on Linux".to_string())
}

#[tauri::command]
pub async fn cmd_set_background_tracking_at_login(
    enabled: bool,
    app: AppHandle,
) -> Result<crate::app::daemon_service::DaemonServiceDiagnosticsSnapshot, String> {
    #[cfg(target_os = "linux")]
    {
        let mutation_state = app.state::<crate::app::daemon_service::DaemonServiceMutationState>();
        let _mutation_guard = mutation_state.lock().await;
        let runtime_mode = *app.state::<crate::app::runtime::DesktopRuntimeMode>();
        if !runtime_mode.is_managed_daemon_client() {
            return Err(
                "background tracking login preference requires managed daemon client mode"
                    .to_string(),
            );
        }
        let profile = crate::platform::app_paths::app_profile(&app);
        let control_root =
            crate::platform::storage_paths::default_storage_paths(&app)?.control_root;
        crate::app::daemon_service::set_background_tracking_login_preference(
            profile,
            &control_root,
            enabled,
        )
        .await?;
        let pool = crate::data::sqlite_pool::wait_for_sqlite_pool(&app).await?;
        crate::data::repositories::app_settings::save_background_tracking_login_preference(
            &pool, enabled,
        )
        .await?;
        let settings = app
            .state::<crate::app::state::DesktopBehaviorState>()
            .update_background_tracking_at_login(enabled);
        app.emit("app-settings-changed", serde_json::json!({}))
            .map_err(|error| format!("failed to emit settings refresh event: {error}"))?;
        let autostart = crate::app::autostart::inspect_autostart_desktop_file();
        return Ok(crate::app::daemon_service::inspect(
            profile,
            &control_root,
            settings.background_tracking_at_login,
            settings.launch_at_login,
            autostart.valid(),
            false,
        )
        .await);
    }

    #[cfg(not(target_os = "linux"))]
    Err("patinad service settings are only available on Linux".to_string())
}

#[tauri::command]
pub async fn cmd_rollback_runtime_owner_to_embedded(
    confirmed: bool,
    app: AppHandle,
) -> Result<(), String> {
    if !confirmed {
        return Err("runtime owner rollback requires confirmation".to_string());
    }

    #[cfg(target_os = "linux")]
    {
        let mutation_state = app.state::<crate::app::daemon_service::DaemonServiceMutationState>();
        let _mutation_guard = mutation_state.lock().await;
        let runtime_mode = *app.state::<crate::app::runtime::DesktopRuntimeMode>();
        if !runtime_mode.is_managed_daemon_client() {
            return Err("runtime owner rollback requires managed daemon client mode".to_string());
        }
        let profile = crate::platform::app_paths::app_profile(&app);
        let control_root =
            crate::platform::storage_paths::default_storage_paths(&app)?.control_root;
        let pool = crate::data::sqlite_pool::wait_for_sqlite_pool(&app).await?;
        let settings =
            crate::data::repositories::app_settings::load_desktop_behavior_settings(&pool)
                .await
                .map_err(|error| format!("failed to load desktop behavior settings: {error}"))?;
        crate::app::daemon_service::prepare_explicit_runtime_owner_rollback(
            profile,
            &control_root,
            settings,
            &pool,
        )
        .await?;
        app.state::<crate::app::state::DesktopBehaviorState>()
            .update_background_tracking_at_login(false);
        app.state::<crate::app::state::AppExitState>()
            .request_exit();
        app.restart();
    }

    #[cfg(not(target_os = "linux"))]
    Err("patinad runtime owner rollback is only available on Linux".to_string())
}
