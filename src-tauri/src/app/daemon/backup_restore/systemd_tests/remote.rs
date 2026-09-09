use axum::{
    body::Body,
    http::{HeaderMap, Method, Uri},
    response::Response,
    Router,
};
use serde_json::json;
use std::fs;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

pub(super) struct RemoteFixture {
    pub bus: String,
    pub url: String,
    children: Vec<Child>,
    server: Option<tokio::task::JoinHandle<()>>,
    downloads: Arc<AtomicUsize>,
}

fn private_environment(command: &mut Command, root: &Path, bus: &str) {
    command
        .env("DBUS_SESSION_BUS_ADDRESS", bus)
        .env_remove("GNOME_KEYRING_CONTROL")
        .env_remove("GNOME_KEYRING_PID");
    for (key, dir) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ] {
        command.env(key, root.join(dir));
    }
}

impl Drop for RemoteFixture {
    fn drop(&mut self) {
        if let Some(task) = self.server.take() {
            task.abort();
        }
        for child in self.children.iter_mut().rev() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl RemoteFixture {
    pub async fn start(root: &Path, archive: &Path) -> Self {
        let bus = format!("unix:path={}/secret-bus", root.display());
        let mut fixture = Self {
            bus,
            url: String::new(),
            children: Vec::new(),
            server: None,
            downloads: Arc::new(AtomicUsize::new(0)),
        };
        let mut command = Command::new("dbus-daemon");
        command
            .args(["--session", "--nofork"])
            .arg(format!("--address={}", fixture.bus))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        private_environment(&mut command, root, &fixture.bus);
        fixture
            .children
            .push(command.spawn().expect("start private D-Bus"));
        tokio::time::timeout(Duration::from_secs(5), async {
            while !root.join("secret-bus").exists() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .unwrap();
        let mut command = Command::new("gnome-keyring-daemon");
        command
            .args([
                "--foreground",
                "--components=secrets",
                "--unlock",
                "--control-directory",
            ])
            .arg(root.join("keyring-control"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        private_environment(&mut command, root, &fixture.bus);
        fixture
            .children
            .push(command.spawn().expect("start private keyring"));
        fixture
            .children
            .last_mut()
            .unwrap()
            .stdin
            .take()
            .unwrap()
            .write_all(b"isolated-fixture-keyring-password")
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let output = Command::new("timeout")
                    .args([
                        "2s",
                        "gdbus",
                        "call",
                        "--address",
                        &fixture.bus,
                        "--dest",
                        "org.freedesktop.DBus",
                        "--object-path",
                        "/org/freedesktop/DBus",
                        "--method",
                        "org.freedesktop.DBus.NameHasOwner",
                        "org.freedesktop.secrets",
                    ])
                    .output()
                    .unwrap();
                if output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains("true")
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .expect("private Secret Service did not start");

        // Only this child sees the private bus; the parent keeps its real systemd connection.
        let mut command = Command::new("timeout");
        command
            .arg("20s")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "app::daemon::backup_restore::systemd_tests::seed_private_webdav_credential",
                "--ignored",
            ])
            .env("PATINA_PRIVATE_CREDENTIAL_ROOT", root);
        private_environment(&mut command, root, &fixture.bus);
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "private credential worker failed: {}",
            String::from_utf8_lossy(&output.stdout)
        );

        let bytes = fs::read(archive).unwrap();
        let (preview, _, size) = crate::data::backup::inspect_restore_archive(archive).unwrap();
        let index = json!({"version": 1, "product": "Patina", "updatedAtMs": 1, "backups": [{
            "id": "remote-source", "fileName": "Patina-backup-remote-source.zip",
            "remotePath": "/Patina/Patina-backup-remote-source.zip", "createdAtMs": 1,
            "sizeBytes": size, "appVersion": preview.app_version, "backupVersion": preview.version,
            "schemaVersion": preview.schema_version, "sessionCount": preview.session_count,
            "titleSampleCount": preview.title_sample_count, "settingCount": preview.setting_count,
            "iconCacheCount": preview.icon_cache_count
        }]})
        .to_string();
        let count = fixture.downloads.clone();
        let router = Router::new().fallback(move |method: Method, uri: Uri, headers: HeaderMap| {
            let bytes = bytes.clone();
            let index = index.clone();
            let count = count.clone();
            async move {
                assert_eq!(method, Method::GET);
                assert_eq!(
                    headers.get("authorization").unwrap(),
                    "Basic dXNlcjpwYXNzd29yZA=="
                );
                let body = match uri.path() {
                    "/Patina/backup-index.json" => index.into_bytes(),
                    "/Patina/Patina-backup-remote-source.zip" => {
                        count.fetch_add(1, Ordering::SeqCst);
                        bytes
                    }
                    _ => panic!("unexpected isolated WebDAV request"),
                };
                Response::new(Body::from(body))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        fixture.url = format!("http://{}", listener.local_addr().unwrap());
        fixture.server = Some(tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        }));
        fixture
    }

    pub fn assert_downloaded(&self) {
        assert_eq!(self.downloads.load(Ordering::SeqCst), 1);
    }
}

pub(super) async fn seed_credential() {
    let root = fs::canonicalize(
        std::env::var("PATINA_PRIVATE_CREDENTIAL_ROOT").expect("private worker only"),
    )
    .unwrap();
    assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("patina-systemd_test_"));
    let socket = fs::symlink_metadata(root.join("secret-bus")).unwrap();
    assert!(socket.file_type().is_socket());
    assert_eq!(
        std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(),
        format!("unix:path={}/secret-bus", root.display())
    );
    for (key, dir) in [
        ("HOME", "home"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ] {
        assert_eq!(std::env::var_os(key).unwrap(), root.join(dir).as_os_str());
    }
    let profile = crate::platform::app_paths::AppProfile::Dev;
    crate::platform::credentials::save_webdav_backup_password(profile, "user", "password")
        .await
        .unwrap();
    assert_eq!(
        crate::platform::credentials::read_webdav_backup_password(profile)
            .await
            .unwrap()
            .as_deref(),
        Some("password")
    );
}
