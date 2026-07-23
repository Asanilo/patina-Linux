#[cfg(target_os = "linux")]
mod audio;
mod control;
#[cfg(target_os = "linux")]
mod media;
#[cfg(target_os = "linux")]
mod power;
mod restart;
mod tracking;
mod web_activity;

use super::DaemonSqliteRuntime;
use crate::app::runtime_lease::RuntimeLease;
use crate::engine::api::server::ApiServerHandle;
use crate::engine::runtime_event::RuntimeEventHub;
pub(super) use control::DaemonApiRuntimeControl;
use std::sync::Arc;
use tracking::DaemonTrackingTasks;
pub(super) use web_activity::DaemonWebActivityControl;
use web_activity::DaemonWebActivityTask;

#[cfg(target_os = "linux")]
use audio::DaemonAudioTask;
#[cfg(target_os = "linux")]
use media::DaemonMediaTask;
#[cfg(target_os = "linux")]
use power::DaemonPowerTask;

pub struct DaemonBackgroundTasks {
    browser_activity: DaemonWebActivityTask,
    #[cfg(target_os = "linux")]
    audio: DaemonAudioTask,
    #[cfg(target_os = "linux")]
    media: DaemonMediaTask,
    #[cfg(target_os = "linux")]
    power: DaemonPowerTask,
    tracking: DaemonTrackingTasks,
}

impl DaemonBackgroundTasks {
    pub async fn start(
        context: crate::engine::runtime_context::RuntimeContext,
        snapshot: Arc<crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState>,
        web_activity: DaemonWebActivityControl,
        event_hub: Arc<crate::engine::runtime_event::RuntimeEventHub>,
        #[cfg(target_os = "linux")] audio_source: crate::platform::linux::audio::AudioSignalSource,
    ) -> Self {
        let event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink> = event_hub.clone();
        let browser_activity = DaemonWebActivityTask::start(web_activity, event_hub).await;
        #[cfg(target_os = "linux")]
        let audio = DaemonAudioTask::start(audio_source.clone());
        #[cfg(target_os = "linux")]
        let media_source = crate::platform::linux::media::MediaSignalSource::new();
        #[cfg(target_os = "linux")]
        let media = DaemonMediaTask::start(media_source.clone());
        #[cfg(target_os = "linux")]
        let power = DaemonPowerTask::start(context.clone(), event_sink.clone());
        let tracking = DaemonTrackingTasks::start(
            context,
            snapshot,
            event_sink,
            #[cfg(target_os = "linux")]
            audio_source,
            #[cfg(target_os = "linux")]
            media_source,
        );
        Self {
            browser_activity,
            #[cfg(target_os = "linux")]
            audio,
            #[cfg(target_os = "linux")]
            media,
            #[cfg(target_os = "linux")]
            power,
            tracking,
        }
    }

    async fn shutdown(self) {
        self.browser_activity.shutdown().await;
        #[cfg(target_os = "linux")]
        self.power.shutdown().await;
        #[cfg(target_os = "linux")]
        self.media.shutdown().await;
        #[cfg(target_os = "linux")]
        self.audio.shutdown().await;
        self.tracking.shutdown().await;
    }
}

pub struct DaemonRuntime {
    api_server: Option<ApiServerHandle>,
    event_hub: Option<Arc<RuntimeEventHub>>,
    background_tasks: Option<DaemonBackgroundTasks>,
    sqlite: Option<DaemonSqliteRuntime>,
    lease: Option<RuntimeLease>,
}

impl DaemonRuntime {
    pub fn new(
        api_server: Option<ApiServerHandle>,
        event_hub: Arc<RuntimeEventHub>,
        background_tasks: Option<DaemonBackgroundTasks>,
        sqlite: DaemonSqliteRuntime,
        lease: RuntimeLease,
    ) -> Self {
        Self {
            api_server,
            event_hub: Some(event_hub),
            background_tasks,
            sqlite: Some(sqlite),
            lease: Some(lease),
        }
    }

    pub async fn shutdown(mut self) {
        if let Some(tasks) = self.background_tasks.take() {
            tasks.shutdown().await;
        }
        if let Some(event_hub) = self.event_hub.as_ref() {
            event_hub.shutdown();
        }
        if let Some(server) = self.api_server.take() {
            server.shutdown().await;
        }
        drop(self.event_hub.take());
        if let Some(sqlite) = self.sqlite.take() {
            sqlite.pool.close().await;
        }
        drop(self.lease.take());
    }

    pub async fn wait_for_api_stop(&mut self) {
        if let Some(server) = self.api_server.as_mut() {
            server.wait_until_stopped().await;
        } else {
            std::future::pending::<()>().await;
        }
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
        let event_hub = Arc::new(RuntimeEventHub::new(
            crate::engine::runtime_event::DEFAULT_EVENT_REPLAY_CAPACITY,
        ));
        let server = crate::engine::api::server::prepare_standalone_server_with_events(
            0,
            credentials(&root.join("data/Patina Dev/api_token")),
            context,
            crate::engine::api::surface::ApiSurface::DaemonReadOnly,
            event_hub.clone(),
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

        DaemonRuntime::new(Some(handle), event_hub, None, sqlite, lease)
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

        let event_hub = Arc::new(RuntimeEventHub::new(
            crate::engine::runtime_event::DEFAULT_EVENT_REPLAY_CAPACITY,
        ));
        let available_listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let browser_port = available_listener.local_addr().unwrap().port();
        drop(available_listener);
        crate::data::repositories::app_settings::commit_app_setting_mutations(
            &sqlite.pool,
            &[
                crate::data::repositories::app_settings::AppSettingMutation {
                    key: "web_activity_enabled".into(),
                    value: "1".into(),
                },
                crate::data::repositories::app_settings::AppSettingMutation {
                    key: "web_activity_port".into(),
                    value: browser_port.to_string(),
                },
                crate::data::repositories::app_settings::AppSettingMutation {
                    key: "web_activity_token".into(),
                    value: "browser-test-token".into(),
                },
            ],
        )
        .await
        .unwrap();
        let web_activity_state =
            Arc::new(crate::engine::web_activity::WebActivityRuntimeState::default());
        let runtime_context =
            crate::engine::runtime_context::RuntimeContext::system(sqlite.pool.clone());
        let tracking_snapshot = Arc::new(
            crate::engine::tracking::runtime_snapshot::TrackingRuntimeSnapshotState::default(),
        );
        let event_sink: Arc<dyn crate::engine::runtime_event::RuntimeEventSink> = event_hub.clone();
        let web_activity = DaemonWebActivityControl::new(
            runtime_context.clone(),
            tracking_snapshot.clone(),
            web_activity_state.clone(),
            event_sink,
        );
        let background_tasks = DaemonBackgroundTasks::start(
            runtime_context,
            tracking_snapshot,
            web_activity,
            event_hub.clone(),
            #[cfg(target_os = "linux")]
            crate::platform::linux::audio::AudioSignalSource::new(false),
        )
        .await;
        assert!(
            web_activity_state
                .snapshot(
                    &crate::domain::settings::WebActivitySettings::default(),
                    crate::app::runtime::now_ms() as i64,
                )
                .listening
        );
        let mut stalled = tokio::net::TcpStream::connect(("127.0.0.1", browser_port))
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::write_all(
            &mut stalled,
            b"POST /web-activity HTTP/1.1\r\nAuthorization:",
        )
        .await
        .unwrap();
        DaemonRuntime::new(None, event_hub, Some(background_tasks), sqlite, lease)
            .shutdown()
            .await;
        assert!(
            !web_activity_state
                .snapshot(
                    &crate::domain::settings::WebActivitySettings::default(),
                    crate::app::runtime::now_ms() as i64,
                )
                .listening
        );

        let rebound = tokio::net::TcpListener::bind(("127.0.0.1", browser_port))
            .await
            .unwrap();
        drop(rebound);

        let next_lease =
            acquire_runtime_lease(&control_root, AppProfile::Dev, RuntimeRole::Desktop).unwrap();
        drop(next_lease);
        std::fs::remove_dir_all(root).unwrap();
    }
}
