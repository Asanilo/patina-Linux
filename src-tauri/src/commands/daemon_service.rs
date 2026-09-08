use tauri::{AppHandle, Manager};

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
