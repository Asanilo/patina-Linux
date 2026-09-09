//! Opt-in acceptance against a real transient user service, never the product unit.
use std::cell::Cell;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use serde_json::{json, Value};

use crate::data::repositories::backup_restore::test_support as fixture;

mod remote;

struct TestUnit {
    name: String,
    stopped: Cell<bool>,
}

impl TestUnit {
    fn stop(&self) -> bool {
        if self.stopped.get() {
            return true;
        }
        let stopped = Command::new("timeout")
            .args(["20s", "systemctl", "--user", "stop", &self.name])
            .output()
            .is_ok_and(|output| output.status.success());
        self.stopped.set(stopped);
        stopped
    }

    fn pid(&self) -> String {
        let output = Command::new("timeout")
            .args([
                "5s",
                "systemctl",
                "--user",
                "show",
                &self.name,
                "-p",
                "MainPID",
                "--value",
            ])
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }
}

impl Drop for TestUnit {
    fn drop(&mut self) {
        // Keep evidence on failure; only stop the exact unit allocated by this test.
        if !self.stop() {
            eprintln!("test service cleanup needs inspection: {}", self.name);
        }
    }
}

async fn wait_json(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    matches: impl Fn(&Value) -> bool,
) -> Value {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(response) = client.get(url).bearer_auth(token).send().await {
                if response.status().is_success() {
                    if let Ok(bytes) = response.bytes().await {
                        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                            if matches(&value) {
                                return value;
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("isolated daemon did not reach expected state")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit PATINA_SYSTEMD_TEST_BINARY and a real user systemd manager"]
async fn real_systemd_restore_crosses_process_boundary() {
    run_restore_cases(false).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires explicit daemon binary, private D-Bus/keyring and real user systemd"]
async fn real_webdav_restore_crosses_private_credentials_and_systemd() {
    run_restore_cases(true).await;
}

#[tokio::test]
#[ignore = "private credential fixture worker; only invoked by the isolated parent test"]
async fn seed_private_webdav_credential() {
    remote::seed_credential().await;
}

async fn run_restore_cases(webdav: bool) {
    let binary = std::env::var("PATINA_SYSTEMD_TEST_BINARY")
        .expect("set an absolute path to the daemon binary under test");
    assert!(Path::new(&binary).is_absolute());
    let binary = fs::canonicalize(binary).unwrap();
    assert!(binary.is_file());
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();

    for (strategy, fail) in [("replace", false), ("merge", false), ("replace", true)] {
        let id = super::random_id("systemd_test").unwrap();
        let root = std::env::temp_dir().join(format!("patina-{id}"));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let control = root.join("config/Patina Dev");
        let data = root.join("data/Patina Dev");
        let staging = control.join("backup-restore-staging");
        let database = data.join("patina.db");
        let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(&database, true)
            .await
            .unwrap();
        crate::data::repositories::tracker_settings::save_tracking_paused_setting(&pool, true)
            .await
            .unwrap();
        let settings = [
            "audio_participation_enabled",
            "web_activity_enabled",
            "remote_status_bridge_enabled",
        ]
        .map(
            |key| crate::data::repositories::app_settings::AppSettingMutation {
                key: key.to_string(),
                value: "false".to_string(),
            },
        );
        crate::data::repositories::app_settings::commit_app_setting_mutations(&pool, &settings)
            .await
            .unwrap();
        fixture::seed_session(&pool, 1, "source", 1000).await;
        let archive = root.join("synthetic.zip");
        crate::data::backup::export_scheduled_backup_create_new(&pool, &archive)
            .await
            .unwrap();
        let archive_bytes = fs::read(&archive).unwrap();
        crate::data::maintenance::delete_tracking_data_before(&pool, 3000)
            .await
            .unwrap();
        fixture::seed_session(&pool, 2, "existing", 3000).await;
        if fail {
            fixture::inject_source_insert_failure(&pool).await;
        }
        pool.close().await;
        fs::create_dir_all(&staging).unwrap();
        let stage = if webdav {
            None
        } else {
            Some(crate::platform::backup_restore_staging::stage_file(&staging, &archive).unwrap())
        };
        let sentinel = staging.join("unrelated.txt");
        fs::write(&sentinel, b"keep").unwrap();
        fs::create_dir_all(root.join("home")).unwrap();
        fs::create_dir_all(root.join("runtime")).unwrap();
        fs::set_permissions(root.join("runtime"), fs::Permissions::from_mode(0o700)).unwrap();

        let remote = if webdav {
            Some(remote::RemoteFixture::start(&root, &archive).await)
        } else {
            None
        };

        // A racing bind must fail the test, never fall back to a production endpoint.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let unit = TestUnit {
            name: format!("patina-restore-{id}.service"),
            stopped: Cell::new(false),
        };
        let mut command = Command::new("timeout");
        command.args([
            "15s",
            "systemd-run",
            "--user",
            "--quiet",
            "--unit",
            &unit.name,
            "--property=Restart=on-failure",
            "--property=RestartSec=200ms",
            "--property=RuntimeMaxSec=120s",
            "--property=StartLimitIntervalSec=600s",
            "--property=StartLimitBurst=3",
            "--property=KillSignal=SIGINT",
            "--property=TimeoutStopSec=15s",
            "--property=UMask=0077",
            "--property=NoNewPrivileges=yes",
            "--setenv=PATINA_SYSTEMD_SERVICE=patinad.service",
            "--setenv=XDG_SESSION_TYPE=wayland",
            "--setenv=XDG_CURRENT_DESKTOP=isolated-test",
            "--setenv=DISPLAY=",
            "--setenv=WAYLAND_DISPLAY=",
        ]);
        for (name, path) in [
            ("HOME", root.join("home")),
            ("XDG_CONFIG_HOME", root.join("config")),
            ("XDG_DATA_HOME", root.join("data")),
            ("XDG_CACHE_HOME", root.join("cache")),
            ("XDG_RUNTIME_DIR", root.join("runtime")),
        ] {
            command.arg(format!("--setenv={name}={}", path.display()));
        }
        let bus = remote
            .as_ref()
            .map(|fixture| fixture.bus.clone())
            .unwrap_or_else(|| format!("unix:path={}/no-session-bus", root.display()));
        command.arg(format!("--setenv=DBUS_SESSION_BUS_ADDRESS={bus}"));
        command.arg("--").arg(&binary).args([
            "--profile",
            "dev",
            "--serve-api",
            "--track",
            "--port",
            &port.to_string(),
        ]);
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let token_path = data.join("api_token");
        tokio::time::timeout(Duration::from_secs(10), async {
            while !token_path.is_file() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap();
        let token = fs::read_to_string(token_path).unwrap();
        let base = format!("http://127.0.0.1:{port}/api/v1");
        let capabilities = wait_json(
            &client,
            &format!("{base}/capabilities"),
            token.trim(),
            |v| v["data"]["tracking"]["ready"] == true,
        )
        .await;
        assert_eq!(
            capabilities["data"]["server_version"],
            env!("CARGO_PKG_VERSION")
        );
        let before_pid = unit.pid();
        assert_ne!(before_pid, "0");
        let (endpoint, body) = if let Some(remote) = &remote {
            let config = json!({"url": remote.url, "username": "user", "remoteDir": "/Patina"});
            let list = client
                .post(format!("{base}/backups/remote/list"))
                .bearer_auth(token.trim())
                .header("content-type", "application/json")
                .body(serde_json::to_vec(&json!({"config": config})).unwrap())
                .send()
                .await
                .unwrap();
            assert_eq!(list.status(), 200);
            let listed: Value = serde_json::from_slice(&list.bytes().await.unwrap()).unwrap();
            assert_eq!(listed["data"][0]["id"], "remote-source");
            (
                "backups/remote/restore",
                json!({"config": config, "id": "remote-source", "strategy": strategy, "confirmed": true}),
            )
        } else {
            let stage = stage.as_ref().unwrap();
            (
                "backups/restore",
                json!({"ticket": stage.ticket, "expected_sha256": stage.sha256,
                "expected_size_bytes": stage.size_bytes, "strategy": strategy, "confirmed": true}),
            )
        };
        let response = client
            .post(format!("{base}/{endpoint}"))
            .bearer_auth(token.trim())
            .header("content-type", "application/json")
            .body(serde_json::to_vec(&body).unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 202);
        let scheduled: Value = serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
        let request_id = scheduled["data"]["restore"]["request_id"].as_str().unwrap();
        let terminal = if fail { "failed" } else { "completed" };
        let restored = wait_json(
            &client,
            &format!("{base}/backups/restore?request_id={request_id}"),
            token.trim(),
            |v| v["data"]["status"] == terminal,
        )
        .await;
        assert_eq!(restored["data"]["request_id"], request_id);
        let after_pid = unit.pid();
        assert_ne!(before_pid, after_pid);
        assert_ne!(after_pid, "0");
        assert!(unit.stop(), "failed to stop isolated service");

        let pool = crate::data::sqlite_pool::open_prepared_sqlite_pool_at_path(&database, false)
            .await
            .unwrap();
        let names = fixture::session_names(&pool).await;
        let expected = if fail {
            vec!["existing"]
        } else if strategy == "merge" {
            vec!["existing", "source"]
        } else {
            vec!["source"]
        };
        assert_eq!(names, expected);
        assert_eq!(
            fixture::receipt_count(&pool).await,
            if fail { 0 } else { 1 }
        );
        fixture::assert_integrity(&pool).await;
        pool.close().await;
        assert_eq!(fs::read(&archive).unwrap(), archive_bytes);
        assert_eq!(fs::read(&sentinel).unwrap(), b"keep");
        let reservation = super::read_reservation(&control).unwrap().unwrap();
        assert_eq!(
            staging.join(format!("{}.zip", reservation.ticket)).exists(),
            fail
        );
        if let Some(remote) = &remote {
            remote.assert_downloaded();
        }
        let evidence = json!({
            "format": "patina.isolated-systemd-restore.v1",
            "daemon_version": capabilities["data"]["server_version"],
            "unit": unit.name, "strategy": strategy, "injected_failure": fail,
            "before_pid": before_pid, "after_pid": after_pid,
            "restore": restored["data"], "sessions": names,
            "database_integrity": "ok", "service_stopped": unit.stopped.get(),
            "source_and_unrelated_file_preserved": true,
            "remote_credentials_and_download": webdav,
        });
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.join("evidence.json"))
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(&evidence).unwrap())
            .unwrap();
        file.sync_all().unwrap();
        eprintln!("isolated systemd restore passed: webdav={webdav}, strategy={strategy}, injected_failure={fail}, pid={before_pid}->{after_pid}, evidence={}", root.display());
    }
}
