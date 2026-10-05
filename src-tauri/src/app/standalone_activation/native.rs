use super::*;
use crate::platform::linux::{patinad_service_unit as unit, systemd_user_service as systemd};
use patina_protocol::service::DaemonExecutableIdentity;
use std::fs;

pub(super) struct NativeHost {
    roots: AppPathRoots,
    unit_path: PathBuf,
    unit_text: String,
    executable: PathBuf,
    identity: DaemonExecutableIdentity,
    token_path: PathBuf,
    control_root: PathBuf,
    port: u16,
    migration: Option<super::migration::LegacySource>,
    runtime_root: PathBuf,
    restore_mask: Option<unit::OwnedMask>,
}

impl NativeHost {
    pub(super) fn new(
        roots: &AppPathRoots,
        paths: &StoragePaths,
        runtime_root: &Path,
        selected: &standalone_runtime::StagedRuntime,
        port: u16,
        migration_request: Option<super::migration::MigrationRequest>,
        previous: Option<&ActivationRecord>,
    ) -> Result<Self, String> {
        if !(selected.build.protocol.min_supported_client
            ..=selected.build.protocol.max_supported_client)
            .contains(&patina_protocol::CURRENT_PROTOCOL_VERSION)
        {
            return Err(
                "selected runtime does not support this installer's client protocol".into(),
            );
        }
        let unit_text = unit::standalone(runtime_root, &roots.config, &roots.data)?;
        let previous = previous.filter(|record| {
            record.runtime_root == runtime_root
                && record.config_root == roots.config
                && record.data_root == roots.data
        });
        let restore_mask = previous.and_then(|record| record.deactivation_mask.clone());
        if restore_mask.is_some() && migration_request.is_some() {
            return Err(
                "a deactivated standalone installation cannot also migrate a legacy source".into(),
            );
        }
        let migration = migration_request
            .map(|request| {
                super::migration::LegacySource::open(
                    roots,
                    paths,
                    request,
                    &selected.build.package_version,
                    &unit_text,
                    previous,
                )
            })
            .transpose()?;
        Ok(Self {
            roots: roots.clone(),
            unit_path: roots.config.join("systemd/user/patinad.service"),
            unit_text,
            executable: selected.directory.join("bin/patinad"),
            identity: DaemonExecutableIdentity {
                build: selected.build.clone(),
                binary_sha256: selected.binary_sha256.clone(),
            },
            token_path: paths.api_token_path.clone(),
            control_root: paths.control_root.clone(),
            port,
            migration,
            runtime_root: runtime_root.to_path_buf(),
            restore_mask,
        })
    }

    async fn definition_matches(&self, allow_missing: bool) -> Result<(), String> {
        match systemd::load_patinad_definition().await? {
            Some(definition) if !definition.drop_in_paths.is_empty() => Err("patinad service drop-ins require explicit migration; existing configuration preserved".into()),
            Some(definition) if definition.fragment_path.is_empty() && allow_missing => {
                let state = systemd::inspect_patinad_service().await;
                if !state.manager_available || state.error.is_some() || state.active {
                    Err("unidentified loaded patinad service must not be replaced".into())
                } else { Ok(()) }
            },
            Some(definition) if Path::new(&definition.fragment_path) == self.unit_path => {
                if self.migration.is_some() { self.verify_target_launch().await?; }
                Ok(())
            },
            None if allow_missing => Ok(()),
            _ => Err("systemd patinad definition is not the expected standalone user unit".into()),
        }
    }

    async fn verify_target_launch(&self) -> Result<(), String> {
        let launch = systemd::inspect_patinad_launch().await?;
        unit::validate_standalone_launch(
            &self.runtime_root,
            &self.roots.config,
            &self.roots.data,
            &launch,
        )
    }
}

impl ActivationHost for NativeHost {
    fn migration_proof(&self) -> Option<super::migration::MigrationProof> {
        self.migration.as_ref().map(|source| source.proof.clone())
    }
    async fn preflight(&self) -> Result<(), String> {
        verify_manager_roots(&self.roots).await?;
        if let Some(mask) = &self.restore_mask {
            if mask.is_installed(&self.roots.config)? {
                super::deactivation::verify_masked(&self.roots, mask).await?;
                return standalone_runtime::verify_executable_metadata(
                    &self.executable,
                    &self.identity.build,
                )
                .await;
            }
            // A crash may occur after the owned unit is restored on disk but
            // before Reload updates a cached masked (or unloaded) definition.
            if unit::read_existing(&self.unit_path)?.as_deref() != Some(self.unit_text.as_str()) {
                return Err(
                    "restoring standalone unit changed; existing configuration preserved".into(),
                );
            }
            if systemd::patinad_load_state()
                .await?
                .as_deref()
                .is_none_or(|state| state == "masked")
            {
                let state = systemd::inspect_patinad_service().await;
                if !state.manager_available || state.error.is_some() || state.active {
                    return Err("restoring standalone unit has an unexpected active owner".into());
                }
                validate_login_intent(&self.control_root, state.enabled)?;
                return standalone_runtime::verify_executable_metadata(
                    &self.executable,
                    &self.identity.build,
                )
                .await;
            }
        }
        let existing = unit::read_existing(&self.unit_path)?;
        if let Some(source) = &self.migration {
            let original = match source.proof.request.kind {
                super::migration::SourceKind::Packaged => None,
                super::migration::SourceKind::Appimage => Some(source.proof.original_unit.as_str()),
            };
            if existing.as_deref() != Some(self.unit_text.as_str())
                && existing.as_deref() != original
            {
                return Err("source user unit changed; existing configuration preserved".into());
            }
            if existing.as_deref() == Some(self.unit_text.as_str())
                && self.definition_matches(false).await.is_ok()
            {
                // The matching durable intent permits recovery after file publication.
            } else {
                let definition = systemd::load_patinad_definition()
                    .await?
                    .ok_or("migration source is not loaded")?;
                source
                    .verify_source(&definition, &self.control_root, &self.token_path, self.port)
                    .await?;
            }
        } else {
            if existing
                .as_ref()
                .is_some_and(|text| text != &self.unit_text)
            {
                return Err(
                    "custom or legacy user unit preserved; explicit service migration is required"
                        .into(),
                );
            }
            if existing.is_none()
                && [
                    "/usr/lib/systemd/user/patinad.service",
                    "/lib/systemd/user/patinad.service",
                    "/etc/systemd/user/patinad.service",
                ]
                .iter()
                .any(|path| fs::symlink_metadata(path).is_ok())
            {
                return Err(
                "packaged patinad service requires explicit migration before standalone activation"
                    .into(),
            );
            }
            // An owned unit may have been published just before an interrupted reload.
            // Empty inactive definitions can be recovered; loaded foreign units cannot.
            self.definition_matches(true).await?;
        }
        let state = systemd::inspect_patinad_service().await;
        if !state.manager_available || state.error.is_some() {
            return Err("cannot verify service login preference".into());
        }
        if self.restore_mask.is_some() {
            self.verify_target_launch().await?;
        }
        validate_login_intent(&self.control_root, state.enabled)?;
        standalone_runtime::verify_executable_metadata(&self.executable, &self.identity.build).await
    }
    async fn stop(&self) -> Result<(), String> {
        if let Some(mask) = &self.restore_mask {
            if mask.is_installed(&self.roots.config)? {
                return super::deactivation::verify_masked(&self.roots, mask).await;
            }
        }
        let state = systemd::inspect_patinad_service().await;
        if let Some(error) = state.error {
            return Err(error);
        }
        if !state.manager_available {
            return Err("systemd user manager unavailable".into());
        }
        if state.unit_installed {
            systemd::control_patinad_service(systemd::PatinadServiceControlAction::Stop).await?;
        }
        Ok(())
    }
    async fn install(&self) -> Result<(), String> {
        if let Some(mask) = &self.restore_mask {
            let restored = async {
                unit::restore_owned_mask(&self.roots.config, &self.unit_text, mask)?;
                systemd::reload_user_units().await?;
                self.definition_matches(false).await?;
                self.verify_target_launch().await?;
                let state = systemd::inspect_patinad_service().await;
                if !state.manager_available || state.error.is_some() || state.active {
                    return Err("cannot confirm restored standalone unit is inactive".into());
                }
                validate_login_intent(&self.control_root, state.enabled)
            }
            .await;
            if let Err(error) = restored {
                let protection = async {
                    unit::publish_owned_mask(&self.roots.config, &self.unit_text, mask)?;
                    systemd::reload_user_units().await?;
                    super::deactivation::verify_masked(&self.roots, mask).await
                }
                .await;
                return Err(match protection {
                    Ok(()) => error,
                    Err(protection) => {
                        format!("{error}; could not confirm restored mask: {protection}")
                    }
                });
            }
            return Ok(());
        }
        if let Some(source) = &self.migration {
            source.verify_files().await?;
            match source.proof.request.kind {
                super::migration::SourceKind::Packaged => {
                    unit::install_identical_or_new(&self.roots.config, &self.unit_text)?
                }
                super::migration::SourceKind::Appimage => unit::replace_known(
                    &self.roots.config,
                    &source.proof.original_unit,
                    &self.unit_text,
                )?,
            }
        } else {
            unit::install_identical_or_new(&self.roots.config, &self.unit_text)?;
        }
        systemd::reload_user_units().await?;
        self.definition_matches(false).await
    }
    async fn start(&self) -> Result<(), String> {
        self.definition_matches(false).await?;
        systemd::control_patinad_service(systemd::PatinadServiceControlAction::Start).await?;
        Ok(())
    }
    async fn matches_target(&self) -> Result<bool, String> {
        let state = systemd::inspect_patinad_service().await;
        if !state.manager_available || state.error.is_some() || !state.active {
            return Ok(false);
        }
        let credentials = crate::engine::api::auth::ApiCredentialStore::new();
        let Some(token) = credentials.load_existing_if_present_at(&self.token_path)? else {
            return Ok(false);
        };
        let client =
            patina_client::Client::new(self.port, token).map_err(|error| error.to_string())?;
        Ok(api_matches_target(&client, &self.identity).await)
    }
    async fn verify(&self) -> Result<(), String> {
        tokio::time::timeout(Duration::from_secs(45), async {
            loop {
                if self.matches_target().await? {
                    self.definition_matches(false).await?;
                    return Ok(());
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
        }).await.map_err(|_| "activation target was not confirmed ready; inspect the pending installation before retrying".to_string())?
    }
    fn finished(&self) {
        // Runtime readiness and the completed record are already durable. A
        // leftover inactive temporary mask must not turn success into a retry.
        if let Some(mask) = &self.restore_mask {
            if let Err(error) = unit::discard_owned_mask(&self.roots.config, mask) {
                eprintln!(
                    "[patinad] active runtime confirmed; saved mask cleanup deferred: {error}"
                );
            }
        }
    }
}

pub(super) async fn verify_manager_roots(roots: &AppPathRoots) -> Result<(), String> {
    let environment = systemd::manager_environment().await?;
    let value = |key: &str| {
        environment.iter().rev().find_map(|entry| {
            let (name, value) = entry.split_once('=')?;
            (name == key && !value.is_empty()).then(|| PathBuf::from(value))
        })
    };
    let home = value("HOME").ok_or("systemd manager does not expose HOME")?;
    if value("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config")) != roots.config
        || value("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share")) != roots.data
    {
        return Err("installer and systemd manager profile roots differ".into());
    }
    Ok(())
}

fn validate_login_intent(control_root: &Path, observed_enabled: bool) -> Result<(), String> {
    let intent = cutover::diagnose(control_root, AppProfile::Production);
    let expected = match intent.state.as_str() {
        "not-requested" => Some(false),
        "prepared" | "activating" | "completed" => intent.background_tracking_at_login,
        _ => None, // The activation owner will reject failed, rolled-back or invalid cutover.
    };
    if expected.is_some_and(|expected| expected != observed_enabled) {
        return Err("service login state differs from the saved preference; reconcile it before activation (no login setting was changed)".into());
    }
    Ok(())
}

async fn api_matches_target(
    client: &patina_client::Client,
    identity: &DaemonExecutableIdentity,
) -> bool {
    let Ok(before) = client.service_snapshot().await else {
        return false;
    };
    let Ok(negotiated) = client.negotiate_tracking_owner().await else {
        return false;
    };
    let Ok(after) = client.service_snapshot().await else {
        return false;
    };
    before.instance_id == after.instance_id
        && before.managed_by_systemd
        && after.managed_by_systemd
        && before.executable.as_ref() == Some(identity)
        && after.executable.as_ref() == Some(identity)
        && negotiated.tracking_ready
        && negotiated.server_version == identity.build.package_version
}

#[cfg(test)]
mod tests;
