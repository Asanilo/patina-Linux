use crate::app::autostart;
use crate::app::main_window;
use crate::app::state::DesktopBehaviorState;
use crate::app::tray::{apply_tray_visibility, show_main_window};
use crate::app::widget;
use crate::data::repositories::{app_settings, update_state};
use crate::data::sqlite_pool::wait_for_sqlite_pool;
use crate::domain::settings::MinimizeBehavior;
use tauri::{AppHandle, Manager, Runtime};
#[cfg(not(target_os = "linux"))]
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InitialWindowPlan {
    MainVisible,
    MainTaskbarMinimized,
    WidgetOnly,
}

fn resolve_initial_window_plan(
    launched_by_autostart: bool,
    should_reopen_main_window: bool,
    settings: crate::domain::settings::DesktopBehaviorSettings,
) -> InitialWindowPlan {
    if should_reopen_main_window
        || !launched_by_autostart
        || !settings.should_start_minimized_on_autostart()
    {
        return InitialWindowPlan::MainVisible;
    }

    match settings.minimize_behavior {
        MinimizeBehavior::Taskbar => InitialWindowPlan::MainTaskbarMinimized,
        MinimizeBehavior::Widget => InitialWindowPlan::WidgetOnly,
    }
}

pub(crate) fn apply_autostart<R: Runtime>(
    app: &AppHandle<R>,
    launch_at_login: bool,
) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let _ = app;
        autostart::apply_linux_autostart(launch_at_login)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let autostart_manager = app.autolaunch();

        if launch_at_login {
            #[cfg(all(debug_assertions, target_os = "windows"))]
            {
                let executable_path = std::env::current_exe()
                    .ok()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                return Err(format!(
                "autostart enable blocked in debug build on Windows to avoid registering a debug executable path ({executable_path}). Please enable launch-at-login from the installed release build."
            ));
            }

            #[cfg(not(all(debug_assertions, target_os = "windows")))]
            autostart_manager
                .enable()
                .map_err(|error| format!("failed to enable autostart: {error}"))?;
        } else {
            autostart_manager
                .disable()
                .map_err(|error| format!("failed to disable autostart: {error}"))?;
        }

        Ok(())
    }
}

pub(crate) fn set_desktop_behavior<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopBehaviorState,
    close_behavior: &str,
    minimize_behavior: &str,
) {
    let next = state.update_desktop_from_raw(close_behavior, minimize_behavior);
    apply_tray_visibility(app, next);
}

pub(crate) fn set_launch_behavior<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopBehaviorState,
    launch_at_login: bool,
    start_minimized: bool,
) -> Result<(), String> {
    let next = state.update_launch(launch_at_login, start_minimized);
    apply_autostart(app, next.launch_at_login)?;
    Ok(())
}

pub(crate) fn set_background_optimization(
    state: &DesktopBehaviorState,
    background_optimization: bool,
) {
    let _ = state.update_background_optimization(background_optimization);
}

pub(crate) async fn sync_desktop_behavior_from_storage<R: Runtime>(
    app: AppHandle<R>,
    launched_by_autostart: bool,
) -> Result<(), String> {
    let pool = wait_for_sqlite_pool(&app).await?;
    let loaded = app_settings::load_desktop_behavior_settings(&pool)
        .await
        .map_err(|error| format!("failed to load desktop behavior settings: {error}"))?;
    let should_reopen_main_window = update_state::take_post_install_reopen_main_window(&pool)
        .await
        .map_err(|error| format!("failed to load post-install reopen intent: {error}"))?;

    let state = app.state::<DesktopBehaviorState>();
    let next = state.replace(loaded);

    if let Err(error) = apply_autostart(&app, next.launch_at_login) {
        eprintln!("[tray] failed to apply autostart setting: {error}");
    }
    apply_tray_visibility(&app, next);

    match resolve_initial_window_plan(launched_by_autostart, should_reopen_main_window, next) {
        InitialWindowPlan::MainVisible => show_main_window(&app),
        InitialWindowPlan::MainTaskbarMinimized => {
            show_main_window(&app);
            main_window::minimize_main_window(&app);
        }
        InitialWindowPlan::WidgetOnly => {
            // setup() waits for settings on the GTK thread. First-window mapping
            // needs that thread's event loop, so build the Widget after it resumes.
            tauri::async_runtime::spawn(async move {
                if let Err(error) = widget::show_widget_window(&app, None).await {
                    eprintln!("[widget] failed to show startup widget window: {error}");
                }
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{resolve_initial_window_plan, InitialWindowPlan};
    use crate::domain::settings::{DesktopBehaviorSettings, MinimizeBehavior};

    #[test]
    fn direct_launch_and_update_reopen_always_show_main_window() {
        let minimized = DesktopBehaviorSettings::default();

        assert_eq!(
            resolve_initial_window_plan(false, false, minimized),
            InitialWindowPlan::MainVisible
        );
        assert_eq!(
            resolve_initial_window_plan(true, true, minimized),
            InitialWindowPlan::MainVisible
        );
    }

    #[test]
    fn autostart_uses_the_configured_minimize_surface_without_hidden_main() {
        let widget = DesktopBehaviorSettings::default();
        assert_eq!(
            resolve_initial_window_plan(true, false, widget),
            InitialWindowPlan::WidgetOnly
        );

        let taskbar =
            widget.with_desktop_behavior(widget.close_behavior, MinimizeBehavior::Taskbar);
        assert_eq!(
            resolve_initial_window_plan(true, false, taskbar),
            InitialWindowPlan::MainTaskbarMinimized
        );
    }

    #[test]
    fn autostart_without_start_minimized_shows_main_window() {
        let settings = DesktopBehaviorSettings::default().with_launch_behavior(true, false);

        assert_eq!(
            resolve_initial_window_plan(true, false, settings),
            InitialWindowPlan::MainVisible
        );
    }
}
