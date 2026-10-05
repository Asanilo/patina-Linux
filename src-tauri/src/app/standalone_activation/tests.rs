use super::*;
use std::{
    fs,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};

struct Fixture {
    root: PathBuf,
    paths: StoragePaths,
    roots: AppPathRoots,
    selected: standalone_runtime::StagedRuntime,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "patina-activation-{}",
            journal::random_id().unwrap()
        ));
        fs::create_dir(&root).unwrap();
        let roots = AppPathRoots {
            config: root.join("config"),
            data: root.join("data"),
            local_data: root.join("data"),
        };
        let paths = crate::platform::storage_paths::default_storage_paths_for_profile(
            &roots,
            AppProfile::Production,
        );
        let mut build = crate::app::daemon::build_info::current();
        build.desktop_feature = false;
        let selected = standalone_runtime::StagedRuntime {
            manifest_sha256: "a".repeat(64),
            binary_sha256: "b".repeat(64),
            directory: root.join("runtime/versions/version"),
            build,
        };
        Self {
            root,
            paths,
            roots,
            selected,
        }
    }
    fn host(&self) -> Host {
        Host {
            control: self.paths.control_root.clone(),
            effects: Mutex::new(Vec::new()),
            failure: Mutex::new(None),
            running: AtomicBool::new(false),
            wait_on_verify: AtomicBool::new(false),
            verification_entered: tokio::sync::Notify::new(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[cfg(feature = "desktop")]
#[tokio::test]
async fn client_binding_does_not_require_payload_and_reload_excludes_activation() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let host = fixture.host();
    let store = Store::open(&fixture.paths.control_root).unwrap();
    let root = fixture.root.join("runtime");
    execute(
        &store,
        &fixture.paths,
        &fixture.roots,
        &root,
        &fixture.selected,
        &host,
    )
    .await
    .unwrap();
    drop(store);
    let unit = crate::platform::linux::patinad_service_unit::standalone(
        &root,
        &fixture.roots.config,
        &fixture.roots.data,
    )
    .unwrap();
    crate::platform::linux::patinad_service_unit::install_identical_or_new(
        &fixture.roots.config,
        &unit,
    )
    .unwrap();
    let path = fixture
        .paths
        .control_root
        .join("standalone-activation.json");
    let mut record: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    record["future_audit_field"] = serde_json::json!({"format":2});
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!root.exists());
    assert!(uses_standalone_binding(&fixture.roots, &fixture.paths.control_root).unwrap());
    let guard =
        hold_client_reload(&fixture.roots, &fixture.paths.control_root, Some(&root)).unwrap();
    assert!(Store::open(&fixture.paths.control_root).is_err());
    drop(guard);
    assert!(Store::open(&fixture.paths.control_root).is_ok());
    record["phase"] = "starting".into();
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(uses_standalone_binding(&fixture.roots, &fixture.paths.control_root).unwrap());
    assert!(hold_client_reload(&fixture.roots, &fixture.paths.control_root, Some(&root)).is_err());
    // Unknown installer state remains independent of client binding recognition.
    record["phase"] = serde_json::json!({"new_phase_format":1});
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(uses_standalone_binding(&fixture.roots, &fixture.paths.control_root).unwrap());
    assert!(hold_client_reload(&fixture.roots, &fixture.paths.control_root, Some(&root)).is_err());
}

#[cfg(feature = "desktop")]
#[test]
fn bundled_reload_excludes_a_concurrent_first_standalone_migration() {
    let fixture = Fixture::new();
    let guard = hold_client_reload(&fixture.roots, &fixture.paths.control_root, None).unwrap();
    assert!(Store::open(&fixture.paths.control_root).is_err());
    assert!(!fixture
        .paths
        .control_root
        .join("standalone-activation.json")
        .exists());
    drop(guard);
    assert!(Store::open(&fixture.paths.control_root).is_ok());
}

struct Host {
    control: PathBuf,
    effects: Mutex<Vec<&'static str>>,
    failure: Mutex<Option<&'static str>>,
    running: AtomicBool,
    wait_on_verify: AtomicBool,
    verification_entered: tokio::sync::Notify,
}

#[tokio::test]
async fn disabled_startup_keeps_daemon_ownership_and_requires_explicit_activation() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let store = Store::open(&fixture.paths.control_root).unwrap();
    let root = fixture.root.join("runtime");
    let mut record = execute(
        &store,
        &fixture.paths,
        &fixture.roots,
        &root,
        &fixture.selected,
        &host,
    )
    .await
    .unwrap();
    let original_cutover = cutover::diagnose(&fixture.paths.control_root, AppProfile::Production);
    record.runtime_start_allowed = false;
    store.write(&record).unwrap();
    drop(store);
    assert!(require_runtime_start(&fixture.paths.control_root).is_err());
    assert!(matches!(
        cutover::decide_desktop_startup(&fixture.paths.control_root, AppProfile::Production),
        cutover::RuntimeOwnerStartupDecision::Blocked { .. }
    ));
    assert!(matches!(
        cutover::decide_owner_for_installation(&fixture.paths.control_root, AppProfile::Production),
        cutover::RuntimeOwnerStartupDecision::DaemonClient { .. }
    ));
    #[cfg(feature = "desktop")]
    assert!(hold_client_reload(&fixture.roots, &fixture.paths.control_root, Some(&root)).is_err());
    let store = Store::open(&fixture.paths.control_root).unwrap();
    host.effects.lock().unwrap().clear();
    // Explicit activation can re-admit a verified healthy target without restart.
    let restored = execute(
        &store,
        &fixture.paths,
        &fixture.roots,
        &root,
        &fixture.selected,
        &host,
    )
    .await
    .unwrap();
    assert!(restored.runtime_start_allowed);
    assert!(require_runtime_start(&fixture.paths.control_root).is_ok());
    assert_eq!(*host.effects.lock().unwrap(), vec!["preflight", "verify"]);
    assert_eq!(
        cutover::diagnose(&fixture.paths.control_root, AppProfile::Production).request_id,
        original_cutover.request_id
    );
}

#[test]
fn disabled_or_invalid_installation_cannot_fall_back_to_embedded_without_cutover() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.paths.control_root).unwrap();
    let record = serde_json::json!({"format_version":1, "runtime_root":fixture.root.join("runtime"),
        "config_root":fixture.roots.config, "data_root":fixture.roots.data, "runtime_start_allowed":false});
    let path = fixture
        .paths
        .control_root
        .join("standalone-activation.json");
    fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(
        !cutover::decide_desktop_startup(&fixture.paths.control_root, AppProfile::Production)
            .owns_embedded_runtime()
    );
    fs::write(&path, b"{incomplete").unwrap();
    assert!(
        !cutover::decide_desktop_startup(&fixture.paths.control_root, AppProfile::Production)
            .owns_embedded_runtime()
    );
    assert!(!fixture.paths.db_path.exists());
}

impl Host {
    fn effect(&self, stage: &'static str) -> Result<(), String> {
        self.effects.lock().unwrap().push(stage);
        if *self.failure.lock().unwrap() == Some(stage) {
            return Err(format!("injected {stage} failure"));
        }
        Ok(())
    }
}
impl ActivationHost for Host {
    async fn preflight(&self) -> Result<(), String> {
        self.effect("preflight")
    }
    async fn stop(&self) -> Result<(), String> {
        assert!(journal::read_at(&self.control)?.is_some());
        assert!(require_runtime_start(&self.control).is_err());
        self.effect("stop")?;
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }
    async fn install(&self) -> Result<(), String> {
        assert!(require_runtime_start(&self.control).is_err());
        assert!(
            runtime_lease::acquire_runtime_lease(
                &self.control,
                AppProfile::Production,
                runtime_lease::RuntimeRole::Daemon
            )
            .is_err(),
            "maintenance must exclude writers while installing"
        );
        self.effect("install")
    }
    async fn start(&self) -> Result<(), String> {
        require_runtime_start(&self.control)?;
        assert_eq!(
            journal::read_at(&self.control)?.unwrap().phase,
            Phase::Starting
        );
        let lease = runtime_lease::acquire_runtime_lease(
            &self.control,
            AppProfile::Production,
            runtime_lease::RuntimeRole::Daemon,
        )
        .map_err(|error| error.to_string())?;
        drop(lease);
        self.effect("start")?;
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn matches_target(&self) -> Result<bool, String> {
        Ok(self.running.load(Ordering::SeqCst))
    }
    async fn verify(&self) -> Result<(), String> {
        self.effect("verify")?;
        self.verification_entered.notify_one();
        if self.wait_on_verify.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        Ok(())
    }
}

#[tokio::test]
async fn fresh_activation_records_intent_excludes_writers_and_finishes_in_client_mode() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let store = Store::open(&fixture.paths.control_root).unwrap();
    let result = execute(
        &store,
        &fixture.paths,
        &fixture.roots,
        &fixture.root.join("runtime"),
        &fixture.selected,
        &host,
    )
    .await
    .unwrap();
    assert_eq!(result.phase, Phase::Completed);
    let cutover::RuntimeOwnerStartupDecision::DaemonClient { reservation, .. } =
        cutover::decide_desktop_startup(&fixture.paths.control_root, AppProfile::Production)
    else {
        panic!("Desktop must connect to daemon")
    };
    assert_eq!(
        reservation.status,
        cutover::RuntimeOwnerCutoverStatus::Completed
    );
    assert!(!reservation.background_tracking_at_login && !reservation.desktop_launch_at_login);
    assert_eq!(
        *host.effects.lock().unwrap(),
        vec![
            "preflight",
            "stop",
            "preflight",
            "install",
            "start",
            "verify"
        ]
    );
    host.effects.lock().unwrap().clear();
    execute(
        &store,
        &fixture.paths,
        &fixture.roots,
        &fixture.root.join("runtime"),
        &fixture.selected,
        &host,
    )
    .await
    .unwrap();
    assert_eq!(
        *host.effects.lock().unwrap(),
        vec!["preflight", "verify"],
        "healthy repeat must not restart"
    );
}

#[tokio::test]
async fn activation_adopts_an_already_verified_selected_target_without_another_restart() {
    let mut fixture = Fixture::new();
    let host = fixture.host();
    let store = Store::open(&fixture.paths.control_root).unwrap();
    let root = fixture.root.join("runtime");
    let first = execute(
        &store,
        &fixture.paths,
        &fixture.roots,
        &root,
        &fixture.selected,
        &host,
    )
    .await
    .unwrap();
    fixture.selected.manifest_sha256 = "c".repeat(64);
    fixture.selected.binary_sha256 = "d".repeat(64);
    // This host's matches_target/verify contract now represents the selected
    // target already running after an explicit client reload.
    host.effects.lock().unwrap().clear();
    let adopted = execute(
        &store,
        &fixture.paths,
        &fixture.roots,
        &root,
        &fixture.selected,
        &host,
    )
    .await
    .unwrap();
    assert_eq!(adopted.cutover_request_id, first.cutover_request_id);
    assert_eq!(adopted.manifest_sha256, fixture.selected.manifest_sha256);
    assert_eq!(adopted.binary_sha256, fixture.selected.binary_sha256);
    assert_eq!(*host.effects.lock().unwrap(), vec!["preflight", "verify"]);
    assert_eq!(
        store.read().unwrap().unwrap().manifest_sha256,
        fixture.selected.manifest_sha256
    );
}

#[tokio::test]
async fn interrupted_activation_resumes_same_cutover_without_implicit_rollback() {
    for stage in ["stop", "install", "start", "verify"] {
        let fixture = Fixture::new();
        let host = fixture.host();
        let store = Store::open(&fixture.paths.control_root).unwrap();
        *host.failure.lock().unwrap() = Some(stage);
        assert!(execute(
            &store,
            &fixture.paths,
            &fixture.roots,
            &fixture.root.join("runtime"),
            &fixture.selected,
            &host
        )
        .await
        .is_err());
        let pending = store.read().unwrap().unwrap();
        assert_ne!(pending.phase, Phase::Completed);
        assert!(pending.last_error.unwrap().contains(stage));
        assert_eq!(pending.binary_sha256, fixture.selected.binary_sha256);
        assert!(!cutover::decide_desktop_startup(
            &fixture.paths.control_root,
            AppProfile::Production
        )
        .owns_embedded_runtime());
        *host.failure.lock().unwrap() = None;
        let completed = execute(
            &store,
            &fixture.paths,
            &fixture.roots,
            &fixture.root.join("runtime"),
            &fixture.selected,
            &host,
        )
        .await
        .unwrap();
        assert_eq!(completed.cutover_request_id, pending.cutover_request_id);
        assert_eq!(completed.phase, Phase::Completed);
    }
}

#[tokio::test]
async fn cancellation_after_start_leaves_recoverable_intent_and_releases_profile_lock() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let store = Store::open(&fixture.paths.control_root).unwrap();
    host.wait_on_verify.store(true, Ordering::SeqCst);
    let runtime_root = fixture.root.join("runtime");
    {
        let mut run = Box::pin(execute(
            &store,
            &fixture.paths,
            &fixture.roots,
            &runtime_root,
            &fixture.selected,
            &host,
        ));
        tokio::select! {
            result = &mut run => panic!("activation unexpectedly finished: {result:?}"),
            entered = tokio::time::timeout(Duration::from_secs(5), host.verification_entered.notified()) => entered.unwrap(),
        }
        // Dropping the future models cancellation after the actual start checkpoint.
    }
    let pending = store.read().unwrap().unwrap();
    assert_eq!(pending.phase, Phase::Starting);
    drop(store);
    let recovered = Store::open(&fixture.paths.control_root).unwrap();
    host.wait_on_verify.store(false, Ordering::SeqCst);
    let result = execute(
        &recovered,
        &fixture.paths,
        &fixture.roots,
        &fixture.root.join("runtime"),
        &fixture.selected,
        &host,
    )
    .await
    .unwrap();
    assert_eq!(result.cutover_request_id, pending.cutover_request_id);
    assert_eq!(
        host.effects
            .lock()
            .unwrap()
            .iter()
            .filter(|effect| **effect == "start")
            .count(),
        1,
        "ready interrupted activation must not restart again"
    );
}

#[tokio::test]
async fn completed_cutover_preferences_survive_backend_activation() {
    let fixture = Fixture::new();
    let host = fixture.host();
    let store = Store::open(&fixture.paths.control_root).unwrap();
    let prepared = cutover::prepare(
        &fixture.paths.control_root,
        AppProfile::Production,
        false,
        true,
        10,
    )
    .unwrap();
    cutover::mark_activating(
        &fixture.paths.control_root,
        AppProfile::Production,
        &prepared.request_id,
        11,
    )
    .unwrap();
    cutover::mark_completed(
        &fixture.paths.control_root,
        AppProfile::Production,
        &prepared.request_id,
        12,
    )
    .unwrap();
    execute(
        &store,
        &fixture.paths,
        &fixture.roots,
        &fixture.root.join("runtime"),
        &fixture.selected,
        &host,
    )
    .await
    .unwrap();
    let cutover::RuntimeOwnerStartupDecision::DaemonClient { reservation, .. } =
        cutover::decide_desktop_startup(&fixture.paths.control_root, AppProfile::Production)
    else {
        panic!()
    };
    assert_eq!(reservation.request_id, prepared.request_id);
    assert!(!reservation.background_tracking_at_login);
    assert!(reservation.desktop_launch_at_login);
}

#[tokio::test]
async fn unrelated_pending_cutover_existing_embedded_data_and_custom_units_stop_before_effects() {
    for mode in ["pending", "embedded-data", "preflight"] {
        let fixture = Fixture::new();
        let host = fixture.host();
        let store = Store::open(&fixture.paths.control_root).unwrap();
        match mode {
            "pending" => {
                cutover::prepare(
                    &fixture.paths.control_root,
                    AppProfile::Production,
                    false,
                    false,
                    10,
                )
                .unwrap();
            }
            "embedded-data" => {
                fs::create_dir_all(&fixture.paths.data_root).unwrap();
                fs::write(&fixture.paths.db_path, b"existing").unwrap();
            }
            _ => {
                *host.failure.lock().unwrap() = Some("preflight");
            }
        }
        assert!(execute(
            &store,
            &fixture.paths,
            &fixture.roots,
            &fixture.root.join("runtime"),
            &fixture.selected,
            &host
        )
        .await
        .is_err());
        assert_eq!(*host.effects.lock().unwrap(), vec!["preflight"]);
        assert!(store.read().unwrap().is_none());
    }
}

#[test]
fn activation_journal_rejects_concurrent_writers_links_and_damaged_state() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let fixture = Fixture::new();
    let store = Store::open(&fixture.paths.control_root).unwrap();
    assert!(Store::open(&fixture.paths.control_root).is_err());
    let path = fixture
        .paths
        .control_root
        .join("standalone-activation.json");
    let target = fixture.root.join("unrelated");
    fs::write(&target, b"keep").unwrap();
    symlink(&target, &path).unwrap();
    assert!(store.read().is_err());
    assert_eq!(fs::read(&target).unwrap(), b"keep");
    fs::remove_file(&path).unwrap();
    fs::write(&path, b"damaged").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(store.read().is_err());
}
