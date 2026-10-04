//! Local installation host. This owns activation intent, not tracking or business writes.
mod journal;
mod native;

use crate::app::{runtime_lease, runtime_owner_cutover as cutover};
use crate::platform::{
    app_paths::{AppPathRoots, AppProfile},
    linux::standalone_runtime,
    storage_paths::StoragePaths,
};
use journal::{ActivationRecord, Phase, Store};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

trait ActivationHost {
    async fn preflight(&self) -> Result<(), String>;
    async fn stop(&self) -> Result<(), String>;
    async fn install(&self) -> Result<(), String>;
    async fn start(&self) -> Result<(), String>;
    async fn matches_target(&self) -> Result<bool, String>;
    async fn verify(&self) -> Result<(), String>;
}

pub(crate) async fn activate(
    root: &Path,
    expected: &str,
    api_port: u16,
    allow_debug: bool,
) -> Result<ActivationRecord, String> {
    if api_port < 1024 {
        return Err("activation requires the configured API port in 1024..65535".into());
    }
    let selection = standalone_runtime::hold_selected(root, expected, allow_debug)?;
    let roots = crate::platform::app_paths::environment_roots();
    let paths = crate::platform::storage_paths::resolve_storage_paths_for_profile(
        &roots,
        AppProfile::Production,
    )?;
    let host = native::NativeHost::new(
        &roots,
        &paths,
        selection.root(),
        &selection.selected,
        api_port,
    )?;
    host.preflight().await?;
    let store = Store::open(&paths.control_root)?;
    execute(
        &store,
        &paths,
        &roots,
        selection.root(),
        &selection.selected,
        &host,
    )
    .await
}

async fn execute(
    store: &Store,
    paths: &StoragePaths,
    roots: &AppPathRoots,
    runtime_root: &Path,
    selected: &standalone_runtime::StagedRuntime,
    host: &impl ActivationHost,
) -> Result<ActivationRecord, String> {
    host.preflight().await?;
    let previous = store.read()?;
    if previous.as_ref().is_some_and(|record| {
        record.runtime_root != runtime_root
            || record.config_root != roots.config
            || record.data_root != roots.data
    }) {
        return Err("activation belongs to a different installation or profile; explicit migration is required".into());
    }
    let decision = cutover::decide_desktop_startup(&paths.control_root, AppProfile::Production);
    let (request_id, background, desktop, create_cutover) = match decision {
        cutover::RuntimeOwnerStartupDecision::Embedded => {
            // A rolled-back reservation is not an uninitialized profile.
            if cutover::diagnose(&paths.control_root, AppProfile::Production).state
                != "not-requested"
                || paths.db_path.exists()
            {
                return Err("existing embedded profile requires explicit owner migration before standalone activation".into());
            }
            (
                previous
                    .as_ref()
                    .map(|record| record.cutover_request_id.clone())
                    .unwrap_or(journal::random_id()?),
                false,
                false,
                true,
            )
        }
        cutover::RuntimeOwnerStartupDecision::DaemonClient { reservation, .. } => {
            use cutover::RuntimeOwnerCutoverStatus as Status;
            if reservation.status != Status::Completed
                && !(matches!(reservation.status, Status::Prepared | Status::Activating)
                    && previous
                        .as_ref()
                        .is_some_and(|record| record.cutover_request_id == reservation.request_id))
            {
                return Err(
                    "another owner migration is pending or requires explicit repair".into(),
                );
            }
            (
                reservation.request_id,
                reservation.background_tracking_at_login,
                reservation.desktop_launch_at_login,
                false,
            )
        }
        cutover::RuntimeOwnerStartupDecision::Blocked { reason } => return Err(reason),
    };
    if let Some(record) = previous.as_ref().filter(|record| {
        matches!(record.phase, Phase::Starting | Phase::Completed)
            && record.manifest_sha256 == selected.manifest_sha256
    }) {
        if record.cutover_request_id != request_id {
            return Err("completed activation cutover identity changed".into());
        }
        if host.matches_target().await? {
            host.verify().await?;
            cutover::mark_completed(
                &paths.control_root,
                AppProfile::Production,
                &record.cutover_request_id,
                now(),
            )?;
            let mut completed = record.clone();
            completed.phase = Phase::Completed;
            completed.last_error = None;
            store.write(&completed)?;
            return Ok(completed);
        }
    }
    let mut record = ActivationRecord {
        format_version: 1,
        runtime_root: runtime_root.to_path_buf(),
        config_root: roots.config.clone(),
        data_root: roots.data.clone(),
        manifest_sha256: selected.manifest_sha256.clone(),
        binary_sha256: selected.binary_sha256.clone(),
        cutover_request_id: request_id,
        phase: Phase::Prepared,
        last_error: None,
    };
    store.write(&record)?;
    let result = async {
        host.stop().await?;
        runtime_lease::wait_for_runtime_lease_release(&paths.control_root, Duration::from_secs(15))
            .await?;
        let lease = runtime_lease::acquire_runtime_lease(
            &paths.control_root,
            AppProfile::Production,
            runtime_lease::RuntimeRole::Maintenance,
        )
        .map_err(|error| error.to_string())?;
        // Recheck external configuration after stopping, before any file mutation.
        host.preflight().await?;
        if create_cutover {
            cutover::prepare_reserved(
                &paths.control_root,
                AppProfile::Production,
                &record.cutover_request_id,
                background,
                desktop,
                now(),
            )?;
        }
        match cutover::decide_desktop_startup(&paths.control_root, AppProfile::Production) {
            cutover::RuntimeOwnerStartupDecision::DaemonClient { reservation, .. }
                if reservation.request_id == record.cutover_request_id =>
            {
                if reservation.status != cutover::RuntimeOwnerCutoverStatus::Completed {
                    cutover::mark_activating(
                        &paths.control_root,
                        AppProfile::Production,
                        &record.cutover_request_id,
                        now(),
                    )?;
                }
            }
            _ => return Err("owner cutover changed during activation".into()),
        }
        host.install().await?;
        record.phase = Phase::Starting;
        store.write(&record)?;
        drop(lease);
        host.start().await?;
        host.verify().await?;
        cutover::mark_completed(
            &paths.control_root,
            AppProfile::Production,
            &record.cutover_request_id,
            now(),
        )?;
        record.phase = Phase::Completed;
        record.last_error = None;
        store.write(&record)?;
        Ok::<(), String>(())
    }
    .await;
    if let Err(error) = result {
        // No rollback: the candidate may already have migrated the database.
        if record.phase == Phase::Completed {
            record.phase = Phase::Starting;
        }
        record.last_error = Some(error.chars().take(512).collect());
        if let Err(persist) = store.write(&record) {
            return Err(format!(
                "{error}; failed to record activation state: {persist}"
            ));
        }
        return Err(error);
    }
    Ok(record)
}

fn now() -> u64 {
    crate::engine::runtime_context::now_ms()
}

#[cfg(feature = "desktop")]
pub(crate) fn uses_standalone_binding(
    roots: &AppPathRoots,
    control_root: &Path,
) -> Result<bool, String> {
    let Some(record) = journal::read_at(control_root)? else {
        return Ok(false);
    };
    if record.config_root != roots.config || record.data_root != roots.data {
        return Err("standalone service registration uses different profile roots".into());
    }
    // The operator may have selected a newer target before restarting. Binding
    // ownership follows the registered root, not equality with the running version.
    standalone_runtime::inspect(&record.runtime_root, true)?
        .selected
        .ok_or("registered standalone runtime has no selected version")?;
    let expected = crate::platform::linux::patinad_service_unit::standalone(
        &record.runtime_root,
        &roots.config,
        &roots.data,
    )?;
    if crate::platform::linux::patinad_service_unit::read_existing(
        &roots.config.join("systemd/user/patinad.service"),
    )?
    .as_deref()
        != Some(expected.as_str())
    {
        return Err("standalone activation is incomplete or its unit changed; existing configuration preserved".into());
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
