mod options;
mod status;
mod storage;

use std::path::PathBuf;

use sqlx::{Pool, Sqlite};

pub use options::DaemonRunOptions;
pub use status::DaemonStartupStatus;

#[derive(Debug)]
pub struct DaemonSqliteRuntime {
    #[cfg_attr(not(test), allow(dead_code))]
    pub db_path: PathBuf,
    pub pool: Pool<Sqlite>,
}

pub fn build_startup_status(
    version: impl Into<String>,
    options: DaemonRunOptions,
    storage_paths: &crate::platform::storage_paths::StoragePaths,
    local_api_port: u16,
) -> DaemonStartupStatus {
    status::build_startup_status(
        version,
        options.profile,
        options.serve_minimal_api,
        local_api_port,
        storage_paths.api_token_path.clone(),
        storage_paths.data_root.clone(),
        storage_paths.db_path.clone(),
        storage_paths.webview_root.clone(),
    )
}

pub fn run(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<(), String> {
    run_with_options(DaemonRunOptions::from_args(args)?)
}

pub fn run_with_options(options: DaemonRunOptions) -> Result<(), String> {
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("failed to create daemon async runtime: {error}"))?;
    let roots = crate::platform::app_paths::environment_roots();
    let default_paths =
        crate::platform::storage_paths::default_storage_paths_for_profile(&roots, options.profile);
    let runtime_lease = crate::app::runtime_lease::acquire_runtime_lease(
        &default_paths.control_root,
        options.profile,
        crate::app::runtime_lease::RuntimeRole::Daemon,
    )
    .map_err(|error| error.to_string())?;
    println!(
        "[patinad] runtime lease acquired for profile {} as {:?}",
        runtime_lease.owner.profile, runtime_lease.owner.role
    );
    let storage_paths = storage::resolve(&roots, options.profile)?;
    let api_credentials = crate::engine::api::auth::ApiCredentialStore::new();
    api_credentials.initialize_at(&storage_paths.api_token_path, None)?;
    let sqlite_runtime = runtime.block_on(prepare_sqlite_runtime_at_path(
        storage_paths.db_path.clone(),
        storage_paths.database_creation_allowed,
    ))?;
    let requested_port = options
        .port_override
        .unwrap_or(crate::engine::api::server::DEFAULT_PORT);
    let api_server = if options.serve_minimal_api {
        Some(runtime.block_on(
            crate::engine::api::server::prepare_standalone_minimal_server(
                requested_port,
                api_credentials.clone(),
            ),
        )?)
    } else {
        None
    };
    let confirmed_port = api_server
        .as_ref()
        .map(|server| server.port())
        .unwrap_or(requested_port);
    let status = build_startup_status(
        env!("CARGO_PKG_VERSION"),
        options,
        &storage_paths,
        confirmed_port,
    );
    println!(
        "[{}] {} {} ({})",
        status.service_name, status.mode, status.version, status.stage
    );
    for note in &status.notes {
        println!("[{}] {note}", status.service_name);
    }
    if status.local_api_enabled {
        println!(
            "[{}] local API prepared at http://127.0.0.1:{}",
            status.service_name, status.local_api_port
        );
    } else {
        println!("[{}] local API disabled", status.service_name);
    }
    println!(
        "[{}] API token file {}",
        status.service_name,
        status.api_token_path.display()
    );
    println!(
        "[{}] data root {}",
        status.service_name,
        status.data_root.display()
    );
    println!("[{}] db {}", status.service_name, status.db_path.display());
    println!("[{}] sqlite ready", status.service_name);
    if let Some(server) = api_server {
        println!(
            "[{}] minimal API listening on http://127.0.0.1:{}",
            status.service_name,
            server.port()
        );
        runtime.block_on(server.run());
    }
    runtime.block_on(async move {
        sqlite_runtime.pool.close().await;
    });
    Ok(())
}

pub async fn prepare_sqlite_runtime_at_path(
    db_path: impl Into<PathBuf>,
    database_creation_allowed: bool,
) -> Result<DaemonSqliteRuntime, String> {
    let db_path = db_path.into();
    let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(
        &db_path,
        database_creation_allowed,
    )
    .await?;
    Ok(DaemonSqliteRuntime { db_path, pool })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_api_credentials() -> crate::engine::api::auth::ApiCredentialStore {
        let path = std::env::temp_dir().join(format!(
            "patina-daemon-api-token-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ));
        let credentials = crate::engine::api::auth::ApiCredentialStore::new();
        credentials
            .initialize_at(&path, Some("test-token"))
            .unwrap();
        credentials
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

        let runtime = prepare_sqlite_runtime_at_path(&db_path, true)
            .await
            .unwrap();

        assert_eq!(runtime.db_path, db_path);
        assert!(runtime.db_path.is_file());
        let connection = runtime.pool.acquire().await.unwrap();
        drop(connection);

        runtime.pool.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn daemon_sqlite_runtime_does_not_create_disallowed_database() {
        let root = std::env::temp_dir().join(format!(
            "patina-daemon-sqlite-closed-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let db_path = root.join("mounted/Patina/patina.db");

        let error = prepare_sqlite_runtime_at_path(&db_path, false)
            .await
            .unwrap_err();

        assert!(error.contains("failed to open sqlite db"));
        assert!(!db_path.exists());
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn daemon_minimal_api_routes_health_and_openapi_without_tauri_app_handle() {
        let health = crate::engine::api::router::route_minimal_request("GET", "/api/v1/health");
        assert_eq!(health.status, 200);
        assert_eq!(health.body["data"]["status"], "ok");
        assert_eq!(health.body["data"]["version"], env!("CARGO_PKG_VERSION"));

        let openapi =
            crate::engine::api::router::route_minimal_request("GET", "/api/v1/openapi.json");
        assert_eq!(openapi.status, 200);
        assert_eq!(openapi.body["openapi"], "3.1.0");

        let missing = crate::engine::api::router::route_minimal_request("GET", "/api/v1/current");
        assert_eq!(missing.status, 404);
    }

    #[test]
    fn daemon_run_options_enable_minimal_api_from_flag() {
        let options = DaemonRunOptions::from_args(["patinad", "--serve-api"]).unwrap();
        assert!(options.serve_minimal_api);

        let default_options = DaemonRunOptions::from_args(["patinad"]).unwrap();
        assert!(!default_options.serve_minimal_api);
    }

    #[test]
    fn daemon_acquires_lease_before_storage_and_sqlite() {
        let source = include_str!("mod.rs");
        let run = source
            .split("pub fn run_with_options")
            .nth(1)
            .expect("daemon run function");
        let lease = run
            .find("acquire_runtime_lease")
            .expect("runtime lease acquisition");
        let storage = run.find("storage::resolve").expect("storage resolution");
        let sqlite = run
            .find("prepare_sqlite_runtime_at_path")
            .expect("sqlite initialization");

        assert!(lease < storage);
        assert!(storage < sqlite);
    }

    #[tokio::test]
    async fn daemon_minimal_api_server_serves_health_without_tauri_app_handle() {
        let server = crate::engine::api::server::prepare_standalone_minimal_server(
            0,
            test_api_credentials(),
        )
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
        let server = crate::engine::api::server::prepare_standalone_minimal_server(
            0,
            test_api_credentials(),
        )
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
}
