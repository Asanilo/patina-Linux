//! Opt-in native lifecycle regression. No tracker, service or production database.
use super::{background_resource_reclaimer, desktop_behavior, main_window, state::*, widget};
use crate::data::sqlite_pool::{open_prepared_sqlite_pool_at_path, SQLITE_DB_NAME};
use std::path::PathBuf;
use std::sync::{atomic::Ordering, Arc, Mutex};
use std::time::Duration;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

const IDLE_WAIT: Duration = Duration::from_secs(310);

fn isolated_root() -> PathBuf {
    let root =
        PathBuf::from(std::env::var_os("PATINA_NATIVE_TEST_ROOT").expect("use native test runner"));
    assert_eq!(root.parent(), Some(std::path::Path::new("/tmp")));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("patina-window-test-"));
    assert_eq!(std::fs::canonicalize(&root).unwrap(), root);
    for (key, child) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ] {
        assert_eq!(
            std::env::var_os(key).map(PathBuf::from),
            Some(root.join(child))
        );
    }
    assert_eq!(
        std::fs::read_to_string(root.join("marker")).unwrap(),
        "native-window-lifecycle\n"
    );
    root
}

async fn wait_window(app: &tauri::AppHandle, label: &str, present: bool) {
    for _ in 0..100 {
        if app.get_webview_window(label).is_some() == present {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("window {label} presence did not become {present}");
}

fn make_main(app: &tauri::AppHandle) {
    WebviewWindowBuilder::new(
        app,
        main_window::MAIN_WINDOW_LABEL,
        WebviewUrl::External("about:blank".parse().unwrap()),
    )
    .title("Patina isolated lifecycle test")
    .inner_size(500.0, 300.0)
    .data_directory(isolated_root().join("webview-main"))
    .build()
    .unwrap();
    main_window::show_main_window(app);
}

#[test]
#[ignore = "real Wayland windows and two five-minute waits; use scripts/native-window-lifecycle.mjs"]
fn native_window_lifecycle() {
    run_native_window_test(false, false);
}

#[test]
#[ignore = "real Wayland timers; use scripts/native-window-lifecycle.mjs --background-delay"]
fn native_background_delay() {
    run_native_window_test(true, false);
}

#[test]
#[ignore = "requires the isolated native window runner and a real display"]
fn native_widget_autostart() {
    run_native_window_test(false, true);
}

async fn apply_background_policy(app: &tauri::AppHandle, enabled: bool, minutes: u32) {
    crate::commands::settings::cmd_set_background_optimization(enabled, Some(minutes), app.clone())
        .unwrap();
    for _ in 0..100 {
        let policy = app.state::<DesktopBehaviorState>().snapshot();
        if policy.background_optimization == enabled
            && policy.background_optimization_delay_minutes == minutes
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("background policy was not applied on the UI thread");
}

fn hide_main(app: &tauri::AppHandle) {
    let window = app.get_webview_window("main").unwrap();
    main_window::hide_main_window_for_background(app, &window.as_ref().window());
}

async fn verify_background_delay(app: &tauri::AppHandle) {
    make_main(app);
    apply_background_policy(app, true, 1).await;
    hide_main(app);
    tokio::time::sleep(Duration::from_secs(20)).await;
    main_window::show_main_window(app);
    tokio::time::sleep(Duration::from_secs(45)).await;
    assert!(app
        .get_webview_window("main")
        .unwrap()
        .is_visible()
        .unwrap());
    eprintln!("NATIVE delay-reopen-cancelled-one-minute-timer");

    hide_main(app);
    tokio::time::sleep(Duration::from_secs(2)).await;
    apply_background_policy(app, false, 1).await;
    tokio::time::sleep(Duration::from_secs(65)).await;
    assert!(!app
        .get_webview_window("main")
        .unwrap()
        .is_visible()
        .unwrap());
    eprintln!("NATIVE delay-disabled-preserved-hidden-main");

    apply_background_policy(app, true, 1).await;
    tokio::time::sleep(Duration::from_secs(20)).await;
    apply_background_policy(app, true, 2).await;
    tokio::time::sleep(Duration::from_secs(45)).await;
    assert!(!app
        .get_webview_window("main")
        .unwrap()
        .is_visible()
        .unwrap());
    assert_eq!(
        app.state::<background_resource_reclaimer::BackgroundResourceReclaimerState>()
            .reclaim_attempt_count(),
        0
    );
    eprintln!("NATIVE delay-longer-policy-invalidated-old-timer");

    apply_background_policy(app, true, 1).await;
    tokio::time::sleep(Duration::from_secs(65)).await;
    wait_window(app, "main", false).await;
    assert_eq!(
        app.state::<background_resource_reclaimer::BackgroundResourceReclaimerState>()
            .reclaim_attempt_count(),
        1
    );
    eprintln!("NATIVE delay-one-minute-destroyed-and-reclaimed");
    make_main(app);
    assert!(app
        .get_webview_window("main")
        .unwrap()
        .is_visible()
        .unwrap());
    eprintln!("NATIVE delay-main-recreated-after-reclaim");
    app.get_webview_window("main").unwrap().destroy().unwrap();
    wait_window(app, "main", false).await;
}

fn run_native_window_test(delay_test: bool, autostart_only: bool) {
    let root = isolated_root();
    let backend = std::env::var("GDK_BACKEND").unwrap();
    assert!(backend == "wayland" || (autostart_only && backend == "x11"));
    let pool = tauri::async_runtime::block_on(open_prepared_sqlite_pool_at_path(
        &root.join("fixture.db"),
        true,
    ))
    .unwrap();
    let instances = tauri_plugin_sql::DbInstances::default();
    tauri::async_runtime::block_on(async {
        instances.0.write().await.insert(
            SQLITE_DB_NAME.into(),
            tauri_plugin_sql::DbPool::Sqlite(pool.clone()),
        );
    });
    let behavior = DesktopBehaviorState::default();
    behavior.update_desktop_from_raw("tray", "widget");
    behavior.update_background_optimization(true);
    let result = Arc::new(Mutex::new(None));
    let outcome = result.clone();
    let mut context = tauri::generate_context!("tauri.local.conf.json");
    // Serve inert HTML for the widget; this tests native lifecycle, not React.
    context.config_mut().build.dev_url = Some(
        std::env::var("PATINA_NATIVE_TEST_URL")
            .unwrap()
            .parse()
            .unwrap(),
    );
    context.config_mut().app.windows.clear();
    let app = tauri::Builder::default()
        .any_thread()
        .manage(behavior)
        .manage(background_resource_reclaimer::BackgroundResourceReclaimerState::default())
        .manage(MainWindowLifecycleState::default())
        .manage(WidgetWindowLifecycleState::default())
        .manage(AppExitState::default())
        .manage(instances)
        .setup(move |app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let worker_app = handle.clone();
                let worker = tauri::async_runtime::spawn(async move {
                    if autostart_only {
                        // No main window exists: every creation takes the
                        // app-level monitor fallback from an async worker.
                        for _ in 0..20 {
                            widget::show_widget_window(&worker_app, None).await.unwrap();
                            let window = worker_app.get_webview_window("widget").unwrap();
                            assert!(window.is_visible().unwrap());
                            assert!(worker_app.get_webview_window("main").is_none());
                            widget::close_widget_window(&worker_app);
                            window.destroy().unwrap();
                            wait_window(&worker_app, "widget", false).await;
                        }
                        crate::data::repositories::backup_restore::test_support::assert_integrity(&pool).await;
                        pool.close().await;
                        eprintln!("NATIVE widget-autostart-without-main-20-cycles");
                        return;
                    }
                    if delay_test {
                        verify_background_delay(&worker_app).await;
                        crate::data::repositories::backup_restore::test_support::assert_integrity(&pool).await;
                        assert!(crate::data::repositories::backup_restore::test_support::session_names(&pool).await.is_empty());
                        pool.close().await;
                        eprintln!("NATIVE delay-fixture-unchanged");
                        return;
                    }
                    desktop_behavior::sync_desktop_behavior_from_storage(
                        worker_app.clone(),
                        true,
                    )
                    .await
                    .unwrap();
                    wait_window(&worker_app, "widget", true).await;
                    assert!(worker_app.get_webview_window("main").is_none());
                    assert!(worker_app
                        .get_webview_window("widget")
                        .unwrap()
                        .is_visible()
                        .unwrap());
                    worker_app
                        .state::<DesktopBehaviorState>()
                        .update_background_optimization(true);
                    make_main(&worker_app);
                    worker_app
                        .get_webview_window("widget")
                        .unwrap()
                        .destroy()
                        .unwrap();
                    wait_window(&worker_app, "widget", false).await;
                    eprintln!("NATIVE autostart-widget-without-main");

                    tokio::time::sleep(Duration::from_secs(2)).await;
                    widget::CANCEL_NEXT_CREATION.store(true, Ordering::SeqCst);
                    main_window::minimize_main_window(&worker_app);
                    wait_window(&worker_app, "widget", true).await;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    assert!(!widget::CANCEL_NEXT_CREATION.load(Ordering::SeqCst));
                    assert!(!worker_app
                        .get_webview_window("widget")
                        .unwrap()
                        .is_visible()
                        .unwrap());
                    // Reopen before the original main destroy timer expires.
                    // Do not call show_main_window/Focus handlers here: those
                    // close the widget again and would mask its missing timer.
                    worker_app.state::<MainWindowLifecycleState>().show();
                    worker_app
                        .get_webview_window("main")
                        .unwrap()
                        .show()
                        .unwrap();
                    eprintln!("NATIVE phase=cancel-and-reopen waiting=310s");
                    tokio::time::sleep(IDLE_WAIT).await;
                    assert!(worker_app
                        .get_webview_window("main")
                        .unwrap()
                        .is_visible()
                        .unwrap());
                    wait_window(&worker_app, "widget", false).await;
                    eprintln!("NATIVE cancelled-widget-gone stale-main-timer-cancelled");

                    main_window::minimize_main_window(&worker_app);
                    wait_window(&worker_app, "widget", true).await;
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    assert!(worker_app
                        .get_webview_window("widget")
                        .unwrap()
                        .is_visible()
                        .unwrap());
                    eprintln!("NATIVE phase=minimize-to-widget waiting=310s");
                    tokio::time::sleep(IDLE_WAIT).await;
                    wait_window(&worker_app, "main", false).await;
                    assert!(worker_app
                        .get_webview_window("widget")
                        .unwrap()
                        .is_visible()
                        .unwrap());
                    eprintln!("NATIVE main-destroyed widget-survived");
                    make_main(&worker_app);
                    assert!(worker_app
                        .get_webview_window("main")
                        .unwrap()
                        .is_visible()
                        .unwrap());
                    crate::data::repositories::backup_restore::test_support::assert_integrity(
                        &pool,
                    )
                    .await;
                    assert!(
                        crate::data::repositories::backup_restore::test_support::session_names(
                            &pool
                        )
                        .await
                        .is_empty()
                    );
                    worker_app
                        .get_webview_window("widget")
                        .unwrap()
                        .destroy()
                        .unwrap();
                    wait_window(&worker_app, "widget", false).await;
                    worker_app
                        .get_webview_window("main")
                        .unwrap()
                        .destroy()
                        .unwrap();
                    wait_window(&worker_app, "main", false).await;
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    assert_eq!(
                        worker_app
                            .state::<background_resource_reclaimer::BackgroundResourceReclaimerState>()
                            .reclaim_attempt_count(),
                        1
                    );
                    eprintln!("NATIVE last-webview-reclaim-once");
                    pool.close().await;
                    eprintln!("NATIVE reopened fixture-unchanged");
                });
                let passed = worker.await.is_ok();
                *outcome.lock().unwrap() = Some(passed);
                handle.state::<AppExitState>().request_exit();
                handle.exit(if passed { 0 } else { 1 });
            });
            Ok(())
        })
        .build(context)
        .unwrap();
    let code = app.run_return(|app, event| {
        if matches!(
            event,
            tauri::RunEvent::WindowEvent {
                event: tauri::WindowEvent::Destroyed,
                ..
            }
        ) {
            background_resource_reclaimer::schedule_after_webview_destroyed(app.clone());
        }
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !app.state::<AppExitState>().is_exit_requested() {
                api.prevent_exit();
            }
        }
    });
    assert_eq!(code, 0);
    assert_eq!(*result.lock().unwrap(), Some(true));
}
