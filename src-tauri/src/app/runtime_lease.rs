use crate::platform::app_paths::AppProfile;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Duration;

const RUNTIME_LEASE_FILE_NAME: &str = "runtime-owner.lock";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRole {
    Desktop,
    Daemon,
    Maintenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOwner {
    pub role: RuntimeRole,
    pub pid: u32,
    pub profile: String,
    pub acquired_at_ms: u64,
}

#[derive(Debug)]
pub struct RuntimeLease {
    file: File,
    pub owner: RuntimeOwner,
}

#[derive(Debug)]
pub struct RuntimeLeaseError {
    pub owner: Option<RuntimeOwner>,
    reason: String,
}

impl fmt::Display for RuntimeLeaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)?;
        if let Some(owner) = &self.owner {
            write!(
                formatter,
                "; current owner is {:?} pid {} for profile {}",
                owner.role, owner.pid, owner.profile
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeLeaseError {}

pub fn acquire_runtime_lease(
    control_root: &Path,
    profile: AppProfile,
    role: RuntimeRole,
) -> Result<RuntimeLease, RuntimeLeaseError> {
    std::fs::create_dir_all(control_root).map_err(|error| RuntimeLeaseError {
        owner: None,
        reason: format!(
            "failed to create runtime control directory `{}`: {error}",
            control_root.display()
        ),
    })?;
    let path = control_root.join(RUNTIME_LEASE_FILE_NAME);
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| RuntimeLeaseError {
            owner: None,
            reason: format!("failed to open runtime lease `{}`: {error}", path.display()),
        })?;

    if let Err(error) = file.try_lock_exclusive() {
        let owner = read_owner(&mut file);
        return Err(RuntimeLeaseError {
            owner,
            reason: format!(
                "runtime profile `{}` is already owned: {error}",
                profile.key()
            ),
        });
    }

    let owner = RuntimeOwner {
        role,
        pid: std::process::id(),
        profile: profile.key().to_string(),
        acquired_at_ms: crate::engine::runtime_context::now_ms(),
    };
    write_owner(&mut file, &owner).map_err(|message| {
        let _ = FileExt::unlock(&file);
        RuntimeLeaseError {
            owner: None,
            reason: message,
        }
    })?;

    Ok(RuntimeLease { file, owner })
}

pub async fn wait_for_runtime_lease_release(
    control_root: &Path,
    timeout: Duration,
) -> Result<(), String> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if runtime_lease_is_available(control_root)? {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("timed out waiting for the previous runtime owner to exit".to_string());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Read the identity only while another open file description owns the lease.
/// Stale metadata in an unlocked file is not evidence of a running process.
#[cfg(target_os = "linux")]
pub(crate) fn inspect_locked_owner(control_root: &Path) -> Result<Option<RuntimeOwner>, String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(control_root.join(RUNTIME_LEASE_FILE_NAME))
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    // SAFETY: geteuid takes no pointers and has no side effects.
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.len() > 4096
        || metadata.mode() & 0o022 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err("runtime owner identity must be a bounded user-owned regular file".into());
    }
    match FileExt::try_lock_shared(&file) {
        Ok(()) => {
            FileExt::unlock(&file).map_err(|error| error.to_string())?;
            Ok(None)
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            let mut bytes = Vec::new();
            file.take(4097)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            if bytes.len() > 4096 {
                return Err("runtime owner identity exceeds size limit".into());
            }
            let owner: RuntimeOwner =
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            if owner.pid == 0 {
                return Err("runtime owner PID is invalid".into());
            }
            Ok(Some(owner))
        }
        Err(error) => Err(error.to_string()),
    }
}

fn runtime_lease_is_available(control_root: &Path) -> Result<bool, String> {
    let path = control_root.join(RUNTIME_LEASE_FILE_NAME);
    let file = match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => {
            return Err(format!(
                "failed to inspect runtime lease `{}`: {error}",
                path.display()
            ))
        }
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            FileExt::unlock(&file)
                .map_err(|error| format!("failed to release runtime lease probe: {error}"))?;
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

fn read_owner(file: &mut File) -> Option<RuntimeOwner> {
    file.seek(SeekFrom::Start(0)).ok()?;
    let mut raw = String::new();
    file.read_to_string(&mut raw).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_owner(file: &mut File, owner: &RuntimeOwner) -> Result<(), String> {
    let raw = serde_json::to_vec(owner)
        .map_err(|error| format!("failed to serialize runtime owner: {error}"))?;
    file.set_len(0)
        .map_err(|error| format!("failed to reset runtime lease metadata: {error}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("failed to seek runtime lease metadata: {error}"))?;
    file.write_all(&raw)
        .map_err(|error| format!("failed to write runtime lease metadata: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("failed to sync runtime lease metadata: {error}"))
}

impl Drop for RuntimeLease {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::app_paths::AppProfile;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "patina-runtime-lease-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn inspection_requires_a_held_lock_and_preserves_metadata() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = temp_root("inspect");
        assert!(inspect_locked_owner(&root).unwrap().is_none());
        assert!(!root.exists());
        let lease =
            acquire_runtime_lease(&root, AppProfile::Production, RuntimeRole::Daemon).unwrap();
        let path = root.join(RUNTIME_LEASE_FILE_NAME);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let original = fs::read(&path).unwrap();
        assert_eq!(
            inspect_locked_owner(&root).unwrap(),
            Some(lease.owner.clone())
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        drop(lease);
        assert!(inspect_locked_owner(&root).unwrap().is_none());
        assert_eq!(fs::read(&path).unwrap(), original);
        fs::hard_link(&path, root.join("alias")).unwrap();
        assert!(inspect_locked_owner(&root).is_err());
        fs::remove_file(&path).unwrap();
        symlink(root.join("alias"), &path).unwrap();
        assert!(inspect_locked_owner(&root).is_err());
        fs::remove_file(&path).unwrap();
        fs::write(&path, vec![b'x'; 4097]).unwrap();
        assert!(inspect_locked_owner(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn second_owner_for_same_profile_is_rejected() {
        let root = temp_root("contended");
        let first = acquire_runtime_lease(&root, AppProfile::Dev, RuntimeRole::Daemon).unwrap();

        let error =
            acquire_runtime_lease(&root, AppProfile::Dev, RuntimeRole::Desktop).unwrap_err();

        assert_eq!(error.owner.as_ref().unwrap().role, RuntimeRole::Daemon);
        assert_eq!(error.owner.as_ref().unwrap().profile, "dev");
        assert_eq!(error.owner.as_ref().unwrap().pid, std::process::id());
        assert!(error.owner.as_ref().unwrap().acquired_at_ms > 0);
        drop(first);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dropping_the_lease_allows_the_next_owner() {
        let root = temp_root("drop");
        let first =
            acquire_runtime_lease(&root, AppProfile::Production, RuntimeRole::Daemon).unwrap();
        drop(first);

        let second =
            acquire_runtime_lease(&root, AppProfile::Production, RuntimeRole::Desktop).unwrap();

        assert_eq!(second.owner.role, RuntimeRole::Desktop);
        drop(second);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn separate_profile_control_roots_do_not_conflict() {
        let root = temp_root("profiles");
        let production_root = root.join("Patina");
        let dev_root = root.join("Patina Dev");

        let production = acquire_runtime_lease(
            &production_root,
            AppProfile::Production,
            RuntimeRole::Desktop,
        )
        .unwrap();
        let dev = acquire_runtime_lease(&dev_root, AppProfile::Dev, RuntimeRole::Daemon).unwrap();

        drop((production, dev));
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn release_barrier_waits_for_the_previous_owner_without_taking_ownership() {
        let root = temp_root("release-barrier");
        let lease = acquire_runtime_lease(&root, AppProfile::Dev, RuntimeRole::Desktop).unwrap();
        assert!(!runtime_lease_is_available(&root).unwrap());
        let release = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            drop(lease);
        });

        wait_for_runtime_lease_release(&root, Duration::from_secs(1))
            .await
            .unwrap();
        release.await.unwrap();
        assert!(runtime_lease_is_available(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn release_barrier_times_out_while_an_owner_is_alive() {
        let root = temp_root("release-timeout");
        let lease = acquire_runtime_lease(&root, AppProfile::Dev, RuntimeRole::Desktop).unwrap();

        let error = wait_for_runtime_lease_release(&root, Duration::from_millis(10))
            .await
            .unwrap_err();

        assert!(error.contains("timed out"));
        drop(lease);
        fs::remove_dir_all(root).unwrap();
    }
}
