//! Offline, user-requested storage maintenance belongs to the local host.
//! This coordinator never starts an embedded tracker in daemon-client mode.

use std::future::Future;
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

use crate::app::runtime::DesktopRuntimeMode;
use crate::app::runtime_lease::{
    acquire_runtime_lease, wait_for_runtime_lease_release, RuntimeRole,
};
use crate::platform::app_paths::AppProfile;
use crate::platform::storage_access::DesktopStorageAccess;
use crate::platform::{storage_anchor, storage_paths};

pub async fn acquire_startup_access<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<DesktopStorageAccess, String> {
    let paths = storage_paths::default_storage_paths(app)?;
    let has_maintenance = storage_anchor::read_pending_migration(app)?.is_some()
        || storage_anchor::read_maintenance_state(app)?.pending_webview_cache_clear
        || storage_anchor::storage_migration_journal_exists(&paths.control_root)?;
    DesktopStorageAccess::acquire(
        &paths.control_root,
        has_maintenance,
        Duration::from_secs(15),
    )
    .await
}

pub async fn run_managed_startup_maintenance<R: Runtime>(
    app: &AppHandle<R>,
    access: &DesktopStorageAccess,
    mode: DesktopRuntimeMode,
) -> Result<(), String> {
    if !access.is_exclusive() {
        return Ok(());
    }
    if !mode.is_managed_daemon_client() {
        return Err(
            "storage maintenance requires a managed daemon or the embedded owner".to_string(),
        );
    }
    let gate = app.state::<crate::app::daemon_service::DaemonServiceMutationState>();
    let _guard = gate.lock().await;
    let paths = storage_paths::default_storage_paths(app)?;
    let pending = storage_anchor::read_pending_migration(app)?;
    let needs_runtime_stop = pending
        .as_ref()
        .is_some_and(|pending| pending.source_data_root != pending.target_data_root)
        || storage_anchor::storage_migration_journal_exists(&paths.control_root)?;
    if !needs_runtime_stop {
        // The daemon never opens WebView storage. Desktop exclusion is enough.
        wait_for_legacy_access(app, false).await?;
        return crate::data::storage_migration::run_startup_storage_maintenance(app).await;
    }

    #[cfg(target_os = "linux")]
    {
        use crate::app::runtime_owner_cutover::{
            decide_desktop_startup, RuntimeOwnerStartupDecision,
        };
        use crate::platform::linux::systemd_user_service::{
            control_patinad_service, PatinadServiceControlAction,
        };
        let profile = crate::platform::app_paths::app_profile(app);
        if profile != AppProfile::Production
            || !matches!(
                decide_desktop_startup(&paths.control_root, profile),
                RuntimeOwnerStartupDecision::DaemonClient {
                    should_attempt_service_start: true,
                    ..
                }
            )
        {
            return Err(
                "storage maintenance requires a valid managed runtime reservation".to_string(),
            );
        }
        with_stopped_runtime(
            &paths.control_root,
            profile,
            || async {
                control_patinad_service(PatinadServiceControlAction::Stop)
                    .await
                    .map(|_| ())
            },
            || async {
                control_patinad_service(PatinadServiceControlAction::Start)
                    .await
                    .map(|_| ())
            },
            || async {
                wait_for_legacy_access(app, true).await?;
                crate::data::storage_migration::run_startup_storage_maintenance(app).await
            },
        )
        .await
    }
    #[cfg(not(target_os = "linux"))]
    Err("managed storage maintenance requires Linux".to_string())
}

pub async fn wait_for_legacy_access<R: Runtime>(
    app: &AppHandle<R>,
    data_changed: bool,
) -> Result<(), String> {
    let defaults = storage_paths::default_storage_paths(app)?;
    let pending = storage_anchor::read_pending_migration(app)?;
    // Recovery may already have switched one anchor. Check the original paths
    // from the appointment while leaving resolution/validation to the executor.
    let paths = if let Some(pending) = pending.as_ref() {
        storage_paths::StoragePaths::from_roots(
            defaults.control_root.clone(),
            defaults.stable_product_data_root.clone(),
            pending.source_data_root.clone(),
            pending.source_webview_root.clone(),
            pending.source_data_root != defaults.data_root,
            pending.source_webview_root != defaults.webview_root,
        )
    } else {
        storage_paths::resolve_storage_paths(app)?
    };
    let webview_changed = pending
        .as_ref()
        .is_some_and(|pending| pending.source_webview_root != pending.target_webview_root)
        || storage_anchor::read_maintenance_state(app)?.pending_webview_cache_clear
        || storage_anchor::storage_migration_journal_exists(&defaults.control_root)?;
    crate::platform::storage_access::wait_for_legacy_storage_release(
        &paths,
        data_changed,
        webview_changed,
        Duration::from_secs(15),
    )
    .await?;
    if let Some(pending) = pending {
        let target = storage_paths::StoragePaths::from_roots(
            defaults.control_root,
            defaults.stable_product_data_root,
            pending.target_data_root.clone(),
            pending.target_webview_root.clone(),
            pending.target_data_root != defaults.data_root,
            pending.target_webview_root != defaults.webview_root,
        );
        crate::platform::storage_access::wait_for_legacy_storage_release(
            &target,
            data_changed,
            webview_changed,
            Duration::from_secs(15),
        )
        .await?;
    }
    Ok(())
}

async fn with_stopped_runtime<Stop, StopFuture, Start, StartFuture, Maintain, MaintainFuture>(
    control_root: &Path,
    profile: AppProfile,
    stop: Stop,
    start: Start,
    maintain: Maintain,
) -> Result<(), String>
where
    Stop: FnOnce() -> StopFuture,
    StopFuture: Future<Output = Result<(), String>>,
    Start: FnOnce() -> StartFuture,
    StartFuture: Future<Output = Result<(), String>>,
    Maintain: FnOnce() -> MaintainFuture,
    MaintainFuture: Future<Output = Result<(), String>>,
{
    stop().await?;
    let result = async {
        wait_for_runtime_lease_release(control_root, Duration::from_secs(5)).await?;
        let lease = acquire_runtime_lease(control_root, profile, RuntimeRole::Maintenance)
            .map_err(|error| error.to_string())?;
        let result = maintain().await;
        drop(lease);
        result
    }
    .await;
    // Restore service availability on ordinary failure too. A recovery journal
    // makes daemon startup fail closed if maintenance could not safely finish.
    let restart = start().await;
    match (result, restart) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(format!(
            "storage maintenance finished, but daemon restart failed: {error}"
        )),
        (Err(error), Err(restart)) => {
            Err(format!("{error}; daemon restart also failed: {restart}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[tokio::test]
    async fn offline_maintenance_holds_runtime_lease_and_restarts_after_failure() {
        let root = std::env::temp_dir().join(format!(
            "patina-offline-maintenance-{}-{}",
            std::process::id(),
            crate::app::runtime::now_ms()
        ));
        let events = Mutex::new(Vec::new());
        let daemon = Mutex::new(Some(
            acquire_runtime_lease(&root, AppProfile::Dev, RuntimeRole::Daemon).unwrap(),
        ));
        let error = with_stopped_runtime(
            &root,
            AppProfile::Dev,
            || async {
                events.lock().unwrap().push("stop");
                daemon.lock().unwrap().take();
                Ok(())
            },
            || async {
                let lease =
                    acquire_runtime_lease(&root, AppProfile::Dev, RuntimeRole::Daemon).unwrap();
                events.lock().unwrap().push("start");
                drop(lease);
                Ok(())
            },
            || async {
                assert!(
                    acquire_runtime_lease(&root, AppProfile::Dev, RuntimeRole::Daemon).is_err()
                );
                events.lock().unwrap().push("maintain");
                Err("synthetic copy failure".to_string())
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error, "synthetic copy failure");
        assert_eq!(*events.lock().unwrap(), ["stop", "maintain", "start"]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn failed_stop_does_not_enter_maintenance_or_start_another_owner() {
        let error = with_stopped_runtime(
            Path::new("/unused"),
            AppProfile::Dev,
            || async { Err("stop refused".to_string()) },
            || async { panic!("must not start after an unsuccessful stop") },
            || async { panic!("must not migrate with the runtime still active") },
        )
        .await
        .unwrap_err();
        assert_eq!(error, "stop refused");
    }
}
