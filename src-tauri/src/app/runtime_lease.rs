use crate::platform::app_paths::AppProfile;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const RUNTIME_LEASE_FILE_NAME: &str = "runtime-owner.lock";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRole {
    Desktop,
    Daemon,
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
        acquired_at_ms: crate::app::runtime::now_ms(),
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
}
