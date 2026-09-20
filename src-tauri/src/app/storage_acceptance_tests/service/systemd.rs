//! Explicit opt-in: map the private fixture name to one random real user unit.
//! Never load, stop, enable or alter the installed patinad.service.
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub(super) struct TestUnit {
    name: String,
    runtime: PathBuf,
    root: PathBuf,
    observer: Mutex<Option<(Child, u64)>>,
}

impl TestUnit {
    pub fn new(root: &Path) -> Self {
        let uid = unsafe { libc::geteuid() };
        let runtime = PathBuf::from(format!("/run/user/{uid}"));
        let socket = fs::symlink_metadata(runtime.join("bus")).unwrap();
        assert!(socket.file_type().is_socket());
        assert_eq!(socket.uid(), uid);
        let suffix = root.file_name().unwrap().to_str().unwrap();
        assert!(suffix.starts_with("patina-storage-test-"));
        assert!(suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-'));
        let unit = Self {
            name: format!("{suffix}.service"),
            runtime,
            root: root.to_path_buf(),
            observer: Mutex::new(None),
        };
        assert_eq!(unit.property("LoadState").unwrap(), "not-found");
        super::super::evidence(
            root,
            "systemd-unit.json",
            serde_json::json!({
                "name": unit.name,
                "manager": "real user systemd",
                "adapter": "private D-Bus name mapping",
                "packaged_hardening": false,
            }),
            false,
        );
        unit
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new("/usr/bin/timeout");
        command.args(["15s", program, "--user"]);
        self.manager_environment(&mut command);
        command
    }

    fn manager_environment(&self, command: &mut Command) {
        command
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("LANG", "C.UTF-8")
            .env("XDG_RUNTIME_DIR", &self.runtime)
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}/bus", self.runtime.display()),
            );
    }

    fn checked(&self, output: Output) -> Result<(), String> {
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "isolated unit {}: {}",
                self.name,
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    fn property(&self, key: &str) -> Result<String, String> {
        let output = self
            .command("/usr/bin/systemctl")
            .args(["show", &self.name, "--property", key, "--value"])
            .output()
            .map_err(|e| e.to_string())?;
        let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        // systemctl returns nonzero for the correctly absent transient unit.
        if !(key == "LoadState" && value == "not-found") {
            self.checked(output)?;
        }
        Ok(value)
    }

    pub fn pid(&self) -> Result<Option<u32>, String> {
        if self.property("LoadState")? == "not-found" {
            return Ok(None);
        }
        let pid: u32 = self
            .property("MainPID")?
            .parse()
            .map_err(|e| format!("invalid unit PID: {e}"))?;
        Ok((pid != 0).then_some(pid))
    }

    pub fn start(&self, root: &Path, binary: &Path, number: u64) -> Result<(), String> {
        if self.property("LoadState")? != "not-found" {
            return Err(format!(
                "refusing to replace existing test unit {}",
                self.name
            ));
        }
        let log = root.join(format!("daemon-{number}.log"));
        // Keep systemd-run --wait alive to obtain the actual service exit code
        // before --collect removes properties. A forced stop must fail acceptance.
        let mut command = Command::new("/usr/bin/systemd-run");
        self.manager_environment(&mut command);
        command.args([
            "--user",
            "--wait",
            "--quiet",
            "--collect",
            "--unit",
            &self.name,
            "--property=Type=exec",
            "--property=Restart=no",
            "--property=RuntimeMaxSec=300s",
            "--property=KillSignal=SIGINT",
            "--property=TimeoutStopSec=10s",
            "--property=UMask=0077",
            "--property=NoNewPrivileges=yes",
            // Synthetic data is intentionally shared with Desktop under /tmp.
            // Packaged namespace hardening needs a separate installation gate.
            "--property=PrivateTmp=no",
            "--setenv=LANG=C.UTF-8",
            "--setenv=XDG_SESSION_TYPE=wayland",
            "--setenv=XDG_CURRENT_DESKTOP=isolated-storage-test",
            "--setenv=DISPLAY=",
            "--setenv=WAYLAND_DISPLAY=",
            "--setenv=PATINA_SYSTEMD_SERVICE=patinad.service",
        ]);
        for key in [
            "HOME",
            "XDG_CONFIG_HOME",
            "XDG_DATA_HOME",
            "XDG_CACHE_HOME",
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
            "DBUS_SYSTEM_BUS_ADDRESS",
        ] {
            let value = std::env::var(key).map_err(|e| e.to_string())?;
            command.arg(format!("--setenv={key}={value}"));
        }
        command.arg(format!("--property=WorkingDirectory={}", root.display()));
        command.arg(format!(
            "--property=StandardOutput=append:{}",
            log.display()
        ));
        command.arg(format!("--property=StandardError=append:{}", log.display()));
        command
            .arg("--")
            .arg(binary)
            .args(["--profile", "production", "--serve-api", "--track"]);
        let observer_log = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(root.join(format!("systemd-wait-{number}.log")))
            .map_err(|e| e.to_string())?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::from(
                observer_log.try_clone().map_err(|e| e.to_string())?,
            ))
            .stderr(Stdio::from(observer_log));
        let mut observer = self.observer.lock().unwrap();
        if observer.is_some() {
            return Err("the previous systemd exit has not been verified".into());
        }
        *observer = Some((command.spawn().map_err(|e| e.to_string())?, number));
        drop(observer);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.pid()?.is_some() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(format!("{} did not start", self.name))
    }

    pub fn stop(&self) -> Result<(), String> {
        if self.property("LoadState")? != "not-found" {
            let output = self
                .command("/usr/bin/systemctl")
                .args(["stop", &self.name])
                .output()
                .map_err(|e| e.to_string())?;
            self.checked(output)?;
        }
        self.verify_exit()?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.property("LoadState")? == "not-found" {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err(format!("{} was not collected after stop", self.name))
    }

    fn verify_exit(&self) -> Result<(), String> {
        let mut observer = self.observer.lock().unwrap();
        let Some((child, number)) = observer.as_mut() else {
            return Ok(());
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                super::super::evidence(
                    &self.root,
                    &format!("systemd-exit-{number}.json"),
                    serde_json::json!({"unit":self.name, "observer":"systemd-run --wait", "code":status.code(), "success":status.success()}),
                    false,
                );
                observer.take();
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("{} did not exit normally: {status}", self.name))
                };
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                observer.take();
                return Err(format!("{} exit observer timed out", self.name));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
