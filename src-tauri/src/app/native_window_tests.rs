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
    let root = isolated_root();
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "wayland");
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
