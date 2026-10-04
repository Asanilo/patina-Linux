use super::*;
use crate::engine::api::runtime_control::DaemonServiceRestartSnapshot;

struct FixedTarget(ReloadTarget);
impl TargetSource for FixedTarget {
    type Guard = ();
    async fn inspect(&self) -> Result<ReloadTarget, String> {
        Ok(self.0.clone())
    }
    async fn hold(&self, expected: &ReloadTarget) -> Result<(), String> {
        if expected == &self.0 {
            Ok(())
        } else {
            Err("changed target".into())
        }
    }
}

struct HoldSource {
    target: ReloadTarget,
    busy: bool,
}
impl TargetSource for HoldSource {
    type Guard = ();
    async fn inspect(&self) -> Result<ReloadTarget, String> {
        Ok(self.target.clone())
    }
    async fn hold(&self, expected: &ReloadTarget) -> Result<(), String> {
        if self.busy || expected != &self.target {
            Err("installation is busy or changed".into())
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn standalone_reload_uses_target_identity_and_rejects_stale_confirmation() {
    use axum::{
        http::{Method, StatusCode, Uri},
        Json, Router,
    };
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    for mode in [
        "success",
        "current",
        "hash",
        "version",
        "protocol",
        "stale-instance",
        "stale-target",
        "busy",
        "verification-instance",
        "delayed-ready",
    ] {
        let mut build = crate::app::daemon::build_info::current();
        build.package_version = "2.0.0".into();
        build.desktop_feature = false;
        let identity = DaemonExecutableIdentity {
            build,
            binary_sha256: "b".repeat(64),
        };
        let mut source = HoldSource {
            target: ReloadTarget {
                version: "2.0.0".into(),
                identity: Some(identity.clone()),
                manifest: Some("a".repeat(64)),
                root: Some("/fixture/runtime".into()),
            },
            busy: mode == "busy",
        };
        let posts = Arc::new(AtomicUsize::new(0));
        let generation = Arc::new(AtomicUsize::new(0));
        let reads = Arc::new(AtomicUsize::new(0));
        let caps = Arc::new(AtomicUsize::new(0));
        let state = (posts.clone(), generation.clone(), reads, caps);
        let router = Router::new().fallback(move |method: Method, uri: Uri| {
            let (posts, generation, reads, caps) = state.clone();
            let identity = identity.clone();
            async move {
                let restarted = posts.load(Ordering::SeqCst) > 0;
                let ticket = |completed: bool| json!({"request_id":"ticket", "status":if completed {"completed"} else {"pending"}, "requested_at_ms":1, "requested_instance_id":"old", "completed_at_ms":if completed {Some(2)} else {None}, "completed_instance_id":if completed {Some("new")} else {None}});
                let service = |completed: bool, changed: bool| {
                    let mut executable = identity.clone();
                    executable.binary_sha256 = if completed { if mode == "hash" {"d"} else {"b"} } else if mode == "current" {"b"} else {"c"}.repeat(64);
                    json!({"service_name":"patinad.service", "managed_by_systemd":true, "instance_id":if changed {"external"} else if completed {"new"} else {"old"}, "restart":if completed {ticket(true)} else {serde_json::Value::Null}, "executable":executable})
                };
                let data = match (method, uri.path()) {
                    (Method::GET, "/api/v1/capabilities") => {
                        let protocol = if restarted && mode == "protocol" {3} else {2};
                        let ready = !(restarted && mode == "delayed-ready" && caps.fetch_add(1, Ordering::SeqCst) == 0);
                        json!({"server_version":if restarted && mode == "version" {"2.0.1"} else {"2.0.0"}, "protocol_version":protocol,
                            "protocol":{"current":protocol,"min_supported_client":protocol,"max_supported_client":protocol},
                            "runtime_host":"daemon", "event_stream":{"available":true}, "tracking":{"owned":true,"ready":ready},
                            "browser_activity_bridge":{"owned":true,"ready":false},"tools":{"owned":true,"ready":true},
                            "daemon_service":{"owned":true,"ready":true},"write_api":{"available":true,"operations":["service-lifecycle"]}})
                    },
                    (Method::GET, "/api/v1/system/service") => {
                        let changed = generation.load(Ordering::SeqCst) > 0 || (restarted && mode == "verification-instance" && reads.fetch_add(1, Ordering::SeqCst) > 0);
                        service(restarted, changed)
                    },
                    (Method::POST, "/api/v1/system/service/restart") => {
                        posts.fetch_add(1, Ordering::SeqCst);
                        let mut value = service(false, false); value["restart"] = ticket(false);
                        return (StatusCode::ACCEPTED, Json(json!({"data":{"service":value,"reconnect_required":true}})));
                    },
                    _ => panic!("unexpected request"),
                };
                (StatusCode::OK, Json(json!({"data":data})))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = PatinadClient::new(listener.local_addr().unwrap().port(), "fixture").unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let observed = inspect_with_source(&client, true, &source).await.0;
        assert!(observed.error.is_none(), "{mode}: {:?}", observed.error);
        assert_eq!(observed.distribution, "standalone");
        assert_ne!(observed.desktop_version, "2.0.0");
        assert_eq!(
            observed.target_state,
            if mode == "current" {
                "current"
            } else {
                "pending"
            }
        );
        assert_eq!(observed.restart_available, mode != "current");
        if mode == "stale-instance" {
            generation.store(1, Ordering::SeqCst);
        }
        if mode == "stale-target" {
            source.target.manifest = Some("e".repeat(64));
        }
        let result = restart_with_source(
            &client,
            "2.0.0",
            observed.reload_revision.as_deref().unwrap_or(""),
            &source,
        )
        .await;
        assert_eq!(
            result.is_ok(),
            matches!(mode, "success" | "delayed-ready"),
            "{mode}: {result:?}"
        );
        assert_eq!(
            posts.load(Ordering::SeqCst),
            usize::from(!matches!(
                mode,
                "current" | "stale-instance" | "stale-target" | "busy"
            )),
            "{mode}"
        );
        task.abort();
        let _ = task.await;
    }
}

#[tokio::test]
async fn reload_transport_posts_once_and_verifies_the_result() {
    use axum::{
        body::Bytes,
        http::{HeaderMap, Method, StatusCode, Uri},
        Json, Router,
    };
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    for mode in ["success", "wrong-version", "rejected", "wrong-ticket"] {
        let posts = Arc::new(AtomicUsize::new(0));
        let seen = posts.clone();
        let router = Router::new().fallback(move |method: Method, uri: Uri, headers: HeaderMap, body: Bytes| {
            let posts = seen.clone();
            async move {
                assert_eq!(headers.get("authorization").unwrap(), "Bearer isolated-test-token");
                let restarted = posts.load(Ordering::SeqCst) > 0;
                let ticket = |completed: bool| json!({
                    "request_id": "ticket", "status": if completed {"completed"} else {"pending"},
                    "requested_at_ms": 1, "requested_instance_id": if mode == "wrong-ticket" {"unrelated"} else {"old"},
                    "completed_at_ms": if completed {Some(2)} else {None},
                    "completed_instance_id": if completed {Some("new")} else {None},
                });
                let service = |completed: bool| json!({
                    "service_name": "patinad.service", "managed_by_systemd": true,
                    "instance_id": if completed {"new"} else {"old"},
                    "restart": if completed {ticket(true)} else {serde_json::Value::Null},
                });
                let data = match (method, uri.path()) {
                    (Method::GET, "/api/v1/capabilities") => json!({
                        "server_version": if restarted && mode != "wrong-version" {env!("CARGO_PKG_VERSION")} else {"old-version"},
                        "protocol_version": 2, "protocol": {"current":2,"min_supported_client":1,"max_supported_client":2},
                        "runtime_host":"daemon", "event_stream":{"available":true},
                        "tracking":{"owned":true,"ready":true}, "browser_activity_bridge":{"owned":true,"ready":true},
                        "tools":{"owned":true,"ready":true}, "daemon_service":{"owned":true,"ready":true},
                        "write_api":{"available":true,"operations":["service-lifecycle"]},
                    }),
                    (Method::GET, "/api/v1/system/service") => service(restarted),
                    (Method::POST, "/api/v1/system/service/restart") => {
                        assert_eq!(serde_json::from_slice::<serde_json::Value>(&body).unwrap(), json!({"confirmed":true}));
                        posts.fetch_add(1, Ordering::SeqCst);
                        if mode == "rejected" {
                            return (StatusCode::CONFLICT, Json(json!({"error":{"code":"conflict","message":"busy"}})));
                        }
                        let mut value = service(false);
                        value["restart"] = ticket(false);
                        return (StatusCode::ACCEPTED, Json(json!({"data":{"service":value,"reconnect_required":true}})));
                    }
                    _ => panic!("unexpected test request"),
                };
                (StatusCode::OK, Json(json!({"data":data})))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = PatinadClient::new(port, "isolated-test-token").unwrap();
        let source = FixedTarget(ReloadTarget::bundled());
        assert!(
            !inspect_with_source(&client, false, &source)
                .await
                .0
                .restart_available
        );
        let revision = inspect_with_source(&client, true, &source)
            .await
            .0
            .reload_revision
            .unwrap();
        assert!(
            restart_with_source(&client, "stale-confirmation", &revision, &source)
                .await
                .is_err()
        );
        assert_eq!(posts.load(Ordering::SeqCst), 0);
        let result = restart_with_source(&client, "old-version", &revision, &source).await;
        task.abort();
        let _ = task.await;
        assert_eq!(result.is_ok(), mode == "success", "{mode}: {result:?}");
        assert_eq!(posts.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn completion_requires_matching_ticket_and_new_instance() {
    let mut service = DaemonServiceRuntimeSnapshot {
        executable: None,
        executable_error: None,
        service_name: "patinad.service".to_string(),
        managed_by_systemd: true,
        instance_id: "new".to_string(),
        restart: Some(DaemonServiceRestartSnapshot {
            request_id: "ticket".to_string(),
            status: "completed".to_string(),
            requested_at_ms: 1,
            requested_instance_id: "old".to_string(),
            completed_at_ms: Some(2),
            completed_instance_id: Some("new".to_string()),
        }),
    };
    assert!(restart_completed(&service, "ticket", "old"));
    assert!(!restart_completed(&service, "other", "old"));
    assert!(!restart_completed(&service, "ticket", "new"));
    service.restart.as_mut().unwrap().status = "pending".to_string();
    assert!(!restart_completed(&service, "ticket", "old"));
    service.restart.as_mut().unwrap().status = "completed".to_string();
    service.restart.as_mut().unwrap().completed_instance_id = Some("other".to_string());
    assert!(!restart_completed(&service, "ticket", "old"));
    service.restart = None;
    assert!(!restart_completed(&service, "ticket", "old"));
}

#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires standalone-activation.py's private D-Bus/profile and two real candidates"]
async fn native_reload_selected_backend() {
    use crate::app::standalone_activation;
    use crate::platform::linux::standalone_runtime;
    let root = PathBuf::from(
        std::env::var("PATINA_RELOAD_ACCEPTANCE_ROOT").expect("private fixture only"),
    );
    assert!(root.starts_with(std::env::temp_dir()));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("patina-activate-private-"));
    let bus = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
    assert_ne!(bus, std::env::var("PATINA_ACCEPTANCE_PARENT_BUS").unwrap());
    assert!(bus.contains("/tmp/"));
    let roots = crate::platform::app_paths::environment_roots();
    assert_eq!(roots.config, root.join("config"));
    assert_eq!(roots.data, root.join("data"));
    let port = std::env::var("PATINA_RELOAD_ACCEPTANCE_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let token = crate::engine::api::auth::ApiCredentialStore::new()
        .load_existing_at(&roots.data.join("Patina/api_token"))
        .unwrap();
    let client = PatinadClient::new(port, token).unwrap();
    let control = roots.config.join("Patina");
    struct ObservedSource {
        native: installation::NativeSource,
        acquired: tokio::sync::Notify,
        proceed: tokio::sync::Notify,
    }
    impl TargetSource for ObservedSource {
        type Guard = <installation::NativeSource as TargetSource>::Guard;
        async fn inspect(&self) -> Result<ReloadTarget, String> {
            self.native.inspect().await
        }
        async fn hold(&self, expected: &ReloadTarget) -> Result<Self::Guard, String> {
            let guard = self.native.hold(expected).await?;
            self.acquired.notify_one();
            self.proceed.notified().await;
            Ok(guard)
        }
    }
    let source = ObservedSource {
        native: installation::NativeSource::new(&control),
        acquired: tokio::sync::Notify::new(),
        proceed: tokio::sync::Notify::new(),
    };
    let (before, target, _) = inspect_with_source(&client, true, &source).await;
    assert!(before.error.is_none(), "{:?}", before.error);
    assert_eq!(before.distribution, "standalone");
    assert_eq!(before.target_state, "pending");
    let target = target.unwrap();
    let version = before.running_version.unwrap();
    let revision = before.reload_revision.unwrap();
    let monitor = async {
        source.acquired.notified().await;
        assert!(standalone_runtime::select(
            target.root.as_ref().unwrap(),
            target.manifest.as_ref().unwrap(),
            &standalone_runtime::ExpectedCurrent::Manifest(target.manifest.clone().unwrap()),
            true
        )
        .is_err());
        assert!(standalone_activation::hold_client_reload(
            &roots,
            &control,
            target.root.as_deref()
        )
        .is_err());
        source.proceed.notify_one();
    };
    let (reloaded, ()) = tokio::time::timeout(Duration::from_secs(65), async {
        tokio::join!(
            restart_with_source(&client, &version, &revision, &source),
            monitor
        )
    })
    .await
    .unwrap();
    reloaded.unwrap();
    let after = inspect_with_source(&client, true, &source).await.0;
    assert_eq!(after.target_state, "current");
    assert!(!after.restart_available && after.error.is_none());
    let service = client.service_snapshot().await.unwrap();
    assert_eq!(service.executable, target.identity);
    let evidence = serde_json::json!({"passed":true,"running_version":after.running_version,"binary_sha256":service.executable.unwrap().binary_sha256,"selection_and_profile_locked":true});
    std::fs::write(
        root.join("reload-result.json"),
        serde_json::to_vec(&evidence).unwrap(),
    )
    .unwrap();
}
