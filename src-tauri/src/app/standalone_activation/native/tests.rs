use super::*;
use axum::{routing::get, Json, Router};
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[test]
fn login_preflight_rejects_external_drift_without_rewriting_preference() {
    let root = std::env::temp_dir().join(format!(
        "patina-login-intent-{}",
        super::super::journal::random_id().unwrap()
    ));
    assert!(validate_login_intent(&root, false).is_ok());
    assert!(validate_login_intent(&root, true).is_err());
    let reservation = cutover::prepare(&root, AppProfile::Production, true, false, 1).unwrap();
    cutover::mark_activating(&root, AppProfile::Production, &reservation.request_id, 2).unwrap();
    cutover::mark_completed(&root, AppProfile::Production, &reservation.request_id, 3).unwrap();
    let before = fs::read(root.join("runtime-owner-cutover.json")).unwrap();
    assert!(validate_login_intent(&root, true).is_ok());
    assert!(validate_login_intent(&root, false).is_err());
    assert_eq!(
        fs::read(root.join("runtime-owner-cutover.json")).unwrap(),
        before
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn target_verification_accepts_independent_version_and_requires_one_ready_instance() {
    for mode in [
        "ok",
        "hash",
        "version",
        "unready",
        "instance",
        "unmanaged",
        "missing",
    ] {
        let mut build = crate::app::daemon::build_info::current();
        build.package_version = "2.0.0".into();
        build.desktop_feature = false;
        let identity = DaemonExecutableIdentity {
            build,
            binary_sha256: "a".repeat(64),
        };
        let advertised = identity.clone();
        let reads = Arc::new(AtomicUsize::new(0));
        let seen = reads.clone();
        let router = Router::new().route("/api/v1/system/service", get(move || {
            let index = seen.fetch_add(1, Ordering::SeqCst);
            let mut executable = json!(advertised);
            if mode == "hash" { executable["binary_sha256"] = json!("b".repeat(64)); }
            if mode == "missing" { executable = serde_json::Value::Null; }
            async move { Json(json!({"data":{"service_name":"patinad.service","managed_by_systemd":mode!="unmanaged",
                "instance_id":if mode=="instance" && index>0 {"new"} else {"original"},"restart":null,"executable":executable}})) }
        })).route("/api/v1/capabilities", get(move || async move { Json(json!({"data":{
            "server_version":if mode=="version" {"1.0.0"} else {"2.0.0"}, "protocol_version":2,
            "protocol":{"current":2,"min_supported_client":1,"max_supported_client":2}, "runtime_host":"daemon",
            "event_stream":{"available":true},"tracking":{"owned":true,"ready":mode!="unready"},
            "browser_activity_bridge":{"owned":true,"ready":false},"tools":{"owned":true,"ready":true},
            "daemon_service":{"owned":true,"ready":true},"write_api":{"available":true,"operations":["service-lifecycle"]}
        }})) }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client =
            patina_client::Client::new(listener.local_addr().unwrap().port(), "fixture").unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        assert_eq!(
            api_matches_target(&client, &identity).await,
            mode == "ok",
            "{mode}"
        );
        assert_eq!(reads.load(Ordering::SeqCst), 2);
        server.abort();
    }
}
