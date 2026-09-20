//! Opt-in native maintenance acceptance. The service boundary is a private
//! D-Bus fixture; Desktop, WebKit, IPC, daemon and filesystem operations are real.
use super::{bootstrap, runtime, runtime_lease, runtime_owner_cutover, state::AppExitState};
use crate::data::repositories::daily_activity::desktop_fixture;
use crate::engine::tracking::watchdog::RuntimeHealthState;
use crate::platform::{app_paths::AppProfile, storage_anchor, storage_paths};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::Manager;

mod service;

const STAGES: [&str; 5] = [
    "move-data",
    "restore-data",
    "move-webview",
    "cache",
    "verify-cache",
];

fn process_start_ticks() -> String {
    // Everything after the command's closing ')' is whitespace-separated.
    // Linux proc field 22 is the process start time (index 19 after field 2).
    std::fs::read_to_string("/proc/self/stat")
        .unwrap()
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .nth(19)
        .unwrap()
        .to_owned()
}

fn root() -> PathBuf {
    use std::os::unix::fs::MetadataExt;
    let root = PathBuf::from(std::env::var_os("PATINA_STORAGE_TEST_ROOT").unwrap());
    assert_eq!(root.parent(), Some(Path::new("/tmp")));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("patina-storage-test-"));
    assert_eq!(root.canonicalize().unwrap(), root);
    let metadata = std::fs::metadata(&root).unwrap();
    assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
    assert_eq!(metadata.mode() & 0o777, 0o700);
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
    let bus = format!("unix:path={}/runtime/bus", root.display());
    assert_eq!(std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(), bus);
    assert_eq!(std::env::var("DBUS_SYSTEM_BUS_ADDRESS").unwrap(), bus);
    assert_eq!(
        std::fs::read_to_string(root.join("marker")).unwrap(),
        "storage-acceptance\n"
    );
    root
}

fn evidence(root: &Path, name: &str, value: Value, replace: bool) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = root.join(name);
    let temporary = path.with_extension("pending");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .unwrap();
    file.write_all(serde_json::to_string_pretty(&value).unwrap().as_bytes())
        .unwrap();
    drop(file);
    assert!(replace || !path.exists(), "refusing to replace evidence");
    std::fs::rename(temporary, path).unwrap();
}

async fn wait_report(root: &Path, stage: &str) {
    for _ in 0..1200 {
        if let Ok(text) = std::fs::read_to_string(root.join(format!("stage-{stage}.json"))) {
            let report: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(report["stage"], stage);
            assert_eq!(report["passed"], true, "{report}");
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("native storage stage {stage} did not report");
}

async fn serve(root: &Path) {
    // Install handlers before advertising readiness or creating an owned child.
    let mut interrupt =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt()).unwrap();
    let mut terminate =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
    evidence(
        root,
        "service-worker.json",
        json!({"pid":std::process::id(),"start_ticks":process_start_ticks()}),
        false,
    );
    let binary = PathBuf::from(std::env::var_os("PATINA_STORAGE_TEST_BINARY").unwrap());
    let fixture = service::serve(root, &binary).await;
    fixture.start().await.unwrap();
    evidence(
        root,
        "service-state.json",
        serde_json::to_value(fixture.snapshot().await).unwrap(),
        true,
    );
    evidence(root, "service-ready.json", json!({"ready":true}), false);
    loop {
        tokio::select! {
            _ = interrupt.recv() => break,
            _ = terminate.recv() => break,
            _ = tokio::time::sleep(Duration::from_millis(200)) => {
                evidence(root, "service-state.json", serde_json::to_value(fixture.snapshot().await).unwrap(), true);
            }
        }
    }
    fixture.stop().await.unwrap();
    fixture.cleanup().await.unwrap();
    evidence(
        root,
        "service-stopped.json",
        serde_json::to_value(fixture.snapshot().await).unwrap(),
        false,
    );
}

#[test]
#[ignore = "real Wayland/React and daemon processes; use scripts/storage-desktop-acceptance.mjs"]
fn storage_desktop_worker() {
    let root = root();
    let mode = std::env::var("PATINA_STORAGE_TEST_MODE").unwrap();
    let db = root.join("data/Patina/patina.db");
    if mode == "seed" {
        tauri::async_runtime::block_on(async {
            let seed =
                desktop_fixture::seed(&db, &std::env::var("PATINA_STORAGE_TEST_PORT").unwrap())
                    .await;
            evidence(&root, "fixture.json", seed, false);
            let control = root.join("config/Patina");
            let reservation = runtime_owner_cutover::prepare(
                &control,
                AppProfile::Production,
                false,
                false,
                runtime::now_ms(),
            )
            .unwrap();
            runtime_owner_cutover::mark_activating(
                &control,
                AppProfile::Production,
                &reservation.request_id,
                runtime::now_ms(),
            )
            .unwrap();
            runtime_owner_cutover::mark_completed(
                &control,
                AppProfile::Production,
                &reservation.request_id,
                runtime::now_ms(),
            )
            .unwrap();
        });
        return;
    }
    if mode == "service" {
        tauri::async_runtime::block_on(serve(&root));
        return;
    }
    if mode == "verify" {
        tauri::async_runtime::block_on(async {
            let integrity = desktop_fixture::verify(&db).await;
            assert!(root.join("moved-data/Patina/patina.db").is_file());
            assert!(std::fs::read_dir(root.join("data/Patina/backups"))
                .unwrap()
                .next()
                .is_some());
            let control = root.join("config/Patina");
            assert!(!storage_anchor::storage_migration_journal_exists(&control).unwrap());
            let paths = storage_paths::resolve_storage_paths_for_profile(
                &crate::platform::app_paths::environment_roots(),
                AppProfile::Production,
            )
            .unwrap();
            assert_eq!(paths.data_root, root.join("data/Patina"));
            let appointment: Value = serde_json::from_str(
                &std::fs::read_to_string(root.join("stage-move-webview.json")).unwrap(),
            )
            .unwrap();
            let planned_webview = PathBuf::from(
                appointment["snapshot"]["pendingMigration"]["targetWebviewRoot"]
                    .as_str()
                    .unwrap(),
            );
            assert!(planned_webview.starts_with(root.join("moved-webview")));
            assert_eq!(paths.webview_root, planned_webview);
            let lease = runtime_lease::acquire_runtime_lease(
                &control,
                AppProfile::Production,
                runtime_lease::RuntimeRole::Maintenance,
            )
            .unwrap();
            drop(lease);
            evidence(&root, "integrity.json", integrity, false);
        });
        return;
    }
    assert_eq!(mode, "desktop");
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "wayland");
    // Real app.restart retains the libtest arguments and environment. Only
    // this test's private continuation file advances the next test process.
    let continuation: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("continuation.json")).unwrap())
            .unwrap();
    let stage = continuation["stage"].as_str().unwrap().to_owned();
    let index = STAGES.iter().position(|value| *value == stage).unwrap();
    let next_stage = STAGES.get(index + 1).copied();
    let pid = std::process::id();
    if index > 0 {
        assert!(continuation["previous_pid"].as_u64().unwrap() > 0);
        assert_ne!(continuation["previous_pid"], pid);
    } else {
        assert!(continuation["previous_pid"].is_null());
    }
    evidence(
        &root,
        &format!("boot-{stage}.json"),
        json!({"stage":stage,"pid":pid,"start_ticks":process_start_ticks(),
            "previous_pid":continuation["previous_pid"]}),
        false,
    );
    let mut context = tauri::generate_context!();
    context.config_mut().build.dev_url = Some(
        std::env::var("PATINA_STORAGE_TEST_URL")
            .unwrap()
            .parse()
            .unwrap(),
    );
    context.config_mut().app.windows.clear();
    assert_eq!(
        context.config().identifier,
        crate::platform::app_paths::IDENTIFIER_PROD
    );
    let config = json!({"stage":stage,"root":root});
    let probe = format!(
        "window.__storageAcceptance={config};\n{}",
        include_str!("../../../scripts/storage-desktop-page.js")
    );
    let outcome = Arc::new(Mutex::new(false));
    let result = outcome.clone();
    let app = bootstrap::build(bootstrap::BootstrapInput {
        runtime_health: Arc::new(RuntimeHealthState::default()),
        launched_by_autostart: false,
        runtime_mode: runtime::DesktopRuntimeMode::DaemonClientManaged,
        app_version: env!("CARGO_PKG_VERSION").into(),
    })
    .any_thread()
    .on_page_load(move |view, event| {
        if event.event() == tauri::webview::PageLoadEvent::Finished && view.label() == "main" {
            view.eval(&probe).unwrap();
        }
    })
    .build(context)
    .unwrap();
    assert!(
        app.try_state::<runtime_lease::RuntimeLease>().is_none(),
        "Desktop must never own tracking in managed mode"
    );
    let handle = app.handle().clone();
    let event_root = root.clone();
    let event_stage = stage.clone();
    let completion_root = root.clone();
    tauri::async_runtime::spawn(async move {
        let worker_app = handle.clone();
        let worker = tauri::async_runtime::spawn(async move {
            wait_report(&root, &stage).await;
            let snapshot = crate::data::storage_migration::storage_snapshot(&worker_app).unwrap();
            assert!(snapshot.maintenance.last_error.is_none());
            let integrity = desktop_fixture::verify(&snapshot.paths.database_path).await;
            if stage == "cache" {
                std::fs::create_dir_all(&snapshot.webview_cache.path).unwrap();
                std::fs::write(
                    snapshot
                        .webview_cache
                        .path
                        .join("patina-acceptance-cache-marker"),
                    b"disposable synthetic cache",
                )
                .unwrap();
            } else if stage == "verify-cache" {
                assert!(!snapshot
                    .webview_cache
                    .path
                    .join("patina-acceptance-cache-marker")
                    .exists());
            }
            if let Some(next) = next_stage {
                evidence(
                    &root,
                    "continuation.json",
                    json!({"stage":next,"previous_pid":pid}),
                    true,
                );
            }
            evidence(
                &root,
                &format!("native-{stage}.json"),
                json!({"snapshot":snapshot,"integrity":integrity,"embedded_owner":false,"pid":pid}),
                false,
            );
        });
        let passed = worker.await.is_ok();
        *result.lock().unwrap() = passed;
        if passed && next_stage.is_some() {
            // The page clicks the production restart button after the runner
            // has inspected this stage. Do not replace app.restart with exit.
            return;
        }
        handle.state::<AppExitState>().request_exit();
        handle.exit(if passed { 0 } else { 1 });
    });
    assert_eq!(
        app.run_return(move |app, event| {
            if let tauri::RunEvent::ExitRequested {
                code: Some(tauri::RESTART_EXIT_CODE),
                ..
            } = &event
            {
                assert!(next_stage.is_some(), "unexpected final-stage restart");
                assert!(app.state::<AppExitState>().is_exit_requested());
                evidence(
                    &event_root,
                    &format!("restart-{event_stage}.json"),
                    json!({"stage":event_stage,"pid":pid,"tauri_restart_exit":true}),
                    false,
                );
            }
            bootstrap::handle_run_event(app, event);
        }),
        0
    );
    assert!(*outcome.lock().unwrap());
    assert!(
        next_stage.is_none(),
        "intermediate stage exited without app.restart"
    );
    evidence(
        &completion_root,
        "desktop-completed.json",
        json!({"passed":true,"pid":pid}),
        false,
    );
}
