use crate::engine::api::runtime_control::{
    DaemonServiceRestartResult, DaemonServiceRestartSnapshot, DaemonServiceRuntimeSnapshot,
    RuntimeControlError,
};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::watch;

pub const SERVICE_NAME: &str = "patinad.service";
pub const SERVICE_ENV_NAME: &str = "PATINA_SYSTEMD_SERVICE";
const RESTART_MARKER_FILE: &str = "patinad-restart.json";
const RESTART_MARKER_VERSION: u32 = 1;
const RESTART_SIGNAL_DELAY: Duration = Duration::from_millis(250);
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct RestartMarker {
    version: u32,
    request_id: String,
    status: RestartMarkerStatus,
    requested_at_ms: i64,
    requested_instance_id: String,
    completed_at_ms: Option<i64>,
    completed_instance_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RestartMarkerStatus {
    Pending,
    Completed,
}

pub struct DaemonServiceLifecycleOwner {
    managed_by_systemd: bool,
    instance_id: String,
    marker_path: PathBuf,
    marker: Mutex<Option<RestartMarker>>,
    restart_tx: watch::Sender<Option<String>>,
}

impl DaemonServiceLifecycleOwner {
    pub fn from_environment(control_root: &Path, now_ms: i64) -> Result<Self, String> {
        Self::new(control_root, managed_by_systemd_environment(), now_ms)
    }

    pub(crate) fn new(
        control_root: &Path,
        managed_by_systemd: bool,
        now_ms: i64,
    ) -> Result<Self, String> {
        let instance_id = random_id("instance")?;
        let marker_path = control_root.join(RESTART_MARKER_FILE);
        let mut marker = match read_marker(&marker_path) {
            Ok(marker) => marker,
            Err(error) => {
                eprintln!("[patinad] ignored invalid service restart marker: {error}");
                None
            }
        };
        if managed_by_systemd {
            if let Some(current) = marker.as_mut() {
                if current.status == RestartMarkerStatus::Pending
                    && current.requested_instance_id != instance_id
                {
                    current.status = RestartMarkerStatus::Completed;
                    current.completed_at_ms = Some(now_ms);
                    current.completed_instance_id = Some(instance_id.clone());
                    write_marker_atomic(&marker_path, current)?;
                }
            }
        }
        let (restart_tx, _restart_rx) = watch::channel(None);
        Ok(Self {
            managed_by_systemd,
            instance_id,
            marker_path,
            marker: Mutex::new(marker),
            restart_tx,
        })
    }

    pub fn managed_by_systemd(&self) -> bool {
        self.managed_by_systemd
    }

    pub fn snapshot(&self) -> DaemonServiceRuntimeSnapshot {
        let marker = match self.marker.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        DaemonServiceRuntimeSnapshot {
            service_name: SERVICE_NAME.to_string(),
            managed_by_systemd: self.managed_by_systemd,
            instance_id: self.instance_id.clone(),
            restart: marker.map(marker_snapshot),
        }
    }

    pub fn request_restart(
        &self,
        requested_at_ms: i64,
    ) -> Result<DaemonServiceRestartResult, RuntimeControlError> {
        if !self.managed_by_systemd {
            return Err(RuntimeControlError::Conflict(
                "patinad is not running under its systemd user service".to_string(),
            ));
        }

        let mut marker_guard = match self.marker.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if marker_guard.as_ref().is_some_and(|marker| {
            marker.status == RestartMarkerStatus::Pending
                && marker.requested_instance_id == self.instance_id
        }) {
            return Err(RuntimeControlError::Conflict(
                "a daemon service restart is already pending".to_string(),
            ));
        }

        let marker = RestartMarker {
            version: RESTART_MARKER_VERSION,
            request_id: random_id("restart").map_err(RuntimeControlError::Internal)?,
            status: RestartMarkerStatus::Pending,
            requested_at_ms,
            requested_instance_id: self.instance_id.clone(),
            completed_at_ms: None,
            completed_instance_id: None,
        };
        write_marker_atomic(&self.marker_path, &marker).map_err(RuntimeControlError::Internal)?;
        *marker_guard = Some(marker.clone());
        drop(marker_guard);

        let restart_tx = self.restart_tx.clone();
        let request_id = marker.request_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(RESTART_SIGNAL_DELAY).await;
            restart_tx.send_replace(Some(request_id));
        });

        Ok(DaemonServiceRestartResult {
            service: self.snapshot(),
            reconnect_required: true,
        })
    }

    pub async fn wait_for_restart_request(&self) -> String {
        let mut receiver = self.restart_tx.subscribe();
        loop {
            if let Some(request_id) = receiver.borrow().clone() {
                return request_id;
            }
            if receiver.changed().await.is_err() {
                return String::new();
            }
        }
    }
}

pub(super) fn managed_by_systemd_environment() -> bool {
    std::env::var(SERVICE_ENV_NAME).is_ok_and(|value| value.trim() == SERVICE_NAME)
        && std::env::var("INVOCATION_ID").is_ok_and(|value| !value.trim().is_empty())
}

fn marker_snapshot(marker: RestartMarker) -> DaemonServiceRestartSnapshot {
    DaemonServiceRestartSnapshot {
        request_id: marker.request_id,
        status: match marker.status {
            RestartMarkerStatus::Pending => "pending",
            RestartMarkerStatus::Completed => "completed",
        }
        .to_string(),
        requested_at_ms: marker.requested_at_ms,
        requested_instance_id: marker.requested_instance_id,
        completed_at_ms: marker.completed_at_ms,
        completed_instance_id: marker.completed_instance_id,
    }
}

fn random_id(prefix: &str) -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("failed to generate daemon service identifier: {error}"))?;
    let payload = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(format!("{prefix}_{payload}"))
}

fn read_marker(path: &Path) -> Result<Option<RestartMarker>, String> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read service restart marker: {error}")),
    };
    secure_permissions(path)?;
    let marker = serde_json::from_str::<RestartMarker>(&raw)
        .map_err(|error| format!("failed to parse service restart marker: {error}"))?;
    if marker.version != RESTART_MARKER_VERSION {
        return Err(format!(
            "unsupported service restart marker version {}",
            marker.version
        ));
    }
    Ok(Some(marker))
}

fn write_marker_atomic(path: &Path, marker: &RestartMarker) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "service restart marker has no parent directory".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create service control directory: {error}"))?;
    let temporary_path = parent.join(format!(
        ".{RESTART_MARKER_FILE}.tmp-{}-{}",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> Result<(), String> {
        let mut file = options
            .open(&temporary_path)
            .map_err(|error| format!("failed to create service restart marker: {error}"))?;
        let bytes = serde_json::to_vec(marker)
            .map_err(|error| format!("failed to serialize service restart marker: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("failed to write service restart marker: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("failed to sync service restart marker: {error}"))?;
        drop(file);
        std::fs::rename(&temporary_path, path)
            .map_err(|error| format!("failed to replace service restart marker: {error}"))?;
        secure_permissions(path)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    result
}

#[cfg(unix)]
fn secure_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)
        .map_err(|error| format!("failed to inspect service restart marker: {error}"))?
        .permissions();
    if permissions.mode() & 0o777 != 0o600 {
        permissions.set_mode(0o600);
        std::fs::set_permissions(path, permissions)
            .map_err(|error| format!("failed to secure service restart marker: {error}"))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<(), String> {
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("failed to sync service control directory: {error}"))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "patina-service-lifecycle-{label}-{}-{}",
            std::process::id(),
            TEST_COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn manual_daemon_reports_service_state_but_rejects_restart() {
        let root = root("manual");
        let owner = DaemonServiceLifecycleOwner::new(&root, false, 1_000).unwrap();

        assert!(!owner.snapshot().managed_by_systemd);
        assert!(matches!(
            owner.request_restart(2_000),
            Err(RuntimeControlError::Conflict(_))
        ));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn managed_restart_writes_owner_only_pending_marker_and_signals_after_response_delay() {
        let root = root("pending");
        let owner = DaemonServiceLifecycleOwner::new(&root, true, 1_000).unwrap();

        let result = owner.request_restart(2_000).unwrap();

        assert!(result.reconnect_required);
        assert_eq!(
            result
                .service
                .restart
                .as_ref()
                .map(|value| value.status.as_str()),
            Some("pending")
        );
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), owner.wait_for_restart_request())
                .await
                .unwrap(),
            result.service.restart.as_ref().unwrap().request_id
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.join(RESTART_MARKER_FILE))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn next_managed_instance_confirms_the_same_restart_ticket() {
        let root = root("complete");
        let first = DaemonServiceLifecycleOwner::new(&root, true, 1_000).unwrap();
        let pending = first.request_restart(2_000).unwrap();
        let request_id = pending.service.restart.unwrap().request_id;

        let second = DaemonServiceLifecycleOwner::new(&root, true, 3_000).unwrap();
        let completed = second.snapshot().restart.unwrap();

        assert_eq!(completed.request_id, request_id);
        assert_eq!(completed.status, "completed");
        assert_eq!(completed.completed_at_ms, Some(3_000));
        assert_eq!(
            completed.completed_instance_id,
            Some(second.instance_id.clone())
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
