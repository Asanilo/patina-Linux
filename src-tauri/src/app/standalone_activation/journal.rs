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

fn start_allowed_by_default() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Phase {
    Prepared,
    Starting,
    Completed,
    Deactivating,
    Deactivated,
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
    // Stable startup contract, independent of the installer's audit/phase schema.
    #[serde(default = "start_allowed_by_default")]
    pub runtime_start_allowed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_runtime_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deactivation_mask: Option<crate::platform::linux::patinad_service_unit::OwnedMask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deactivation_binary_sha256: Option<String>,
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<super::migration::MigrationProof>,
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
    let Some(bytes) = read_bytes(root)? else {
        return Ok(None);
    };
    let record: ActivationRecord =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    validate(&record)?;
    Ok(Some(record))
}

// A client only needs the stable binding, not the installer's evolving audit fields.
#[derive(Deserialize)]
pub(super) struct Binding {
    format_version: u32,
    pub runtime_root: PathBuf,
    pub config_root: PathBuf,
    pub data_root: PathBuf,
}

pub(super) fn require_runtime_start(root: &Path) -> Result<(), String> {
    #[derive(Deserialize)]
    struct StartPolicy {
        #[serde(flatten)]
        binding: Binding,
        #[serde(default = "start_allowed_by_default")]
        runtime_start_allowed: bool,
    }
    let Some(bytes) = read_bytes(root)? else {
        return Ok(());
    };
    let policy: StartPolicy = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    validate_binding(&policy.binding)?;
    if !policy.runtime_start_allowed {
        return Err(
            "standalone backend startup is disabled; resume it through explicit runtime activation"
                .into(),
        );
    }
    Ok(())
}

#[cfg(feature = "desktop")]
pub(super) fn activation_completed(root: &Path) -> Result<bool, String> {
    #[derive(Deserialize)]
    struct PhaseProjection {
        phase: String,
    }
    let Some(bytes) = read_bytes(root)? else {
        return Ok(false);
    };
    let value: PhaseProjection =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    Ok(value.phase == "completed")
}

#[cfg(feature = "desktop")]
pub(super) fn read_binding_at(root: &Path) -> Result<Option<Binding>, String> {
    let Some(bytes) = read_bytes(root)? else {
        return Ok(None);
    };
    let binding: Binding = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    validate_binding(&binding)?;
    Ok(Some(binding))
}

fn validate_binding(binding: &Binding) -> Result<(), String> {
    if binding.format_version != 1
        || [
            &binding.runtime_root,
            &binding.config_root,
            &binding.data_root,
        ]
        .iter()
        .any(|path| {
            !path.is_absolute()
                || path
                    .to_str()
                    .is_none_or(|value| value.chars().any(char::is_control))
        })
    {
        return Err("invalid standalone binding".into());
    }
    Ok(())
}

fn read_bytes(root: &Path) -> Result<Option<Vec<u8>>, String> {
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
    Ok(Some(bytes))
}

fn validate(record: &ActivationRecord) -> Result<(), String> {
    if let Some(mask) = &record.deactivation_mask {
        mask.validate()?;
    }
    if let Some(version) = &record.minimum_runtime_version {
        if version.len() > 64 || semver::Version::parse(version).is_err() {
            return Err("invalid standalone runtime version floor".into());
        }
    }
    if matches!(record.phase, Phase::Deactivating | Phase::Deactivated)
        && (record.runtime_start_allowed
            || record.deactivation_mask.is_none()
            || record.minimum_runtime_version.is_none())
    {
        return Err(
            "deactivation requires disabled startup, version floor and mask identity".into(),
        );
    }
    if let Some(proof) = &record.migration {
        proof.validate()?;
    }
    let digest = |value: &str| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    if record
        .deactivation_binary_sha256
        .as_ref()
        .is_some_and(|value| !digest(value))
        || record.deactivation_mask.is_some() != record.deactivation_binary_sha256.is_some()
    {
        return Err("invalid saved deactivation executable identity".into());
    }
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
        || (matches!(record.phase, Phase::Completed | Phase::Deactivated)
            && record.last_error.is_some())
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
    fn startup_policy_is_stable_and_invalid_state_cannot_enable_a_runtime() {
        let root =
            std::env::temp_dir().join(format!("patina-start-policy-{}", random_id().unwrap()));
        fs::create_dir(&root).unwrap();
        let path = root.join(RECORD);
        let baseline = serde_json::json!({"format_version":1, "runtime_root":"/runtime", "config_root":"/config", "data_root":"/data",
            "phase":{"future_format":2}, "future_audit":{"value":1}});
        let write = |value: &serde_json::Value| {
            fs::write(&path, serde_json::to_vec(value).unwrap()).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        };
        assert!(require_runtime_start(&root).is_ok());
        write(&baseline);
        assert!(
            require_runtime_start(&root).is_ok(),
            "old records default to allowed, regardless of audit schema"
        );
        let mut denied = baseline.clone();
        denied["runtime_start_allowed"] = false.into();
        write(&denied);
        assert!(require_runtime_start(&root)
            .unwrap_err()
            .contains("startup is disabled"));
        for (field, value) in [
            ("runtime_start_allowed", serde_json::json!("false")),
            ("runtime_start_allowed", serde_json::Value::Null),
            ("format_version", serde_json::json!(2)),
            ("runtime_root", serde_json::json!("relative")),
        ] {
            let mut invalid = baseline.clone();
            invalid[field] = value;
            write(&invalid);
            assert!(require_runtime_start(&root).is_err(), "invalid {field}");
        }
        fs::write(&path, b"{broken").unwrap();
        assert!(require_runtime_start(&root).is_err());
        write(&baseline);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(require_runtime_start(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(feature = "desktop")]
    #[test]
    fn client_binding_does_not_require_the_installers_audit_schema() {
        let root = std::env::temp_dir().join(format!("patina-binding-{}", random_id().unwrap()));
        fs::create_dir(&root).unwrap();
        let record = serde_json::json!({"format_version":1, "runtime_root":"/runtime", "config_root":"/config", "data_root":"/data", "future_audit_field": {"migration":"v2"}});
        let path = root.join(RECORD);
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            read_binding_at(&root).unwrap().unwrap().runtime_root,
            Path::new("/runtime")
        );
        assert!(read_at(&root).is_err());
        let mut invalid = record;
        invalid["runtime_root"] = "relative".into();
        fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
        assert!(read_binding_at(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }
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
