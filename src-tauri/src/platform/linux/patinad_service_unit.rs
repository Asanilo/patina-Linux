//! Fixed service commands shared by the two Linux distribution hosts.
use std::path::Path;

pub(crate) fn read_existing(path: &Path) -> Result<Option<String>, String> {
    use std::{
        fs::OpenOptions,
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    // SAFETY: geteuid takes no arguments and has no side effects.
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.len() > 16 * 1024
        || metadata.mode() & 0o022 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err("existing user unit is not a bounded user-owned regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(16 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 16 * 1024 {
        return Err("user unit exceeds size limit".into());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|error| error.to_string())
}

pub(crate) fn install_identical_or_new(config: &Path, expected: &str) -> Result<(), String> {
    use std::{
        fs::{self, File, OpenOptions},
        io::Write,
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    };
    for directory in [
        config.to_path_buf(),
        config.join("systemd"),
        config.join("systemd/user"),
    ] {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&directory)
            .map_err(|error| error.to_string())?;
        let metadata = fs::symlink_metadata(&directory).map_err(|error| error.to_string())?;
        // SAFETY: geteuid takes no arguments and has no side effects.
        if !metadata.is_dir()
            || metadata.mode() & 0o022 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err("user unit directory must be user-owned and not writable by others".into());
        }
    }
    let parent = config.join("systemd/user");
    let path = parent.join("patinad.service");
    if let Some(existing) = read_existing(&path)? {
        return if existing == expected {
            Ok(())
        } else {
            Err("custom or different installation unit preserved".into())
        };
    }
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(
        ".patina-unit-{:x}.tmp",
        u128::from_ne_bytes(random)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(expected.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|error| error.to_string())?;
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let source =
            CString::new(temporary.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
        let target =
            CString::new(path.as_os_str().as_bytes()).map_err(|error| error.to_string())?;
        // SAFETY: both C strings live through the call; no pointers are retained.
        // NOREPLACE preserves concurrent custom units, without a crash window that
        // leaves a two-link unit which our trust check would correctly reject.
        if unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        File::open(&parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| error.to_string())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

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
    fn unit_publication_is_single_link_idempotent_and_preserves_custom_files_and_masks() {
        use std::{
            fs,
            os::unix::fs::{symlink, MetadataExt},
        };
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).unwrap();
        let root = std::env::temp_dir().join(format!(
            "patina-publish-unit-{:x}",
            u128::from_ne_bytes(random)
        ));
        let config = root.join("config");
        let path = config.join("systemd/user/patinad.service");
        install_identical_or_new(&config, "known unit").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().nlink(), 1);
        let inode = fs::metadata(&path).unwrap().ino();
        install_identical_or_new(&config, "known unit").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        assert!(install_identical_or_new(&config, "different unit").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "known unit");
        fs::remove_file(&path).unwrap();
        symlink("/dev/null", &path).unwrap();
        assert!(install_identical_or_new(&config, "known unit").is_err());
        assert_eq!(fs::read_link(&path).unwrap(), Path::new("/dev/null"));
        fs::remove_file(&path).unwrap();
        let outside = root.join("outside");
        fs::write(&outside, "outside contents").unwrap();
        fs::hard_link(&outside, &path).unwrap();
        assert!(install_identical_or_new(&config, "known unit").is_err());
        assert_eq!(fs::read_to_string(&outside).unwrap(), "outside contents");
        fs::remove_dir_all(root).unwrap();
    }

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
