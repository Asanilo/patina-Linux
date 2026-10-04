use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

const RECORD: &str = "standalone-activation.json";
const LIMIT: u64 = 16 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Prepared,
    Starting,
    Completed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivationRecord {
    pub format_version: u32,
    pub runtime_root: PathBuf,
    pub config_root: PathBuf,
    pub data_root: PathBuf,
    pub manifest_sha256: String,
    pub binary_sha256: String,
    pub cutover_request_id: String,
    pub phase: Phase,
    pub last_error: Option<String>,
}

pub(super) struct Store {
    root: PathBuf,
    _lock: File,
}

impl Drop for Store {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self._lock);
    }
}

fn secure(file: &File) -> Result<(), String> {
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    // SAFETY: geteuid has no pointer arguments or side effects.
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.mode() & 0o7777 != 0o600
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err("activation state must be a user-owned regular 0600 file".into());
    }
    Ok(())
}

impl Store {
    pub(super) fn open(root: &Path) -> Result<Self, String> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(|error| error.to_string())?;
        let metadata = fs::symlink_metadata(root).map_err(|error| error.to_string())?;
        // Existing product control roots need not be 0700, but must not be writable by others.
        // SAFETY: geteuid has no pointer arguments or side effects.
        if !metadata.is_dir()
            || metadata.mode() & 0o022 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err("activation control root must be a user-owned directory".into());
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join("standalone-activation.lock"))
            .map_err(|error| error.to_string())?;
        secure(&lock)?;
        lock.try_lock_exclusive()
            .map_err(|_| "another profile activation is in progress".to_string())?;
        Ok(Self {
            root: root.to_path_buf(),
            _lock: lock,
        })
    }
    pub(super) fn read(&self) -> Result<Option<ActivationRecord>, String> {
        read_at(&self.root)
    }
    pub(super) fn write(&self, record: &ActivationRecord) -> Result<(), String> {
        validate(record)?;
        self.read()?; // Preserve a damaged or custom entry instead of replacing it.
        let temporary = self.root.join(format!(".activation-{}.tmp", random_id()?));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)
                .map_err(|error| error.to_string())?;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
            let bytes = serde_json::to_vec(record).map_err(|error| error.to_string())?;
            if bytes.len() as u64 > LIMIT {
                return Err("activation state exceeds size limit".into());
            }
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|error| error.to_string())?;
            fs::rename(&temporary, self.root.join(RECORD)).map_err(|error| error.to_string())?;
            File::open(&self.root)
                .and_then(|dir| dir.sync_all())
                .map_err(|error| error.to_string())
        })();
        let _ = fs::remove_file(&temporary);
        result
    }
}

pub(super) fn read_at(root: &Path) -> Result<Option<ActivationRecord>, String> {
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(root.join(RECORD))
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    secure(&file)?;
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > LIMIT {
        return Err("activation state exceeds size limit".into());
    }
    let record: ActivationRecord =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    validate(&record)?;
    Ok(Some(record))
}

fn validate(record: &ActivationRecord) -> Result<(), String> {
    let digest = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    if record.format_version != 1
        || !digest(&record.manifest_sha256)
        || !digest(&record.binary_sha256)
        || !record
            .cutover_request_id
            .strip_prefix("cutover_")
            .is_some_and(|value| {
                value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        || [&record.runtime_root, &record.config_root, &record.data_root]
            .iter()
            .any(|path| {
                !path.is_absolute()
                    || path
                        .to_str()
                        .is_none_or(|value| value.chars().any(char::is_control))
            })
        || record
            .last_error
            .as_ref()
            .is_some_and(|error| error.chars().count() > 512)
        || (record.phase == Phase::Completed && record.last_error.is_some())
    {
        return Err("invalid standalone activation record".into());
    }
    Ok(())
}

pub(super) fn random_id() -> Result<String, String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| error.to_string())?;
    Ok(format!("cutover_{:032x}", u128::from_ne_bytes(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_guard_unlocks_before_duplicate_file_descriptions_close() {
        let root =
            std::env::temp_dir().join(format!("patina-activation-lock-{}", random_id().unwrap()));
        let guard = Store::open(&root).unwrap();
        let inherited = guard._lock.try_clone().unwrap();
        assert!(Store::open(&root).is_err());
        drop(guard);
        let next = Store::open(&root).unwrap();
        drop(inherited);
        drop(next);
        fs::remove_dir_all(root).unwrap();
    }
}
