//! Private D-Bus service fixture, not a real systemd manager.
//! Only its own independently built daemon child can be started or stopped.

use std::fs::{self, OpenOptions};
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::zvariant::OwnedObjectPath;

mod systemd;

const BUS_NAME: &str = "org.freedesktop.systemd1";
const UNIT_NAME: &str = "patinad.service";
const MANAGER_PATH: &str = "/org/freedesktop/systemd1";
const UNIT_PATH: &str = "/org/freedesktop/systemd1/unit/patinad_2eservice";
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Serialize)]
pub(super) struct Snapshot {
    pub pid: Option<u32>,
    pub start_count: u64,
    pub stop_count: u64,
}

#[derive(Default)]
struct ProcessState {
    child: Option<Child>,
    start_count: u64,
    stop_count: u64,
    closing: bool,
}

impl ProcessState {
    fn reap(&mut self) -> Result<(), String> {
        if let Some(child) = self.child.as_mut() {
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                self.child.take();
            }
        }
        Ok(())
    }

    fn kill_owned_child(&mut self) {
        self.closing = true;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ProcessState {
    fn drop(&mut self) {
        self.kill_owned_child();
    }
}

struct Controller {
    root: PathBuf,
    binary: PathBuf,
    systemd: Option<systemd::TestUnit>,
    state: Mutex<ProcessState>,
    operations: tokio::sync::Mutex<()>,
}

impl Controller {
    async fn start(&self) -> Result<(), String> {
        let _operation = self.operations.lock().await;
        let mut state = self.state.lock().unwrap();
        if state.closing {
            return Err("the private service fixture has been closed".into());
        }
        if let Some(unit) = &self.systemd {
            if unit.pid()?.is_some() {
                return Ok(());
            }
            unit.start(&self.root, &self.binary, state.start_count + 1)?;
            state.start_count += 1;
            return Ok(());
        }
        state.reap()?;
        if state.child.is_some() {
            return Ok(());
        }
        let number = state.start_count + 1;
        let log = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(self.root.join(format!("daemon-{number}.log")))
            .map_err(|error| error.to_string())?;
        let mut command = Command::new(&self.binary);
        command
            .args(["--profile", "production", "--serve-api", "--track"])
            .current_dir(&self.root)
            .env_clear()
            .env("LANG", "C.UTF-8")
            .env("XDG_SESSION_TYPE", "wayland")
            .env("XDG_CURRENT_DESKTOP", "isolated-storage-test")
            .env("DISPLAY", "")
            .env("WAYLAND_DISPLAY", "")
            .env("PATINA_SYSTEMD_SERVICE", UNIT_NAME)
            // This test supervisor implements only start/stop. The marker lets
            // the real daemon expose the service capability used by Desktop.
            .env("INVOCATION_ID", format!("storage-test-{number}"))
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                log.try_clone().map_err(|error| error.to_string())?,
            ))
            .stderr(Stdio::from(log));
        for key in [
            "PATH",
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
            "DBUS_SYSTEM_BUS_ADDRESS",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let child = command.spawn().map_err(|error| error.to_string())?;
        state.child = Some(child);
        state.start_count = number;
        Ok(())
    }

    async fn stop(&self) -> Result<(), String> {
        let _operation = self.operations.lock().await;
        self.stop_under_gate().await
    }

    async fn stop_under_gate(&self) -> Result<(), String> {
        if let Some(unit) = &self.systemd {
            if unit.pid()?.is_some() {
                unit.stop()?;
                self.state.lock().unwrap().stop_count += 1;
            }
            return Ok(());
        }
        {
            let mut state = self.state.lock().unwrap();
            state.reap()?;
            let Some(child) = state.child.as_ref() else {
                return Ok(());
            };
            // The child remains owned and unreaped, so its PID cannot be reused
            // between this signal and the subsequent try_wait calls.
            let result = unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGINT) };
            if result != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            state.stop_count += 1;
        }
        let deadline = Instant::now() + STOP_TIMEOUT;
        loop {
            {
                let mut state = self.state.lock().unwrap();
                state.reap()?;
                if state.child.is_none() {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    if let Some(child) = state.child.as_mut() {
                        child.kill().map_err(|error| error.to_string())?;
                        child.wait().map_err(|error| error.to_string())?;
                    }
                    state.child.take();
                    return Err("private daemon exceeded the graceful stop timeout".into());
                }
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    fn snapshot(&self) -> Snapshot {
        let mut state = self.state.lock().unwrap();
        state.reap().expect("inspect the owned daemon child");
        Snapshot {
            pid: match &self.systemd {
                Some(unit) => unit.pid().expect("inspect the isolated systemd unit"),
                None => state.child.as_ref().map(Child::id),
            },
            start_count: state.start_count,
            stop_count: state.stop_count,
        }
    }
}

struct Manager {
    controller: Arc<Controller>,
}

fn only_unit(name: &str) -> zbus::fdo::Result<()> {
    if name == UNIT_NAME {
        Ok(())
    } else {
        Err(zbus::fdo::Error::InvalidArgs(
            "private fixture only accepts patinad.service".into(),
        ))
    }
}

fn only_replace(mode: &str) -> zbus::fdo::Result<()> {
    if mode == "replace" {
        Ok(())
    } else {
        Err(zbus::fdo::Error::InvalidArgs(
            "private fixture only accepts replace mode".into(),
        ))
    }
}

#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn get_unit_file_state(&self, name: &str) -> zbus::fdo::Result<String> {
        only_unit(name)?;
        Ok("disabled".into())
    }

    fn get_unit(&self, name: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        only_unit(name)?;
        Ok(UNIT_PATH.try_into().unwrap())
    }

    async fn start_unit(&self, name: &str, mode: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        only_unit(name)?;
        only_replace(mode)?;
        self.controller
            .start()
            .await
            .map_err(zbus::fdo::Error::Failed)?;
        Ok("/org/freedesktop/systemd1/job/1".try_into().unwrap())
    }

    async fn stop_unit(&self, name: &str, mode: &str) -> zbus::fdo::Result<OwnedObjectPath> {
        only_unit(name)?;
        only_replace(mode)?;
        self.controller
            .stop()
            .await
            .map_err(zbus::fdo::Error::Failed)?;
        Ok("/org/freedesktop/systemd1/job/2".try_into().unwrap())
    }

    #[zbus(property)]
    fn environment(&self) -> Vec<String> {
        Vec::new()
    }
}

struct Unit {
    controller: Arc<Controller>,
}

#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn active_state(&self) -> String {
        if self.controller.snapshot().pid.is_some() {
            "active".into()
        } else {
            "inactive".into()
        }
    }

    #[zbus(property)]
    fn sub_state(&self) -> String {
        if self.controller.snapshot().pid.is_some() {
            "running".into()
        } else {
            "dead".into()
        }
    }
}

pub(super) struct Fixture {
    connection: zbus::Connection,
    controller: Arc<Controller>,
}

impl Fixture {
    pub async fn start(&self) -> Result<(), String> {
        self.controller.start().await
    }

    pub async fn stop(&self) -> Result<(), String> {
        self.controller.stop().await
    }

    pub async fn snapshot(&self) -> Snapshot {
        self.controller.snapshot()
    }

    pub async fn cleanup(&self) -> Result<(), String> {
        {
            let _operation = self.controller.operations.lock().await;
            self.controller.state.lock().unwrap().closing = true;
        }
        let stopped = self.stop().await;
        let released = self.connection.release_name(BUS_NAME).await;
        stopped?;
        released.map_err(|error| error.to_string())?;
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(unit) = &self.controller.systemd {
            if let Err(error) = unit.stop() {
                eprintln!("isolated service cleanup failed: {error}");
            }
        }
        self.controller
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .kill_owned_child();
    }
}

pub(super) async fn serve(root: &Path, binary: &Path) -> Fixture {
    assert_eq!(root.parent(), Some(Path::new("/tmp")));
    assert!(root
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .starts_with("patina-storage-test-"));
    assert_eq!(root.canonicalize().unwrap(), root);
    assert_eq!(
        fs::read_to_string(root.join("marker")).unwrap(),
        "storage-acceptance\n"
    );
    let metadata = fs::metadata(root).unwrap();
    assert_eq!(metadata.uid(), unsafe { libc::geteuid() });
    assert_eq!(metadata.mode() & 0o077, 0);
    for (key, child) in [
        ("HOME", "home"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_DATA_HOME", "data"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_RUNTIME_DIR", "runtime"),
    ] {
        assert_eq!(
            std::env::var_os(key).map(PathBuf::from),
            Some(root.join(child))
        );
        assert_eq!(root.join(child).canonicalize().unwrap(), root.join(child));
    }
    let bus = format!("unix:path={}/runtime/bus", root.display());
    assert_eq!(std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap(), bus);
    assert_eq!(std::env::var("DBUS_SYSTEM_BUS_ADDRESS").unwrap(), bus);
    let socket = fs::symlink_metadata(root.join("runtime/bus")).unwrap();
    assert!(socket.file_type().is_socket());
    assert_eq!(socket.uid(), unsafe { libc::geteuid() });
    assert!(binary.is_absolute());
    let binary = binary.canonicalize().unwrap();
    assert!(binary.is_file());
    assert_ne!(
        binary,
        std::env::current_exe().unwrap().canonicalize().unwrap()
    );

    let controller = Arc::new(Controller {
        root: root.to_path_buf(),
        binary,
        systemd: match std::env::var("PATINA_STORAGE_TEST_SYSTEMD").as_deref() {
            Ok("1") => Some(systemd::TestUnit::new(root)),
            Err(std::env::VarError::NotPresent) => None,
            _ => panic!("unsupported storage service backend"),
        },
        state: Mutex::new(ProcessState::default()),
        operations: tokio::sync::Mutex::new(()),
    });
    let connection = zbus::Connection::session().await.unwrap();
    connection
        .object_server()
        .at(
            MANAGER_PATH,
            Manager {
                controller: controller.clone(),
            },
        )
        .await
        .unwrap();
    connection
        .object_server()
        .at(
            UNIT_PATH,
            Unit {
                controller: controller.clone(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        connection
            .request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into())
            .await
            .unwrap(),
        RequestNameReply::PrimaryOwner,
        "private service bus name was already owned; refusing to replace or queue"
    );
    Fixture {
        connection,
        controller,
    }
}
