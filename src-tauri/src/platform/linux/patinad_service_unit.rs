//! Fixed service commands shared by the two Linux distribution hosts.
use std::path::Path;

#[cfg(feature = "desktop")]
pub(crate) const APPIMAGE_HEADER: &str = "# Managed by Patina AppImage runtime v1\n";
pub(crate) const STANDALONE_HEADER: &str = "# Managed by Patina standalone runtime v1\n";

#[cfg(feature = "desktop")]
pub(crate) fn appimage(launcher: &Path, config: &Path, data: &Path) -> Result<String, String> {
    render(launcher, config, data, APPIMAGE_HEADER, " --patinad")
}

pub(crate) fn standalone(root: &Path, config: &Path, data: &Path) -> Result<String, String> {
    render(
        &root.join("current/bin/patinad"),
        config,
        data,
        STANDALONE_HEADER,
        "",
    )
}

fn quoted_path(path: &Path) -> Result<String, String> {
    let value = path.to_str().ok_or("runtime path must be UTF-8")?;
    if !path.is_absolute() || value.chars().any(char::is_control) {
        return Err("runtime path must be absolute and contain no control characters".into());
    }
    Ok(value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%"))
}

fn render(
    launcher: &Path,
    config: &Path,
    data: &Path,
    header: &str,
    arguments: &str,
) -> Result<String, String> {
    let quoted = quoted_path(launcher)?;
    let base = include_str!("../../../../packaging/systemd/patinad.service");
    if base.matches("ExecStart=/usr/bin/patinad").count() != 1
        || base.matches("[Service]\n").count() != 1
    {
        return Err("unexpected packaged daemon service layout".into());
    }
    let mut unit = format!(
        "{header}{}",
        base.replace(
            "ExecStart=/usr/bin/patinad",
            &format!("ExecStart=\"{quoted}\"{arguments}"),
        )
    );
    let roots = format!(
        "Environment=\"XDG_CONFIG_HOME={}\"\nEnvironment=\"XDG_DATA_HOME={}\"\n",
        quoted_path(config)?,
        quoted_path(data)?
    );
    unit = unit.replace("[Service]\n", &format!("[Service]\n{roots}"));
    Ok(unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_uses_the_selected_binary_and_keeps_service_policy() {
        let text = standalone(
            Path::new("/private/runtime"),
            Path::new("/config"),
            Path::new("/data"),
        )
        .unwrap();
        assert!(text.starts_with(STANDALONE_HEADER));
        assert!(text.contains("ExecStart=\"/private/runtime/current/bin/patinad\" --profile production --serve-api --track\n"));
        assert!(text.contains("Environment=PATINA_SYSTEMD_SERVICE=patinad.service\n"));
        assert!(text.contains("KillSignal=SIGINT\n"));
        assert!(text.contains("Restart=on-failure\n"));
        assert!(!text.contains("--patinad"));
    }

    #[test]
    fn quoted_paths_do_not_inject_unit_lines_or_systemd_specifiers() {
        let text = standalone(
            Path::new("/path with space/100%/\"quoted\""),
            Path::new("/config\\path"),
            Path::new("/data"),
        )
        .unwrap();
        assert!(text.contains("100%%/\\\"quoted\\\"/current"));
        assert!(text.contains("XDG_CONFIG_HOME=/config\\\\path"));
        for bad in ["relative", "/bad\nExecStart=bad", "/bad\r", "/bad\0"] {
            assert!(standalone(Path::new(bad), Path::new("/config"), Path::new("/data")).is_err());
            assert!(standalone(Path::new("/runtime"), Path::new(bad), Path::new("/data")).is_err());
            assert!(
                standalone(Path::new("/runtime"), Path::new("/config"), Path::new(bad)).is_err()
            );
        }
    }
}
