#[test]
fn daemon_status_identifies_stage_one_runtime() {
    let status = patina_lib::app::daemon::build_startup_status("1.8.3");

    assert_eq!(status.service_name, "patinad");
    assert_eq!(status.mode, "daemon");
    assert_eq!(status.version, "1.8.3");
    assert_eq!(status.stage, "stage-1");
    assert!(status.sqlite_enabled);
    assert!(!status.tracking_enabled);
    assert!(!status.local_api_enabled);
    assert_eq!(status.local_api_port, 14_840);
    assert!(status.api_token_path.ends_with("api_token"));
    assert!(status.data_root.ends_with("Patina"));
    assert!(status.db_path.ends_with("patina.db"));
    assert!(status.webview_root.ends_with("Patina"));
    assert!(status.notes.iter().any(|note| note.contains("skeleton")));
}

#[tokio::test]
async fn daemon_sqlite_runtime_prepares_database_without_tauri_app_handle() {
    let root = std::env::temp_dir().join(format!(
        "patina-daemon-sqlite-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db_path = root.join("Patina").join("patina.db");

    let runtime = patina_lib::app::daemon::prepare_sqlite_runtime_at_path(&db_path)
        .await
        .unwrap();

    assert_eq!(runtime.db_path, db_path);
    let table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'sessions'",
    )
    .fetch_one(&runtime.pool)
    .await
    .unwrap();
    assert_eq!(table_count, 1);

    runtime.pool.close().await;
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn daemon_minimal_api_routes_health_and_openapi_without_tauri_app_handle() {
    let health = patina_lib::app::daemon::route_minimal_api_request("GET", "/api/v1/health");
    assert_eq!(health.status, 200);
    assert_eq!(health.body["data"]["status"], "ok");
    assert_eq!(health.body["data"]["version"], env!("CARGO_PKG_VERSION"));

    let openapi = patina_lib::app::daemon::route_minimal_api_request("GET", "/api/v1/openapi.json");
    assert_eq!(openapi.status, 200);
    assert_eq!(openapi.body["openapi"], "3.1.0");

    let missing = patina_lib::app::daemon::route_minimal_api_request("GET", "/api/v1/current");
    assert_eq!(missing.status, 404);
}

#[test]
fn daemon_run_options_enable_minimal_api_from_flag() {
    let options = patina_lib::app::daemon::DaemonRunOptions::from_args(["patinad", "--serve-api"]);
    assert!(options.serve_minimal_api);

    let default_options = patina_lib::app::daemon::DaemonRunOptions::from_args(["patinad"]);
    assert!(!default_options.serve_minimal_api);
}

#[tokio::test]
async fn daemon_minimal_api_server_serves_health_without_tauri_app_handle() {
    let server = patina_lib::app::daemon::prepare_minimal_api_server(0, "test-token")
        .await
        .unwrap();
    let port = server.port();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());

    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let request = "GET /api/v1/health HTTP/1.1\r\nAuthorization: Bearer test-token\r\n\r\n";
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
        .await
        .unwrap();
    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .unwrap();

    assert!(response.contains("HTTP/1.1 200 OK"));
    assert!(response.contains("\"status\":\"ok\""));

    shutdown.shutdown();
    task.await.unwrap();
}

#[tokio::test]
async fn daemon_minimal_api_server_rejects_missing_token() {
    let server = patina_lib::app::daemon::prepare_minimal_api_server(0, "test-token")
        .await
        .unwrap();
    let port = server.port();
    let shutdown = server.shutdown_handle();
    let task = tokio::spawn(server.run());

    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    let request = "GET /api/v1/health HTTP/1.1\r\n\r\n";
    tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
        .await
        .unwrap();
    let mut response = String::new();
    tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut response)
        .await
        .unwrap();

    assert!(response.contains("HTTP/1.1 401 Unauthorized"));
    assert!(response.contains("unauthorized"));

    shutdown.shutdown();
    task.await.unwrap();
}
