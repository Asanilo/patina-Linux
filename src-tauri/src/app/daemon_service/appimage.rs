//! AppImage packaging orchestration; tracking ownership remains the existing service/lease.
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};

const UNIT_HEADER: &str = "# Managed by Patina AppImage runtime v1\n";

pub(crate) async fn ensure_runtime<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    use crate::platform::{
        app_paths,
        linux::{appimage_runtime::PreparedRuntime, appimage_update, systemd_user_service},
    };
    if app_paths::app_profile(app) != app_paths::AppProfile::Production
        || tauri::utils::platform::bundle_type() != Some(tauri::utils::config::BundleType::AppImage)
    {
        return Ok(());
    }
    let roots = app_paths::environment_roots();
    let paths = crate::platform::storage_paths::default_storage_paths_for_profile(
        &roots,
        app_paths::AppProfile::Production,
    );
    let unit_path = roots.config.join("systemd/user/patinad.service");
    let packaged_unit = [
        "/usr/lib/systemd/user/patinad.service",
        "/lib/systemd/user/patinad.service",
    ]
    .iter()
    .any(|path| Path::new(path).is_file());
    let root = paths.stable_product_data_root.join("runtime-appimage");
    let unit = unit_text(&root.join("current/AppRun"), &roots.config, &roots.data)?;
    let install = should_install(&unit_path, packaged_unit, &unit)?;
    validate_manager_roots(
        &systemd_user_service::manager_environment().await?,
        &roots,
        !install,
    )?;
    if !install {
        return Ok(());
    }
    let source =
        PathBuf::from(std::env::var_os("APPDIR").ok_or("AppImage did not provide APPDIR")?)
            .canonicalize()
            .map_err(|error| error.to_string())?;
    let executable = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if executable
        != source
            .join("usr/bin/Patina")
            .canonicalize()
            .map_err(|error| error.to_string())?
    {
        return Err("AppImage APPDIR does not match the running executable".into());
    }
    let image = PathBuf::from(
        std::env::var_os("APPIMAGE").ok_or("AppImage did not provide its package path")?,
    );
    let parent = paths.stable_product_data_root;
    let prepared = tokio::task::spawn_blocking(move || {
        fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let fingerprint = appimage_update::fingerprint(&image)?;
        PreparedRuntime::prepare(&source, &root, env!("CARGO_PKG_VERSION"), &fingerprint)
    })
    .await
    .map_err(|error| error.to_string())??;
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new(prepared.launcher())
            .args(["--patinad", "--version"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "AppImage runtime preflight timed out")?
    .map_err(|error| error.to_string())?;
    if !output.status.success()
        || output.stdout != format!("patinad {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    {
        return Err("AppImage runtime preflight failed; existing service was not changed".into());
    }
    tokio::task::spawn_blocking(move || {
        prepared.publish()?;
        install_unit(&unit_path, &unit)
    })
    .await
    .map_err(|error| error.to_string())??;
    systemd_user_service::reload_user_units().await
}

fn validate_manager_roots(
    environment: &[String],
    roots: &crate::platform::app_paths::AppPathRoots,
    reuse_deb: bool,
) -> Result<(), String> {
    let value = |key: &str| {
        environment.iter().rev().find_map(|entry| {
            let (name, value) = entry.split_once('=')?;
            (name == key && !value.is_empty()).then(|| PathBuf::from(value))
        })
    };
    let home = value("HOME").ok_or("systemd manager does not expose HOME")?;
    let config = value("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
    let data = value("XDG_DATA_HOME").unwrap_or_else(|| home.join(".local/share"));
    if roots.config != config || (reuse_deb && roots.data != data) {
        return Err("AppImage and systemd use different profile roots; portable HOME/config roots or DEB data-root overrides are not supported".into());
    }
    Ok(())
}

fn should_install(path: &Path, packaged_unit: bool, expected: &str) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            // Never replace a mask, custom unit or another user's configuration.
            // SAFETY: geteuid has no pointer arguments.
            if !meta.is_file()
                || meta.len() > 16 * 1024
                || meta.mode() & 0o022 != 0
                || meta.uid() != unsafe { libc::geteuid() }
            {
                return Err(
                    "existing patinad user unit is not a writable managed AppImage unit".into(),
                );
            }
            let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
            if text != expected {
                return Err("custom patinad user unit preserved; inspect service configuration before AppImage setup".into());
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(!packaged_unit),
        Err(error) => Err(error.to_string()),
    }
}

fn unit_text(launcher: &Path, config: &Path, data: &Path) -> Result<String, String> {
    let value = launcher.to_str().ok_or("runtime path must be UTF-8")?;
    if !launcher.is_absolute() || value.chars().any(char::is_control) {
        return Err("runtime path must be absolute and contain no control characters".into());
    }
    let quoted = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    let base = include_str!("../../../../packaging/systemd/patinad.service");
    let mut unit = format!(
        "{UNIT_HEADER}{}",
        base.replace(
            "ExecStart=/usr/bin/patinad",
            &format!("ExecStart=\"{quoted}\" --patinad")
        )
    );
    let environment = |key: &str, path: &Path| -> Result<String, String> {
        let value = path.to_str().ok_or("runtime environment must be UTF-8")?;
        if !path.is_absolute() || value.chars().any(char::is_control) {
            return Err("invalid runtime environment path".into());
        }
        let value = value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%");
        Ok(format!("Environment=\"{key}={value}\"\n"))
    };
    let roots = format!(
        "{}{}",
        environment("XDG_CONFIG_HOME", config)?,
        environment("XDG_DATA_HOME", data)?
    );
    unit = unit.replace("[Service]\n", &format!("[Service]\n{roots}"));
    Ok(unit)
}

fn install_unit(path: &Path, expected: &str) -> Result<(), String> {
    if path.try_exists().map_err(|error| error.to_string())? {
        should_install(path, false, expected)?;
        return Ok(());
    }
    let parent = path.parent().ok_or("missing user unit directory")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(|error| error.to_string())?;
    let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let temporary = parent.join(format!(".patina-unit-{suffix}.tmp"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    // No overwrite: an external unit installed during staging wins and is preserved.
    let result = (|| {
        file.write_all(expected.as_bytes())
            .and_then(|_| file.sync_all())?;
        fs::hard_link(&temporary, path)?;
        File::open(parent)?.sync_all()
    })();
    let _ = fs::remove_file(temporary);
    result.map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coexistence_preserves_custom_units_masks_and_profile_roots() {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let root = std::env::temp_dir().join(format!("patina-unit-{suffix}"));
        fs::create_dir(&root).unwrap();
        let path = root.join("patinad.service");
        assert!(!should_install(&path, true, "managed").unwrap());
        assert!(should_install(&path, false, "managed").unwrap());
        install_unit(&path, "managed").unwrap();
        assert!(should_install(&path, true, "managed").unwrap());
        install_unit(&path, "managed").unwrap();
        assert!(install_unit(&path, "other").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "managed");
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink("/dev/null", &path).unwrap();
        assert!(should_install(&path, true, "managed").is_err());
        assert!(install_unit(&path, "managed").is_err());
        assert_eq!(fs::read_link(&path).unwrap(), Path::new("/dev/null"));
        let mut roots = crate::platform::app_paths::AppPathRoots {
            config: "/home/test/.config".into(),
            data: "/home/test/.local/share".into(),
            local_data: "/home/test/.local/share".into(),
        };
        let environment = vec!["HOME=/home/test".into()];
        assert!(validate_manager_roots(&environment, &roots, true).is_ok());
        roots.data = "/other/data".into();
        assert!(validate_manager_roots(&environment, &roots, true).is_err());
        assert!(validate_manager_roots(&environment, &roots, false).is_ok());
        roots.config = "/portable/config".into();
        assert!(validate_manager_roots(&environment, &roots, false).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn service_uses_stable_launcher_and_escapes_systemd_specifiers() {
        let unit = unit_text(
            Path::new("/home/test user/100%/$name/current/AppRun"),
            Path::new("/home/test user/.config"),
            Path::new("/home/test user/.local/share"),
        )
        .unwrap();
        assert!(unit.starts_with(UNIT_HEADER));
        assert!(unit.contains("ExecStart=\"/home/test user/100%%/$$name/current/AppRun\" --patinad --profile production --serve-api --track"));
        assert!(unit.contains("Environment=PATINA_SYSTEMD_SERVICE=patinad.service"));
        assert!(!unit.contains("/tmp/"));
        assert!(unit.contains("Environment=\"XDG_CONFIG_HOME=/home/test user/.config\""));
        assert!(unit_text(
            Path::new("/path\nExecStart=bad"),
            Path::new("/config"),
            Path::new("/data")
        )
        .is_err());
    }
}
