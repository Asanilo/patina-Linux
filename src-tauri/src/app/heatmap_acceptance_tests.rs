//! Opt-in real frontend/IPC/daemon acceptance, exclusively under private test roots.
use super::{bootstrap, main_window, runtime, state::AppExitState};
use crate::data::repositories::daily_activity::desktop_fixture;
use crate::engine::tracking::watchdog::RuntimeHealthState;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Manager;

fn root() -> PathBuf {
    let root = PathBuf::from(std::env::var_os("PATINA_HEATMAP_TEST_ROOT").unwrap());
    assert_eq!(root.parent(), Some(Path::new("/tmp")));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("patina-heatmap-test-"));
    assert_eq!(root.canonicalize().unwrap(), root);
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
        "heatmap-acceptance\n"
    );
    root
}

fn evidence(root: &Path, name: &str, data: serde_json::Value) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join(name))
        .unwrap();
    file.write_all(serde_json::to_string_pretty(&data).unwrap().as_bytes())
        .unwrap();
}

async fn wait_report(root: &Path, round: usize) {
    for _ in 0..1200 {
        if let Ok(text) = std::fs::read_to_string(root.join(format!("ui-{round}.json"))) {
            let report: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(report["passed"], true, "{report}");
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("real frontend round {round} did not finish");
}

#[test]
#[ignore = "real Wayland/React/daemon and a five-minute idle wait; use perf:heatmap-desktop"]
fn heatmap_desktop_worker() {
    let root = root();
    let mode = std::env::var("PATINA_HEATMAP_TEST_MODE").unwrap();
    let db = root.join("data/Patina Local/patina.db");
    if mode == "seed" || mode == "verify" {
        let result = tauri::async_runtime::block_on(async {
            if mode == "seed" {
                desktop_fixture::seed(&db, &std::env::var("PATINA_HEATMAP_TEST_PORT").unwrap())
                    .await
            } else {
                desktop_fixture::verify(&db).await
            }
        });
        evidence(
            &root,
            if mode == "seed" {
                "fixture.json"
            } else {
                "integrity.json"
            },
            result,
        );
        return;
    }
    if mode == "daemon" {
        crate::app::daemon::run(["patinad", "--profile", "local", "--serve-api", "--track"])
            .unwrap();
        return;
    }
    assert_eq!(mode, "desktop");
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "wayland");
    let mut context = tauri::generate_context!("tauri.local.conf.json");
    context.config_mut().identifier = crate::platform::app_paths::IDENTIFIER_LOCAL.into();
    context.config_mut().build.dev_url = Some(
        std::env::var("PATINA_HEATMAP_TEST_URL")
            .unwrap()
            .parse()
            .unwrap(),
    );
    context.config_mut().app.windows.clear();
    let outcome = Arc::new(Mutex::new(false));
    let result = outcome.clone();
    let app = bootstrap::build(bootstrap::BootstrapInput {
        runtime_health: Arc::new(RuntimeHealthState::default()),
        launched_by_autostart: false,
        runtime_mode: runtime::DesktopRuntimeMode::DaemonClientPreview,
        app_version: env!("CARGO_PKG_VERSION").into(),
    })
    .any_thread()
    .on_page_load(|view, event| {
        if event.event() == tauri::webview::PageLoadEvent::Finished && view.label() == "main" {
            view.eval(include_str!(
                "../../../scripts/perf/heatmap-desktop-page.js"
            ))
            .unwrap();
        }
    })
    .build(context)
    .unwrap();
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let worker_app = handle.clone();
        let worker = tauri::async_runtime::spawn(async move {
            wait_report(&root, 1).await;
            worker_app
                .get_webview_window("main")
                .unwrap()
                .close()
                .unwrap();
            evidence(&root, "closed.json", json!({"at_ms":runtime::now_ms()}));
            tokio::time::sleep(Duration::from_secs(310)).await;
            assert!(worker_app.webview_windows().is_empty());
            evidence(&root, "destroyed.json", json!({"at_ms":runtime::now_ms()}));
            tokio::time::sleep(Duration::from_secs(5)).await;
            main_window::show_main_window(&worker_app);
            wait_report(&root, 2).await;
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let passed = worker.await.is_ok();
        *result.lock().unwrap() = passed;
        handle.state::<AppExitState>().request_exit();
        handle.exit(if passed { 0 } else { 1 });
    });
    let code = app.run_return(bootstrap::handle_run_event);
    assert_eq!(code, 0);
    assert!(*outcome.lock().unwrap());
}
