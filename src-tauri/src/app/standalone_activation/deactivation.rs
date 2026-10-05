//! Explicit standalone service deactivation. Data and staged runtimes are retained.
use super::*;
use crate::platform::linux::{patinad_service_unit as unit, systemd_user_service as systemd};
use std::fs::File;

pub(super) trait DeactivationHost {
    async fn preflight(&self, record: &ActivationRecord) -> Result<(), String>;
    fn prepare_mask(&self) -> Result<unit::OwnedMask, String>;
    async fn stop(&self) -> Result<(), String>;
    async fn install_mask(&self, mask: &unit::OwnedMask) -> Result<(), String>;
    async fn verify_masked(&self, mask: &unit::OwnedMask) -> Result<(), String>;
}

pub(crate) async fn deactivate(
    root: &Path,
    expected: &str,
    allow_debug: bool,
) -> Result<ActivationRecord, String> {
    let selected = standalone_runtime::hold_selected(root, expected, allow_debug)?;
    let roots = crate::platform::app_paths::environment_roots();
    let paths = crate::platform::storage_paths::resolve_storage_paths_for_profile(
        &roots,
        AppProfile::Production,
    )?;
    let store = Store::open(&paths.control_root)?;
    let host = NativeDeactivation {
        roots: &roots,
        paths: &paths,
        root: selected.root(),
        selected_binary: &selected.selected.binary_sha256,
        unit: unit::standalone(selected.root(), &roots.config, &roots.data)?,
    };
    execute_deactivation(
        &store,
        &paths,
        &roots,
        selected.root(),
        &selected.selected,
        &host,
    )
    .await
}

pub(super) async fn execute_deactivation(
    store: &Store,
    paths: &StoragePaths,
    roots: &AppPathRoots,
    root: &Path,
    selected: &standalone_runtime::StagedRuntime,
    host: &impl DeactivationHost,
) -> Result<ActivationRecord, String> {
    let mut record = store
        .read()?
        .ok_or("there is no registered standalone installation to deactivate")?;
    if record.runtime_root != root
        || record.config_root != roots.config
        || record.data_root != roots.data
    {
        return Err("deactivation belongs to a different installation or profile".into());
    }
    if !(matches!(
        record.phase,
        Phase::Completed | Phase::Starting | Phase::Deactivating | Phase::Deactivated
    ) || record.phase == Phase::Prepared && record.deactivation_mask.is_some())
    {
        return Err(
            "the installation has not yet established a standalone service to deactivate".into(),
        );
    }
    let cutover::RuntimeOwnerStartupDecision::DaemonClient { reservation, .. } =
        cutover::decide_owner_for_installation(&paths.control_root, AppProfile::Production)
    else {
        return Err("deactivation requires the registered daemon owner".into());
    };
    if reservation.request_id != record.cutover_request_id
        || !matches!(
            reservation.status,
            cutover::RuntimeOwnerCutoverStatus::Completed
                | cutover::RuntimeOwnerCutoverStatus::Activating
        )
    {
        return Err("deactivation owner identity changed or migration is incomplete".into());
    }
    require_version_floor(Some(&record), &selected.build.package_version)?;
    host.preflight(&record).await?;
    if record.phase == Phase::Deactivated {
        let _lease = runtime_lease::acquire_runtime_lease(
            &paths.control_root,
            AppProfile::Production,
            runtime_lease::RuntimeRole::Maintenance,
        )
        .map_err(|error| error.to_string())?;
        host.verify_masked(
            record
                .deactivation_mask
                .as_ref()
                .ok_or("deactivation mask is missing")?,
        )
        .await?;
        return Ok(record);
    }
    if record.deactivation_mask.is_none() {
        record.deactivation_mask = Some(host.prepare_mask()?);
        record.deactivation_binary_sha256 = Some(selected.binary_sha256.clone());
    }
    record.minimum_runtime_version = Some(selected.build.package_version.clone());
    record.phase = Phase::Deactivating;
    record.runtime_start_allowed = false;
    record.last_error = None;
    store.write(&record)?;
    let result = async {
        host.preflight(&record).await?;
        host.stop().await?;
        runtime_lease::wait_for_runtime_lease_release(&paths.control_root, Duration::from_secs(15))
            .await?;
        let _lease = runtime_lease::acquire_runtime_lease(
            &paths.control_root,
            AppProfile::Production,
            runtime_lease::RuntimeRole::Maintenance,
        )
        .map_err(|error| error.to_string())?;
        host.preflight(&record).await?;
        let mask = record
            .deactivation_mask
            .as_ref()
            .ok_or("deactivation mask is missing")?;
        host.install_mask(mask).await?;
        host.verify_masked(mask).await?;
        record.phase = Phase::Deactivated;
        store.write(&record)?;
        Ok::<(), String>(())
    }
    .await;
    if let Err(error) = result {
        record.phase = Phase::Deactivating;
        record.last_error = Some(error.chars().take(512).collect());
        store
            .write(&record)
            .map_err(|persist| format!("{error}; failed to record deactivation: {persist}"))?;
        return Err(error);
    }
    Ok(record)
}

pub(super) async fn verify_masked(
    roots: &AppPathRoots,
    mask: &unit::OwnedMask,
) -> Result<(), String> {
    if !mask.is_installed(&roots.config)? {
        return Err("standalone mask was replaced; existing configuration preserved".into());
    }
    let state = systemd::inspect_patinad_service().await;
    if !state.manager_available
        || state.error.is_some()
        || state.active
        || state.unit_file_state.as_deref() != Some("masked")
        || systemd::patinad_load_state()
            .await?
            .as_deref()
            .is_some_and(|state| state != "masked")
    {
        return Err(
            "systemd has not confirmed the standalone service is masked and inactive".into(),
        );
    }
    Ok(())
}

struct NativeDeactivation<'a> {
    roots: &'a AppPathRoots,
    paths: &'a StoragePaths,
    root: &'a Path,
    selected_binary: &'a str,
    unit: String,
}

impl DeactivationHost for NativeDeactivation<'_> {
    async fn preflight(&self, record: &ActivationRecord) -> Result<(), String> {
        native::verify_manager_roots(self.roots).await?;
        let owned_mask = record
            .deactivation_mask
            .as_ref()
            .map(|mask| mask.is_installed(&self.roots.config))
            .transpose()?
            .unwrap_or(false);
        if !owned_mask
            && unit::read_existing(&self.roots.config.join("systemd/user/patinad.service"))?
                .as_deref()
                != Some(&self.unit)
        {
            return Err(
                "deactivation must preserve a missing, custom or replaced standalone unit".into(),
            );
        }
        let state = systemd::inspect_patinad_service().await;
        if !state.manager_available || state.error.is_some() {
            return Err("cannot inspect standalone service before deactivation".into());
        }
        let owner = runtime_lease::inspect_locked_owner(&self.paths.control_root)?;
        if owned_mask
            && !state.active
            && systemd::patinad_load_state()
                .await?
                .as_deref()
                .is_none_or(|state| state == "masked")
        {
            require_idle_owner(owner)?;
            return Ok(());
        }
        let definition = systemd::load_patinad_definition()
            .await?
            .ok_or("standalone unit is not loaded")?;
        if !definition.drop_in_paths.is_empty()
            || Path::new(&definition.fragment_path)
                != self.roots.config.join("systemd/user/patinad.service")
        {
            return Err(
                "loaded unit or drop-ins no longer belong to this standalone installation".into(),
            );
        }
        let launch = systemd::inspect_patinad_launch().await?;
        unit::validate_standalone_launch(self.root, &self.roots.config, &self.roots.data, &launch)?;
        if state.active {
            let owner = owner.ok_or("active service does not own a runtime lease")?;
            if owner.pid != launch.main_pid
                || owner.role != runtime_lease::RuntimeRole::Daemon
                || owner.profile != "production"
            {
                return Err("service PID does not own this standalone profile".into());
            }
            // This works even when the API is unhealthy. Read the kernel-held
            // executable, not the pathname that an update may have replaced.
            use sha2::{Digest, Sha256};
            use std::io::Read;
            let mut image = File::open(format!("/proc/{}/exe", owner.pid))
                .map_err(|error| error.to_string())?;
            let size = image.metadata().map_err(|error| error.to_string())?.len();
            if size > 512 * 1024 * 1024 {
                return Err("running executable exceeds verification limit".into());
            }
            let mut hash = Sha256::new();
            let mut buffer = [0_u8; 65536];
            let mut total = 0_u64;
            loop {
                let bytes = image.read(&mut buffer).map_err(|error| error.to_string())?;
                if bytes == 0 {
                    break;
                }
                total += bytes as u64;
                if total > size {
                    return Err("running executable changed while reading".into());
                }
                hash.update(&buffer[..bytes]);
            }
            if total != size {
                return Err("running executable changed while reading".into());
            }
            let digest = format!("{:x}", hash.finalize());
            if digest != record.binary_sha256
                && digest != self.selected_binary
                && record.deactivation_binary_sha256.as_deref() != Some(digest.as_str())
            {
                return Err(
                    "running executable does not belong to the registered or selected runtime"
                        .into(),
                );
            }
            if runtime_lease::inspect_locked_owner(&self.paths.control_root)?.as_ref()
                != Some(&owner)
                || systemd::inspect_patinad_launch().await?.main_pid != owner.pid
            {
                return Err("service owner changed during deactivation verification".into());
            }
        } else {
            require_idle_owner(owner)?;
        }
        Ok(())
    }

    fn prepare_mask(&self) -> Result<unit::OwnedMask, String> {
        unit::prepare_owned_mask(&self.roots.config, &self.unit)
    }
    async fn stop(&self) -> Result<(), String> {
        let state = systemd::inspect_patinad_service().await;
        if !state.manager_available || state.error.is_some() {
            return Err("cannot inspect service before stopping".into());
        }
        if state.active {
            systemd::control_patinad_service(systemd::PatinadServiceControlAction::Stop).await?;
        }
        Ok(())
    }
    async fn install_mask(&self, mask: &unit::OwnedMask) -> Result<(), String> {
        unit::publish_owned_mask(&self.roots.config, &self.unit, mask)?;
        systemd::reload_user_units().await
    }
    async fn verify_masked(&self, mask: &unit::OwnedMask) -> Result<(), String> {
        verify_masked(self.roots, mask).await
    }
}

fn require_idle_owner(owner: Option<runtime_lease::RuntimeOwner>) -> Result<(), String> {
    if owner.is_some_and(|owner| {
        owner.pid != std::process::id()
            || owner.role != runtime_lease::RuntimeRole::Maintenance
            || owner.profile != "production"
    }) {
        return Err("profile is owned outside this standalone service".into());
    }
    Ok(())
}
