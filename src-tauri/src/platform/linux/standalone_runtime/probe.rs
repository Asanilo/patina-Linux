//! Bounded executable preflight shared by installation and explicit client reload.
use patina_protocol::build_info::DaemonBuildInfo;
use std::{path::Path, process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;

fn command(executable: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(executable);
    // AppImage clients carry mount-specific loader paths. A standalone target
    // must use its own native dependencies, as it does when started by systemd.
    for key in [
        "APPIMAGE",
        "APPDIR",
        "ARGV0",
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
    ] {
        command.env_remove(key);
    }
    command
        .arg("--build-info")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

pub(crate) async fn verify_executable_metadata(
    executable: &Path,
    expected: &DaemonBuildInfo,
) -> Result<(), String> {
    let mut child = command(executable)
        .spawn()
        .map_err(|error| error.to_string())?;
    let mut output = child
        .stdout
        .take()
        .ok_or("missing metadata stdout")?
        .take(16 * 1024 + 1);
    let mut errors = child
        .stderr
        .take()
        .ok_or("missing metadata stderr")?
        .take(16 * 1024 + 1);
    let mut bytes = Vec::new();
    let mut stderr = Vec::new();
    let info: DaemonBuildInfo = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::try_join!(
            output.read_to_end(&mut bytes),
            errors.read_to_end(&mut stderr)
        )
        .map_err(|error| error.to_string())?;
        if bytes.len() > 16 * 1024
            || !stderr.is_empty()
            || !child
                .wait()
                .await
                .map_err(|error| error.to_string())?
                .success()
        {
            return Err("candidate metadata preflight failed".to_string());
        }
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "candidate metadata preflight timed out".to_string())??;
    if &info != expected {
        return Err("candidate executable metadata differs from its installed manifest".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_target_does_not_inherit_appimage_loader_overrides() {
        let command = command(Path::new("/private/bin/patinad"));
        let env: std::collections::HashMap<_, _> = command.as_std().get_envs().collect();
        for key in [
            "APPIMAGE",
            "APPDIR",
            "ARGV0",
            "LD_LIBRARY_PATH",
            "LD_PRELOAD",
        ] {
            assert_eq!(env[std::ffi::OsStr::new(key)], None);
        }
    }
    #[tokio::test]
    async fn candidate_metadata_is_checked_before_runtime_use() {
        use std::{fs, os::unix::fs::PermissionsExt};
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).unwrap();
        let root = std::env::temp_dir().join(format!(
            "patina-metadata-probe-{:x}",
            u128::from_ne_bytes(nonce)
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("probe");
        let expected = crate::app::daemon::build_info::current();
        let json = serde_json::to_string(&expected).unwrap();
        fs::write(&path, format!("#!/bin/sh\nprintf '%s' '{json}'\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        verify_executable_metadata(&path, &expected).await.unwrap();
        let mut different = expected.clone();
        different.package_version = "99.0.0".into();
        assert!(verify_executable_metadata(&path, &different)
            .await
            .unwrap_err()
            .contains("differs"));
        fs::write(
            &path,
            format!("#!/bin/sh\nprintf '%s' '{json}'\nprintf error >&2\n"),
        )
        .unwrap();
        assert!(verify_executable_metadata(&path, &expected).await.is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
