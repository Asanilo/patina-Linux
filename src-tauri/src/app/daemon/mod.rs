pub(crate) mod activity_import;
mod api_runtime;
mod options;
mod runtime;
mod service_lifecycle;
mod status;
mod storage;

use std::path::PathBuf;

use sqlx::{Pool, Sqlite};
use std::sync::Arc;

pub use options::DaemonRunOptions;
pub use runtime::DaemonRuntime;
pub use status::DaemonStartupStatus;

pub const CONTROLLED_RESTART_EXIT_CODE: i32 = 75;
const CONTROLLED_RESTART_ERROR_PREFIX: &str = "controlled service restart requested";

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
        options.serve_api,
        options.track,
        local_api_port,
        storage_paths,
    )
}

pub fn run(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<(), String> {
    run_with_options(DaemonRunOptions::from_args(args)?)
}

pub fn is_controlled_restart_error(error: &str) -> bool {
    error.starts_with(CONTROLLED_RESTART_ERROR_PREFIX)
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
    let service_lifecycle = Arc::new(
        service_lifecycle::DaemonServiceLifecycleOwner::from_environment(
            &storage_paths.control_root,
            crate::app::runtime::now_ms().min(i64::MAX as u64) as i64,
        )?,
    );
    let sqlite_runtime = runtime.block_on(prepare_sqlite_runtime_at_path(
        storage_paths.db_path.clone(),
        storage_paths.database_creation_allowed,
    ))?;
    let stored_local_api = runtime
        .block_on(
            crate::data::repositories::app_settings::load_local_api_settings(&sqlite_runtime.pool),
        )
        .map_err(|error| format!("failed to load local API settings: {error}"))?;
    let api_credentials = crate::engine::api::auth::ApiCredentialStore::new();
    api_credentials.initialize_at(&storage_paths.api_token_path, Some(&stored_local_api.token))?;
    if !stored_local_api.token.trim().is_empty() {
        runtime.block_on(
            crate::data::repositories::app_settings::delete_legacy_local_api_token(
                &sqlite_runtime.pool,
            ),
        )?;
    }
    let requested_port = options.port_override.unwrap_or(stored_local_api.port);
    let event_hub = Arc::new(crate::engine::runtime_event::RuntimeEventHub::new(
        crate::engine::runtime_event::DEFAULT_EVENT_REPLAY_CAPACITY,
    ));
    let tracking_snapshot = options.track.then(|| {
        Arc::new(crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState::default())
    });
    let web_activity_state = options
        .track
        .then(|| Arc::new(crate::engine::web_activity::WebActivityRuntimeState::default()));
    let runtime_context =
        crate::engine::runtime_context::RuntimeContext::system(sqlite_runtime.pool.clone());
    let event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink> = event_hub.clone();
    let tools_ready = options
        .track
        .then(|| Arc::new(std::sync::atomic::AtomicBool::new(false)));
    let tools_owner = tools_ready.as_ref().map(|ready| {
        let sink: Arc<dyn crate::engine::tools::ToolsRuntimeSink> = Arc::new(
            runtime::DaemonToolsRuntimeSink::new(event_sink.clone(), ready.clone()),
        );
        crate::engine::tools::ToolsRuntimeOwner::new(runtime_context.clone(), sink)
    });
    let web_activity_control = match (tracking_snapshot.as_ref(), web_activity_state.as_ref()) {
        (Some(snapshot), Some(state)) => Some(runtime::DaemonWebActivityControl::new(
            runtime_context.clone(),
            snapshot.clone(),
            state.clone(),
            event_sink.clone(),
        )),
        _ => None,
    };
    #[cfg(target_os = "linux")]
    let audio_source = if options.track {
        let enabled = runtime
            .block_on(
                crate::data::repositories::app_settings::load_audio_participation_enabled(
                    runtime_context.pool(),
                ),
            )
            .unwrap_or_else(|error| {
                eprintln!("[patinad] failed to load audio participation setting: {error}");
                crate::domain::settings::DEFAULT_AUDIO_PARTICIPATION_ENABLED
            });
        Some(crate::platform::linux::audio::AudioSignalSource::new(
            enabled,
        ))
    } else {
        None
    };
    let api_surface = if options.track {
        crate::engine::api::surface::ApiSurface::DaemonTracking
    } else {
        crate::engine::api::surface::ApiSurface::DaemonReadOnly
    };
    let api_listener = options.serve_api.then(|| {
        Arc::new(
            crate::engine::api::listener_owner::LocalApiListenerOwner::new(
                api_credentials.clone(),
                api_surface,
                event_hub.clone(),
            ),
        )
    });
    let api_runtime_control = match (web_activity_control.as_ref(), api_listener.as_ref()) {
        (Some(web_activity), Some(api_listener)) => Some(Arc::new(
            runtime::DaemonApiRuntimeControl::new(
                runtime_context.clone(),
                web_activity.clone(),
                event_sink.clone(),
                api_listener.clone(),
                api_credentials.clone(),
                service_lifecycle.clone(),
                #[cfg(target_os = "linux")]
                audio_source
                    .as_ref()
                    .expect("tracking daemon audio source")
                    .clone(),
            ),
        )
            as Arc<dyn crate::engine::api::runtime_control::ApiRuntimeControl>),
        _ => None,
    };
    let activity_import_owner = options.track.then(|| {
        Arc::new(activity_import::DaemonActivityImportOwner::new(
            runtime_context.clone(),
            storage_paths.activity_import_staging_dir.clone(),
        )) as Arc<dyn crate::engine::api::activity_import_owner::ActivityImportOwner>
    });
    let confirmed_port = if let Some(api_listener) = api_listener.as_ref() {
        let mut context = api_runtime::build_context(
            runtime_context.clone(),
            tracking_snapshot.clone(),
            web_activity_state.clone(),
            tools_ready.clone(),
            event_hub.clone(),
            api_runtime_control,
            tools_owner.clone().map(Arc::new),
        );
        if let Some(activity_import_owner) = activity_import_owner {
            context = context.with_activity_import_owner(activity_import_owner);
        }
        runtime.block_on(api_listener.start(requested_port, context))?
    } else {
        requested_port
    };
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
    if options.serve_api {
        println!(
            "[{}] local API listening on http://127.0.0.1:{}",
            status.service_name, confirmed_port
        );
    }
    let background_tasks = runtime.block_on(async {
        match (
            tracking_snapshot,
            tools_owner,
            tools_ready,
            web_activity_control,
        ) {
            (Some(snapshot), Some(tools_owner), Some(tools_ready), Some(web_activity)) => Some(
                runtime::DaemonBackgroundTasks::start(
                    runtime_context,
                    snapshot,
                    tools_owner,
                    tools_ready,
                    web_activity,
                    event_hub.clone(),
                    #[cfg(target_os = "linux")]
                    audio_source.expect("tracking daemon audio source"),
                )
                .await,
            ),
            _ => None,
        }
    });
    let mut daemon_runtime = DaemonRuntime::new(
        api_listener,
        event_hub,
        background_tasks,
        sqlite_runtime,
        runtime_lease,
    );
    runtime.block_on(async move {
        if options.serve_api {
            tokio::select! {
                result = tokio::signal::ctrl_c() => {
                    result.map_err(|error| format!("failed to wait for shutdown signal: {error}"))?;
                }
                _ = daemon_runtime.wait_for_api_stop() => {
                    daemon_runtime.shutdown().await;
                    return Err("local API task stopped unexpectedly".to_string());
                }
                request_id = service_lifecycle.wait_for_restart_request() => {
                    daemon_runtime.shutdown().await;
                    return Err(format!("{CONTROLLED_RESTART_ERROR_PREFIX}: {request_id}"));
                }
            }
        }
        daemon_runtime.shutdown().await;
        Ok::<(), String>(())
    })?;
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

    fn test_event_hub() -> Arc<crate::engine::runtime_event::RuntimeEventHub> {
        Arc::new(crate::engine::runtime_event::RuntimeEventHub::new(8))
    }

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

    async fn test_api_context() -> crate::engine::api::context::ApiRuntimeContext {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        crate::engine::api::context::ApiRuntimeContext::new(
            crate::engine::runtime_context::RuntimeContext::system(pool),
        )
    }

    fn request(method: &str, path: &str) -> crate::engine::api::router::ApiRequest {
        crate::engine::api::router::ApiRequest {
            method: method.to_string(),
            path: path.to_string(),
            query: None,
            body: Vec::new(),
        }
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

    #[tokio::test]
    async fn daemon_read_only_api_routes_without_tauri_app_handle() {
        let context = test_api_context().await;
        let surface = crate::engine::api::surface::ApiSurface::DaemonReadOnly;
        let health = crate::engine::api::router::route_request(
            request("GET", "/api/v1/health"),
            &context,
            surface,
        )
        .await;
        assert_eq!(health.status, 200);
        assert_eq!(health.body["data"]["status"], "ok");
        assert_eq!(health.body["data"]["version"], env!("CARGO_PKG_VERSION"));

        let openapi = crate::engine::api::router::route_request(
            request("GET", "/api/v1/openapi.json"),
            &context,
            surface,
        )
        .await;
        assert_eq!(openapi.status, 200);
        assert_eq!(openapi.body["openapi"], "3.1.0");

        let current = crate::engine::api::router::route_request(
            request("GET", "/api/v1/current"),
            &context,
            surface,
        )
        .await;
        assert_eq!(current.status, 503);

        let rejected_write = crate::engine::api::router::route_request(
            request("POST", "/api/v1/apps/ghostty/rename"),
            &context,
            surface,
        )
        .await;
        assert_eq!(rejected_write.status, 404);
    }

    #[test]
    fn daemon_run_options_enable_api_from_flag() {
        let options = DaemonRunOptions::from_args(["patinad", "--serve-api"]).unwrap();
        assert!(options.serve_api);

        let default_options = DaemonRunOptions::from_args(["patinad"]).unwrap();
        assert!(!default_options.serve_api);
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
    async fn daemon_read_only_api_server_serves_health_without_tauri_app_handle() {
        let server = crate::engine::api::server::prepare_standalone_server_with_events(
            0,
            test_api_credentials(),
            test_api_context().await,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
            test_event_hub(),
        )
        .await
        .unwrap();
        let port = server.port();
        let shutdown = server.shutdown_handle();
        let task = tokio::spawn(server.run());

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let request = "GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer test-token\r\nConnection: close\r\n\r\n";
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
    async fn daemon_read_only_api_server_rejects_missing_token() {
        let server = crate::engine::api::server::prepare_standalone_server_with_events(
            0,
            test_api_credentials(),
            test_api_context().await,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
            test_event_hub(),
        )
        .await
        .unwrap();
        let port = server.port();
        let shutdown = server.shutdown_handle();
        let task = tokio::spawn(server.run());

        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let request = "GET /api/v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
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
