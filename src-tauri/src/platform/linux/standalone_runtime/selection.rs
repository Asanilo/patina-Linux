//! Desired installed version. Selection does not imply a running or ready service.
use super::*;
use std::os::unix::fs::symlink;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ExpectedCurrent {
    Absent,
    Manifest(String),
}

impl ExpectedCurrent {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        if value == "none" {
            Ok(Self::Absent)
        } else if is_digest(value) {
            Ok(Self::Manifest(value.to_owned()))
        } else {
            Err(error(
                "expected current must be none or a lowercase manifest SHA256",
            ))
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct SelectionSnapshot {
    pub selected: Option<StagedRuntime>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ServicePlan {
    pub selected: StagedRuntime,
    pub unit_path: PathBuf,
    pub unit_text: String,
}

pub(crate) fn service_plan(
    root: &Path,
    expected: &str,
    config: &Path,
    data: &Path,
    allow_debug: bool,
) -> Result<ServicePlan, String> {
    let installed = Installation::open(root, false)?;
    let selected = installed
        .current(allow_debug)?
        .ok_or_else(|| error("no standalone version is selected"))?;
    if selected.manifest_sha256 != expected {
        return Err(error(
            "installed selection changed; inspect before preparing its service",
        ));
    }
    let unit_text =
        crate::platform::linux::patinad_service_unit::standalone(&installed.root, config, data)?;
    Ok(ServicePlan {
        selected,
        unit_path: config.join("systemd/user/patinad.service"),
        unit_text,
    })
}

struct Installation {
    root: PathBuf,
    _lock: File,
}

impl Installation {
    fn open(root: &Path, exclusive: bool) -> Result<Self, String> {
        validate_root_path(root)?;
        validate_root(root)?;
        let root = root.canonicalize().map_err(error)?;
        let lock = installation_lock(&root, false, exclusive)?;
        Ok(Self { root, _lock: lock })
    }

    fn current(&self, allow_debug: bool) -> Result<Option<StagedRuntime>, String> {
        let path = self.root.join("current");
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(failure) if failure.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(failure) => return Err(error(failure)),
        };
        if !metadata.file_type().is_symlink() || metadata.uid() != uid() {
            return Err(error(
                "current must be a user-owned standalone version link",
            ));
        }
        let target = fs::read_link(path).map_err(error)?;
        let value = target
            .to_str()
            .ok_or_else(|| error("invalid current target"))?;
        let expected = value
            .strip_prefix("versions/")
            .filter(|digest| is_digest(digest))
            .ok_or_else(|| error("current must refer to versions/<manifest SHA256>"))?;
        installed_version(&self.root, expected, allow_debug).map(Some)
    }
}

pub(crate) fn inspect(root: &Path, allow_debug: bool) -> Result<SelectionSnapshot, String> {
    let installed = Installation::open(root, false)?;
    Ok(SelectionSnapshot {
        selected: installed.current(allow_debug)?,
    })
}

pub(crate) fn select(
    root: &Path,
    manifest_sha256: &str,
    expected_current: &ExpectedCurrent,
    allow_debug: bool,
) -> Result<StagedRuntime, String> {
    let installed = Installation::open(root, true)?;
    let before = installed.current(allow_debug)?;
    let matches = match (expected_current, &before) {
        (ExpectedCurrent::Absent, None) => true,
        (ExpectedCurrent::Manifest(expected), Some(current)) => {
            expected == &current.manifest_sha256
        }
        _ => false,
    };
    if !matches {
        return Err(error(
            "installed selection changed; inspect before selecting again",
        ));
    }
    let target = installed_version(&installed.root, manifest_sha256, allow_debug)?;
    if let Some(before) = before {
        let previous_version =
            semver::Version::parse(&before.build.package_version).map_err(error)?;
        let target_version =
            semver::Version::parse(&target.build.package_version).map_err(error)?;
        if target_version.cmp_precedence(&previous_version).is_lt() {
            return Err(error(
                "selecting an older runtime is not supported; data migration may prevent downgrade",
            ));
        }
        if before.manifest_sha256 == target.manifest_sha256 {
            return Ok(target);
        }
    }
    // Keep the only selection record atomic. Staged payloads are never modified.
    let temporary = TemporaryDirectory::create(&installed.root, "selection")?;
    let next = temporary.path().join("current");
    symlink(Path::new("versions").join(manifest_sha256), &next).map_err(error)?;
    File::open(temporary.path())
        .and_then(|dir| dir.sync_all())
        .map_err(error)?;
    fs::rename(&next, installed.root.join("current")).map_err(error)?;
    // Once rename succeeds, errors may mean the selection was committed. Callers
    // must inspect; never automatically select the previous (possibly older) image.
    File::open(&installed.root)
        .and_then(|dir| dir.sync_all())
        .map_err(error)?;
    File::open(temporary.path())
        .and_then(|dir| dir.sync_all())
        .map_err(error)?;
    Ok(target)
}

#[cfg(test)]
mod tests;
