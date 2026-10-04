//! Explicit migration of known legacy service layouts, never arbitrary user units.
use super::*;
use crate::platform::linux::{
    appimage_runtime::PublishedRuntime, patinad_service_unit as unit,
    systemd_user_service as systemd,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    process::Stdio,
};
use tokio::io::AsyncReadExt;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceKind {
    Packaged,
    Appimage,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct MigrationRequest {
    pub kind: SourceKind,
    pub unit_sha256: String,
    pub source_version: String,
}
impl MigrationRequest {
    pub(crate) fn parse(
        kind: &str,
        unit_sha256: &str,
        source_version: &str,
    ) -> Result<Self, String> {
        let kind = match kind {
            "packaged" => SourceKind::Packaged,
            "appimage" => SourceKind::Appimage,
            _ => return Err("migration source must be packaged or appimage".into()),
        };
        let result = Self {
            kind,
            unit_sha256: unit_sha256.to_owned(),
            source_version: source_version.to_owned(),
        };
        result.validate()?;
        Ok(result)
    }
    pub(super) fn validate(&self) -> Result<(), String> {
        if self.unit_sha256.len() != 64
            || !self
                .unit_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("migration requires a lowercase source unit SHA256".into());
        }
        semver::Version::parse(&self.source_version).map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MigrationProof {
    pub request: MigrationRequest,
    pub original_unit: String,
    pub source_unit_path: PathBuf,
    pub image_sha256: Option<String>,
}
impl MigrationProof {
    pub(super) fn validate(&self) -> Result<(), String> {
        self.request.validate()?;
        if unit_hash(&self.original_unit) != self.request.unit_sha256
            || self.original_unit.len() > 16 * 1024
            || !self.source_unit_path.is_absolute()
            || self
                .source_unit_path
                .to_str()
                .is_none_or(|path| path.chars().any(char::is_control))
            || match self.request.kind {
                SourceKind::Packaged => self.image_sha256.is_some(),
                SourceKind::Appimage => !self.image_sha256.as_ref().is_some_and(|value| {
                    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
                }),
            }
        {
            return Err("invalid saved migration source identity".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

pub(super) fn unit_hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub(super) struct LegacySource {
    pub proof: MigrationProof,
    launcher: PathBuf,
    arguments: Vec<String>,
    environment: Vec<String>,
    appimage: Option<PublishedRuntime>,
    replaced: bool,
}

impl LegacySource {
    pub(super) fn open(
        roots: &AppPathRoots,
        paths: &StoragePaths,
        request: MigrationRequest,
        target_version: &str,
        target_unit: &str,
        previous: Option<&ActivationRecord>,
    ) -> Result<Self, String> {
        request.validate()?;
        let source_version =
            semver::Version::parse(&request.source_version).map_err(|error| error.to_string())?;
        if semver::Version::parse(target_version)
            .map_err(|error| error.to_string())?
            .cmp_precedence(&source_version)
            .is_lt()
        {
            return Err("standalone target is older than the confirmed migration source".into());
        }
        let user_unit = roots.config.join("systemd/user/patinad.service");
        let current = unit::read_existing(&user_unit)?;
        let replaced = current.as_deref() == Some(target_unit);
        let mut environment = vec!["PATINA_SYSTEMD_SERVICE=patinad.service".to_string()];
        let old_root = paths.stable_product_data_root.join("runtime-appimage");
        let (text, launcher, mut arguments) = match request.kind {
            SourceKind::Packaged => (
                include_str!("../../../../packaging/systemd/patinad.service").to_string(),
                PathBuf::from("/usr/bin/patinad"),
                Vec::new(),
            ),
            SourceKind::Appimage => {
                environment.push(format!("XDG_CONFIG_HOME={}", roots.config.display()));
                environment.push(format!("XDG_DATA_HOME={}", roots.data.display()));
                (
                    unit::appimage(&old_root.join("current/AppRun"), &roots.config, &roots.data)?,
                    old_root.join("current/AppRun"),
                    vec!["--patinad".into()],
                )
            }
        };
        if unit_hash(&text) != request.unit_sha256 {
            return Err("source unit digest does not identify a known service recipe".into());
        }
        arguments.extend(["--profile", "production", "--serve-api", "--track"].map(str::to_owned));
        arguments.insert(
            0,
            launcher
                .to_str()
                .ok_or("source launcher must be UTF-8")?
                .to_owned(),
        );
        let (source_unit_path, appimage, image_sha256) = if replaced {
            let saved = previous
                .and_then(|record| record.migration.as_ref())
                .ok_or("target unit already exists without a matching migration intent")?;
            saved.validate()?;
            if saved.request != request || saved.original_unit != text {
                return Err("migration confirmation differs from the saved intent".into());
            }
            (
                saved.source_unit_path.clone(),
                None,
                saved.image_sha256.clone(),
            )
        } else {
            match request.kind {
                SourceKind::Packaged => {
                    if current.is_some() {
                        return Err(
                            "user override preserved; it is not a packaged source unit".into()
                        );
                    }
                    let path = [
                        "/usr/lib/systemd/user/patinad.service",
                        "/lib/systemd/user/patinad.service",
                    ]
                    .iter()
                    .map(PathBuf::from)
                    .find(|path| path.exists())
                    .ok_or("packaged source unit is missing")?;
                    if read_packaged(&path)? != text {
                        return Err("packaged source unit differs from the known recipe".into());
                    }
                    require_packaged_executable(&launcher)?;
                    (path, None, None)
                }
                SourceKind::Appimage => {
                    if current.as_deref() != Some(text.as_str()) {
                        return Err("custom or changed AppImage unit preserved".into());
                    }
                    let published = PublishedRuntime::inspect(&old_root)?;
                    if published.version() != request.source_version {
                        return Err("published AppImage version differs from confirmation".into());
                    }
                    let image = published.image_sha256().to_owned();
                    (user_unit, Some(published), Some(image))
                }
            }
        };
        Ok(Self {
            proof: MigrationProof {
                request,
                original_unit: text,
                source_unit_path,
                image_sha256,
            },
            launcher,
            arguments,
            environment,
            appimage,
            replaced,
        })
    }

    pub(super) async fn verify_source(
        &self,
        definition: &systemd::ServiceDefinition,
        control_root: &Path,
        token_path: &Path,
        port: u16,
    ) -> Result<(), String> {
        if !definition.drop_in_paths.is_empty() {
            return Err("source service drop-ins preserved; migration is not applicable".into());
        }
        let path = Path::new(&definition.fragment_path);
        if !same_unit_path(path, &self.proof.source_unit_path) {
            return Err("loaded service is not the confirmed migration source".into());
        }
        let launch = systemd::inspect_patinad_launch().await?;
        self.verify_launch(&launch)?;
        let state = systemd::inspect_patinad_service().await;
        if !state.manager_available || state.error.is_some() {
            return Err("cannot inspect source service state".into());
        }
        let owner = runtime_lease::inspect_locked_owner(control_root)?;
        if state.active {
            if !owner.is_some_and(|owner| {
                owner.pid == launch.main_pid
                    && owner.profile == "production"
                    && owner.role == runtime_lease::RuntimeRole::Daemon
            }) {
                return Err("source service PID does not own the selected profile".into());
            }
            let token =
                crate::engine::api::auth::ApiCredentialStore::new().load_existing_at(token_path)?;
            let client =
                patina_client::Client::new(port, token).map_err(|error| error.to_string())?;
            let before = client
                .service_snapshot()
                .await
                .map_err(|error| error.to_string())?;
            let negotiated = client
                .negotiate_tracking_owner()
                .await
                .map_err(|error| error.to_string())?;
            let after = client
                .service_snapshot()
                .await
                .map_err(|error| error.to_string())?;
            if !before.managed_by_systemd
                || !after.managed_by_systemd
                || before.instance_id != after.instance_id
                || semver::Version::parse(&negotiated.server_version)
                    .map_err(|error| error.to_string())?
                    .cmp_precedence(
                        &semver::Version::parse(&self.proof.request.source_version)
                            .map_err(|error| error.to_string())?,
                    )
                    .is_gt()
            {
                return Err(
                    "running source identity changed or is newer than the confirmed source version"
                        .into(),
                );
            }
            if systemd::inspect_patinad_launch().await?.main_pid != launch.main_pid {
                return Err("source service restarted during inspection".into());
            }
        } else if owner.is_some_and(|owner| {
            owner.pid != std::process::id()
                || owner.role != runtime_lease::RuntimeRole::Maintenance
                || owner.profile != "production"
        }) {
            return Err("profile is owned outside the confirmed source service".into());
        }
        if !self.replaced {
            self.verify_files().await?;
        }
        Ok(())
    }

    fn verify_launch(&self, launch: &systemd::ServiceLaunch) -> Result<(), String> {
        let mut actual = launch.environment.clone();
        actual.sort();
        let mut expected = self.environment.clone();
        expected.sort();
        if launch.commands.len() != 1
            || launch.commands[0].0 != self.launcher.to_string_lossy()
            || launch.commands[0].1 != self.arguments
            || launch.commands[0].2
            || actual != expected
            || !launch.environment_files.is_empty()
        {
            return Err(
                "loaded source command or environment differs from the known service recipe".into(),
            );
        }
        Ok(())
    }

    pub(super) async fn verify_files(&self) -> Result<(), String> {
        if self.replaced {
            return Ok(());
        }
        match self.proof.request.kind {
            SourceKind::Appimage => {
                self.appimage
                    .as_ref()
                    .ok_or("AppImage migration lock is missing")?
                    .verify()
                    .await
            }
            SourceKind::Packaged => {
                if read_packaged(&self.proof.source_unit_path)? != self.proof.original_unit {
                    return Err("packaged source unit changed during migration".into());
                }
                require_packaged_executable(&self.launcher)?;
                let mut child = tokio::process::Command::new(&self.launcher)
                    .arg("--version")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .kill_on_drop(true)
                    .spawn()
                    .map_err(|error| error.to_string())?;
                let mut bytes = Vec::new();
                let mut stdout = child
                    .stdout
                    .take()
                    .ok_or("missing source version stdout")?
                    .take(129);
                tokio::time::timeout(Duration::from_secs(5), async {
                    stdout
                        .read_to_end(&mut bytes)
                        .await
                        .map_err(|error| error.to_string())?;
                    if bytes
                        != format!("patinad {}\n", self.proof.request.source_version).as_bytes()
                    {
                        return Err(
                            "installed source version differs from confirmation".to_string()
                        );
                    }
                    if !child
                        .wait()
                        .await
                        .map_err(|error| error.to_string())?
                        .success()
                    {
                        return Err("source version probe failed".into());
                    }
                    Ok(())
                })
                .await
                .map_err(|_| "source version probe timed out".to_string())?
            }
        }
    }
}

fn same_unit_path(actual: &Path, expected: &Path) -> bool {
    let normalize = |path: &Path| {
        path.parent()?
            .canonicalize()
            .ok()
            .map(|parent| parent.join(path.file_name().unwrap_or_default()))
    };
    actual == expected
        || normalize(actual).is_some_and(|actual| Some(actual) == normalize(expected))
}

fn packaged_file(path: &Path, limit: u64) -> Result<std::fs::File, String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .map_err(|error| error.to_string())?;
    let meta = file.metadata().map_err(|error| error.to_string())?;
    if !meta.is_file()
        || meta.nlink() != 1
        || meta.uid() != 0
        || meta.mode() & 0o022 != 0
        || meta.len() > limit
    {
        return Err("packaged source must be a bounded root-owned regular file".into());
    }
    Ok(file)
}
fn read_packaged(path: &Path) -> Result<String, String> {
    let mut bytes = Vec::new();
    packaged_file(path, 16 * 1024)?
        .take(16 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 16 * 1024 {
        return Err("packaged unit exceeds size limit".into());
    }
    String::from_utf8(bytes).map_err(|error| error.to_string())
}
fn require_packaged_executable(path: &Path) -> Result<(), String> {
    let mut file = packaged_file(path, 512 * 1024 * 1024)?;
    let mut magic = [0_u8; 4];
    file.read_exact(&mut magic)
        .map_err(|error| error.to_string())?;
    if magic != *b"\x7fELF"
        || file.metadata().map_err(|error| error.to_string())?.mode() & 0o100 == 0
    {
        return Err("packaged daemon is not an executable ELF".into());
    }
    Ok(())
}
