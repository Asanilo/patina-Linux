//! Desktop's local delivery adapter. Business/API compatibility stays in the client.
use super::{ReloadTarget, TargetSource};
use std::path::{Path, PathBuf};

pub(super) struct NativeSource {
    control_root: PathBuf,
}
impl NativeSource {
    pub(super) fn new(control_root: &Path) -> Self {
        Self {
            control_root: control_root.to_path_buf(),
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use crate::app::standalone_activation as activation;
    use crate::platform::{
        app_paths::{self, AppPathRoots},
        linux::{
            patinad_service_unit as unit, standalone_runtime as runtime,
            systemd_user_service as systemd,
        },
    };
    use patina_protocol::service::DaemonExecutableIdentity;

    pub(in crate::app::daemon_service::upgrade) struct Guard {
        selection: Option<runtime::SelectedRuntimeGuard>,
        _profile: activation::ClientReloadGuard,
    }

    fn target(
        root: &Path,
        manifest: &str,
        identity: &DaemonExecutableIdentity,
    ) -> Result<ReloadTarget, String> {
        if !(identity.build.protocol.min_supported_client
            ..=identity.build.protocol.max_supported_client)
            .contains(&patina_protocol::CURRENT_PROTOCOL_VERSION)
        {
            return Err("selected backend does not support this client's protocol; finish the backend/client upgrade before reloading".into());
        }
        Ok(ReloadTarget {
            version: identity.build.package_version.clone(),
            identity: Some(identity.clone()),
            manifest: Some(manifest.to_owned()),
            root: Some(root.to_path_buf()),
        })
    }

    async fn verify_definition(root: &Path, roots: &AppPathRoots) -> Result<(), String> {
        let unit_path = roots.config.join("systemd/user/patinad.service");
        if unit::read_existing(&unit_path)?.as_deref()
            != Some(unit::standalone(root, &roots.config, &roots.data)?.as_str())
        {
            return Err(
                "standalone service file changed; inspect installation before reloading".into(),
            );
        }
        let definition = systemd::load_patinad_definition()
            .await?
            .ok_or("standalone service is not loaded")?;
        if Path::new(&definition.fragment_path) != unit_path || !definition.drop_in_paths.is_empty()
        {
            return Err("loaded service no longer belongs to the standalone installation".into());
        }
        unit::validate_standalone_launch(
            root,
            &roots.config,
            &roots.data,
            &systemd::inspect_patinad_launch().await?,
        )
    }

    impl TargetSource for NativeSource {
        type Guard = Guard;
        async fn inspect(&self) -> Result<ReloadTarget, String> {
            let roots = app_paths::environment_roots();
            let control = self.control_root.clone();
            let read_roots = roots.clone();
            let result = tokio::task::spawn_blocking(move || {
                let Some(root) = activation::registered_runtime_root(&read_roots, &control)? else {
                    return Ok(ReloadTarget::bundled());
                };
                activation::require_completed_registration(&control)?;
                let selected = runtime::inspect_declared(&root, true)?
                    .ok_or("no standalone backend is selected")?;
                target(&root, &selected.manifest_sha256, &selected.executable)
            })
            .await
            .map_err(|error| error.to_string())??;
            if let Some(root) = &result.root {
                verify_definition(root, &roots).await?;
            }
            Ok(result)
        }
        async fn hold(&self, expected: &ReloadTarget) -> Result<Self::Guard, String> {
            let roots = app_paths::environment_roots();
            let control = self.control_root.clone();
            let observed = activation::registered_runtime_root(&roots, &control)?;
            if observed != expected.root {
                return Err("backend installation changed after confirmation".into());
            }
            let Some(root) = observed else {
                let profile = activation::hold_client_reload(&roots, &control, None)?;
                return Ok(Guard {
                    selection: None,
                    _profile: profile,
                });
            };
            let manifest = expected
                .manifest
                .clone()
                .ok_or("standalone target is missing its manifest")?;
            let read_roots = roots.clone();
            let read_root = root.clone();
            let (guard, held_target) = tokio::task::spawn_blocking(move || {
                // Use the installer's root -> profile lock order. Both are try-locks.
                let selection = runtime::hold_selected(&read_root, &manifest, true)?;
                let profile =
                    activation::hold_client_reload(&read_roots, &control, Some(&read_root))?;
                let identity = DaemonExecutableIdentity {
                    build: selection.selected.build.clone(),
                    binary_sha256: selection.selected.binary_sha256.clone(),
                };
                let held_target =
                    target(&read_root, &selection.selected.manifest_sha256, &identity)?;
                Ok::<_, String>((
                    Guard {
                        selection: Some(selection),
                        _profile: profile,
                    },
                    held_target,
                ))
            })
            .await
            .map_err(|error| error.to_string())??;
            if held_target != *expected {
                return Err("selected backend changed after confirmation".into());
            }
            verify_definition(&root, &roots).await?;
            let selected = &guard
                .selection
                .as_ref()
                .ok_or("missing held selection")?
                .selected;
            runtime::verify_executable_metadata(
                &selected.directory.join("bin/patinad"),
                &selected.build,
            )
            .await?;
            Ok(guard)
        }
    }
}

#[cfg(not(target_os = "linux"))]
impl TargetSource for NativeSource {
    type Guard = ();
    async fn inspect(&self) -> Result<ReloadTarget, String> {
        let _ = &self.control_root;
        Ok(ReloadTarget::bundled())
    }
    async fn hold(&self, _: &ReloadTarget) -> Result<(), String> {
        Err("backend reload is supported only on Linux".into())
    }
}
