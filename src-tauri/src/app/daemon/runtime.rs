use super::DaemonSqliteRuntime;
use crate::app::runtime_lease::RuntimeLease;
use crate::engine::api::server::ApiServerHandle;

pub struct DaemonRuntime {
    api_server: Option<ApiServerHandle>,
    sqlite: Option<DaemonSqliteRuntime>,
    lease: Option<RuntimeLease>,
}

impl DaemonRuntime {
    pub fn new(
        api_server: Option<ApiServerHandle>,
        sqlite: DaemonSqliteRuntime,
        lease: RuntimeLease,
    ) -> Self {
        Self {
            api_server,
            sqlite: Some(sqlite),
            lease: Some(lease),
        }
    }

    pub async fn shutdown(mut self) {
        if let Some(server) = self.api_server.take() {
            server.shutdown().await;
        }
        if let Some(sqlite) = self.sqlite.take() {
            sqlite.pool.close().await;
        }
        drop(self.lease.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::daemon::prepare_sqlite_runtime_at_path;
    use crate::app::runtime_lease::{acquire_runtime_lease, RuntimeRole};
    use crate::platform::app_paths::AppProfile;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "patina-daemon-runtime-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn credentials(path: &std::path::Path) -> crate::engine::api::auth::ApiCredentialStore {
        let credentials = crate::engine::api::auth::ApiCredentialStore::new();
        credentials
            .initialize_at(path, Some("runtime-test-token"))
            .unwrap();
        credentials
    }

    #[tokio::test]
    async fn graceful_shutdown_releases_listener_pool_and_lease() {
        let root = temp_root("full");
        let control_root = root.join("config/Patina Dev");
        let db_path = root.join("data/Patina Dev/patina.db");
        let lease =
            acquire_runtime_lease(&control_root, AppProfile::Dev, RuntimeRole::Daemon).unwrap();
        let sqlite = prepare_sqlite_runtime_at_path(&db_path, true)
            .await
            .unwrap();
        let context = crate::engine::api::context::ApiRuntimeContext::new(
            crate::engine::runtime_context::RuntimeContext::system(sqlite.pool.clone()),
        );
        let server = crate::engine::api::server::prepare_standalone_server(
            0,
            credentials(&root.join("data/Patina Dev/api_token")),
            context,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
        )
        .await
        .unwrap();
        let port = server.port();
        let handle = server.start();
        let mut stalled = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::write_all(
            &mut stalled,
            b"GET /api/v1/health HTTP/1.1\r\nAuthorization:",
        )
        .await
        .unwrap();

        DaemonRuntime::new(Some(handle), sqlite, lease)
            .shutdown()
            .await;

        let rebound = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .unwrap();
        drop(rebound);
        let reopened = prepare_sqlite_runtime_at_path(&db_path, false)
            .await
            .unwrap();
        reopened.pool.close().await;
        let next_lease =
            acquire_runtime_lease(&control_root, AppProfile::Dev, RuntimeRole::Desktop).unwrap();
        drop(next_lease);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn shutdown_is_safe_when_api_was_not_started() {
        let root = temp_root("no-api");
        let control_root = root.join("config/Patina Dev");
        let db_path = root.join("data/Patina Dev/patina.db");
        let lease =
            acquire_runtime_lease(&control_root, AppProfile::Dev, RuntimeRole::Daemon).unwrap();
        let sqlite = prepare_sqlite_runtime_at_path(&db_path, true)
            .await
            .unwrap();

        DaemonRuntime::new(None, sqlite, lease).shutdown().await;

        let next_lease =
            acquire_runtime_lease(&control_root, AppProfile::Dev, RuntimeRole::Desktop).unwrap();
        drop(next_lease);
        std::fs::remove_dir_all(root).unwrap();
    }
}
